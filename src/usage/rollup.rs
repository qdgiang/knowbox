use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use jiff::{Timestamp, civil::Date};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};

use crate::store;

use super::raw::{self, ExpiryGuard, FileAccess, Month, Scan, Source, SourceState};
use super::totals::{Access, AccessKey, Kind};
use super::{Layer, Totals};

const DATABASE: &str = "rollup.sqlite";
const EXPIRING_DIRECTORY: &str = ".expiring";
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS coverage (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    start TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS checkpoint (
    month TEXT PRIMARY KEY,
    generation TEXT NOT NULL,
    offset INTEGER NOT NULL,
    length INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    dropped INTEGER NOT NULL,
    sealed INTEGER NOT NULL DEFAULT 0,
    gap INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS access (
    note TEXT NOT NULL,
    day TEXT NOT NULL,
    kind TEXT NOT NULL,
    count INTEGER NOT NULL,
    seconds INTEGER NOT NULL,
    nanos INTEGER NOT NULL,
    PRIMARY KEY (note, day, kind)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS access_client (
    note TEXT NOT NULL,
    day TEXT NOT NULL,
    kind TEXT NOT NULL,
    client TEXT NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (note, day, kind, client)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS access_session (
    note TEXT NOT NULL,
    day TEXT NOT NULL,
    kind TEXT NOT NULL,
    session TEXT NOT NULL,
    PRIMARY KEY (note, day, kind, session)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS outcome (
    day TEXT NOT NULL,
    layer TEXT NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (day, layer)
) WITHOUT ROWID;";

struct Checkpoint {
    source: Source,
    dropped: u64,
    sealed: bool,
    gap: bool,
}

type Checkpoints = BTreeMap<Month, Checkpoint>;

pub(super) fn refresh(directory: &Path, today: Date) -> Result<()> {
    let Some(mut db) = open(directory, FileAccess::Create)? else {
        anyhow::bail!("usage database disappeared during creation");
    };
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(SCHEMA)?;
    mark_coverage(&tx, today)?;

    let checkpoints = checkpoints(&tx)?;
    for month in source_months(directory, &checkpoints)? {
        let previous = checkpoints.get(&month);
        if previous.is_some_and(|checkpoint| checkpoint.gap || checkpoint.sealed) {
            continue;
        }

        let path = month.path(directory);
        let source = previous.map(|checkpoint| &checkpoint.source);
        match raw::scan(&path, month, source)? {
            Scan::Complete { source, totals } => {
                store_totals(&tx, &totals)?;
                tx.execute(
                    "INSERT INTO checkpoint (month, generation, offset, length, fingerprint, dropped)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                       ON CONFLICT(month) DO UPDATE SET generation = excluded.generation,
                         offset = excluded.offset, length = excluded.length,
                         fingerprint = excluded.fingerprint, dropped = checkpoint.dropped + excluded.dropped",
                    params![
                        month.to_string(), source.generation, i64::try_from(source.offset)?,
                        i64::try_from(source.length)?, source.fingerprint,
                        i64::try_from(totals.dropped)?,
                    ],
                )?;
            }
            Scan::Gap | Scan::Missing if previous.is_some() => mark_gap(&tx, month)?,
            Scan::Gap => {
                // A source disappeared before its first sample could be committed.
                tx.execute(
                    "INSERT INTO checkpoint VALUES (?1, '', 0, 0, '', 0, 0, 1)",
                    [month.to_string()],
                )?;
            }
            Scan::Missing => {}
        }
    }
    tx.commit()?;

    let expiring = directory.join(EXPIRING_DIRECTORY);
    let mut expiry_candidates = raw::months(directory)?;
    if store::directory(&expiring)? {
        expiry_candidates.extend(raw::months(&expiring)?);
    }
    for month in expiry_candidates {
        if month.is_due(today) {
            expire(&mut db, directory, &expiring, month)?;
        }
    }
    Ok(())
}

fn expire(db: &mut Connection, directory: &Path, expiring: &Path, month: Month) -> Result<()> {
    let Some(mut guard) = ExpiryGuard::try_acquire(directory, month)? else {
        return Ok(());
    };
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let name = month.to_string();
    let checkpoint = tx
        .query_row(
            "SELECT generation, offset, length, fingerprint, dropped, sealed, gap
             FROM checkpoint WHERE month = ?1",
            [&name],
            checkpoint,
        )
        .optional()?;
    let Some(checkpoint) = checkpoint.filter(|checkpoint| !checkpoint.gap) else {
        return Ok(());
    };
    if !checkpoint.source.has_full_fingerprint() {
        mark_gap(&tx, month)?;
        tx.commit()?;
        return Ok(());
    }
    let staged = month.path(expiring);
    if store::regular_file(&staged)? {
        ensure!(
            checkpoint.sealed,
            "unsealed usage month {month} has a staged expiry"
        );
        tx.commit()?;
        return finish_expiry(db, month, &checkpoint.source, &staged);
    }
    let path = month.path(directory);
    let dropped_tail = match checkpoint.source.state(&path)? {
        SourceState::Eof => false,
        SourceState::Torn => !checkpoint.sealed,
        SourceState::Pending if !checkpoint.sealed => return Ok(()),
        SourceState::Missing if checkpoint.sealed => return Ok(()),
        SourceState::Pending | SourceState::Changed | SourceState::Missing => {
            mark_gap(&tx, month)?;
            tx.commit()?;
            return Ok(());
        }
    };

    guard.prevent_appends()?;
    tx.execute(
        "UPDATE checkpoint SET sealed = 1, dropped = dropped + ?1 WHERE month = ?2",
        params![dropped_tail, name],
    )?;
    tx.commit()?;

    // Verify the moved file so replacing the active path cannot redirect deletion.
    raw::create_directory(expiring)?;
    match fs::rename(&path, &staged) {
        Ok(()) => finish_expiry(db, month, &checkpoint.source, &staged),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("staging expiry for {}", path.display())),
    }
}

fn finish_expiry(db: &Connection, month: Month, source: &Source, staged: &Path) -> Result<()> {
    match source.state(staged)? {
        SourceState::Eof | SourceState::Torn => fs::remove_file(staged)
            .with_context(|| format!("expiring usage source {}", staged.display()))?,
        SourceState::Missing => {}
        SourceState::Pending | SourceState::Changed => mark_gap(db, month)?,
    }
    Ok(())
}

pub(super) fn snapshot(directory: &Path) -> Result<Totals> {
    if let Some(db) = open(directory, FileAccess::Read)? {
        return database_snapshot(db, directory);
    }
    let mut totals = Totals::default();
    overlay(directory, &Checkpoints::new(), &mut totals)?;
    // The initial rollup can publish and prune between finding no database and reading the tails.
    match open(directory, FileAccess::Read)? {
        Some(db) => database_snapshot(db, directory),
        None => Ok(totals),
    }
}

fn database_snapshot(mut db: Connection, directory: &Path) -> Result<Totals> {
    db.execute_batch("PRAGMA query_only=ON")?;
    let tx = db.transaction()?;
    let initialized = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'coverage')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    let mut totals = Totals::default();
    if initialized {
        read_totals(&tx, &mut totals)?;
        overlay(directory, &checkpoints(&tx)?, &mut totals)?;
    } else {
        overlay(directory, &Checkpoints::new(), &mut totals)?;
    }
    tx.commit()?;
    Ok(totals)
}

fn overlay(directory: &Path, checkpoints: &Checkpoints, totals: &mut Totals) -> Result<()> {
    for month in source_months(directory, checkpoints)? {
        let checkpoint = checkpoints.get(&month);
        if let Some(checkpoint) = checkpoint {
            totals.dropped += checkpoint.dropped;
            if checkpoint.gap {
                totals.gaps.insert(month);
            }
            if checkpoint.gap || checkpoint.sealed {
                continue;
            }
        }
        let previous = checkpoint.map(|checkpoint| &checkpoint.source);
        match raw::scan(&month.path(directory), month, previous)? {
            Scan::Complete { totals: tail, .. } => totals.merge(tail),
            Scan::Missing if previous.is_none() => {}
            Scan::Missing | Scan::Gap => {
                totals.gaps.insert(month);
            }
        }
    }
    Ok(())
}

fn open(directory: &Path, access: FileAccess) -> Result<Option<Connection>> {
    let path = directory.join(DATABASE);
    let file = match raw::private_file(&path, access) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => result.with_context(|| format!("opening usage database {}", path.display()))?,
    };
    // Rollback journaling lets reports use consistent read-only transactions without WAL sidecars.
    check_rollback_mode(file)?;
    for name in ["rollup.sqlite-wal", "rollup.sqlite-shm"] {
        ensure!(
            !store::regular_file(&directory.join(name))?,
            "unexpected SQLite usage sidecar {name}"
        );
    }
    let journal = directory.join("rollup.sqlite-journal");
    match raw::private_file(&journal, FileAccess::Read) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("opening usage journal {}", journal.display()));
        }
    }

    let access = match access {
        FileAccess::Read => OpenFlags::SQLITE_OPEN_READ_ONLY,
        FileAccess::Create => OpenFlags::SQLITE_OPEN_READ_WRITE,
        FileAccess::Append => anyhow::bail!("usage databases require transactional writes"),
    };
    let db = Connection::open_with_flags(&path, access | OpenFlags::SQLITE_OPEN_NOFOLLOW)
        .with_context(|| format!("opening SQLite usage database {}", path.display()))?;
    db.execute_batch("PRAGMA trusted_schema=OFF")?;
    Ok(Some(db))
}

