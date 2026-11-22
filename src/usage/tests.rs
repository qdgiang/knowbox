use std::fs;

use anyhow::Result;
use jiff::civil::date;
use rusqlite::Connection;

use super::*;

fn fixture() -> Result<(tempfile::TempDir, KbRoot)> {
    let directory = tempfile::tempdir()?;
    let root = KbRoot::open(directory.path())?;
    Ok((directory, root))
}

fn capture(root: &KbRoot) -> Result<String> {
    let mut out = Vec::new();
    report(root, None, &mut out)?;
    Ok(String::from_utf8(out)?)
}

fn event(
    root: &KbRoot,
    time: &str,
    client: &str,
    session: Option<&str>,
    read: &ReadEvent,
) -> Result<()> {
    let now = time.parse()?;
    record(
        root,
        &Context {
            now: &now,
            client,
            session,
        },
        read,
    )
}

fn search(layer: Layer) -> ReadEvent {
    ReadEvent::Search {
        layer,
        notes: vec!["kb-0042".into(), "kb-0042".into(), "kb-0043".into()],
        expanded: vec!["kb-0042".into(), "kb-0042".into()],
    }
}

#[test]
fn rollup_refuses_a_multiply_linked_rollback_journal() -> Result<()> {
    let (directory, root) = fixture()?;
    let today = date(2026, 5, 1);
    rollup(&root, today)?;
    let canary = directory.path().join("canary");
    let original = b"preserve this file";
    fs::write(&canary, original)?;
    let journal = root.path().join(DIRECTORY).join("rollup.sqlite-journal");
    fs::hard_link(&canary, &journal)?;
    let error = match rollup(&root, today) {
        Err(error) => error,
        Ok(()) => anyhow::bail!("rollup accepted the linked journal"),
    };
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput),
    );
    assert_eq!(fs::read(canary)?, original);
    assert_eq!(fs::read(journal)?, original);
    Ok(())
}

#[cfg(unix)]
#[test]
fn append_uses_the_directory_that_owns_its_month_lock() -> Result<()> {
    use std::os::unix::fs::symlink;

    let (fixture, root) = fixture()?;
    let directory = root.path().join(DIRECTORY);
    raw::create_directory(&directory)?;
    let month = Month::of(date(2026, 5, 1));
    let guard = raw::AppendGuard::acquire(&directory, month)?;
    let original = fixture.path().join("pinned");
    let target = fixture.path().join("elsewhere");
    fs::create_dir(&target)?;
    fs::rename(&directory, &original)?;
    symlink(&target, &directory)?;
    guard.append(b"usage record\n")?;
    assert_eq!(fs::read(month.path(&original))?, b"usage record\n");
    assert_eq!(fs::read_dir(&target)?.count(), 0);
    Ok(())
}

#[test]
fn expiry_keeps_a_source_with_an_interior_checkpoint_rewrite() -> Result<()> {
    let (_fixture, root) = fixture()?;
    let today = date(2026, 5, 1);
    for _ in 0..16 {
        event(
            &root,
            "2026-05-01T10:00:00+00:00[+00:00]",
            "cli",
            None,
            &search(Layer::Lexical),
        )?;
    }
    rollup(&root, today)?;
    let source = Month::of(today).path(&root.path().join(DIRECTORY));
    let mut bytes = fs::read(&source)?;
    let position = bytes[256..bytes.len() - 256]
        .windows(b"kb-0042".len())
        .position(|window| window == b"kb-0042")
        .ok_or_else(|| anyhow::anyhow!("fixture needs a note ID inside the checkpoint"))?
        + 256;
    bytes[position..position + b"kb-0042".len()].copy_from_slice(b"kb-0049");
    fs::write(&source, &bytes)?;
    rollup(&root, date(2026, 9, 1))?;
    assert_eq!(fs::read(&source)?, bytes);
    assert_eq!(
        capture(&root)?
            .lines()
            .find(|line| line.starts_with("Usage coverage gap")),
        Some("Usage coverage gap\t2026-05"),
    );
    Ok(())
}

