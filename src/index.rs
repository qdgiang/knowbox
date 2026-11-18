mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::Context;
use rusqlite::{Connection, ErrorCode, OpenFlags, params};
use thiserror::Error;

use crate::config::KbRoot;
use crate::notes::{self, NoteEntry};
use crate::store::{self, WriteLock};
pub(crate) use render::ref_names;

// Keep the stored CREATE statements compatible with Python-built databases.
const TABLES: [(&str, &str); 4] = [
    (
        "kb",
        "CREATE VIRTUAL TABLE kb USING fts5(id UNINDEXED, path UNINDEXED, title, typ, domain, tags, aliases, triggers, summary, body, tokenize=\"unicode61 tokenchars '-_.#/'\")",
    ),
    (
        "kb_tri",
        "CREATE VIRTUAL TABLE kb_tri USING fts5(id UNINDEXED, path UNINDEXED, title, body, tokenize='trigram')",
    ),
    (
        "kb_d",
        "CREATE VIRTUAL TABLE kb_d USING fts5(id UNINDEXED, path UNINDEXED, title, typ, domain, tags, aliases, triggers, summary, body)",
    ),
    (
        "kb_meta",
        "CREATE TABLE kb_meta (path TEXT PRIMARY KEY, sha1 TEXT, bytes INTEGER NOT NULL)",
    ),
];

#[derive(Debug, Error)]
pub(crate) enum IndexError {
    #[error(transparent)]
    Notes(#[from] notes::NoteError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Store(#[from] store::StoreError),
    #[error("SQLite index operation failed")]
    Sql(#[from] rusqlite::Error),
    #[error("refusing to use .kb.db while SQLite sidecar exists: {}", .0.display())]
    Sidecar(PathBuf),
    #[error("temporary SQLite index did not enter DELETE journal mode")]
    JournalMode,
}

struct Snapshot {
    notes: Vec<NoteEntry>,
    index: String,
    count: usize,
}

impl Snapshot {
    fn read(root: &KbRoot) -> Result<Self, IndexError> {
        let notes = notes::snapshot(root)?;
        let (index, count) = render::render(&notes, &ref_names(root)?);
        Ok(Self {
            notes,
            index,
            count,
        })
    }

    fn publish(&self, lock: &WriteLock) -> Result<(), IndexError> {
        let root = lock.root();
        let db_path = root.path().join(".kb.db");
        let index_path = root.path().join("index.md");
        // Validate both destinations before changing either one.
        store::regular_file(&index_path)?;
        store::regular_file(&db_path)?;
        guard_sidecars(root)?;
        let tmp = tempfile::Builder::new()
            .prefix(".kb.db.")
            .tempfile_in(root.path())?
            .into_temp_path();
        let mut db = Connection::open_with_flags(
            &tmp,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        let mode: String = db.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
        if mode != "delete" {
            return Err(IndexError::JournalMode);
        }
        populate(&mut db, &self.notes)?;
        db.close().map_err(|(_, e)| e)?;
        guard_sidecars(root)?;
        store::replace(tmp, &db_path)?;
        store::atomic_write(&index_path, self.index.as_bytes())?;
        Ok(())
    }
}

fn populate(db: &mut Connection, notes: &[NoteEntry]) -> rusqlite::Result<()> {
    let tx = db.transaction()?;
    for (_, ddl) in TABLES {
        tx.execute(ddl, [])?;
    }
    {
        let mut primary = tx.prepare("INSERT INTO kb VALUES (?,?,?,?,?,?,?,?,?,?)")?;
        let mut split = tx.prepare("INSERT INTO kb_d VALUES (?,?,?,?,?,?,?,?,?,?)")?;
        let mut tri = tx.prepare("INSERT INTO kb_tri VALUES (?,?,?,?)")?;
        let mut meta = tx.prepare("INSERT INTO kb_meta VALUES (?,?,?)")?;
        for entry in notes {
            let bytes = i64::try_from(entry.size)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            meta.execute(params![entry.relative, entry.sha1, bytes])?;
            let Ok(note) = &entry.parsed else { continue };
            if note.scalar("status") == Some("superseded") {
                continue;
            }
            let [title, summary, tags, aliases, triggers] = note.search_text_parts();
            let id = note.text("id", "");
            let typ = note.text("type", "");
            let domain = note.text("domain", "");
            let row = params![
                id,
                entry.relative,
                title,
                typ,
                domain,
                tags,
                aliases,
                triggers,
                summary,
                note.body
            ];
            primary.execute(row)?;
            split.execute(row)?;
            tri.execute(params![id, entry.relative, title, note.body])?;
        }
    }
    tx.commit()
}

fn guard_sidecars(root: &KbRoot) -> Result<(), IndexError> {
    for name in [".kb.db-wal", ".kb.db-shm", ".kb.db-journal"] {
        let path = root.path().join(name);
        if store::regular_file(&path)? {
            return Err(IndexError::Sidecar(path));
        }
    }
    Ok(())
}

/// Regenerate both derived artifacts from one snapshot; return the index.md note count.
pub(crate) fn rebuild(lock: &WriteLock) -> Result<usize, IndexError> {
    let snapshot = Snapshot::read(lock.root())?;
    snapshot.publish(lock)?;
    Ok(snapshot.count)
}

fn manifest_drift(db: &Connection, notes: &[NoteEntry]) -> rusqlite::Result<Option<Vec<String>>> {
    // A database predating kb_d is rebuilt once rather than serving degraded results.
    let mut schema = db.prepare(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=? AND sql=?)",
    )?;
    for (name, ddl) in TABLES {
        if !schema.query_row([name, ddl], |r| r.get::<_, bool>(0))? {
            return Ok(None);
        }
    }
    let mut meta = db.prepare("SELECT path, sha1, bytes FROM kb_meta")?;
    let mut stored = meta
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, String>(1)?, row.get::<_, i64>(2)?),
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut drifted = Vec::new();
    for entry in notes {
        if !stored.remove(&entry.relative).is_some_and(|(hash, bytes)| {
            hash == entry.sha1 && usize::try_from(bytes) == Ok(entry.size)
        }) {
            drifted.push(entry.relative.clone());
        }
    }
    drifted.extend(stored.into_keys());
    drifted.sort_unstable();
    Ok(Some(drifted))
}

fn corrupt(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    ) || matches!(
        e,
        rusqlite::Error::InvalidColumnType(..)
            | rusqlite::Error::Utf8Error(..)
            | rusqlite::Error::IntegralValueOutOfRange(..)
    )
}

