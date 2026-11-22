mod review;

use std::io::{Read, Write};
use std::path::{Component, Path};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::KbRoot;
use crate::notes::{is_id, sha1_hex};
use crate::store::{self, EntryKind, Inventory};

use super::{is_line_break, iso_date, split_line};
pub(crate) use review::{
    Inbox, PreparedRaw, ReviewError, Verdict, VerdictDecision, prepare_verdict, raw_mark,
};

const MARK_USAGE: &str = "knowbox raw-mark <file> <N> --note kb-XXXX | --rejected \"reason\"";
const DISTILL_WARN_MIN: usize = 5;

#[derive(Debug)]
struct Entry<'a> {
    title: &'a str,
    marker: Option<Marker<'a>>,
    insertion: usize,
    end: usize,
    ending: &'a str,
}

#[derive(Clone, Copy, Debug)]
struct Marker<'a> {
    line: &'a str,
    start: usize,
    end: usize,
}

impl Entry<'_> {
    fn revision(&self, text: &str) -> String {
        // Ignore the review marker so the original selection can identify its completed ingest.
        let (before, after) = self
            .marker
            .map_or(("", &text[self.insertion..self.end]), |marker| {
                (
                    &text[self.insertion..marker.start],
                    &text[marker.end..self.end],
                )
            });
        sha1_hex(format!("{}:{}{before}{after}", self.title.len(), self.title).as_bytes())
    }
}

#[derive(Debug)]
struct Pair<'a> {
    title: &'a str,
    line: usize,
    offset: usize,
    end: usize,
}

#[derive(Debug, Default)]
struct Scan<'a> {
    entries: Vec<Entry<'a>>,
    pairs: Vec<Pair<'a>>,
    orphans: usize,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum RawEvidence {
    Ingested {
        file: String,
        number: usize,
        id: String,
        marker: String,
        date: Option<String>,
    },
    Verdict {
        file: String,
        line: usize,
        ids: [String; 2],
        verdict: String,
        rationale: String,
        date: Option<String>,
    },
}

fn is_state(line: &str) -> bool {
    ["> state: ingested", "> state: rejected"]
        .iter()
        .any(|marker| {
            line.strip_prefix(marker).is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|ch| ch != '_' && !ch.is_alphanumeric())
            })
        })
}

fn scan(text: &str) -> Scan<'_> {
    let mut result = Scan::default();
    // Ignore a leading BOM only for recognition; insertion offsets still include it.
    let mut rest = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut current: Option<usize> = None;
    let mut pair: Option<usize> = None;
    let mut line_number = 0;
    let mut first = false;
    while !rest.is_empty() {
        line_number += 1;
        let start = text.len() - rest.len();
        let (line, ending, next) = split_line(rest);
        let offset = text.len() - next.len();
        rest = next;
        if let Some(title) = line
            .strip_prefix("## ")
            .filter(|title| !title.starts_with('#'))
        {
            if let Some(index) = current.take() {
                result.entries[index].end = start;
            }
            if let Some(index) = pair.take() {
                result.pairs[index].end = start;
            }
            first = true;
            let title = title.trim();
            if title
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("pair:"))
            {
                pair = Some(result.pairs.len());
                result.pairs.push(Pair {
                    title,
                    line: line_number,
                    offset,
                    end: text.len(),
                });
            } else {
                current = Some(result.entries.len());
                result.entries.push(Entry {
                    title,
                    marker: None,
                    insertion: offset,
                    end: text.len(),
                    ending,
                });
            }
        } else {
            let legacy = line.starts_with("> ingested:") || line.starts_with("> rejected:");
            if legacy || (first && is_state(line)) {
                if let Some(index) = current {
                    result.entries[index].marker.get_or_insert(Marker {
                        line,
                        start,
                        end: offset,
                    });
                } else if pair.is_none() && legacy {
                    result.orphans += 1;
                }
            }
            first = false;
        }
    }
    result
}

