use super::*;
use crate::config::KbRoot;
use tempfile::TempDir;

const NAME: &str = "notes/kb-0042.md";
const ABC_SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";
const EMPTY_SHA1: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

fn fixture() -> Result<(TempDir, WriteLock)> {
    let dir = tempfile::tempdir()?;
    fs::create_dir(dir.path().join("notes"))?;
    fs::write(
        dir.path().join("log.md"),
        "## [2020-01-01] init | — | old\r\n",
    )?;
    let lock = WriteLock::acquire(&KbRoot::open(dir.path())?)?;
    Ok((dir, lock))
}

fn changes(lock: &WriteLock) -> Result<Vec<Transition>> {
    Ok(Baseline::load(lock)?.drift(&notes::snapshot(lock.root())?))
}

fn journal(lock: &WriteLock, transition: &Transition) -> Result<()> {
    let path = lock.root().path().join("log.md");
    let mut bytes = fs::read(&path)?;
    writeln!(
        bytes,
        "## [2026-11-22T10:00:00Z] update | {} | client=manual | host=test | session= | {}",
        transition.id(),
        transition.description()
    )?;
    transition.write_line(&mut bytes)?;
    store::atomic_write(&path, &bytes)?;
    Ok(())
}

#[test]
fn legacy_seed_from_index_keeps_pre_upgrade_edits_across_an_unrelated_rebuild() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let note = root.join(NAME);
    fs::write(&note, "abc")?;
    crate::index::rebuild(&lock)?;
    fs::write(&note, "")?;

    let expected = [Transition {
        path: NAME.to_owned(),
        before: Some(Fingerprint {
            id: Some("kb-0042".into()),
            sha1: ABC_SHA1.into(),
            lf_sha1: None,
        }),
        after: Some(Fingerprint {
            id: Some("kb-0042".into()),
            sha1: EMPTY_SHA1.into(),
            lf_sha1: None,
        }),
    }];
    let log = fs::read(root.join("log.md"))?;
    assert_eq!(changes(&lock)?, expected);
    assert_eq!(fs::read(root.join("log.md"))?, log);
    let seed = fs::read(root.join(SEED))?;
    crate::index::rebuild(&lock)?;
    assert_eq!(changes(&lock)?, expected);
    assert_eq!(fs::read(root.join(SEED))?, seed);
    journal(&lock, &expected[0])?;
    assert_eq!(changes(&lock)?, []);
    Ok(())
}

#[test]
fn fallback_seed_preserves_paths_and_ids_across_git_newline_conversion() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let odd = root.join("notes/not-an-id.md");
    let invalid = root.join("notes/kb-0099-bad.md");
    fs::write(&odd, b"---\r\nid: kb-0023\r\n---\r\nbody\r\n")?;
    fs::write(&invalid, [0xff])?;
    fs::write(root.join(".kb.db"), "corrupt")?;
    let old_log = fs::read(root.join("log.md"))?;
    assert_eq!(changes(&lock)?, []);
    assert_eq!(fs::read(root.join("log.md"))?, old_log);
    fs::write(&odd, b"---\nid: kb-0023\n---\nbody\n")?;
    assert_eq!(changes(&lock)?, []);
    fs::write(&odd, b"---\nid: kb-0023\n---\nchanged body\n")?;
    fs::remove_file(&invalid)?;
    let rows = changes(&lock)?;
    let ids: Vec<_> = rows.iter().map(Transition::id).collect();
    assert_eq!(ids, ["kb-0099", "kb-0023"]);
    assert_eq!(rows[0].description(), "removed \"notes/kb-0099-bad.md\"");
    assert_eq!(rows[1].description(), "edited \"notes/not-an-id.md\"");
    for row in rows {
        journal(&lock, &row)?;
    }
    assert_eq!(changes(&lock)?, []);
    Ok(())
}