#[test]
fn expiry_preserves_months_with_a_legacy_sampled_prefix() -> Result<()> {
    let (_fixture, root) = fixture()?;
    let today = date(2026, 5, 1);
    let time = "2026-05-01T10:00:00+00:00[+00:00]";
    let read = search(Layer::Lexical);
    for _ in 0..16 {
        event(&root, time, "cli", None, &read)?;
    }
    rollup(&root, today)?;
    let directory = root.path().join(DIRECTORY);
    let source = Month::of(today).path(&directory);
    let bytes = fs::read(&source)?;
    let mut samples = bytes[..256].to_vec();
    samples.extend(&bytes[bytes.len() - 256..]);
    let db = Connection::open(directory.join("rollup.sqlite"))?;
    db.execute(
        "UPDATE checkpoint SET fingerprint = ?1 WHERE month = '2026-05'",
        [crate::notes::sha1_hex(&samples)],
    )?;
    event(&root, time, "cli", None, &read)?;
    rollup(&root, today)?;
    assert_eq!(
        capture(&root)?.lines().last(),
        Some("2026-05-01\tlexical\t17")
    );
    rollup(&root, date(2026, 9, 1))?;
    assert!(fs::read(source)?.starts_with(&bytes));
    assert_eq!(
        capture(&root)?
            .lines()
            .find(|line| line.starts_with("Usage coverage gap")),
        Some("Usage coverage gap\t2026-05"),
    );
    Ok(())
}

#[test]
fn expiry_retries_a_verified_staged_month_and_keeps_a_replacement() -> Result<()> {
    let (_fixture, root) = fixture()?;
    let today = date(2026, 5, 1);
    let time = "2026-05-01T10:00:00+00:00[+00:00]";
    let read = search(Layer::Lexical);
    event(&root, time, "cli", None, &read)?;
    rollup(&root, today)?;
    let directory = root.path().join(DIRECTORY);
    let source = Month::of(today).path(&directory);
    let staging = directory.join(".expiring");
    fs::create_dir(&staging)?;
    let staged = Month::of(today).path(&staging);
    fs::rename(&source, &staged)?;
    let db = Connection::open(directory.join("rollup.sqlite"))?;
    db.execute(
        "UPDATE checkpoint SET sealed = 1 WHERE month = '2026-05'",
        [],
    )?;
    let replacement = b"the replacement is outside the checkpoint";
    fs::write(&source, replacement)?;
    rollup(&root, date(2026, 9, 1))?;
    assert!(!staged.exists());
    assert_eq!(fs::read(&source)?, replacement);
    rollup(&root, date(2026, 9, 1))?;
    assert_eq!(fs::read(source)?, replacement);
    assert_eq!(
        capture(&root)?
            .lines()
            .find(|line| line.starts_with("Usage coverage gap")),
        Some("Usage coverage gap\t2026-05"),
    );
    Ok(())
}

#[test]
fn finite_usage_windows_end_on_the_requested_date() -> Result<()> {
    let (_fixture, root) = fixture()?;
    for time in [
        "2026-05-01T10:00:00+00:00[+00:00]",
        "2026-05-02T10:00:00+00:00[+00:00]",
    ] {
        event(&root, time, "cli", None, &search(Layer::Lexical))?;
    }
    let today = date(2026, 5, 1);
    let mut output = Vec::new();
    report(&root, Some(today..=today), &mut output)?;
    let output = String::from_utf8(output)?;
    assert!(output.lines().any(|line| line == "2026-05-01\tlexical\t1"));
    assert_eq!(
        output
            .lines()
            .filter(|line| line.starts_with("2026-05-02\t"))
            .count(),
        0
    );
    Ok(())
}