fn updates(body: &str) -> Option<String> {
    body.split(is_line_break).find_map(|line| {
        let line = line.trim();
        let field = line
            .strip_prefix("> ")
            .unwrap_or(line)
            .strip_prefix("updates:")?;
        let id = field.split_whitespace().next()?;
        is_id(id).then(|| id.to_owned())
    })
}

fn date(value: &str) -> Option<String> {
    iso_date(value).then(|| value.to_owned())
}

fn verdict(file: &str, pair: Pair<'_>, text: &str) -> Option<RawEvidence> {
    let date_or_label = file.strip_prefix("conflicts-")?.strip_suffix(".md")?;
    if date_or_label.is_empty() {
        return None;
    }
    let mut ids = pair.title.get(5..)?.split_whitespace();
    let first = ids.next().filter(|id| is_id(id))?;
    let second = ids.next().filter(|id| is_id(id) && *id != first)?;
    let ids = [first.to_owned(), second.to_owned()];
    let mut lines = text[pair.offset..pair.end].split_inclusive(is_line_break);
    let mut value = lines.find_map(|line| line.strip_prefix("> verdict:").map(str::trim))?;
    let file_date = date(date_or_label);
    let verdict = value.split_whitespace().next()?.trim_end_matches(':');
    let kind = <Verdict as clap::ValueEnum>::from_str(verdict, false).ok()?;
    value = &value[verdict.len()..];
    value = value.strip_prefix(':').unwrap_or(value).trim_start();
    let rationale = format!("{value}\n{}", lines.collect::<String>())
        .trim()
        .to_owned();
    Some(RawEvidence::Verdict {
        file: file.into(),
        line: pair.line,
        ids,
        verdict: kind.as_str().into(),
        rationale,
        date: file_date,
    })
}