#[test]
fn each_observed_transition_is_recorded_once_even_when_a_hash_or_path_repeats() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let path = root.join(NAME);
    assert_eq!(changes(&lock)?, []);
    for (bytes, kind) in [
        (Some("abc"), "added"),
        (Some(""), "edited"),
        (Some("abc"), "edited"),
        (None, "removed"),
        (Some("abc"), "added"),
    ] {
        match bytes {
            Some(bytes) => fs::write(&path, bytes)?,
            None => fs::remove_file(&path)?,
        }
        let drift = changes(&lock)?;
        let [row] = drift.as_slice() else {
            anyhow::bail!("expected one transition, got {drift:?}")
        };
        assert_eq!(row.description(), format!("{kind} \"{NAME}\""));
        assert_eq!(row.id(), "kb-0042");
        journal(&lock, row)?;
        assert_eq!(changes(&lock)?, []);
    }
    assert_eq!(
        fs::read_to_string(root.join("log.md"))?
            .lines()
            .filter(|l| l.starts_with(PREFIX))
            .count(),
        5
    );
    Ok(())
}

#[test]
fn preimage_reports_only_the_requested_path_and_survives_a_failed_rebuild() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let path = root.join(NAME);
    let unrelated = root.join("notes/kb-0023.md");
    fs::write(&path, "abc")?;
    fs::write(&unrelated, "abc")?;
    let baseline = Baseline::load(&lock)?;
    fs::write(&path, "preimage")?;
    fs::write(&unrelated, "unrelated")?;
    let preimage = baseline
        .preimage(&path, Some(b"preimage"))?
        .context("missing manual preimage")?;
    journal(&lock, &preimage)?;
    fs::write(&path, "postimage")?;
    journal(
        &lock,
        &Transition::between(&path, Some(b"preimage"), Some(b"postimage"))?,
    )?;

    fs::create_dir(root.join("index.md"))?;
    let err = crate::index::rebuild(&lock).unwrap_err();
    assert!(
        matches!(err, crate::index::IndexError::Io(e) if e.kind() == std::io::ErrorKind::InvalidInput)
    );
    fs::remove_dir(root.join("index.md"))?;
    crate::index::rebuild(&lock)?;
    let remaining = changes(&lock)?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id(), "kb-0023");
    assert_eq!(
        Baseline::load(&lock)?.preimage(&path, Some(b"postimage"))?,
        None
    );
    Ok(())
}

#[test]
fn replay_recognizes_a_synced_journal_on_a_host_without_its_original_seed() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let path = root.join(NAME);
    fs::write(&path, "abc")?;
    journal(&lock, &Transition::between(&path, None, Some(b""))?)?;
    journal(&lock, &Transition::between(&path, Some(b""), Some(b"abc"))?)?;
    assert_eq!(changes(&lock)?, []);
    fs::write(&path, "local")?;
    let expected = Transition::between(&path, Some(b"abc"), Some(b"local"))?;
    assert_eq!(changes(&lock)?, [expected]);
    Ok(())
}

#[test]
fn conflicting_synced_predecessor_is_reported_before_seeding_or_rewriting() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let path = root.join(NAME);
    fs::write(&path, "abc")?;
    journal(&lock, &Transition::between(&path, None, Some(b"abc"))?)?;
    journal(
        &lock,
        &Transition::between(&path, Some(b"different"), Some(b""))?,
    )?;
    let old_log = fs::read(root.join("log.md"))?;
    let err = Baseline::load(&lock).unwrap_err();
    assert_eq!(
        err.to_string(),
        "conflicting audit predecessor for \"notes/kb-0042.md\" at log.md line 5"
    );
    assert!(!root.join(SEED).exists());
    assert_eq!(fs::read(root.join("log.md"))?, old_log);
    assert_eq!(fs::read(&path)?, b"abc");
    Ok(())
}

