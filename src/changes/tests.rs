use super::*;
use clap::Parser;
use tempfile::TempDir;

use crate::journal::test_stamp;

#[derive(Parser)]
struct NewCommand {
    #[command(flatten)]
    args: NewArgs,
}

fn root() -> Result<(TempDir, WriteLock)> {
    let dir = tempfile::tempdir()?;
    fs::create_dir(dir.path().join("notes"))?;
    fs::write(dir.path().join("log.md"), b"existing journal\r\n")?;
    let lock = WriteLock::acquire(&KbRoot::open(dir.path())?)?;
    Ok((dir, lock))
}

fn originals(dir: &Path) -> Result<[(PathBuf, Vec<u8>); 2]> {
    let write_note = |id| -> Result<(PathBuf, Vec<u8>)> {
        let path = dir.join("notes").join(format!("{id}-test.md"));
        let bytes = format!("\u{feff}---\r\nid: {id}\r\nrelated: []\r\nsupersedes: []\r\nstatus: draft\r\nupdated: 2025-01-01\r\n---\r\nupdated: body line\r\n").into_bytes();
        fs::write(&path, &bytes)?;
        Ok((path, bytes))
    };
    Ok([write_note("kb-0001")?, write_note("kb-0002")?])
}

#[test]
fn second_replacement_failure_restores_old_bytes_without_journaling() -> Result<()> {
    let (_dir, lock) = root()?;
    let dir = lock.root().path();
    let [(old, before_old), (new, before_new)] = originals(dir)?;
    let log = dir.join("log.md");
    let before_log = fs::read(&log)?;
    let mut attempted = false;
    let result = supersede::with_second_write(
        &lock,
        "kb-0001",
        "kb-0002",
        None,
        &test_stamp(),
        |path, bytes| {
            attempted = true;
            assert_eq!(path, new);
            assert_eq!(
                notes::parse(&fs::read(&old)?)?.scalar("status"),
                Some("superseded")
            );
            assert_eq!(notes::parse(bytes)?.items("supersedes"), ["kb-0001"]);
            // Obstruct this one real atomic_write portably without relying on chmod
            // or timing. Keep NEW's original inode intact; do not rewrite its bytes.
            let backup = dir.join("saved.new");
            fs::rename(path, &backup)?;
            fs::create_dir(path)?;
            let write = store::atomic_write(path, bytes);
            fs::remove_dir(path)?;
            fs::rename(backup, path)?;
            Ok(write?)
        },
    );
    assert!(attempted);
    assert_eq!(
        result.unwrap_err().to_string(),
        format!(
            "NEW {} was not published; OLD {} restored byte-identical",
            new.display(),
            old.display()
        )
    );
    assert_eq!(fs::read(old)?, before_old);
    assert_eq!(fs::read(new)?, before_new);
    assert_eq!(fs::read(log)?, before_log);
    Ok(())
}

#[test]
fn supersede_links_both_notes_and_retains_bom_line_endings_and_body() -> Result<()> {
    let (_dir, lock) = root()?;
    let [(old, _), (new, _)] = originals(lock.root().path())?;
    let event = supersede(&lock, "kb-0001", "kb-0002", None, &test_stamp())?;
    let retired = fs::read_to_string(&old)?;
    assert!(retired.starts_with("\u{feff}---\r\n"));
    assert!(!retired.replace("\r\n", "").contains('\n'));
    assert!(retired.ends_with("---\r\n\r\n> **Superseded by [[kb-0002]] — kept for history, excluded from search.**\r\nupdated: body line\r\n"));
    let successor = notes::parse(&fs::read(&new)?)?;
    assert_eq!(successor.items("supersedes"), ["kb-0001"]);
    assert_eq!(successor.items("related"), ["kb-0001"]);
    assert_eq!(event.stdout, "kb-0001 superseded by kb-0002");
    Ok(())
}