fn visit(
    root: &KbRoot,
    policy: &mut Inventory<'_>,
    include: impl Fn(&str) -> bool,
    mut inspect: impl FnMut(&str, &str, Scan<'_>),
) -> Result<()> {
    let raw = root.path().join("raw");
    let inspecting = || format!("inspecting {}", raw.display());
    if !policy.directory(&raw).with_context(inspecting)? {
        return Ok(());
    }
    for name in policy
        .names(&raw, EntryKind::MarkdownFile)
        .with_context(inspecting)?
    {
        if !include(&name) {
            continue;
        }
        let path = raw.join(&name);
        if !policy.file(&path)? {
            continue;
        }
        let text = read_raw(&path)?;
        inspect(&name, &text, scan(&text));
    }
    Ok(())
}

fn read_raw(path: &Path) -> Result<String> {
    let mut text = String::new();
    store::open_read(path)?
        .read_to_string(&mut text)
        .with_context(|| format!("reading {} as UTF-8", path.display()))?;
    Ok(text)
}

pub(super) fn warnings(
    root: &KbRoot,
    policy: &mut Inventory<'_>,
    mut warn: impl FnMut(&'static str, &str, String),
) -> Result<()> {
    let mut backlog = Vec::new();
    visit(
        root,
        policy,
        |_| true,
        |name, _, scan| {
            let path = format!("raw/{name}");
            if scan.orphans != 0 {
                let plural = if scan.orphans == 1 { "" } else { "s" };
                warn(
                    "raw_orphan_marker",
                    &path,
                    format!(
                        "{path}: {} orphan legacy marker{plural} in the preamble (above the first `## ` heading) — they resolve no entry but skew the janitor's backlog count; attach each to its entry's block",
                        scan.orphans
                    ),
                );
            }
            let open = scan
                .entries
                .iter()
                .filter(|entry| entry.marker.is_none())
                .count();
            if open == 0 {
                return;
            }
            if name.starts_with("session-distill-") {
                if open > DISTILL_WARN_MIN {
                    let message = format!(
                        "{path}: distill backlog: {open} unreviewed proposals (> {DISTILL_WARN_MIN}) — promote or reject via the knowbox skill + `knowbox raw-mark`",
                    );
                    backlog.push(("raw_distill_backlog", path, message));
                }
            } else {
                let noun = if open == 1 { "entry" } else { "entries" };
                let message = format!(
                    "{path}: {open} unreviewed {noun} — ingest via the knowbox skill, then `{MARK_USAGE}`",
                );
                backlog.push(("raw_backlog", path, message));
            }
        },
    )?;
    for (code, path, message) in backlog {
        warn(code, &path, message);
    }
    Ok(())
}

/// Report all unreviewed entries, without changing the staging files.
pub(crate) fn raw_backlog(root: &KbRoot, policy: &mut Inventory<'_>) -> Result<RawBacklog> {
    let mut total = 0;
    let mut entries = Vec::new();
    visit(
        root,
        policy,
        |_| true,
        |name, text, scan| {
            total += scan.entries.len();
            for (index, entry) in scan.entries.into_iter().enumerate() {
                if entry.marker.is_some() {
                    continue;
                }
                let body = &text[entry.insertion..entry.end];
                entries.push(OpenProposal {
                    file: name.to_owned(),
                    number: index + 1,
                    title: entry.title.to_owned(),
                    body: body.to_owned(),
                    updates: updates(body),
                    revision: entry.revision(text),
                });
            }
        },
    )?;
    Ok(RawBacklog {
        total,
        unreviewed: entries.len(),
        entries,
    })
}

#[derive(Debug, Serialize)]
struct OpenProposal {
    file: String,
    number: usize,
    title: String,
    body: String,
    updates: Option<String>,
    revision: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RawBacklog {
    total: usize,
    unreviewed: usize,
    entries: Vec<OpenProposal>,
}

impl RawBacklog {
    pub(crate) fn unreviewed(&self) -> usize {
        self.unreviewed
    }

    pub(crate) fn render(&self, out: &mut impl Write) -> Result<()> {
        writeln!(
            out,
            "raw backlog: {} unreviewed of {} entries",
            self.unreviewed, self.total
        )?;
        for entry in &self.entries {
            writeln!(out, "  {} #{}: {}", entry.file, entry.number, entry.title)?;
        }
        if self.unreviewed != 0 {
            writeln!(out, "\nmark after ingest: {MARK_USAGE}")?;
        }
        Ok(())
    }
}

pub(crate) fn raw_evidence(root: &KbRoot, policy: &mut Inventory<'_>) -> Result<Vec<RawEvidence>> {
    let mut evidence = Vec::new();
    visit(
        root,
        policy,
        |_| true,
        |name, text, scan| {
            for (index, proposal) in scan.entries.into_iter().enumerate() {
                let Some(marker) = proposal.marker else {
                    continue;
                };
                let Some(details) = ingest_details(marker.line) else {
                    continue;
                };
                let Some(id) = details.split_whitespace().next().filter(|id| is_id(id)) else {
                    continue;
                };
                let date = details
                    .trim_end()
                    .strip_suffix(')')
                    .and_then(|text| text.rsplit_once('('))
                    .and_then(|(_, value)| date(value));
                evidence.push(RawEvidence::Ingested {
                    file: name.to_owned(),
                    number: index + 1,
                    id: id.to_owned(),
                    marker: marker.line.to_owned(),
                    date,
                });
            }
            evidence.extend(
                scan.pairs
                    .into_iter()
                    .filter_map(|pair| verdict(name, pair, text)),
            );
        },
    )?;
    Ok(evidence)
}

fn ingest_details(marker: &str) -> Option<&str> {
    marker
        .strip_prefix("> state: ingested")
        .or_else(|| marker.strip_prefix("> ingested:"))
}

/// Review action for a raw entry.
#[derive(Debug)]
pub(crate) enum Mark<'a> {
    Ingested(&'a str),
    Rejected(&'a str),
}

fn valid_basename(file: &str) -> bool {
    !file.contains(['/', '\\', ':', '\0'])
        && !file.contains(is_line_break)
        && file.ends_with(".md")
        && matches!(Path::new(file).components().next(), Some(Component::Normal(name)) if name == file)
}