#[test]
fn report_overlays_all_months_and_unions_distinct_sessions_and_clients() -> Result<()> {
    let (_directory, root) = fixture()?;
    let read = search(Layer::Lexical);
    event(
        &root,
        "2026-04-30T23:58:00-05:00[-05:00]",
        "agent",
        Some("same-session"),
        &read,
    )?;
    rollup(&root, date(2026, 4, 30))?;
    event(
        &root,
        "2026-05-01T10:00:00+10:00[+10:00]",
        "agent",
        Some("same-session"),
        &read,
    )?;
    event(
        &root,
        "2026-04-30T23:59:00.123456789-05:00[-05:00]",
        "agent",
        Some("same-session"),
        &search(Layer::Partial),
    )?;
    event(
        &root,
        "2026-05-01T10:01:00+10:00[+10:00]",
        "cli",
        None,
        &read,
    )?;
    event(
        &root,
        "2026-05-01T10:02:00+10:00[+10:00]",
        "cli",
        Some("new-session"),
        &ReadEvent::Show {
            note: "kb-0042".into(),
        },
    )?;

    let db_path = root.path().join(DIRECTORY).join("rollup.sqlite");
    let bytes_before = fs::read(&db_path)?;
    let snapshot = capture(&root)?;
    assert_eq!(fs::read(&db_path)?, bytes_before);
    assert_eq!(
        snapshot.lines().next(),
        Some("Usage coverage since 2026-04-30")
    );
    assert_eq!(
        snapshot
            .lines()
            .filter(|line| line.starts_with("kb-0042\t"))
            .collect::<Vec<_>>(),
        [
            "kb-0042\topen\t1\t1\t1\t2026-05-01T00:02:00Z",
            "kb-0042\tsearch\t4\t2\t1\t2026-05-01T04:59:00.123456789Z",
            "kb-0042\tgraph\t4\t2\t1\t2026-05-01T04:59:00.123456789Z",
        ]
    );
    assert_eq!(
        snapshot.lines().rev().take(3).collect::<Vec<_>>(),
        [
            "2026-05-01\tlexical\t2",
            "2026-04-30\tpartial\t1",
            "2026-04-30\tlexical\t1"
        ]
    );
    rollup(&root, date(2026, 5, 1))?;
    rollup(&root, date(2026, 5, 1))?;
    assert_eq!(capture(&root)?, snapshot);
    Ok(())
}

#[test]
fn malformed_rows_are_dropped_once_and_partial_rows_wait_for_a_boundary() -> Result<()> {
    let (_directory, root) = fixture()?;
    let day = date(2026, 5, 1);
    let now = "2026-05-01T10:00:00+00:00[+00:00]";
    let miss = ReadEvent::Search {
        layer: Layer::Miss,
        notes: Vec::new(),
        expanded: Vec::new(),
    };
    event(&root, now, "cli", None, &miss)?;
    let directory = root.path().join(DIRECTORY);
    let path = Month::of(day).path(&directory);
    let oversized = vec![b'x'; raw::MAX_ROW + 2];
    store::append_record(&path, &oversized)?;
    store::append_record(&path, b"\n\xff\n{\"verb\":")?;
    rollup(&root, day)?;
    let before = capture(&root)?;
    assert_eq!(
        before
            .lines()
            .find(|line| line.starts_with("Dropped malformed")),
        Some("Dropped malformed usage rows\t2")
    );
    assert_eq!(before.lines().last(), Some("2026-05-01\tmiss\t1"));

    event(
        &root,
        now,
        "cli",
        Some("session"),
        &ReadEvent::Search {
            layer: Layer::Refs,
            notes: Vec::new(),
            expanded: Vec::new(),
        },
    )?;
    let after = capture(&root)?;
    assert_eq!(
        after
            .lines()
            .find(|line| line.starts_with("Dropped malformed")),
        Some("Dropped malformed usage rows\t3")
    );
    assert_eq!(
        after.lines().rev().take(2).collect::<Vec<_>>(),
        ["2026-05-01\tmiss\t1", "2026-05-01\trefs\t1"]
    );
    rollup(&root, day)?;
    rollup(&root, day)?;
    assert_eq!(capture(&root)?, after);
    Ok(())
}

#[test]
fn report_without_storage_does_not_create_it_and_rollup_marks_zero_activity() -> Result<()> {
    let (_directory, root) = fixture()?;
    assert_eq!(
        capture(&root)?.lines().next(),
        Some("Usage coverage unavailable")
    );
    let directory = root.path().join(DIRECTORY);
    assert!(!directory.exists());
    fs::create_dir(&directory)?;
    assert_eq!(
        capture(&root)?.lines().next(),
        Some("Usage coverage unavailable")
    );
    assert_eq!(fs::read_dir(&directory)?.count(), 0);
    rollup(&root, date(2026, 5, 1))?;
    assert_eq!(
        capture(&root)?.lines().next(),
        Some("Usage coverage since 2026-05-01")
    );
    assert_eq!(fs::read_dir(directory)?.count(), 1);
    Ok(())
}

