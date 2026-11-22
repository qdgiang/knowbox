mod draft;
mod heartbeat;
mod history;
pub(crate) mod metrics;
pub(crate) mod raw;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::KbRoot;
use crate::notes::{self, Note, NoteEntry, ParseError, body_links, is_id};
use crate::store::{Inventory, regular_file};
use crate::{git, index, journal};

pub(crate) use draft::{DraftTarget, lint_draft};
pub(crate) use heartbeat::heartbeats;
use history::{History, Waivers};
pub(crate) use raw::{Mark, RawEvidence, raw_backlog, raw_evidence, raw_mark};

const REQUIRED: [&str; 9] = [
    "id",
    "title",
    "type",
    "domain",
    "status",
    "confidence",
    "created",
    "updated",
    "summary",
];
const ENUMS: [(&str, &[&str]); 3] = [
    (
        "type",
        &[
            "decision",
            "fact",
            "gotcha",
            "howto",
            "project",
            "reference",
            "session",
        ],
    ),
    ("status", &["active", "draft", "superseded", "verified"]),
    ("confidence", &["high", "low", "medium"]),
];
const LINK_FIELDS: [&str; 2] = ["related", "supersedes"];

#[derive(Debug)]
pub(crate) enum HeartbeatCheck {
    Enabled(SystemTime),
    Disabled,
}

impl HeartbeatCheck {
    pub(crate) fn from_env() -> Self {
        match std::env::var_os("KNOWBOX_NO_HEARTBEAT") {
            Some(value) if !matches!(value.to_string_lossy().trim(), "" | "0" | "false" | "no") => {
                Self::Disabled
            }
            _ => Self::Enabled(SystemTime::now()),
        }
    }
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    if !regular_file(path).with_context(|| format!("inspecting {}", path.display()))? {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("reading {} as UTF-8", path.display()))
}

fn is_line_break(ch: char) -> bool {
    matches!(
        ch,
        '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

fn split_line(text: &str) -> (&str, &str, &str) {
    let Some((index, ch)) = text.char_indices().find(|&(_, ch)| is_line_break(ch)) else {
        return (text, "", "");
    };
    let end = if text[index..].starts_with("\r\n") {
        index + 2
    } else {
        index + ch.len_utf8()
    };
    (&text[..index], &text[index..end], &text[end..])
}

fn iso_date(text: &str) -> bool {
    // The legacy lint tests the field's lexical form, not calendar validity.
    journal::iso_day(text)
}

fn repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for ch in value.chars() {
        match ch {
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            ch if ch == quote || ch == '\\' => {
                out.push('\\');
                out.push(ch);
            }
            ch if ch.is_control() || (ch.is_whitespace() && ch != ' ') => {
                let code = u32::from(ch);
                let encoded = if code <= 0xff {
                    format!("\\x{code:02x}")
                } else {
                    format!("\\u{code:04x}")
                };
                out.push_str(&encoded);
            }
            ch => out.push(ch),
        }
    }
    out.push(quote);
    out
}

fn field_repr(note: &Note, field: &str) -> String {
    match note.scalar(field) {
        Some(value) => repr(value),
        None => note.text(field, "None").into_owned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Default)]
struct Location<'a> {
    path: Option<&'a str>,
    note_id: Option<&'a str>,
    domain: Option<&'a str>,
}

impl<'a> From<&'a str> for Location<'a> {
    fn from(path: &'a str) -> Self {
        Self {
            path: Some(path),
            ..Self::default()
        }
    }
}

impl<'a> Location<'a> {
    fn note(path: &'a str, note: &'a Note) -> Self {
        Self {
            path: Some(path),
            note_id: note.scalar("id").filter(|id| is_id(id)),
            domain: note.scalar("domain"),
        }
    }
}

#[derive(Debug, Serialize)]
struct Finding {
    code: &'static str,
    severity: Severity,
    waived: bool,
    path: Option<String>,
    note_id: Option<String>,
    domain: Option<String>,
    message: String,
    #[serde(skip)]
    word_count: Option<usize>,
}

impl Finding {
    fn new(
        code: &'static str,
        severity: Severity,
        waived: bool,
        location: Location<'_>,
        message: String,
    ) -> Self {
        Self {
            code,
            severity,
            waived,
            message,
            path: location.path.map(str::to_owned),
            note_id: location.note_id.map(str::to_owned),
            domain: location.domain.map(str::to_owned),
            word_count: None,
        }
    }

