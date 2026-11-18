mod new;
mod rewrite;
mod supersede;
#[cfg(test)]
mod tests;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use jiff::civil::Date;

use crate::config::KbRoot;
use crate::notes::{self, LookupError};
use crate::store::{self, WriteLock};

pub(crate) use new::{NewArgs, create, prepare_new};
pub(crate) use supersede::supersede;

#[derive(Debug)]
pub(crate) struct Draft {
    args: NewArgs,
    body: String,
}

#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) subject: String,
    pub(crate) stdout: String,
}

fn single_line(field: &str, value: &str) -> Result<()> {
    ensure!(
        !value.contains(['\r', '\n', '\0']),
        "{field} must be a single line without NUL"
    );
    Ok(())
}

fn read_log(root: &KbRoot) -> Result<String> {
    let path = root.path().join("log.md");
    if !store::regular_file(&path)
        .with_context(|| format!("checking journal {}", path.display()))?
    {
        return Ok(String::new());
    }
    fs::read_to_string(&path).with_context(|| format!("reading journal {}", path.display()))
}

pub(crate) fn append_entry(
    lock: &WriteLock,
    today: Date,
    action: &str,
    id: &str,
    detail: &str,
) -> Result<()> {
    for (name, value) in [("action", action), ("id", id), ("detail", detail)] {
        single_line(name, value)?;
    }
    ensure!(
        !action.contains('|') && !id.contains('|'),
        "journal action and id cannot contain a pipe"
    );
    let mut bytes = read_log(lock.root())?.into_bytes();
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    writeln!(bytes, "## [{today}] {action} | {id} | {detail}")?;
    store::atomic_write(&lock.root().path().join("log.md"), &bytes)?;
    Ok(())
}

// Any-width filename prefix, including tombstones with a non-Markdown suffix.
fn id_number(bytes: &[u8]) -> Option<u32> {
    let rest = bytes.strip_prefix(b"kb-")?;
    let digits = rest.iter().take_while(|c| c.is_ascii_digit());
    let mut total: Option<u32> = None;
    for &digit in digits {
        total = Some(
            total
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(u32::from(digit - b'0')),
        );
    }
    total
}

pub(crate) fn next_id(root: &KbRoot) -> Result<String> {
    let dir = root.path().join("notes");
    let mut highest = 0;
    if store::directory(&dir).with_context(|| format!("checking {}", dir.display()))? {
        for entry in fs::read_dir(&dir).with_context(|| format!("listing {}", dir.display()))? {
            let name = entry
                .with_context(|| format!("reading directory {}", dir.display()))?
                .file_name();
            if let Some(n) = id_number(name.as_encoded_bytes()) {
                highest = highest.max(n);
            }
        }
    }
    for line in notes::normalize_newlines(&read_log(root)?).lines() {
        if let Some((date, rest)) = line.strip_prefix("## [").and_then(|s| s.split_once(']'))
            && !date.is_empty()
            && let Some(number) = rest
                .strip_prefix(" create | ")
                .and_then(|s| id_number(s.as_bytes()))
        {
            highest = highest.max(number);
        }
    }
    ensure!(
        highest < 9999,
        "note ID space exhausted (maximum kb-9999); refusing to reuse an ID"
    );
    Ok(format!("kb-{:04}", highest + 1))
}

fn locate(root: &KbRoot, selector: &str) -> Result<PathBuf> {
    notes::find(root, selector)?.with_context(|| format!("not found: {selector}"))
}

fn read_note(path: &Path) -> Result<Vec<u8>> {
    ensure!(
        store::regular_file(path).with_context(|| format!("checking {}", path.display()))?,
        "missing note {}",
        path.display()
    );
    fs::read(path).with_context(|| format!("reading {}", path.display()))
}

fn actual_id(note: &notes::Note) -> Result<&str> {
    note.scalar("id")
        .filter(|s| notes::is_id(s))
        .context("note must have a four-digit kb-NNNN id")
}

pub(crate) fn touch(lock: &WriteLock, selector: &str, today: Date) -> Result<PathBuf> {
    let path = locate(lock.root(), selector)?;
    let bytes = read_note(&path)?;
    let parsed = notes::parse(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    let id = actual_id(&parsed).with_context(|| format!("checking {}", path.display()))?;
    let updated = rewrite::edit(&bytes, &[("updated", &today.to_string())], None, None)
        .with_context(|| format!("updating {}", path.display()))?;
    read_log(lock.root())?;
    store::atomic_write(&path, &updated)?;
    append_entry(lock, today, "update", id, "touched")
        .with_context(|| format!("{} was touched but not journaled", path.display()))?;
    Ok(path)
}

pub(crate) fn verify(
    lock: &WriteLock,
    ids: &[String],
    evidence: &str,
    today: Date,
    out: &mut impl Write,
) -> Result<Option<Event>> {
    single_line("evidence", evidence)?;
    for selector in ids {
        single_line("selector", selector)?;
    }
    read_log(lock.root())?;
    let mut touched = Vec::new();
    if let Err(error) = verify_batch(lock, ids, evidence, today, out, &mut touched) {
        if touched.is_empty() {
            return Err(error);
        }
        return Err(error).context(format!(
            "verification interrupted; earlier verified and journaled notes remain on disk: {}",
            touched.join(", ")
        ));
    }
    if touched.is_empty() {
        return Ok(None);
    }
    let names = touched.join(", ");
    Ok(Some(Event {
        subject: format!("knowbox: verify {names} — {evidence}"),
        stdout: format!("verified: {names}"),
    }))
}

fn verify_batch(
    lock: &WriteLock,
    ids: &[String],
    evidence: &str,
    today: Date,
    out: &mut impl Write,
    touched: &mut Vec<String>,
) -> Result<()> {
    for selector in ids {
        let path = match notes::find(lock.root(), selector) {
            Ok(Some(path)) => path,
            Ok(None) => {
                writeln!(out, "not found: {selector}")?;
                continue;
            }
            Err(error @ LookupError::Ambiguous { .. }) => {
                writeln!(out, "{error}")?;
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let bytes = read_note(&path)?;
        let parsed = match notes::parse(&bytes) {
            Ok(note) => note,
            Err(error) => {
                writeln!(out, "skipping {selector}: {error}")?;
                continue;
            }
        };
        if parsed.scalar("status") == Some("superseded") {
            writeln!(
                out,
                "skipping {selector}: superseded notes are not verified"
            )?;
            continue;
        }
        if parsed.scalar("updated").is_none() || parsed.scalar("status").is_none() {
            writeln!(
                out,
                "skipping {selector}: frontmatter missing status:/updated: — fix the note first"
            )?;
            continue;
        }
        let id = actual_id(&parsed).with_context(|| format!("checking {}", path.display()))?;
        if touched.iter().any(|s| s == id) {
            continue;
        }
        let date = today.to_string();
        let updated = rewrite::edit(
            &bytes,
            &[
                ("updated", &date),
                ("status", "verified"),
                ("verified", &date),
            ],
            Some(("verified", "updated")),
            None,
        )
        .with_context(|| format!("verifying {}", path.display()))?;
        store::atomic_write(&path, &updated)?;
        append_entry(lock, today, "verify", id, evidence)
            .with_context(|| format!("{} was verified but not journaled", path.display()))?;
        touched.push(id.to_owned());
    }
    Ok(())
}