#[test]
fn encoded_transition_round_trips_paths_which_would_end_an_html_comment() -> Result<()> {
    let mut state = Baseline {
        notes: States::new(),
    };
    for name in [
        "kb-0001--data.md",
        "-----x.md",
        "-->&copy;.md",
        "one\ntwo.md",
        "kb-0005-é.md",
    ] {
        let path = format!("notes/{name}");
        // Build the serialized path directly so this is also portable to Windows.
        let row = Transition {
            path: path.clone(),
            before: None,
            after: Some(Fingerprint {
                id: None,
                sha1: ABC_SHA1.into(),
                lf_sha1: None,
            }),
        };
        let mut line = Vec::new();
        row.write_line(&mut line)?;
        let text = std::str::from_utf8(&line)?;
        let inside = text
            .strip_prefix(PREFIX)
            .and_then(|s| s.strip_suffix(" -->\n"))
            .context("invalid framing")?;
        assert!(!inside.contains("--"));
        assert_eq!(text.lines().count(), 1);
        assert_eq!(serde_json::from_str::<Transition>(inside)?, row);
        state.replay(&format!(
            "## [2026-11-22T01:00:00Z] update | — | example\r\n{}",
            text.replace('\n', "\r\n")
        ))?;
        assert_eq!(state.notes.get(&path), row.after.as_ref());
    }
    Ok(())
}

#[test]
fn old_metadata_is_usable_even_when_the_fts_schema_needs_rebuilding() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let path = root.join(NAME);
    fs::write(&path, "abc")?;
    crate::index::rebuild(&lock)?;
    let db = rusqlite::Connection::open(root.join(".kb.db"))?;
    db.execute("DROP TABLE kb_d", [])?;
    drop(db);
    fs::remove_file(&path)?;
    let expected = Transition::between(&path, Some(b"abc"), None)?;
    assert_eq!(changes(&lock)?, [expected]);
    Ok(())
}

#[test]
fn separate_paths_retain_independent_lifetimes_even_with_the_same_id_and_content() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let first = root.join("notes/kb-0001-before.md");
    let second = root.join("notes/kb-0001-copy.md");
    let renamed = root.join("notes/kb-0001-after.md");
    fs::write(&first, "abc")?;
    fs::write(&second, "abc")?;
    assert_eq!(changes(&lock)?, []);

    fs::rename(&first, &renamed)?;
    let drift = changes(&lock)?;
    let expected = [
        Transition::between(&renamed, None, Some(b"abc"))?,
        Transition::between(&first, Some(b"abc"), None)?,
    ];
    assert_eq!(drift, expected);
    for row in &drift {
        journal(&lock, row)?;
    }
    assert_eq!(changes(&lock)?, []);
    Ok(())
}

#[cfg(unix)]
#[test]
fn redirected_seed_cannot_import_or_overwrite_another_roots_state() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let external = tempfile::tempdir()?;
    let canary = external.path().join("other-seed");
    fs::write(&canary, b"outside")?;
    std::os::unix::fs::symlink(&canary, root.join(SEED))?;
    let error = Baseline::load(&lock).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(fs::read(canary)?, b"outside");
    Ok(())
}

#[test]
fn return_to_old_indexed_bytes_does_not_fabricate_an_event_for_an_inferred_id() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let path = lock.root().path().join("notes/shared.md");
    let original = b"---\nid: kb-0009\n---\n";
    fs::write(&path, original)?;
    crate::index::rebuild(&lock)?;
    fs::write(&path, "changed before upgrade")?;
    let baseline = Baseline::load(&lock)?;
    fs::write(&path, original)?;
    assert_eq!(baseline.drift(&notes::snapshot(lock.root())?), []);
    assert_eq!(baseline.preimage(&path, Some(original))?, None);
    Ok(())
}

#[test]
fn comments_in_legacy_journal_prose_do_not_override_note_state() -> Result<()> {
    let (_dir, lock) = fixture()?;
    let root = lock.root().path();
    let note = root.join(NAME);
    fs::write(&note, "abc")?;
    let mut prose = b"## [2020-01-01] verify | kb-0042 | legacy evidence\n".to_vec();
    Transition::between(&note, Some(b"abc"), None)?.write_line(&mut prose)?;
    prose.extend_from_slice(
        b"## [2026-11-22T01:00:00Z] update | kb-0042 | newer entry\n\nAn example follows.\n",
    );
    Transition::between(&note, Some(b"abc"), None)?.write_line(&mut prose)?;
    fs::write(root.join("log.md"), prose)?;
    assert_eq!(changes(&lock)?, []);
    Ok(())
}