fn open_reader(root: &KbRoot) -> Result<Option<Connection>, IndexError> {
    let db_path = root.path().join(".kb.db");
    guard_sidecars(root)?;
    if !store::regular_file(&db_path)? {
        return Ok(None);
    }
    // SQLite in WAL mode can create sidecars even for a read-only connection.
    // Only open self-contained rollback-journal databases.
    let mut header = [0; 20];
    match fs::File::open(&db_path).and_then(|mut f| f.read_exact(&mut header)) {
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof
            ) =>
        {
            return Ok(None);
        }
        result => result?,
    }
    if !header.starts_with(b"SQLite format 3\0") || header[18..20] != [1, 1] {
        return Ok(None);
    }
    let db = match Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    ) {
        Err(e)
            if e.sqlite_error_code() == Some(ErrorCode::CannotOpen)
                && !store::regular_file(&db_path)? =>
        {
            return Ok(None);
        }
        Err(e) if corrupt(&e) => return Ok(None),
        result => result?,
    };
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON; BEGIN")?;
    Ok(Some(db))
}

fn check_manifest(db: &Connection, notes: &[NoteEntry]) -> Result<Option<Vec<String>>, IndexError> {
    match manifest_drift(db, notes) {
        Err(e) if corrupt(&e) => Ok(None),
        result => Ok(result?),
    }
}

fn index_bytes(root: &KbRoot) -> Result<Option<Vec<u8>>, IndexError> {
    let path = root.path().join("index.md");
    if !store::regular_file(&path)? {
        return Ok(None);
    }
    match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        result => Ok(Some(result?)),
    }
}

fn checked<T>(
    root: &KbRoot,
    snapshot: &Snapshot,
    query: &impl Fn(&Connection, &[NoteEntry]) -> rusqlite::Result<T>,
) -> Result<Option<T>, IndexError> {
    let Some(db) = open_reader(root)? else {
        return Ok(None);
    };
    if !check_manifest(&db, &snapshot.notes)?.is_some_and(|drifted| drifted.is_empty())
        || index_bytes(root)?.as_deref() != Some(snapshot.index.as_bytes())
    {
        return Ok(None);
    }
    match query(&db, &snapshot.notes) {
        Err(e) if corrupt(&e) => Ok(None),
        result => Ok(Some(result?)),
    }
}

#[derive(Debug, Default)]
pub(crate) struct Freshness {
    pub(crate) db_missing: bool,
    pub(crate) drifted: Vec<String>,
    pub(crate) orphans: Vec<String>,
    pub(crate) index_drift: bool,
}

pub(crate) fn freshness(root: &KbRoot, notes: &[NoteEntry]) -> Result<Freshness, IndexError> {
    let mut report = Freshness::default();
    if let Some(db) = open_reader(root)?
        && let Some(drifted) = check_manifest(&db, notes)?
    {
        report.drifted = drifted;
    } else {
        report.db_missing = true;
        return Ok(report);
    }
    let Some(bytes) = index_bytes(root)? else {
        report.index_drift = true;
        return Ok(report);
    };
    let disk_ids: BTreeSet<_> = notes
        .iter()
        .filter_map(|entry| {
            entry
                .relative
                .strip_prefix("notes/")
                .and_then(notes::id_prefix)
        })
        .collect();
    report.orphans = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| line.strip_prefix("- ["))
        .filter_map(|line| {
            let id = notes::id_prefix(line)?;
            (line[id.len()..].starts_with(" ·") && !disk_ids.contains(id)).then_some(id.to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if report.drifted.is_empty() && report.orphans.is_empty() {
        let (expected, _) = render::render(notes, &ref_names(root)?);
        report.index_drift = bytes != expected.as_bytes();
    }
    Ok(report)
}

/// Read on the same connection that passed freshness. On drift, recheck under lock
/// before publishing, and retry at most once. No reader handle survives a swap.
pub(crate) fn with_current<T>(
    root: &KbRoot,
    query: impl Fn(&Connection, &[NoteEntry]) -> rusqlite::Result<T>,
) -> anyhow::Result<(T, bool)> {
    if let Some(result) = checked(root, &Snapshot::read(root)?, &query)? {
        return Ok((result, false));
    }
    let lock = WriteLock::acquire(root)?;
    let snapshot = Snapshot::read(root)?;
    if let Some(result) = checked(root, &snapshot, &query)? {
        return Ok((result, false));
    }
    snapshot.publish(&lock)?;
    let result = checked(root, &snapshot, &query)?
        .context("index is still stale after rebuild — run 'knowbox reindex'")?;
    Ok((result, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_schema_requires_rebuild() -> rusqlite::Result<()> {
        let mut db = Connection::open_in_memory()?;
        populate(&mut db, &[])?;
        assert_eq!(manifest_drift(&db, &[])?, Some(Vec::new()));
        db.execute("DROP TABLE kb_d", [])?;
        assert_eq!(manifest_drift(&db, &[])?, None);
        Ok(())
    }
}