#[test]
fn failed_rollup_retries_without_duplicate_outcomes_or_exposures() -> Result<()> {
    let (_directory, root) = fixture()?;
    let day = date(2026, 5, 1);
    rollup(&root, day)?;
    event(
        &root,
        "2026-05-01T10:00:00+00:00[+00:00]",
        "cli",
        None,
        &search(Layer::Id),
    )?;
    let expected = capture(&root)?;
    let db = Connection::open(root.path().join(DIRECTORY).join("rollup.sqlite"))?;
    db.execute_batch(
        "CREATE TRIGGER fail_checkpoint BEFORE INSERT ON checkpoint
         BEGIN SELECT RAISE(ABORT, 'test failed checkpoint'); END;",
    )?;
    let error = rollup(&root, day).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<rusqlite::Error>()
            .and_then(rusqlite::Error::sqlite_error_code),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM access", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    assert_eq!(capture(&root)?, expected);

    db.execute_batch("DROP TRIGGER fail_checkpoint")?;
    std::thread::scope(|scope| -> Result<()> {
        let worker = scope.spawn(|| rollup(&root, day));
        let local = rollup(&root, day);
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("rollup worker panicked"))??;
        local
    })?;
    assert_eq!(capture(&root)?, expected);
    Ok(())
}

#[test]
fn expiry_waits_for_the_months_last_day_and_complete_checkpoint() -> Result<()> {
    let (_directory, root) = fixture()?;
    let day = date(2026, 1, 31);
    let read = search(Layer::Semantic);
    let time = "2026-01-31T10:00:00+00:00[+00:00]";
    let directory = root.path().join(DIRECTORY);
    let path = Month::of(day).path(&directory);
    event(&root, time, "cli", None, &read)?;
    rollup(&root, date(2026, 4, 30))?;
    assert!(path.is_file());
    store::append_record(&path, b"\n{\"verb\":")?;
    rollup(&root, date(2026, 4, 30))?;
    assert!(path.is_file());
    assert!(
        capture(&root)?
            .lines()
            .all(|line| !line.starts_with("Dropped malformed"))
    );
    rollup(&root, date(2026, 5, 1))?;
    assert!(!path.exists());
    let expected = capture(&root)?;
    assert!(
        expected
            .lines()
            .any(|line| line == "Dropped malformed usage rows\t1")
    );
    assert_eq!(expected.lines().last(), Some("2026-01-31\tsemantic\t1"));
    rollup(&root, date(2026, 5, 2))?;
    assert_eq!(capture(&root)?, expected);
    assert_eq!(
        event(&root, time, "cli", None, &read)
            .unwrap_err()
            .to_string(),
        "usage month 2026-01 is sealed"
    );
    assert!(!path.exists());
    Ok(())
}

#[test]
fn truncation_and_replacement_show_a_gap_without_recounting() -> Result<()> {
    let (_directory, root) = fixture()?;
    let day = date(2026, 5, 1);
    let time = "2026-05-01T10:00:00+00:00[+00:00]";
    let read = search(Layer::Typo);
    event(&root, time, "cli", None, &read)?;
    rollup(&root, day)?;
    let directory = root.path().join(DIRECTORY);
    let path = Month::of(day).path(&directory);
    let content = fs::read(&path)?;
    fs::write(&path, [])?;
    let expected = capture(&root)?;
    assert_eq!(
        expected
            .lines()
            .find(|line| line.starts_with("Usage coverage gap")),
        Some("Usage coverage gap\t2026-05")
    );
    assert_eq!(expected.lines().last(), Some("2026-05-01\ttypo\t1"));
    rollup(&root, day)?;
    fs::rename(&path, directory.join("old-source"))?;
    fs::write(&path, content.repeat(2))?;
    rollup(&root, day)?;
    assert_eq!(capture(&root)?, expected);
    Ok(())
}