fn check_rollback_mode(mut file: fs::File) -> Result<()> {
    let mut header = [0_u8; 20];
    match file.read_exact(&mut header) {
        Ok(()) => ensure!(
            header.starts_with(b"SQLite format 3\0") && header[18..20] == [1, 1],
            "usage database must use SQLite rollback journaling"
        ),
        Err(error)
            if error.kind() == io::ErrorKind::UnexpectedEof && file.metadata()?.len() == 0 => {}
        Err(error) => return Err(error).context("reading usage database header"),
    }
    Ok(())
}

fn source_months(directory: &Path, checkpoints: &Checkpoints) -> Result<BTreeSet<Month>> {
    let mut all = raw::months(directory)?;
    all.extend(checkpoints.keys());
    Ok(all)
}

fn checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Checkpoint> {
    let offset = unsigned_column(row, 1)?;
    let length = unsigned_column(row, 2)?;
    if offset > length {
        return Err(rusqlite::Error::IntegralValueOutOfRange(1, row.get(1)?));
    }
    Ok(Checkpoint {
        source: Source {
            generation: row.get(0)?,
            offset,
            length,
            fingerprint: row.get(3)?,
        },
        dropped: unsigned_column(row, 4)?,
        sealed: row.get(5)?,
        gap: row.get(6)?,
    })
}