#[test]
fn interrupted_supersede_recovers_original_actor_reason_and_both_notes() -> Result<()> {
    let (dir, lock) = root()?;
    let [(old, _), (new, original_new)] = originals(dir.path())?;
    let stamp = test_stamp();
    let result = std::panic::catch_unwind(|| {
        supersede::with_second_write(
            &lock,
            "kb-0001",
            "kb-0002",
            Some("corrected | documented"),
            &stamp,
            |_, _| panic!("interrupted after OLD publication"),
        )
    });
    assert!(result.is_err());
    assert_eq!(fs::read(&new)?, original_new);
    assert_eq!(
        fs::read(dir.path().join("log.md"))?,
        b"existing journal\r\n"
    );
    drop(lock);

    let root = KbRoot::open(dir.path())?;
    let sweeper = crate::journal::Actor::new("sweep", "different-host", None)?;
    for _ in 0..2 {
        crate::write_cli::sweep(&root, &sweeper, &mut Vec::new())?;
    }
    assert_eq!(
        notes::parse(&fs::read(old)?)?.scalar("status"),
        Some("superseded")
    );
    assert_eq!(
        notes::parse(&fs::read(new)?)?.items("supersedes"),
        ["kb-0001"]
    );
    let log = fs::read_to_string(root.path().join("log.md"))?;
    let headings: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("## ["))
        .collect();
    assert_eq!(
        headings,
        [
            "## [2026-11-18T09:10:11+07:00] supersede | kb-0001 | client=test | host=test-host | session=test-session | superseded by kb-0002 | reason=corrected | documented",
        ]
    );
    assert_eq!(log.matches("<!-- knowbox-audit-v1 ").count(), 2);
    assert!(!root.path().join(".kb-pending-write.json").exists());
    Ok(())
}

#[test]
fn journal_recovery_preserves_a_file_changed_after_interruption() -> Result<()> {
    let (_dir, lock) = root()?;
    let [(path, original), _] = originals(lock.root().path())?;
    let stamp = test_stamp();
    let log = prepare_entry(&lock, &stamp, "update", "kb-0001", "touched")?;
    transaction::stage(
        &lock,
        &log,
        &[FileWrite {
            path: &path,
            before: Some(&original),
            after: b"planned bytes",
        }],
    )?;
    fs::write(&path, "new external content")?;
    let error = transaction::recover(&lock).unwrap_err();
    assert_eq!(
        error.root_cause().to_string(),
        format!("{} changed during an unfinished write", path.display())
    );
    assert_eq!(fs::read(path)?, b"new external content");
    assert_eq!(
        fs::read(lock.root().path().join("log.md"))?,
        b"existing journal\r\n"
    );
    assert!(lock.root().path().join(".kb-pending-write.json").is_file());
    Ok(())
}

#[test]
fn interrupted_compound_operation_recovers_both_note_links_and_raw_review() -> Result<()> {
    for verdict in [false, true] {
        let (dir, lock) = root()?;
        let base = lock.root().path();
        let [(a, before_a), (b, before_b)] = originals(base)?;
        let raw = base.join("raw");
        fs::create_dir(&raw)?;
        let (raw_path, before_raw, after_raw) = if verdict {
            (
                raw.join("conflicts-2026-11-19.md"),
                None,
                "## pair: kb-0001 kb-0002\n> verdict: complementary: adjacent parts\n",
            )
        } else {
            let path = raw.join("proposals.md");
            let before = b"## Proposal\nBody\n".as_slice();
            fs::write(&path, before)?;
            (
                path,
                Some(before),
                "## Proposal\n> state: ingested kb-0001 (2026-11-19)\nBody\n",
            )
        };
        let after_a = rewrite::patch(&before_a, &[("related", "[kb-0002]")], None)?;
        let after_b = rewrite::patch(&before_b, &[("related", "[kb-0001]")], None)?;
        let note_writes = [
            FileWrite {
                path: &a,
                before: Some(&before_a),
                after: &after_a,
            },
            FileWrite {
                path: &b,
                before: Some(&before_b),
                after: &after_b,
            },
        ];
        let stamp = test_stamp();
        let mut log = prepare_note_entry(
            &lock,
            &stamp,
            "link",
            "kb-0001",
            "linked with kb-0002",
            &note_writes,
        )?;
        let action = if verdict { "verdict" } else { "raw-mark" };
        write_entry(&mut log, &stamp, action, "kb-0001", "reviewed", &[])?;
        let request = [
            note_writes[0],
            note_writes[1],
            FileWrite {
                path: &raw_path,
                before: before_raw,
                after: after_raw.as_bytes(),
            },
        ];
        transaction::stage(&lock, &log, &request)?;
        store::atomic_write(&a, &after_a)?;
        assert_eq!(fs::read(&b)?, before_b);
        drop(lock);
        let lock = WriteLock::acquire(&KbRoot::open(dir.path())?)?;
        transaction::recover(&lock)?;
        transaction::recover(&lock)?;
        assert_eq!(fs::read(&a)?, after_a);
        assert_eq!(fs::read(&b)?, after_b);
        assert_eq!(fs::read(&raw_path)?, after_raw.as_bytes());
        assert_eq!(fs::read(dir.path().join("log.md"))?, log);
        assert!(!dir.path().join(".kb-pending-write.json").exists());
    }
    Ok(())
}

