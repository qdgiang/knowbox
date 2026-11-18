use std::path::Path;

use anyhow::{Context, Result, ensure};
use jiff::civil::Date;

use super::{Event, actual_id, append_entry, locate, read_log, read_note, rewrite};
use crate::notes::{self, Note};
use crate::store::{self, WriteLock};

fn with_link(note: &Note, key: &str, id: &str) -> Result<String> {
    let mut links = note.items(key);
    ensure!(
        links.iter().all(|s| notes::is_id(s)),
        "non-ID item in {key}; refusing to rewrite"
    );
    if !links.contains(&id) {
        links.push(id);
    }
    Ok(format!("[{}]", links.join(", ")))
}

pub(crate) fn supersede(lock: &WriteLock, old: &str, new: &str, today: Date) -> Result<Event> {
    with_second_write(lock, old, new, today, |path, bytes| {
        Ok(store::atomic_write(path, bytes)?)
    })
}

// `second_write` must report Err only if NEW was NOT published. In production
// atomic_write does its last fallible operation at the atomic rename itself.
pub(super) fn with_second_write(
    lock: &WriteLock,
    old: &str,
    new: &str,
    today: Date,
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
        let status = meta
            .scalar("status")
            .with_context(|| format!("missing scalar status: in {id}"))?;
        ensure!(status != "superseded", "{id} is already superseded");
    }
    let date = today.to_string();
    let banner = format!("> **Superseded by [[{nid}]] — kept for history, excluded from search.**");
    let retired = rewrite::edit(
        &raw_old,
        &[
            ("status", "superseded"),
            ("updated", &date),
            ("related", &with_link(&old_meta, "related", nid)?),
        ],
        None,
        Some(&banner),
    )
    .with_context(|| format!("preparing {}", old.display()))?;
    let successor = rewrite::edit(
        &raw_new,
        &[
            ("supersedes", &with_link(&new_meta, "supersedes", oid)?),
            ("updated", &date),
            ("related", &with_link(&new_meta, "related", oid)?),
        ],
        None,
        None,
    )
    .with_context(|| format!("preparing {}", new.display()))?;
    read_log(lock.root())?;
    store::atomic_write(&old, &retired)
        .context("writing OLD failed — neither note was published")?;
    if let Err(second_error) = second_write(&new, &successor) {
        if let Err(restore_error) = store::atomic_write(&old, &raw_old) {
            return Err(second_error).context(format!("NEW {} was not published; restoring OLD also failed ({restore_error:#}); {} is still modified", new.display(), old.display()));
        }
        return Err(second_error).context(format!(
            "NEW {} was not published; OLD {} restored byte-identical",
            new.display(),
            old.display()
        ));
    }
    append_entry(
        lock,
        today,
        "supersede",
        oid,
        &format!("superseded by {nid}"),
    )
    .with_context(|| {
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