fn unsigned_column(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}

fn checkpoints(db: &Connection) -> Result<Checkpoints> {
    let mut stmt = db.prepare(
        "SELECT generation, offset, length, fingerprint, dropped, sealed, gap, month FROM checkpoint",
    )?;
    let mut rows = stmt.query([])?;
    let mut checkpoints = Checkpoints::new();
    while let Some(row) = rows.next()? {
        let name: String = row.get(7)?;
        let month = Month::parse(&name).with_context(|| format!("invalid usage month {name}"))?;
        checkpoints.insert(month, checkpoint(row)?);
    }
    Ok(checkpoints)
}

fn mark_gap(db: &Connection, month: Month) -> Result<()> {
    db.execute(
        "UPDATE checkpoint SET gap = 1 WHERE month = ?1",
        [month.to_string()],
    )?;
    Ok(())
}

fn mark_coverage(db: &Connection, day: Date) -> Result<()> {
    db.execute(
        "INSERT INTO coverage VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET start = MIN(coverage.start, excluded.start)",
        [day.to_string()],
    )?;
    Ok(())
}

fn store_totals(db: &Connection, totals: &Totals) -> Result<()> {
    if let Some(day) = totals.coverage_start {
        mark_coverage(db, day)?;
    }
    let mut access = db.prepare(
        "INSERT INTO access VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(note, day, kind) DO UPDATE SET
           count = access.count + excluded.count,
           seconds = MAX(access.seconds, excluded.seconds),
           nanos = CASE WHEN (excluded.seconds, excluded.nanos) > (access.seconds, access.nanos)
                   THEN excluded.nanos ELSE access.nanos END",
    )?;
    let mut client = db.prepare(
        "INSERT INTO access_client VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(note, day, kind, client) DO UPDATE SET count = access_client.count + excluded.count",
    )?;
    let mut session = db.prepare(
        "INSERT INTO access_session VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(note, day, kind, session) DO NOTHING",
    )?;
    for (key, value) in &totals.access {
        let day = key.day.to_string();
        let kind = key.kind.name();
        access.execute(params![
            key.note,
            day,
            kind,
            i64::try_from(value.count)?,
            value.latest.as_second(),
            value.latest.subsec_nanosecond(),
        ])?;
        for (name, count) in &value.clients {
            client.execute(params![key.note, day, kind, name, i64::try_from(*count)?])?;
        }
        for name in &value.sessions {
            session.execute(params![key.note, day, kind, name])?;
        }
    }
    let mut outcome = db.prepare(
        "INSERT INTO outcome VALUES (?1, ?2, ?3)
         ON CONFLICT(day, layer) DO UPDATE SET count = outcome.count + excluded.count",
    )?;
    for ((day, layer), count) in &totals.outcomes {
        outcome.execute(params![
            day.to_string(),
            layer.name(),
            i64::try_from(*count)?
        ])?;
    }
    Ok(())
}