#[test]
fn fresh_batch_discards_its_pending_record_if_a_preimage_changed() -> Result<()> {
    let (dir, lock) = root()?;
    let [(a, before_a), (b, before_b)] = originals(lock.root().path())?;
    let stamp = test_stamp();
    let after_a = rewrite::patch(&before_a, &[("related", "[kb-0002]")], None)?;
    let after_b = rewrite::patch(&before_b, &[("related", "[kb-0001]")], None)?;
    let writes = [
        FileWrite {
            path: &a,
            before: Some(&before_a),
            after: &after_a,
        },
        FileWrite {
            path: &b,
            before: Some(&before_b),
            after: &after_b,
        },
    ];
    let log = prepare_note_entry(
        &lock,
        &stamp,
        "link",
        "kb-0001",
        "linked with kb-0002",
        &writes,
    )?;
    fs::write(&b, b"concurrent edit")?;
    let error = transaction::apply(&lock, &log, &writes).unwrap_err();
    let conflict = error
        .downcast_ref::<transaction::ChangedNote>()
        .context("expected a draft conflict")?;
    assert_eq!(conflict.path, "notes/kb-0002-test.md");
    assert_eq!(conflict.current_markdown, "concurrent edit");
    assert_eq!(fs::read(a)?, before_a);
    assert_eq!(fs::read(b)?, b"concurrent edit");
    assert!(!dir.path().join(".kb-pending-write.json").exists());
    assert_eq!(
        fs::read(dir.path().join("log.md"))?,
        b"existing journal\r\n"
    );
    Ok(())
}

#[test]
fn verify_rewrites_only_frontmatter_and_skips_missing() -> Result<()> {
    let (_dir, lock) = root()?;
    let [(old, original), _] = originals(lock.root().path())?;
    let mut out = Vec::new();
    let event = verify(
        &lock,
        &["missing".into(), "kb-0001".into()],
        "proof",
        &test_stamp(),
        &mut out,
    )?
    .unwrap();
    let expected = String::from_utf8(original)?
        .replace("status: draft", "status: verified")
        .replace(
            "updated: 2025-01-01",
            "updated: 2026-11-18\r\nverified: 2026-11-18",
        );
    assert_eq!(fs::read(old)?, expected.as_bytes());
    assert_eq!(out, b"not found: missing\n");
    assert_eq!(event.stdout, "verified: kb-0001");
    Ok(())
}