    fn already_present(&self, earlier: &Self) -> bool {
        if self.code != earlier.code || self.path != earlier.path {
            return false;
        }
        match (self.word_count, earlier.word_count) {
            (Some(current), Some(previous)) => current <= previous,
            _ => self.message == earlier.message,
        }
    }
}

#[derive(Debug, Default)]
struct Findings(Vec<Finding>);

#[derive(Debug, Serialize)]
pub(crate) struct LintReport {
    notes: usize,
    errors: usize,
    warnings: usize,
    waived: usize,
    findings: Vec<Finding>,
}

impl LintReport {
    pub(crate) fn counts(&self) -> (usize, usize) {
        (self.errors, self.warnings)
    }
    pub(crate) fn has_errors(&self) -> bool {
        self.errors != 0
    }

    /// Return the messages of unwaived errors added since the baseline.
    pub(crate) fn introduced_errors(&self, baseline: Option<&Self>) -> Vec<String> {
        let mut existing: Vec<_> = baseline.into_iter().flat_map(Self::errors).collect();
        let mut introduced = Vec::new();
        for finding in self.errors() {
            if let Some(index) = existing
                .iter()
                .position(|previous| finding.already_present(previous))
            {
                existing.swap_remove(index);
            } else {
                introduced.push(finding.message.clone());
            }
        }
        introduced
    }

    fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error && !finding.waived)
    }

    pub(crate) fn render(&self, out: &mut impl Write) -> Result<()> {
        for finding in self.findings.iter().filter(|finding| !finding.waived) {
            let label = match finding.severity {
                Severity::Warning => "WARN ",
                Severity::Error => "ERROR",
            };
            writeln!(out, "{label} {}", finding.message)?;
        }
        writeln!(
            out,
            "\n{} errors, {} warnings across {} notes",
            self.errors, self.warnings, self.notes,
        )?;
        if self.waived != 0 {
            writeln!(
                out,
                "\nWAIVED ({}) via lint-waivers.md — historical, not counted:",
                self.waived,
            )?;
            for finding in self.findings.iter().filter(|finding| finding.waived) {
                writeln!(out, "  {}", finding.message)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum NoteDates {
    Validate,
    Pending,
}

impl Findings {
    fn push(
        &mut self,
        code: &'static str,
        severity: Severity,
        waived: bool,
        location: Location<'_>,
        message: String,
    ) {
        self.0
            .push(Finding::new(code, severity, waived, location, message));
    }

    fn error(&mut self, code: &'static str, location: Location<'_>, message: String) {
        self.push(code, Severity::Error, false, location, message);
    }

    fn warning(&mut self, code: &'static str, location: Location<'_>, message: String) {
        self.push(code, Severity::Warning, false, location, message);
    }

    fn report(mut self, notes: usize) -> LintReport {
        let mut report = LintReport {
            notes,
            errors: 0,
            warnings: 0,
            waived: 0,
            findings: Vec::new(),
        };
        for finding in &self.0 {
            if finding.waived {
                report.waived += 1;
            } else {
                match finding.severity {
                    Severity::Error => report.errors += 1,
                    Severity::Warning => report.warnings += 1,
                }
            }
        }
        self.0.sort_by_key(|finding| {
            if finding.waived {
                2
            } else if finding.severity == Severity::Error {
                1
            } else {
                0
            }
        });
        report.findings = self.0;
        report
    }

    fn inspect_id(&mut self, rel: &str, nid: &str, note: &Note) {
        if !is_id(nid) {
            self.error(
                "invalid_id",
                Location::note(rel, note),
                format!("{rel}: bad id {}", repr(nid)),
            );
        }
    }

    fn inspect_filename(&mut self, rel: &str, nid: &str, note: &Note) {
        let file = rel.strip_prefix("notes/").unwrap_or(rel);
        let loc = Location::note(rel, note);
        if !file
            .strip_prefix(nid)
            .is_some_and(|rest| rest.starts_with('-'))
        {
            self.error(
                "filename_id",
                loc,
                format!("{rel}: filename must start with {nid}-"),
            );
        }
    }

    fn inspect_note(&mut self, rel: &str, note: &Note, dates: NoteDates) {
        let loc = Location::note(rel, note);
        for field in REQUIRED {
            if note.scalar(field).map_or_else(
                || matches!(note.text(field, "").as_ref(), "" | "[]"),
                str::is_empty,
            ) {
                self.error("missing_field", loc, format!("{rel}: missing {field}"));
            }
        }
        for (field, allowed) in ENUMS {
            if (!note.text(field, "").is_empty() || note.scalar(field).is_some())
                && note
                    .scalar(field)
                    .is_none_or(|value| !allowed.contains(&value))
            {
                let allowed = allowed
                    .iter()
                    .map(|word| repr(word))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.error(
                    "invalid_enum",
                    loc,
                    format!(
                        "{rel}: {field}={} not in [{allowed}]",
                        field_repr(note, field)
                    ),
                );
            }
        }
        if matches!(dates, NoteDates::Validate) {
            for field in ["created", "updated"] {
                let value = note.text(field, "");
                if (!value.is_empty() || note.scalar(field).is_some()) && !iso_date(&value) {
                    self.error(
                        "invalid_date",
                        loc,
                        format!("{rel}: {field} not ISO date: {}", field_repr(note, field)),
                    );
                }
            }
        }
        for field in LINK_FIELDS {
            for link in note.items(field) {
                if !is_id(link) {
                    self.warning(
                        "invalid_link",
                        loc,
                        format!("{rel}: {field} entry {} is not a kb-id", repr(link)),
                    );
                }
            }
        }
        let summary = note.text("summary", "");
        let chars = summary.chars().count();
        if chars > 220 {
            self.warning(
                "summary_length",
                loc,
                format!("{rel}: summary {chars} chars (>220; it is the index line)"),
            );
        }
        if summary.contains("TODO") {
            self.warning(
                "summary_todo",
                loc,
                format!("{rel}: summary still has TODO"),
            );
        }
        let words = prose_and_diagrams(&note.body, |lines| {
            if lines > MERMAID_LINE_LIMIT {
                self.warning(
                    "mermaid_length",
                    loc,
                    format!("{rel}: Mermaid block has {lines} lines (limit {MERMAID_LINE_LIMIT})"),
                );
            }
        });
        if note.scalar("type") != Some("reference") {
            if words > 600 {
                let mut finding = Finding::new(
                    "note_length",
                    Severity::Error,
                    false,
                    loc,
                    format!("{rel}: {words} words (>600; split it)"),
                );
                finding.word_count = Some(words);
                self.0.push(finding);
            } else if words > 450 {
                self.warning(
                    "note_length",
                    loc,
                    format!("{rel}: {words} words (budget is 400)"),
                );
            }
        }
        let related = BTreeSet::from_iter(note.items("related"));
        let body = BTreeSet::from_iter(body_links(&note.body));
        for link in body.difference(&related) {
            self.warning(
                "unlisted_body_link",
                loc,
                format!("{rel}: body links [[{link}]] but it is not in related:"),
            );
        }
        if note.text("source", "").is_empty() {
            self.warning("missing_source", loc, format!("{rel}: empty source:"));
        }
        let created = note.text("created", "");
        let updated = note.text("updated", "");
        if iso_date(&created) && iso_date(&updated) && updated < created {
            self.error(
                "reversed_dates",
                loc,
                format!("{rel}: updated ({updated}) < created ({created})"),
            );
        }
        let verified = note.text("verified", "");
        let has_verified = note.scalar("verified").map_or_else(
            || !matches!(verified.as_ref(), "" | "[]"),
            |value| !value.is_empty(),
        );
        if has_verified && !iso_date(&verified) {
            self.error(
                "invalid_date",
                loc,
                format!(
                    "{rel}: verified not ISO date: {}",
                    field_repr(note, "verified")
                ),
            );
        }
        if note.scalar("status") == Some("verified") && !has_verified {
            self.error(
                "missing_verification_date",
                loc,
                format!("{rel}: status: verified but no verified: date"),
            );
        }
        if note.items("triggers").is_empty() {
            self.warning(
                "missing_triggers",
                loc,
                format!("{rel}: missing triggers: (WHEN should an agent read this?)"),
            );
        }
    }

    fn inspect_link_targets(&mut self, rel: &str, note: &Note, has_id: impl Fn(&str) -> bool) {
        for field in LINK_FIELDS {
            for link in note.items(field) {
                if is_id(link) && !has_id(link) {
                    self.error(
                        "missing_link_target",
                        Location::note(rel, note),
                        format!("{rel}: {field} points at missing {link}"),
                    );
                }
            }
        }
    }

    fn missing_successor(&mut self, location: Location<'_>) {
        self.warning("missing_successor", location, format!("{}: superseded but no note lists it in supersedes: (no successor — index flag stays bare)", location.path.unwrap_or_default()));
    }

    fn check_notes(
        &mut self,
        entries: &[NoteEntry],
        history: &History<'_>,
        waivers: &mut Waivers<'_>,
    ) -> usize {
        let mut ids = BTreeMap::new();
        let mut titles = BTreeMap::new();
        let mut successors = BTreeSet::new();
        let mut superseded = Vec::new();
        let mut failed = 0;
        for entry in entries {
            let Ok(note) = &entry.parsed else { continue };
            let rel = entry.relative.as_str();
            let loc = Location::note(rel, note);
            let id = note.text("id", "?");
            self.inspect_id(rel, &id, note);
            if let Some(earlier) = ids.insert(id.clone(), rel) {
                self.error(
                    "duplicate_id",
                    loc,
                    format!("{rel}: duplicate id {id} (also {earlier})"),
                );
            }
            if let Some(earlier) = titles.insert(note.text("title", ""), rel) {
                self.warning(
                    "duplicate_title",
                    loc,
                    format!("{rel}: duplicate title (also {earlier})"),
                );
            }
            self.inspect_filename(rel, &id, note);
            self.inspect_note(rel, note, NoteDates::Validate);
            successors.extend(note.items("supersedes"));
            let created = note.text("created", "");
            if history.missing_create(&id, &created) {
                self.push(
                    "missing_create",
                    Severity::Error,
                    waivers.applies(&id),
                    loc,
                    format!("{rel}: {}", history::MISSING_CREATE),
                );
            }
            for msg in history.warnings(&id, &created, &note.text("created", "None")) {
                self.push(
                    "created_vs_log",
                    Severity::Warning,
                    waivers.applies(&id),
                    loc,
                    format!("{rel}: {msg}"),
                );
            }
            if note.scalar("status") == Some("superseded") {
                superseded.push((loc, id));
            }
        }
        for entry in entries {
            if let Ok(note) = &entry.parsed {
                self.inspect_link_targets(&entry.relative, note, |link| ids.contains_key(link));
            }
        }
        for (loc, id) in superseded {
            if !successors.contains(id.as_ref()) {
                self.missing_successor(loc);
            }
        }
        for entry in entries {
            if let Err(error) = &entry.parsed {
                failed += 1;
                let message = match error {
                    ParseError::Malformed => "unterminated frontmatter".into(),
                    ParseError::InvalidUtf8(cause) => format!("invalid UTF-8: {cause}"),
                    ParseError::ListDepth => error.to_string(),
                };
                self.error(
                    "note_parse",
                    Location {
                        path: Some(&entry.relative),
                        note_id: entry
                            .relative
                            .strip_prefix("notes/")
                            .and_then(notes::id_prefix),
                        domain: None,
                    },
                    format!("{}: {message} (excluded from index/search)", entry.relative),
                );
            }
        }
        if failed != 0 {
            self.error("note_parse_summary", "notes/".into(), format!("{failed} of {} notes/*.md failed to parse — excluded from index/search (see errors above)", entries.len()));
        }
        ids.len()
    }

    fn freshness(&mut self, report: index::Freshness) {
        if report.db_missing {
            self.error("index_db_missing", ".kb.db".into(), "index/DB stale: FTS db missing, corrupt, or pre-kb_meta schema → run `knowbox reindex`".into());
        }
        if !report.drifted.is_empty() {
            let mut shown = report.drifted[..report.drifted.len().min(5)].join(", ");
            if report.drifted.len() > 5 {
                shown.push_str(&format!(" +{} more", report.drifted.len() - 5));
            }
            self.error(
                "index_db_drift",
                ".kb.db".into(),
                format!(
                    "index/DB stale: {} note(s) drifted ({shown}) → run `knowbox reindex`",
                    report.drifted.len()
                ),
            );
        }
        if !report.orphans.is_empty() {
            self.error(
                "index_orphans",
                "index.md".into(),
                format!(
                    "index.md orphan entries: {} — no note on disk → run `knowbox reindex`",
                    report.orphans.join(", ")
                ),
            );
        }
        if report.index_drift
            && !report.db_missing
            && report.drifted.is_empty()
            && report.orphans.is_empty()
        {
            self.error(
                "index_markdown_drift",
                "index.md".into(),
                "index.md missing or content drift (hand-edited?) → run `knowbox reindex`".into(),
            );
        }
    }
}

const MERMAID_LINE_LIMIT: usize = 15;

fn prose_and_diagrams(body: &str, mut diagram: impl FnMut(usize)) -> usize {
    use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

    let mut words = 0;
    let mut after_code = 0;
    let mut parser = Parser::new(body).into_offset_iter();
    while let Some((event, range)) = parser.next() {
        if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
            let line = body[after_code..range.start]
                .rfind('\n')
                .map_or(after_code, |index| after_code + index + 1);
            let start = if body[line..range.start].chars().all(|ch| {
                ch.is_whitespace()
                    || ch.is_ascii_digit()
                    || matches!(ch, '>' | '-' | '+' | '*' | '.' | ')')
            }) {
                line
            } else {
                range.start
            };
            words += words_in(&body[after_code..start]);
            after_code = range.end;
            let mermaid = info.split_whitespace().next() == Some("mermaid");
            let mut source = String::new();
            for (event, _) in parser.by_ref() {
                match event {
                    Event::End(pulldown_cmark::TagEnd::CodeBlock) => break,
                    Event::Text(text) if mermaid => source.push_str(&text),
                    _ => {}
                }
            }
            if mermaid {
                diagram(source.lines().count());
            }
        }
    }
    words + words_in(&body[after_code..])
}

fn words_in(text: &str) -> usize {
    text.split(|ch: char| ch.is_whitespace() || matches!(ch, '\x1c'..='\x1f'))
        .filter(|word| !word.is_empty())
        .count()
}

#[derive(Serialize)]
pub(crate) struct NoteLength {
    id: String,
    path: String,
    title: String,
    words: usize,
    reference: bool,
}

pub(crate) fn note_lengths(root: &KbRoot, policy: &mut Inventory<'_>) -> Result<Vec<NoteLength>> {
    Ok(notes::snapshot_with(root, policy)?
        .into_iter()
        .filter_map(|entry| {
            let note = entry.parsed.ok()?;
            let id = note.scalar("id").filter(|id| is_id(id))?;
            Some(NoteLength {
                id: id.to_owned(),
                path: entry.relative,
                title: note.text("title", id).into_owned(),
                words: prose_and_diagrams(&note.body, |_| {}),
                reference: note.scalar("type") == Some("reference"),
            })
        })
        .collect())
}

/// Validate the knowledge base without changing its files.
pub(crate) fn lint(
    root: &KbRoot,
    heartbeat: HeartbeatCheck,
    policy: &mut Inventory<'_>,
) -> Result<LintReport> {
    let waiver_text = read_optional(&root.path().join("lint-waivers.md"))?.unwrap_or_default();
    let mut waivers = Waivers::parse(&waiver_text);
    let log = read_optional(&root.path().join("log.md"))?.unwrap_or_default();
    let history = History::parse(&log);
    let notes = notes::snapshot_with(root, policy)?;
    let mut findings = Findings::default();
    let count = findings.check_notes(&notes, &history, &mut waivers);
    findings.freshness(index::freshness(root, &notes, policy)?);
    raw::warnings(root, policy, |code, path, message| {
        findings.warning(code, path.into(), message);
    })?;
    if let HeartbeatCheck::Enabled(now) = heartbeat {
        findings.check_heartbeats(root, now);
    }
    match git::untracked_toplevel(root) {
        Ok(entries) => {
            for name in entries {
                findings.error("untracked_entry", name.as_str().into(), format!(
                    "untracked top-level entry {} — not in TRACKED_PATHS, so it never backs up; add it to git (TRACKED_PATHS) or .gitignore", repr(&name)
                ));
            }
        }
        Err(error) => {
            let error = anyhow::Error::from(error);
            let detail = format!("{error:#}").replace(['\r', '\n'], " ");
            findings.warning(
                "git_inventory_unavailable",
                Location::default(),
                format!("could not check untracked top-level entries: {detail}"),
            );
        }
    }
    let mut waiver_warnings = Vec::new();
    waivers.append_unused_warnings(&mut waiver_warnings);
    for message in waiver_warnings {
        findings.warning("waiver", "lint-waivers.md".into(), message);
    }
    for (path, message) in policy.take_warnings() {
        let relative = path.strip_prefix(root.path()).unwrap_or(&path);
        let name = relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        findings.warning("inventory_skipped", name.as_str().into(), message);
    }
    Ok(findings.report(count))
}
