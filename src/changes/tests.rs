use super::*;
use clap::Parser;
use tempfile::TempDir;

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

fn date() -> Date {
    Date::new(2026, 11, 18).unwrap()
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
    let result =
        supersede::with_second_write(&lock, "kb-0001", "kb-0002", date(), |path, bytes| {
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
        });
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
    let event = supersede(&lock, "kb-0001", "kb-0002", date())?;
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
fn verify_rewrites_only_frontmatter_and_skips_missing() -> Result<()> {
    let (_dir, lock) = root()?;
    let [(old, original), _] = originals(lock.root().path())?;
    let mut out = Vec::new();
    let event = verify(
        &lock,
        &["missing".into(), "kb-0001".into()],
        "proof",
        date(),
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
    let event = create(&lock, prepare_new(draft, &mut Vec::new())?, date())?;
    let bytes = fs::read(event.stdout)?;
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
        prepare_new(args, &mut Vec::new()).unwrap_err().to_string(),
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
        touch(&lock, "kb-0001", date()).unwrap_err(),
        verify(&lock, &["kb-0002".into()], "proof", date(), &mut Vec::new()).unwrap_err(),
        supersede(&lock, "kb-0001", "kb-0002", date()).unwrap_err(),
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
        date(),
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
    assert_eq!(
        fs::read(dir.join("log.md"))?,
        b"existing journal\r\n## [2026-11-18] verify | kb-0001 | proof\n"
    );
    Ok(())
}
