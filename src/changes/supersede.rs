use std::path::Path;

use anyhow::{Context, Result, ensure};

use super::mutation::links_with;
use super::{Event, FileWrite, actual_id, locate, note_journal, read_note, rewrite, transaction};
use crate::journal::Stamp;
use crate::notes::{self, Note};
use crate::store::{self, WriteLock};

pub(super) fn mark_retired(
    bytes: &[u8],
    note: &Note,
    successor: &str,
    date: &str,
) -> Result<Vec<u8>> {
    let banner =
        format!("> **Superseded by [[{successor}]] — kept for history, excluded from search.**");
    rewrite::patch_with_banner(
        bytes,
        &[
            ("status", "superseded"),
            ("updated", date),
            ("related", &links_with(note, "related", &[successor])?),
        ],
        &banner,
    )
}

pub(super) fn mark_successor(bytes: &[u8], note: &Note, old: &str, date: &str) -> Result<Vec<u8>> {
    rewrite::patch(
        bytes,
        &[
            ("supersedes", &links_with(note, "supersedes", &[old])?),
            ("updated", date),
            ("related", &links_with(note, "related", &[old])?),
        ],
        None,
    )
}

pub(super) fn require_live(note: &Note, id: &str) -> Result<()> {
    let status = note
        .scalar("status")
        .with_context(|| format!("missing scalar status: in {id}"))?;
    ensure!(status != "superseded", "{id} is already superseded");
    Ok(())
}

pub(super) fn detail(successor: &str, reason: Option<&str>) -> Result<String> {
    match reason {
        None => Ok(format!("superseded by {successor}")),
        Some(reason) => {
            super::single_line("reason", reason)?;
            ensure!(!reason.trim().is_empty(), "reason must not be blank");
            Ok(format!("superseded by {successor} | reason={reason}"))
        }
    }
}

pub(crate) fn supersede(
    lock: &WriteLock,
    old: &str,
    new: &str,
    reason: Option<&str>,
    stamp: &Stamp<'_>,
) -> Result<Event> {
    with_second_write(lock, old, new, reason, stamp, |path, bytes| {
        Ok(store::atomic_write(path, bytes)?)
    })
}

// `second_write` must report Err only if NEW was NOT published. In production
// atomic_write does its last fallible operation at the atomic rename itself.
pub(super) fn with_second_write(
    lock: &WriteLock,
    old: &str,
    new: &str,
    reason: Option<&str>,
    stamp: &Stamp<'_>,
    second_write: impl FnOnce(&Path, &[u8]) -> Result<()>,
) -> Result<Event> {
    let old = locate(lock.root(), old)?;
    let new = locate(lock.root(), new)?;
    ensure!(
        old.canonicalize()? != new.canonicalize()?,
        "OLD and NEW resolve to the same note"
    );
    let raw_old = read_note(&old)?;
    let raw_new = read_note(&new)?;
    let old_meta = notes::parse(&raw_old).with_context(|| format!("parsing {}", old.display()))?;
    let new_meta = notes::parse(&raw_new).with_context(|| format!("parsing {}", new.display()))?;
    let oid = actual_id(&old_meta)?;
    let nid = actual_id(&new_meta)?;
    ensure!(oid != nid, "OLD and NEW have the same id {oid}");
    for (id, meta) in [(oid, &old_meta), (nid, &new_meta)] {
        require_live(meta, id)?;
    }
    let date = stamp.date().to_string();
    let detail = detail(nid, reason)?;
    let retired = mark_retired(&raw_old, &old_meta, nid, &date)
        .with_context(|| format!("preparing {}", old.display()))?;
    let successor = mark_successor(&raw_new, &new_meta, oid, &date)
        .with_context(|| format!("preparing {}", new.display()))?;
    let log = note_journal(
        lock,
        stamp,
        "supersede",
        oid,
        &detail,
        &[
            FileWrite {
                path: &old,
                before: Some(&raw_old),
                after: &retired,
            },
            FileWrite {
                path: &new,
                before: Some(&raw_new),
                after: &successor,
            },
        ],
    )?;
    store::atomic_write(&old, &retired)
        .context("writing OLD failed — neither note was published")
        .map_err(|error| transaction::abort(lock, error))?;
    if let Err(second_error) = second_write(&new, &successor) {
        if let Err(restore_error) = store::atomic_write(&old, &raw_old) {
            return Err(second_error).context(format!("NEW {} was not published; restoring OLD also failed ({restore_error:#}); {} is still modified", new.display(), old.display()));
        }
        return Err(transaction::abort(
            lock,
            second_error.context(format!(
                "NEW {} was not published; OLD {} restored byte-identical",
                new.display(),
                old.display()
            )),
        ));
    }
    transaction::finish(lock, &log).with_context(|| {
        format!(
            "both notes were published ({} and {}) but not journaled",
            old.display(),
            new.display()
        )
    })?;
    Ok(Event {
        subject: format!("knowbox: supersede {oid} → {nid}"),
        stdout: format!("{oid} superseded by {nid}"),
    })
}