fn read_totals(db: &Connection, totals: &mut Totals) -> Result<()> {
    let start: String = db.query_row("SELECT start FROM coverage WHERE id = 1", [], |row| {
        row.get(0)
    })?;
    totals.observe_day(start.parse()?);

    let mut stmt = db.prepare("SELECT note, day, kind, count, seconds, nanos FROM access")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let key = read_access_key(row)?;
        let latest = Timestamp::new(row.get(4)?, row.get(5)?)?;
        totals
            .access
            .insert(key, Access::new(unsigned_column(row, 3)?, latest));
    }
    let mut stmt = db.prepare("SELECT note, day, kind, client, count FROM access_client")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let key = read_access_key(row)?;
        let value = totals.access.get_mut(&key).context("orphan usage client")?;
        value.clients.insert(row.get(3)?, unsigned_column(row, 4)?);
    }
    let mut stmt = db.prepare("SELECT note, day, kind, session FROM access_session")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let key = read_access_key(row)?;
        let value = totals
            .access
            .get_mut(&key)
            .context("orphan usage session")?;
        value.sessions.insert(row.get(3)?);
    }
    let mut stmt = db.prepare("SELECT day, layer, count FROM outcome")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let day: String = row.get(0)?;
        let layer: String = row.get(1)?;
        totals.outcomes.insert(
            (day.parse()?, Layer::parse(&layer)?),
            unsigned_column(row, 2)?,
        );
    }
    Ok(())
}

fn read_access_key(row: &rusqlite::Row<'_>) -> Result<AccessKey> {
    let day: String = row.get(1)?;
    let kind: String = row.get(2)?;
    Ok(AccessKey {
        note: row.get(0)?,
        day: day.parse()?,
        kind: Kind::parse(&kind)?,
    })
}