#[test]
fn new_uses_journal_high_water_and_refuses_exhaustion() -> Result<()> {
    let (_dir, lock) = root()?;
    let path = lock.root().path();
    fs::write(path.join("notes/kb-099.md"), b"preserved")?;
    let log = path.join("log.md");
    fs::write(&log, "## [2026-05-05] create | kb-9998 | gone\n")?;
    let draft = NewCommand::try_parse_from([
        "new",
        "--type",
        "decision",
        "--domain",
        "Example",
        "--title",
        "A choice!",
        "--tags",
        "first,\"comma, inside\"",
        "--body",
        "hello\r\nworld\r\n",
    ])?
    .args;
    let event = create(
        &lock,
        prepare_new(lock.root(), draft, &mut Vec::new())?,
        &test_stamp(),
    )?;
    let bytes = fs::read(event.stdout)?;
    assert_eq!(
        std::str::from_utf8(&bytes)?,
        concat!(
            "---\nid: kb-9999\ntitle: \"A choice!\"\ntype: decision\ndomain: example\n",
            "tags: [first,\"comma, inside\"]\naliases: []\nstatus: active\nconfidence: medium\n",
            "created: 2026-11-18\nupdated: 2026-11-18\nsupersedes: []\nrelated: []\n",
            "source: \"\"\nsummary: \"TODO: what question does this answer?\"\n---\nhello\nworld\n",
        ),
    );
    let note = notes::parse(&bytes)?;
    assert_eq!(note.scalar("id"), Some("kb-9999"));
    assert_eq!(note.items("tags"), ["first", "comma, inside"]);
    assert_eq!(note.body, "hello\nworld\n");
    assert!(!bytes.contains(&b'\r'));
    assert_eq!(
        next_id(lock.root()).unwrap_err().to_string(),
        "note ID space exhausted (maximum kb-9999); refusing to reuse an ID"
    );
    Ok(())
}

#[test]
fn refuses_field_injection_and_never_edits_a_body_key() -> Result<()> {
    let (_dir, lock) = root()?;
    let args = NewCommand::try_parse_from([
        "new",
        "--type",
        "fact",
        "--domain",
        "x",
        "--title",
        "a\r\nstatus: verified",
        "--no-edit",
    ])?
    .args;
    assert_eq!(
        prepare_new(lock.root(), args, &mut Vec::new())
            .unwrap_err()
            .to_string(),
        "title must be a single line without NUL"
    );
    assert_eq!(
        rewrite::edit(
            b"---\ntitle: a\n---\nupdated: body",
            &[("updated", "bad")],
            None,
            None
        )
        .unwrap_err()
        .to_string(),
        "frontmatter has no updated: line; refusing to rewrite"
    );
    Ok(())
}

#[test]
fn invalid_journal_is_detected_before_canonical_mutations() -> Result<()> {
    let (_dir, lock) = root()?;
    let dir = lock.root().path();
    let [(old, before_old), (new, before_new)] = originals(dir)?;
    let log = dir.join("log.md");
    fs::write(&log, [0xff])?;
    let errors = [
        touch(&lock, "kb-0001", &test_stamp()).unwrap_err(),
        verify(
            &lock,
            &["kb-0002".into()],
            "proof",
            &test_stamp(),
            &mut Vec::new(),
        )
        .unwrap_err(),
        supersede(&lock, "kb-0001", "kb-0002", None, &test_stamp()).unwrap_err(),
    ];
    for error in errors {
        assert_eq!(
            error.to_string(),
            format!("reading journal {}", log.display())
        );
    }
    assert_eq!(fs::read(old)?, before_old);
    assert_eq!(fs::read(new)?, before_new);
    assert_eq!(fs::read(log)?, [0xff]);
    Ok(())
}

#[test]
fn interrupted_verify_reports_completed_ids() -> Result<()> {
    let (_dir, lock) = root()?;
    let dir = lock.root().path();
    let [(first, _), (second, _)] = originals(dir)?;
    let invalid_id = b"---\nid: kb-2\nstatus: draft\nupdated: 2025-01-01\n---\n";
    fs::write(&second, invalid_id)?;
    let error = verify(
        &lock,
        &["kb-0001".into(), "kb-0002".into()],
        "proof",
        &test_stamp(),
        &mut Vec::new(),
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "verification interrupted; earlier verified and journaled notes remain on disk: kb-0001"
    );
    assert_eq!(
        notes::parse(&fs::read(first)?)?.scalar("status"),
        Some("verified")
    );
    assert_eq!(fs::read(second)?, invalid_id);
    let log = fs::read_to_string(dir.join("log.md"))?;
    let mut lines = log.lines();
    assert_eq!(lines.next(), Some("existing journal"));
    assert_eq!(
        lines.next(),
        Some(
            "## [2026-11-18T09:10:11+07:00] verify | kb-0001 | client=test | host=test-host | session=test-session | proof"
        )
    );
    assert!(
        lines
            .next()
            .is_some_and(|line| line.starts_with("<!-- knowbox-audit-v1 "))
    );
    assert_eq!(lines.next(), None);
    Ok(())
}
