mod history;
mod raw;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::KbRoot;
use crate::notes::{self, Note, NoteEntry, ParseError, body_links, is_id};
use crate::store::regular_file;
use crate::{git, index};

use history::{History, Waivers};
pub(crate) use raw::{Mark, raw_backlog, raw_mark};

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
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, byte)| {
            if i == 4 || i == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
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

#[derive(Debug, Default)]
struct Findings {
    errors: Vec<String>,
    warnings: Vec<String>,
    waived: Vec<String>,
}

impl Findings {
    fn output(self, count: usize, out: &mut impl Write) -> Result<bool> {
        for warning in &self.warnings {
            writeln!(out, "WARN  {warning}")?;
        }
        for error in &self.errors {
            writeln!(out, "ERROR {error}")?;
        }
        writeln!(
            out,
            "\n{} errors, {} warnings across {count} notes",
            self.errors.len(),
            self.warnings.len()
        )?;
        if !self.waived.is_empty() {
            writeln!(
                out,
                "\nWAIVED ({}) via lint-waivers.md — historical, not counted:",
                self.waived.len()
            )?;
            for waived in self.waived {
                writeln!(out, "  {waived}")?;
            }
        }
        Ok(!self.errors.is_empty())
    }

    fn inspect_note(&mut self, rel: &str, nid: &str, note: &Note) {
        let file = rel.strip_prefix("notes/").unwrap_or(rel);
        if !file
            .strip_prefix(nid)
            .is_some_and(|rest| rest.starts_with('-'))
        {
            self.errors
                .push(format!("{rel}: filename must start with {nid}-"));
        }
        for field in REQUIRED {
            if note.scalar(field).map_or_else(
                || matches!(note.text(field, "").as_ref(), "" | "[]"),
                str::is_empty,
            ) {
                self.errors.push(format!("{rel}: missing {field}"));
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
                self.errors.push(format!(
                    "{rel}: {field}={} not in [{allowed}]",
                    field_repr(note, field)
                ));
            }
        }
        for field in ["created", "updated"] {
            let value = note.text(field, "");
            if (!value.is_empty() || note.scalar(field).is_some()) && !iso_date(&value) {
                self.errors.push(format!(
                    "{rel}: {field} not ISO date: {}",
                    field_repr(note, field)
                ));
            }
        }
        for field in LINK_FIELDS {
            for link in note.items(field) {
                if !is_id(link) {
                    self.warnings.push(format!(
                        "{rel}: {field} entry {} is not a kb-id",
                        repr(link)
                    ));
                }
            }
        }
        let summary = note.text("summary", "");
        let chars = summary.chars().count();
        if chars > 220 {
            self.warnings.push(format!(
                "{rel}: summary {chars} chars (>220; it is the index line)"
            ));
        }
        if summary.contains("TODO") {
            self.warnings.push(format!("{rel}: summary still has TODO"));
        }
        let words = note
            .body
            .split(|ch: char| ch.is_whitespace() || matches!(ch, '\x1c'..='\x1f'))
            .filter(|word| !word.is_empty())
            .count();
        if note.scalar("type") != Some("reference") {
            if words > 600 {
                self.errors
                    .push(format!("{rel}: {words} words (>600; split it)"));
            } else if words > 450 {
                self.warnings
                    .push(format!("{rel}: {words} words (budget is 400)"));
            }
        }
        let related = BTreeSet::from_iter(note.items("related"));
        let body = BTreeSet::from_iter(body_links(&note.body));
        for link in body.difference(&related) {
            self.warnings.push(format!(
                "{rel}: body links [[{link}]] but it is not in related:"
            ));
        }
        if note.text("source", "").is_empty() {
            self.warnings.push(format!("{rel}: empty source:"));
        }
        let created = note.text("created", "");
        let updated = note.text("updated", "");
        if iso_date(&created) && iso_date(&updated) && updated < created {
            self.errors
                .push(format!("{rel}: updated ({updated}) < created ({created})"));
        }
        let verified = note.text("verified", "");
        let has_verified = note.scalar("verified").map_or_else(
            || !matches!(verified.as_ref(), "" | "[]"),
            |value| !value.is_empty(),
        );
        if has_verified && !iso_date(&verified) {
            self.errors.push(format!(
                "{rel}: verified not ISO date: {}",
                field_repr(note, "verified")
            ));
        }
        if note.scalar("status") == Some("verified") && !has_verified {
            self.errors
                .push(format!("{rel}: status: verified but no verified: date"));
        }
        if note.items("triggers").is_empty() {
            self.warnings.push(format!(
                "{rel}: missing triggers: (WHEN should an agent read this?)"
            ));
        }
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
            let id = note.text("id", "?");
            if !is_id(&id) {
                self.errors.push(format!("{rel}: bad id {}", repr(&id)));
            }
            if let Some(earlier) = ids.insert(id.clone(), rel) {
                self.errors
                    .push(format!("{rel}: duplicate id {id} (also {earlier})"));
            }
            if let Some(earlier) = titles.insert(note.text("title", ""), rel) {
                self.warnings
                    .push(format!("{rel}: duplicate title (also {earlier})"));
            }
            self.inspect_note(rel, &id, note);
            successors.extend(note.items("supersedes"));
            for msg in history.warnings(
                &id,
                &note.text("created", ""),
                &note.text("created", "None"),
            ) {
                let bucket = if waivers.applies(&id) {
                    &mut self.waived
                } else {
                    &mut self.warnings
                };
                bucket.push(format!("{rel}: {msg}"));
            }
            if note.scalar("status") == Some("superseded") {
                superseded.push((rel, id));
            }
        }
        for entry in entries {
            if let Ok(note) = &entry.parsed {
                for field in LINK_FIELDS {
                    for link in note.items(field) {
                        if is_id(link) && !ids.contains_key(link) {
                            self.errors.push(format!(
                                "{}: {field} points at missing {link}",
                                entry.relative
                            ));
                        }
                    }
                }
            }
        }
        for (rel, id) in superseded {
            if !successors.contains(id.as_ref()) {
                self.warnings.push(format!("{rel}: superseded but no note lists it in supersedes: (no successor — index flag stays bare)"));
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
                self.errors.push(format!(
                    "{}: {message} (excluded from index/search)",
                    entry.relative
                ));
            }
        }
        if failed != 0 {
            self.errors.push(format!("{failed} of {} notes/*.md failed to parse — excluded from index/search (see errors above)", entries.len()));
        }
        ids.len()
    }

    fn freshness(&mut self, report: index::Freshness) {
        if report.db_missing {
            self.errors.push("index/DB stale: FTS db missing, corrupt, or pre-kb_meta schema → run `knowbox reindex`".into());
        }
        if !report.drifted.is_empty() {
            let mut shown = report.drifted[..report.drifted.len().min(5)].join(", ");
            if report.drifted.len() > 5 {
                shown.push_str(&format!(" +{} more", report.drifted.len() - 5));
            }
            self.errors.push(format!(
                "index/DB stale: {} note(s) drifted ({shown}) → run `knowbox reindex`",
                report.drifted.len()
            ));
        }
        if !report.orphans.is_empty() {
            self.errors.push(format!(
                "index.md orphan entries: {} — no note on disk → run `knowbox reindex`",
                report.orphans.join(", ")
            ));
        }
        if report.index_drift
            && !report.db_missing
            && report.drifted.is_empty()
            && report.orphans.is_empty()
        {
            self.errors.push(
                "index.md missing or content drift (hand-edited?) → run `knowbox reindex`".into(),
            );
        }
    }
}

/// Validate the knowledge base without changing its files; returns true on lint errors.
pub(crate) fn lint(root: &KbRoot, out: &mut impl Write) -> Result<bool> {
    let waiver_text = read_optional(&root.path().join("lint-waivers.md"))?.unwrap_or_default();
    let mut waivers = Waivers::parse(&waiver_text);
    let log = read_optional(&root.path().join("log.md"))?.unwrap_or_default();
    let history = History::parse(&log);
    let notes = notes::snapshot(root)?;
    let mut findings = Findings::default();
    let count = findings.check_notes(&notes, &history, &mut waivers);
    findings.freshness(index::freshness(root, &notes)?);
    raw::warnings(&raw::inventory(root)?, &mut findings.warnings);
    for name in git::untracked_toplevel(root)? {
        findings.errors.push(format!(
            "untracked top-level entry {} — not in TRACKED_PATHS, so it never backs up; add it to git (TRACKED_PATHS) or .gitignore", repr(&name)
        ));
    }
    waivers.append_unused_warnings(&mut findings.warnings);
    findings.output(count, out)
}
