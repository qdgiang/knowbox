use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::changes::transaction::{self, FileWrite};
use crate::changes::{self, Event};
use crate::journal::Stamp;
use crate::store::WriteLock;

use super::*;

#[derive(Debug, Error)]
#[error("{0}")]
pub(crate) struct ReviewError(pub(crate) String);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inbox {
    pub(crate) file: String,
    pub(crate) number: usize,
    pub(crate) revision: String,
}

impl Inbox {
    pub(crate) fn prepare(
        &self,
        lock: &WriteLock,
        mark: Mark<'_>,
        stamp: &Stamp<'_>,
    ) -> Result<PreparedRaw> {
        prepare_mark(
            lock,
            &self.file,
            self.number,
            Some(&self.revision),
            mark,
            stamp,
        )
    }
}

pub(crate) struct PreparedRaw {
    path: PathBuf,
    before: Option<String>,
    after: String,
    pub(crate) action: &'static str,
    pub(crate) id: String,
    pub(crate) detail: String,
    output: String,
}

impl PreparedRaw {
    pub(crate) fn file_write(&self) -> FileWrite<'_> {
        FileWrite {
            path: &self.path,
            before: self.before.as_ref().map(|text| text.as_bytes()),
            after: self.after.as_bytes(),
        }
    }
}

fn valid_reason(reason: &str) -> bool {
    !reason.trim().is_empty() && !reason.contains(is_line_break) && !reason.contains('\0')
}

fn prepare_mark(
    lock: &WriteLock,
    file: &str,
    number: usize,
    revision: Option<&str>,
    mark: Mark<'_>,
    stamp: &Stamp<'_>,
) -> Result<PreparedRaw> {
    ensure!(
        valid_basename(file),
        "raw-mark: bad raw filename {file:?} (basename of a .md file inside raw/)"
    );
    let today = stamp.date();
    let (id, marker) = match mark {
        Mark::Ingested(note) => {
            ensure!(
                is_id(note),
                "raw-mark: --note must be a kb-id, got {note:?}"
            );
            (note, format!("state: ingested {note} ({today})"))
        }
        Mark::Rejected(reason) => {
            ensure!(
                valid_reason(reason),
                "raw-mark: rejection reason must be nonempty and on one line"
            );
            ("—", format!("state: rejected {reason} ({today})"))
        }
    };
    let raw = lock.root().path().join("raw");
    let path = raw.join(file);
    let text = read_raw(&path)?.with_context(|| format!("not found: raw/{file}"))?;
    let view = scan(&text);
    let entry = number
        .checked_sub(1)
        .and_then(|i| view.entries.get(i))
        .ok_or_else(|| {
            ReviewError(format!(
                "raw-mark: {file} has {} entries, no #{number}",
                view.entries.len()
            ))
        })?;
    if let Some(expected) = revision
        && entry.revision(&text) != expected
    {
        return Err(
            ReviewError(format!("raw/{file} #{number} changed since it was opened")).into(),
        );
    }
    if entry.marker.is_some() {
        return Err(ReviewError(format!(
            "raw-mark: {file} #{number} ({:?}) is already marked",
            entry.title
        ))
        .into());
    }
    let (before, after) = text.split_at(entry.insertion);
    let after = if entry.ending.is_empty() {
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
    let output = format!("marked {file} #{number}: {} -> {marker}", entry.title);
    Ok(PreparedRaw {
        path,
        before: Some(text),
        after,
        action: "raw-mark",
        id: id.to_owned(),
        detail: format!("raw/{file} #{number} | {marker}"),
        output,
    })
}

pub(crate) fn raw_mark(
    lock: &WriteLock,
    file: &str,
    number: usize,
    mark: Mark<'_>,
    stamp: &Stamp<'_>,
) -> Result<Event> {
    let prepared = prepare_mark(lock, file, number, None, mark, stamp)?;
    let log = changes::prepare_entry(lock, stamp, prepared.action, &prepared.id, &prepared.detail)?;
    transaction::stage(lock, &log, &[prepared.file_write()])?;
    store::atomic_write(&prepared.path, prepared.after.as_bytes())
        .map_err(|error| transaction::abort(lock, error.into()))?;
    transaction::finish(lock, &log)
        .with_context(|| format!("raw/{file} #{number} was marked but not journaled"))?;
    Ok(Event {
        subject: format!("knowbox: raw-mark {file} #{number}"),
        stdout: prepared.output,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Verdict {
    Duplicate,
    Contradiction,
    Complementary,
    FalsePositive,
}

impl Verdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::Contradiction => "contradiction",
            Self::Complementary => "complementary",
            Self::FalsePositive => "false-positive",
        }
    }
}

pub(crate) enum VerdictDecision {
    Prepared(PreparedRaw),
    AlreadyRecorded,
}

pub(crate) fn prepare_verdict(
    lock: &WriteLock,
    mut ids: [&str; 2],
    verdict: Verdict,
    reason: &str,
    stamp: &Stamp<'_>,
) -> Result<VerdictDecision> {
    ensure!(
        ids.iter().all(|id| is_id(id)) && ids[0] != ids[1],
        "verdict needs two different note IDs"
    );
    ensure!(
        valid_reason(reason),
        "verdict reason must be nonempty and on one line"
    );
    ids.sort_unstable();
    let reason = reason.trim();
    let root = lock.root();
    let mut same = false;
    for evidence in raw_evidence(root, &mut Inventory::Strict)? {
        if let RawEvidence::Verdict {
            ids: recorded,
            verdict: kind,
            rationale,
            ..
        } = evidence
        {
            let mut pair = recorded.each_ref().map(String::as_str);
            pair.sort_unstable();
            if pair == ids {
                if kind != verdict.as_str() || rationale != reason {
                    return Err(ReviewError(format!(
                        "{} and {} already have a {kind} verdict: {rationale}",
                        ids[0], ids[1],
                    ))
                    .into());
                }
                same = true;
            }
        }
    }
    if same {
        return Ok(VerdictDecision::AlreadyRecorded);
    }
    let file = format!("conflicts-{}.md", stamp.date());
    let path = root.path().join("raw").join(&file);
    let before = read_raw(&path)?;
    let old = before.as_deref().unwrap_or("");
    let ending = if old.find('\n').is_some_and(|i| old[..i].ends_with('\r')) {
        "\r\n"
    } else {
        "\n"
    };
    let separator = if old.is_empty() || old.ends_with(is_line_break) {
        ""
    } else {
        ending
    };
    let [a, b] = ids;
    let kind = verdict.as_str();
    let after =
        format!("{old}{separator}## pair: {a} {b}{ending}> verdict: {kind}: {reason}{ending}");
    Ok(VerdictDecision::Prepared(PreparedRaw {
        path,
        before,
        after,
        action: "verdict",
        id: a.to_owned(),
        detail: format!("{b} | {kind} | raw/{file} | {reason}"),
        output: format!("{a} and {b}: {kind}"),
    }))
}

fn read_raw(path: &std::path::Path) -> Result<Option<String>> {
    if !store::regular_file(path)? {
        return Ok(None);
    }
    let bytes = store::read_regular(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Some(
        String::from_utf8(bytes).with_context(|| format!("decoding {}", path.display()))?,
    ))
}
