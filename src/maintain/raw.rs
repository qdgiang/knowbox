use std::fs;
use std::io::Write;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail, ensure};
use jiff::civil::Date;

use crate::changes::Event;
use crate::config::KbRoot;
use crate::notes::is_id;
use crate::store::{WriteLock, atomic_write, directory};

use super::{is_line_break, read_optional, split_line};

const MARK_USAGE: &str = "knowbox raw-mark <file> <N> --note kb-XXXX | --rejected \"reason\"";
const DISTILL_WARN_MIN: usize = 5;

#[derive(Debug)]
struct Entry<'a> {
    title: &'a str,
    resolved: bool,
    insertion: usize,
    ending: &'a str,
}

#[derive(Debug, Default)]
struct Scan<'a> {
    entries: Vec<Entry<'a>>,
    orphans: usize,
}

#[derive(Debug)]
pub(super) struct RawFile {
    name: String,
    total: usize,
    orphans: usize,
    open: Vec<(usize, String)>,
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
    let mut seen_heading = false;
    let mut current: Option<usize> = None;
    let mut first = false;
    while !rest.is_empty() {
        let (line, ending, next) = split_line(rest);
        let offset = text.len() - next.len();
        rest = next;
        if let Some(title) = line
            .strip_prefix("## ")
            .filter(|title| !title.starts_with('#'))
        {
            seen_heading = true;
            first = true;
            let title = title.trim();
            if title
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("pair:"))
            {
                current = None;
            } else {
                current = Some(result.entries.len());
                result.entries.push(Entry {
                    title,
                    resolved: false,
                    insertion: offset,
                    ending,
                });
            }
        } else {
            let legacy = line.starts_with("> ingested:") || line.starts_with("> rejected:");
            if let Some(index) = current {
                result.entries[index].resolved |= legacy || (first && is_state(line));
            } else if !seen_heading && legacy {
                result.orphans += 1;
            }
            first = false;
        }
    }
    result
}

pub(super) fn inventory(root: &KbRoot) -> Result<Vec<RawFile>> {
    let raw = root.path().join("raw");
    let inspecting = || format!("inspecting {}", raw.display());
    if !directory(&raw).with_context(inspecting)? {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for item in fs::read_dir(&raw).with_context(inspecting)? {
        let item = item.with_context(inspecting)?;
        let name = item.file_name();
        if name.as_encoded_bytes().ends_with(b".md") {
            let name = name
                .into_string()
                .map_err(|name| anyhow::anyhow!("raw filename is not UTF-8: {name:?}"))?;
            let path = item.path();
            let text = read_optional(&path)?
                .with_context(|| format!("raw file vanished: {}", path.display()))?;
            let scan = scan(&text);
            files.push(RawFile {
                name,
                total: scan.entries.len(),
                orphans: scan.orphans,
                open: scan
                    .entries
                    .into_iter()
                    .enumerate()
                    .filter(|(_, entry)| !entry.resolved)
                    .map(|(i, entry)| (i + 1, entry.title.to_owned()))
                    .collect(),
            });
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

pub(super) fn warnings(files: &[RawFile], warnings: &mut Vec<String>) {
    for file in files.iter().filter(|file| file.orphans != 0) {
        let plural = if file.orphans == 1 { "" } else { "s" };
        warnings.push(format!(
            "raw/{}: {} orphan legacy marker{plural} in the preamble (above the first `## ` heading) — they resolve no entry but skew the janitor's backlog count; attach each to its entry's block",
            file.name, file.orphans
        ));
    }
    for file in files.iter().filter(|file| !file.open.is_empty()) {
        let open = file.open.len();
        if file.name.starts_with("session-distill-") {
            if open > DISTILL_WARN_MIN {
                warnings.push(format!(
                    "raw/{}: distill backlog: {open} unreviewed proposals (> {DISTILL_WARN_MIN}) — promote or reject via the knowbox skill + `knowbox raw-mark`",
                    file.name
                ));
            }
        } else {
            let noun = if open == 1 { "entry" } else { "entries" };
            warnings.push(format!(
                "raw/{}: {open} unreviewed {noun} — ingest via the knowbox skill, then `{MARK_USAGE}`",
                file.name
            ));
        }
    }
}

/// Report all unreviewed entries, without changing the staging files.
pub(crate) fn raw_backlog(root: &KbRoot, out: &mut impl Write) -> Result<()> {
    let files = inventory(root)?;
    let total: usize = files.iter().map(|file| file.total).sum();
    let open: usize = files.iter().map(|file| file.open.len()).sum();
    writeln!(out, "raw backlog: {open} unreviewed of {total} entries")?;
    for file in files {
        for (number, title) in file.open {
            writeln!(out, "  {} #{number}: {title}", file.name)?;
        }
    }
    if open != 0 {
        writeln!(out, "\nmark after ingest: {MARK_USAGE}")?;
    }
    Ok(())
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

/// Mark an entry in the current locked raw file, preserving all surrounding bytes.
pub(crate) fn raw_mark(
    lock: &WriteLock,
    file: &str,
    number: usize,
    mark: Mark<'_>,
    today: Date,
) -> Result<Event> {
    ensure!(
        valid_basename(file),
        "raw-mark: bad raw filename {file:?} (basename of a .md file inside raw/)"
    );
    let marker = match mark {
        Mark::Ingested(note) => {
            ensure!(
                is_id(note),
                "raw-mark: --note must be a kb-id, got {note:?}"
            );
            format!("state: ingested {note} ({today})")
        }
        Mark::Rejected(reason) => {
            ensure!(
                !reason.trim().is_empty()
                    && !reason.contains(is_line_break)
                    && !reason.contains('\0'),
                "raw-mark: rejection reason must be nonempty and on one line"
            );
            format!("state: rejected {reason} ({today})")
        }
    };
    let raw = lock.root().path().join("raw");
    if !directory(&raw).with_context(|| format!("inspecting {}", raw.display()))? {
        bail!("not found: raw/{file}");
    }
    let path = raw.join(file);
    let text = read_optional(&path)?.with_context(|| format!("not found: raw/{file}"))?;
    let scan = scan(&text);
    let entry = number
        .checked_sub(1)
        .and_then(|i| scan.entries.get(i))
        .with_context(|| {
            format!(
                "raw-mark: {file} has {} entries, no #{number}",
                scan.entries.len()
            )
        })?;
    ensure!(
        !entry.resolved,
        "raw-mark: {file} #{number} ({:?}) is already marked",
        entry.title
    );
    let (before, after) = text.split_at(entry.insertion);
    let joined = if entry.ending.is_empty() {
        // At an unterminated heading, add a separator but leave the new EOF unterminated.
        let ending = before
            .rmatch_indices(is_line_break)
            .next()
            .map_or("\n", |(i, e)| {
                if e == "\n" && before[..i].ends_with('\r') {
                    "\r\n"
                } else {
                    e
                }
            });
        format!("{before}{ending}> {marker}")
    } else {
        format!("{before}> {marker}{}{after}", entry.ending)
    };
    atomic_write(&path, joined.as_bytes())?;
    Ok(Event {
        subject: format!("knowbox: raw-mark {file} #{number}"),
        stdout: format!("marked {file} #{number}: {} -> {marker}", entry.title),
    })
}
