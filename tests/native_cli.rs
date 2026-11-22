use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

use tempfile::{TempDir, tempdir, tempfile_in};

#[cfg(windows)]
use rusqlite::{Connection, OpenFlags};

const STALE: &str =
    "(index was stale — searched a temporary snapshot. Run 'knowbox reindex' to refresh.)\n";
const MISS: &str = "no matches. try: a synonym | broader term | `knowbox list`\n";
const WAL_NOTE: &str = "notes/kb-0001-wal-mode-journal.md";

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus-v1")
}

fn golden_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn sandbox() -> io::Result<TempDir> {
    let sandbox = tempdir()?;
    fs::create_dir(sandbox.path().join("home"))?;
    Ok(sandbox)
}

fn command(sandbox: &TempDir) -> Command {
    let home = sandbox.path().join("home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_knowbox"));
    command
        .env_clear()
        .current_dir(sandbox.path())
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("KNOWBOX_HOST", "test-host")
        .stdin(Stdio::null());
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    command
}

fn run(sandbox: &TempDir, root: &Path, args: &[&str]) -> io::Result<Output> {
    command(sandbox)
        .env("KNOWBOX_ROOT", root)
        .args(args)
        .output()
}

fn init(sandbox: &TempDir) -> Command {
    let mut command = command(sandbox);
    command.args(["init", "--no-git"]);
    command
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn note_ids(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with("kb-"))
        .filter_map(|line| {
            let token = line.split_whitespace().next()?;
            let digits = token.strip_prefix("kb-")?;
            (digits.len() == 4 && digits.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| token.to_owned())
        })
        .collect()
}

fn init_target(sandbox: &TempDir, name: &str) -> io::Result<PathBuf> {
    let target = sandbox.path().join(name);
    let output = init(sandbox).arg(&target).output()?;
    assert_success(&output, "init");
    Ok(target)
}

#[test]
fn cli_reports_a_major_minor_release_version() -> io::Result<()> {
    let sandbox = sandbox()?;
    let version = env!("CARGO_PKG_VERSION")
        .strip_suffix(".0")
        .ok_or_else(|| io::Error::other("Cargo package version must be major.minor.0"))?;
    for (arg, expected) in [
        ("version", format!("{version}\n")),
        ("--version", format!("knowbox {version}\n")),
        ("-V", format!("knowbox {version}\n")),
    ] {
        let output = command(&sandbox).arg(arg).output()?;
        assert_success(&output, arg);
        assert_eq!(output.stdout, expected.as_bytes());
        assert_eq!(output.stderr, b"");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn interactive_new(sandbox: &TempDir, root: &Path, script: &str) -> io::Result<Output> {
    use std::os::unix::fs::PermissionsExt;

    use rustix::pty::{OpenptFlags, ioctl_tiocgptpeer, openpt, unlockpt};

    let editor = sandbox.path().join("test editor");
    fs::write(&editor, format!("#!/bin/sh\nset -eu\n{script}\n"))?;
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700))?;
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
    unlockpt(&master)?;
    let terminal = ioctl_tiocgptpeer(
        &master,
        OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
    )?;
    CapturedChild::spawn(
        sandbox,
        git_env(&mut command(sandbox))
            .env("EDITOR", editor)
            .env("KNOWBOX_ROOT", root)
            .env("KNOWBOX_LOCK_TIMEOUT", "0.2")
            .env("TMPDIR", sandbox.path())
            .env("KB_TEST_KNOWBOX", env!("CARGO_BIN_EXE_knowbox"))
            .stdin(Stdio::from(terminal))
            .args([
                "new",
                "--type",
                "fact",
                "--domain",
                "test",
                "--title",
                "Original title",
            ]),
    )?
    .wait()
}

#[cfg(target_os = "linux")]
#[test]
fn interactive_new_preserves_edited_fields_and_allocates_id_after_editor() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    let output = interactive_new(
        &sandbox,
        &root,
        r#"
        grep -qx 'id: pending' "$1"
        grep -qx 'title: "Original title"' "$1"
        case "$1" in "$KNOWBOX_ROOT"/*) exit 41 ;; esac
        "$KB_TEST_KNOWBOX" new --type fact --domain test --title 'Concurrent note' --body 'created in the editor session' --no-edit >/dev/null
        cat > "$1" <<'NOTE'
---
id: pending
title: 'Edited Title'
type: howto
domain: editing
# preserve this comment
tags: [alpha, "two, words"]
aliases: []
triggers:
  - Editing a new note
status: draft
confidence: high
created: pending
updated: pending
supersedes: []
related: [kb-0001]
source: 'session'
summary: 'A new edited summary'
---

Graphemes stay in the edited body.
NOTE
    "#,
    )?;
    assert_success(&output, "interactive new");
    let saved = root.join("notes/kb-0002-edited-title.md");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        saved.canonicalize()?.display().to_string()
    );
    let text = fs::read_to_string(&saved)?;
    let mut dates = text.lines().filter_map(|line| {
        line.strip_prefix("created: ")
            .or_else(|| line.strip_prefix("updated: "))
    });
    let date = dates
        .next()
        .ok_or_else(|| io::Error::other("missing created date"))?;
    date.parse::<jiff::civil::Date>()
        .map_err(io::Error::other)?;
    assert_eq!(dates.next(), Some(date));
    assert_eq!(dates.next(), None);
    assert!(
        text.starts_with("---\nid: kb-0002\ntitle: 'Edited Title'\ntype: howto\ndomain: editing\n")
    );
    assert!(text.contains("# preserve this comment\ntags: [alpha, \"two, words\"]\n"));
    assert!(text.contains("summary: 'A new edited summary'\n"));
    assert!(text.ends_with("---\n\nGraphemes stay in the edited body.\n"));
    assert!(
        fs::read_to_string(root.join("log.md"))?
            .lines()
            .any(|line| line.ends_with(
                "create | kb-0002 | client=cli | host=test-host | session=— | Edited Title"
            ))
    );
    assert_eq!(
        git(&sandbox, &root, &["log", "-1", "--format=%s"])?,
        "knowbox: create kb-0002 — Edited Title\n"
    );
    let search = run(&sandbox, &root, &["search", "graphemes"])?;
    assert_success(&search, "search after editor save");
    assert_eq!(note_ids(&search), ["kb-0002"]);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn interactive_new_refuses_aborted_or_invalid_drafts_without_writing() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "cancelled drafts")?;
    let artifacts = [".kb.db", "index.md", "log.md"].map(|name| root.join(name));
    let before: Vec<Vec<u8>> = artifacts.iter().map(fs::read).collect::<io::Result<_>>()?;
    let count = fs::read_dir(root.join("notes"))?.count();
    for (script, message, retained) in [
        ("exit 42", "editor exited", None),
        (
            "printf '%s\\n' '---' 'title: unterminated' > \"$1\"",
            "unterminated frontmatter",
            Some(("note.md", b"---\ntitle: unterminated\n".as_slice())),
        ),
        (
            r#"printf 'Retained backup.\n' > "$1~"; rm -- "$1""#,
            "editor removed the note draft",
            Some(("note.md~", b"Retained backup.\n".as_slice())),
        ),
    ] {
        let output = interactive_new(&sandbox, &root, script)?;
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, b"");
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        assert_eq!(fs::read_dir(root.join("notes"))?.count(), count);
        for (artifact, bytes) in artifacts.iter().zip(&before) {
            assert_eq!(fs::read(artifact)?, *bytes);
        }
        let drafts = fs::read_dir(sandbox.path())?
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b"knowbox-edit-")
            })
            .collect::<Vec<_>>();
        match (retained, drafts.as_slice()) {
            (Some((name, bytes)), [entry]) => {
                let draft = entry.path().join(name).canonicalize()?;
                assert_eq!(fs::read(&draft)?, bytes);
                let reported_path = if name == "note.md" {
                    draft
                } else {
                    entry.path().canonicalize()?
                };
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains(&format!("{reported_path:?}"))
                );
                fs::remove_dir_all(entry.path())?;
            }
            (None, []) => {}
            _ => return Err(io::Error::other("unexpected draft retention")),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn interactive_new_keeps_the_draft_when_a_writer_blocks_publication() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "held writer")?;
    let log = fs::read(root.join("log.md"))?;
    let lock = OpenOptions::new().write(true).open(root.join(".kb.lock"))?;
    lock.lock()?;
    let output = interactive_new(&sandbox, &root, r#"printf '\nRecovery canary.\n' >> "$1""#)?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("writer lock held by another writer"));
    assert!(message.contains("edited note draft kept at"));
    let drafts: Vec<_> = fs::read_dir(sandbox.path())?
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .as_encoded_bytes()
                .starts_with(b"knowbox-edit-")
        })
        .collect();
    let [draft] = drafts.as_slice() else {
        return Err(io::Error::other("expected one retained draft"));
    };
    let path = draft.path().join("note.md").canonicalize()?;
    assert!(fs::read_to_string(&path)?.ends_with("\nRecovery canary.\n"));
    assert!(message.contains(&format!("{path:?}")));
    assert_eq!(fs::read(root.join("log.md"))?, log);
    assert!(!root.join("notes/kb-0001-original-title.md").exists());
    Ok(())
}

fn install_frozen_corpus(target: &Path) -> io::Result<()> {
    copy_tree(&fixture_root(), target)
}

fn git_env(command: &mut Command) -> &mut Command {
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_ALLOW_PROTOCOL", "file")
}

fn git(sandbox: &TempDir, root: &Path, args: &[&str]) -> io::Result<String> {
    let home = sandbox.path().join("home");
    let path =
        std::env::var_os("PATH").ok_or_else(|| io::Error::other("Git tests require PATH"))?;
    let mut command = Command::new("git");
    command
        .env_clear()
        .env("PATH", path)
        .env("HOME", &home)
        .env("USERPROFILE", home)
        .current_dir(root)
        .stdin(Stdio::null());
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    let output = git_env(&mut command).args(args).output()?;
    assert_success(&output, &format!("git {args:?}"));
    String::from_utf8(output.stdout).map_err(io::Error::other)
}

fn git_kb(sandbox: &TempDir) -> io::Result<PathBuf> {
    let target = sandbox.path().join("git KB # Δ");
    let output = git_env(&mut command(sandbox))
        .arg("init")
        .arg(&target)
        .output()?;
    assert_success(&output, "git-backed init");
    Ok(target)
}

fn git_backed(sandbox: &TempDir, root: &Path, args: &[&str]) -> io::Result<Output> {
    git_env(&mut command(sandbox))
        .env("KNOWBOX_ROOT", root)
        .args(args)
        .output()
}

fn history_json(
    sandbox: &TempDir,
    root: &Path,
    args: &[&str],
) -> anyhow::Result<serde_json::Value> {
    let output = git_backed(sandbox, root, args)?;
    assert_success(&output, &format!("{args:?}"));
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn history_note(sandbox: &TempDir, root: &Path) -> io::Result<()> {
    let output = git_backed(
        sandbox,
        root,
        &[
            "new",
            "--type",
            "fact",
            "--domain",
            "testing",
            "--title",
            "History fixture",
            "--body",
            "original",
            "--no-edit",
            "--client",
            "creator",
        ],
    )?;
    assert_success(&output, "create history fixture");
    Ok(())
}

#[test]
fn edit_and_draft_lint_use_the_current_note_and_preserve_unmodified_body_bytes()
-> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let note = root.join("notes/kb-0001-history-fixture.md");
    let original = fs::read_to_string(&note)?.replace('\n', "\r\n");
    fs::write(&note, &original)?;
    let report = history_json(
        &sandbox,
        &root,
        &["--no-write", "show", "kb-0001", "--json"],
    )?;
    let hash = report["base_sha1"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing base SHA-1"))?;
    let edited = git_backed(
        &sandbox,
        &root,
        &[
            "edit",
            "kb-0001",
            "--set",
            "triggers=[when editing]",
            "--base-sha1",
            hash,
        ],
    )?;
    assert_success(&edited, "edit triggers");
    let current = fs::read_to_string(&note)?;
    assert!(current.contains("triggers: [when editing]\r\n"));
    assert!(current.ends_with("---\r\noriginal\r\n"));
    assert_eq!(current.replace("\r\n", "").matches('\n').count(), 0);
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let stale = git_backed(
        &sandbox,
        &root,
        &[
            "edit",
            "kb-0001",
            "--base-sha1",
            hash,
            "--set",
            "summary=outdated",
        ],
    )?;
    assert_eq!(stale.status.code(), Some(1));
    assert!(String::from_utf8(stale.stderr)?.contains("changed since it was opened"));
    let report = history_json(
        &sandbox,
        &root,
        &[
            "lint",
            "--file",
            note.to_str()
                .ok_or_else(|| io::Error::other("fixture path"))?,
            "--against",
            "kb-0001",
            "--json",
        ],
    )?;
    assert_eq!(report["errors"], 0);
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, head);
    assert_eq!(fs::read_to_string(&note)?, current);
    Ok(())
}

#[test]
fn edit_allows_shrinking_an_existing_overage_and_rejects_invalid_scalar_fields()
-> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "legacy drafting")?;
    let oversized = "word ".repeat(700);
    let create = [
        "new",
        "--type",
        "fact",
        "--domain",
        "testing",
        "--title",
        "Oversized note",
        "--body",
        &oversized,
        "--no-edit",
        "--request-id",
        "oversized-once",
    ];
    assert_success(
        &run(&sandbox, &root, &create)?,
        "request key preserves legacy create acceptance",
    );
    let mut child = command(&sandbox)
        .env("KNOWBOX_ROOT", &root)
        .args(["edit", "kb-0001", "--body-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let shorter = "word ".repeat(650);
    child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("stdin unavailable"))?
        .write_all(shorter.as_bytes())?;
    assert_success(
        &child.wait_with_output()?,
        "shorten an existing lint overage",
    );
    let invalid = run(
        &sandbox,
        &root,
        &["edit", "kb-0001", "--set", "domain=[testing]"],
    )?;
    assert_eq!(invalid.status.code(), Some(1));
    assert!(String::from_utf8(invalid.stderr)?.contains("domain must be a scalar"));
    assert!(fs::read_to_string(root.join("notes/kb-0001-oversized-note.md"))?.ends_with(&shorter));
    Ok(())
}

#[test]
fn verdict_uses_one_transaction_to_link_notes_and_has_one_history_event_per_note()
-> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let second = git_backed(
        &sandbox,
        &root,
        &[
            "new",
            "--type",
            "fact",
            "--domain",
            "testing",
            "--title",
            "Another fixture",
            "--body",
            "The related claim.",
            "--no-edit",
        ],
    )?;
    assert_success(&second, "second note");
    let verdict = [
        "verdict",
        "kb-0002",
        "kb-0001",
        "--verdict",
        "complementary",
        "--reason",
        "both sides | same process",
        "--link",
        "--request-id",
        "pair-once",
    ];
    assert_success(&git_backed(&sandbox, &root, &verdict)?, "record verdict");
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    assert_success(&git_backed(&sandbox, &root, &verdict)?, "replay verdict");
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, head);
    for (id, file, peer) in [
        ("kb-0001", "notes/kb-0001-history-fixture.md", "kb-0002"),
        ("kb-0002", "notes/kb-0002-another-fixture.md", "kb-0001"),
    ] {
        assert!(fs::read_to_string(root.join(file))?.contains(&format!("related: [{peer}]")));
        let history = history_json(&sandbox, &root, &["history", id, "--json"])?;
        let events = history["events"]
            .as_array()
            .ok_or_else(|| io::Error::other("events missing"))?;
        let verdicts: Vec<_> = events
            .iter()
            .filter(|item| item["action"] == "verdict")
            .collect();
        assert_eq!(verdicts.len(), 1);
        assert_eq!(
            verdicts[0]["ids"],
            serde_json::json!(["kb-0001", "kb-0002"])
        );
        assert_eq!(verdicts[0]["raw"]["marker"], "complementary");
        assert_eq!(verdicts[0]["reason"], "both sides | same process");
    }
    Ok(())
}

#[test]
fn replay_repairs_an_interrupted_index_even_if_a_later_raw_write_committed_the_receipt()
-> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let wal = root.join(".kb.db-wal");
    fs::write(&wal, "unavailable writer sidecar")?;
    let edit = [
        "edit",
        "kb-0001",
        "--set",
        "summary=interrupteduniqueindexword",
        "--request-id",
        "index-once",
    ];
    let failure = git_backed(&sandbox, &root, &edit)?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stderr)?.contains("rebuilding the index did not complete"));
    fs::remove_file(wal)?;
    fs::write(root.join("raw/review.md"), "## Proposal\nReviewable text\n")?;
    let marked = git_backed(
        &sandbox,
        &root,
        &["raw-mark", "review.md", "1", "--rejected", "fixture"],
    );
    assert_success(&marked?, "commit via raw review");
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let show = git(&sandbox, &root, &["show", "HEAD:log.md"])?;
    assert!(show.contains("<!-- knowbox-write-v1 index-once "));
    assert_success(&git_backed(&sandbox, &root, &edit)?, "retry index rebuild");
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, head);
    let query = [
        "--no-write",
        "search",
        "--local",
        "interrupteduniqueindexword",
    ];
    let found = git_backed(&sandbox, &root, &query)?;
    assert_success(&found, "search rebuilt index");
    assert!(String::from_utf8(found.stdout)?.starts_with("kb-0001 "));
    Ok(())
}

#[cfg(unix)]
#[test]
fn interrupted_git_commit_resumes_related_creation_without_a_second_id() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let hooks = sandbox.path().join("hooks");
    fs::create_dir(&hooks)?;
    let hook = hooks.join("pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 17\n")?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700))?;
    git(
        &sandbox,
        &root,
        &[
            "config",
            "core.hooksPath",
            hooks
                .to_str()
                .ok_or_else(|| io::Error::other("fixture path"))?,
        ],
    )?;
    let request = [
        "new",
        "--type",
        "fact",
        "--domain",
        "test",
        "--title",
        "Atomic followup",
        "--body",
        "Final content.",
        "--related",
        "kb-0001",
        "--request-id",
        "new-once",
        "--no-edit",
    ];
    let failure = git_backed(&sandbox, &root, &request)?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stderr)?.contains("data changes are on disk"));
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, head);
    let path = root.join("notes/kb-0002-atomic-followup.md");
    assert!(fs::read_to_string(&path)?.contains("related: [kb-0001]"));
    assert!(
        fs::read_to_string(root.join("notes/kb-0001-history-fixture.md"))?
            .contains("related: [kb-0002]")
    );
    fs::remove_file(hook)?;
    assert_success(
        &git_backed(&sandbox, &root, &request)?,
        "finish failed Git operation",
    );
    assert_eq!(
        git(&sandbox, &root, &["rev-list", "--count", "HEAD"])?.trim(),
        "3"
    );
    let next = git_backed(&sandbox, &root, &["next-id"])?;
    assert_success(&next, "next available ID");
    assert_eq!(next.stdout, b"kb-0003\n");
    assert_success(
        &git_backed(&sandbox, &root, &request)?,
        "replay published result",
    );
    assert_eq!(
        git(&sandbox, &root, &["rev-list", "--count", "HEAD"])?.trim(),
        "3"
    );
    assert_eq!(
        fs::read_to_string(root.join("log.md"))?
            .matches(" create | kb-0002 | ")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn history_shares_one_sweep_patch_between_audit_transitions() -> anyhow::Result<()> {
    const NOTE: &str = "notes/kb-0001-history-fixture.md";
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let original = fs::read_to_string(root.join(NOTE))?;
    fs::write(
        root.join(NOTE),
        original
            .replace("\noriginal\n", "\nintermediate\n")
            .replace('\n', "\r\n"),
    )?;
    assert_success(
        &git_backed(&sandbox, &root, &["touch", "kb-0001", "--client", "editor"])?,
        "journal the intermediate content before sweep",
    );
    fs::write(
        root.join(NOTE),
        original
            .replace("\noriginal\n", "\nfinal edit\n")
            .replace('\n', "\r\n"),
    )?;
    let pending = history_json(
        &sandbox,
        &root,
        &["history", "kb-0001", "--json", "--no-write"],
    )?;
    let pending = pending["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    assert!(pending.iter().any(|event| event["action"] == "edit"
        && event["publication"] == "unpublished"
        && event["patch_refs"] == serde_json::json!([])));

    assert_success(
        &git_backed(&sandbox, &root, &["sweep", "--client", "publisher"])?,
        "publish both edits",
    );
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let args = ["history", "kb-0001", "--json", "--no-write"];
    let report = history_json(&sandbox, &root, &args)?;
    assert_eq!(report, history_json(&sandbox, &root, &args)?);
    let events = report["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    let edits: Vec<_> = events
        .iter()
        .filter(|event| event["action"] == "edit")
        .collect();
    assert_eq!(edits.len(), 2);
    for event in edits {
        assert_eq!(event["source"], "journal");
        assert_eq!(event["client"], "manual");
        assert_eq!(event["published_in"]["oid"], head.trim());
        assert_eq!(event["published_in"]["client"], "publisher");
        assert_eq!(event["patch_refs"], serde_json::json!([1]));
    }
    let patches = report["patches"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing patches"))?;
    assert_eq!(patches.len(), 2);
    let patch = patches[1]["text"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing patch text"))?;
    assert!(patch.lines().any(|line| line == "-original"));
    assert!(patch.lines().any(|line| line == "+final edit"));
    assert!(!patch.contains("intermediate"));
    Ok(())
}

#[test]
fn history_does_not_attribute_a_published_patch_to_an_unpublished_replay() -> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    assert_success(
        &git_backed(
            &sandbox,
            &root,
            &["verify", "kb-0001", "--evidence", "verified fixture"],
        )?,
        "verify and publish",
    );
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let args = ["history", "kb-0001", "--json", "--no-write"];
    let original = history_json(&sandbox, &root, &args)?;
    let transition = &original["events"][0]["transitions"][0];
    let undo = serde_json::json!({
        "path": transition["path"],
        "before": transition["after"],
        "after": transition["before"],
    });
    let log = root.join("log.md");
    let mut out = OpenOptions::new().append(true).open(&log)?;
    for (second, detail, transition) in [(0, "undo", &undo), (1, "reapply", transition)] {
        writeln!(
            out,
            "## [2099-01-01T00:00:0{second}+00:00] edit | kb-0001 | client=manual | host=test-host | session=— | {detail}\n<!-- knowbox-audit-v1 {transition} -->",
        )?;
    }
    let report = history_json(&sandbox, &root, &args)?;
    let events = report["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    for (index, detail) in ["reapply", "undo"].into_iter().enumerate() {
        assert_eq!(events[index]["detail"], detail);
        assert_eq!(events[index]["publication"], "unpublished");
        assert_eq!(events[index]["published_in"], serde_json::Value::Null);
        assert_eq!(events[index]["patch_refs"], serde_json::json!([]));
    }
    assert_eq!(events[2]["action"], "verify");
    assert_eq!(events[2]["published_in"]["oid"], head.trim());
    assert_eq!(events[2]["patch_refs"], serde_json::json!([1]));
    Ok(())
}

#[test]
fn history_separates_earlier_content_commit_from_journal_publisher() -> anyhow::Result<()> {
    const NOTE: &str = "notes/kb-0001-history-fixture.md";
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let original = fs::read_to_string(root.join(NOTE))?;
    fs::write(
        root.join(NOTE),
        original.replace("\noriginal\n", "\npublished first\n"),
    )?;
    git(&sandbox, &root, &["add", "--", NOTE])?;
    git(
        &sandbox,
        &root,
        &["commit", "-m", "Unrelated broad publication"],
    )?;
    let content = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    assert_success(
        &git_backed(&sandbox, &root, &["touch", "kb-0001"])?,
        "detect already-published edit",
    );
    let pending = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    let events = pending["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    let edit = events
        .iter()
        .find(|event| event["action"] == "edit")
        .ok_or_else(|| io::Error::other("missing pending edit event"))?;
    assert_eq!(edit["publication"], "unpublished");
    assert_eq!(edit["patch_refs"], serde_json::json!([1]));
    assert_eq!(pending["patches"][1]["commit"]["oid"], content.trim());
    assert_success(
        &git_backed(&sandbox, &root, &["sweep", "--client", "publisher"])?,
        "publish detection",
    );
    let journal = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let report = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    let events = report["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    let edit = events
        .iter()
        .find(|event| event["action"] == "edit")
        .ok_or_else(|| io::Error::other("missing edit event"))?;
    assert_eq!(edit["client"], "manual");
    assert_eq!(edit["published_in"]["oid"], journal.trim());
    assert_eq!(edit["patch_refs"], serde_json::json!([1]));
    assert_eq!(report["patches"][1]["commit"]["oid"], content.trim());
    assert_eq!(
        report["patches"][1]["commit"]["client"],
        serde_json::Value::Null
    );
    Ok(())
}

#[test]
fn history_tracks_rename_and_gives_recycled_id_a_new_lifetime() -> anyhow::Result<()> {
    const NOTE: &str = "notes/kb-0001-history-fixture.md";
    const RENAMED: &str = "notes/title with spaces.md";
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let original = fs::read_to_string(root.join(NOTE))?;
    git(&sandbox, &root, &["mv", NOTE, RENAMED])?;
    let staged = history_json(&sandbox, &root, &["status", "--json", "--no-write"])?;
    assert_eq!(staged["changes"][0]["path"], RENAMED);
    assert_eq!(staged["changes"][0]["original_path"], NOTE);
    assert_eq!(staged["changes"][0]["index"], "R");
    git(&sandbox, &root, &["commit", "-m", "Rename note"])?;
    let renamed = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    assert_eq!(renamed["events"][0]["action"], "rename");
    assert_eq!(
        renamed["lifetimes"][0]["paths"],
        serde_json::json!([NOTE, RENAMED])
    );
    assert_eq!(renamed["lifetimes"][0]["current"], true);
    git(&sandbox, &root, &["rm", RENAMED])?;
    git(&sandbox, &root, &["commit", "-m", "Delete old note"])?;
    let recycled = original
        .lines()
        .map(|line| {
            if line.starts_with("created: ") {
                "created: 2099-01-01"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join(NOTE), recycled)?;
    git(&sandbox, &root, &["add", NOTE])?;
    git(&sandbox, &root, &["commit", "-m", "Reuse the same note ID"])?;
    let report = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    assert_eq!(report["lifetimes"].as_array().map(Vec::len), Some(2));
    assert_eq!(report["lifetimes"][0]["current"], false);
    assert_eq!(report["lifetimes"][1]["current"], true);
    assert_eq!(report["lifetimes"][1]["created"], "2099-01-01");
    assert_eq!(report["events"][0]["lifetime"], 1);
    Ok(())
}

#[test]
fn history_preserves_repeated_and_removed_journal_occurrences() -> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    let path = root.join("log.md");
    let init = fs::read_to_string(&path)?;
    let repeated = "## [2026-01-02] verify | kb-0001 | fixed evidence | section 1\n";
    fs::write(&path, format!("{init}{repeated}"))?;
    git(&sandbox, &root, &["commit", "-am", "First occurrence"])?;
    let first = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    fs::write(&path, format!("{init}{repeated}{repeated}"))?;
    git(&sandbox, &root, &["commit", "-am", "Second occurrence"])?;
    let second = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    fs::write(&path, format!("{init}{repeated}"))?;
    git(&sandbox, &root, &["commit", "-am", "Drop one occurrence"])?;
    let report = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    let events = report["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["current"], false);
    assert_eq!(events[0]["published_in"]["oid"], second.trim());
    assert_eq!(events[1]["current"], true);
    assert_eq!(events[1]["published_in"]["oid"], first.trim());
    for event in events {
        assert_eq!(event["time_precision"], "date");
        assert_eq!(event["client"], serde_json::Value::Null);
        assert_eq!(event["detail"], "fixed evidence | section 1");
        assert_eq!(event["patch_refs"], serde_json::json!([]));
    }
    Ok(())
}

#[test]
fn history_labels_gitless_reads_and_rejects_a_broken_repository() -> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "journal-only history")?;
    fs::write(
        root.join("log.md"),
        "## [2026-01-02] verify | kb-0001 | client=legacy | host=host-a | session=s1 | legacy | source\r\n",
    )?;
    let report = history_json(
        &sandbox,
        &root,
        &["history", "kb-0001", "--json", "--no-write"],
    )?;
    assert_eq!(report["coverage"], "journal_only");
    assert_eq!(report["events"][0]["time"], "2026-01-02");
    assert_eq!(report["events"][0]["publication"], "no_git");
    assert_eq!(report["events"][0]["client"], "legacy");
    assert_eq!(report["events"][0]["host"], "host-a");
    assert_eq!(report["events"][0]["session"], "s1");
    assert_eq!(report["events"][0]["detail"], "legacy | source");
    let status = history_json(&sandbox, &root, &["status", "--json"])?;
    assert_eq!(status["state"], "no_git");
    fs::write(root.join(".git"), "corrupt worktree pointer")?;
    let broken = git_backed(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    assert_eq!(broken.status.code(), Some(1));
    assert_eq!(broken.stdout, b"");
    assert_eq!(
        String::from_utf8(broken.stderr)?,
        "knowbox: invalid .git worktree pointer\n"
    );
    Ok(())
}

#[test]
fn history_attributes_merged_journal_evidence_to_its_side_commit() -> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let log = root.join("log.md");
    let original = fs::read_to_string(&log)?;
    git(&sandbox, &root, &["checkout", "-qb", "verification"])?;
    fs::write(
        &log,
        format!("{original}## [2025-06-07] verify | kb-0001 | evidence on the side\n"),
    )?;
    git(&sandbox, &root, &["commit", "-am", "Verify on side branch"])?;
    let side = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    git(&sandbox, &root, &["checkout", "-q", "main"])?;
    git(
        &sandbox,
        &root,
        &["commit", "--allow-empty", "-m", "Main branch"],
    )?;
    git(
        &sandbox,
        &root,
        &["merge", "--no-ff", "verification", "-m", "Merge evidence"],
    )?;
    let report = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    let events = report["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    let event = events
        .iter()
        .find(|event| event["action"] == "verify")
        .ok_or_else(|| io::Error::other("missing verification"))?;
    assert_eq!(event["published_in"]["oid"], side.trim());
    assert_eq!(
        event["published_in"]["parents"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(event["patch_refs"], serde_json::json!([]));
    Ok(())
}

#[cfg(unix)]
#[test]
fn history_merges_raw_ingest_marker_and_preserves_verdict_participants() -> anyhow::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    history_note(&sandbox, &root)?;
    let file = "proposal | headings.md";
    fs::write(
        root.join("raw").join(file),
        "## Proposal\n\nProposal text\n",
    )?;
    fs::write(
        root.join("raw/conflicts-2026-11-07.md"),
        "## pair: kb-0001 kb-0002\n> verdict: complementary\nKeep both claims.\n",
    )?;
    assert_success(
        &git_backed(
            &sandbox,
            &root,
            &["raw-mark", file, "1", "--note", "kb-0001"],
        )?,
        "mark raw proposal ingested",
    );
    let report = history_json(&sandbox, &root, &["history", "kb-0001", "--json"])?;
    let events = report["events"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing events"))?;
    assert_eq!(
        events
            .iter()
            .filter(|event| event["action"] == "ingest")
            .count(),
        0
    );
    let mark = events
        .iter()
        .find(|event| event["action"] == "raw-mark")
        .ok_or_else(|| io::Error::other("missing raw mark event"))?;
    assert_eq!(mark["raw"]["file"], file);
    assert_eq!(mark["raw"]["number"], 1);
    let verdict = history_json(&sandbox, &root, &["history", "kb-0002", "--json"])?;
    assert_eq!(verdict["events"][0]["action"], "verdict");
    assert_eq!(verdict["events"][0]["detail"], "Keep both claims.");
    assert_eq!(verdict["events"][0]["time"], "2026-11-07");
    assert_eq!(
        verdict["events"][0]["ids"],
        serde_json::json!(["kb-0001", "kb-0002"])
    );
    Ok(())
}

#[test]
fn status_read_skips_external_filters() -> anyhow::Result<()> {
    const NOTE: &str = "notes/filtered.md";
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    git(
        &sandbox,
        &root,
        &[
            "config",
            "filter.fixture.clean",
            "printf x > filter-canary; cat",
        ],
    )?;
    let attrs = root.join(".gitattributes");
    let text = fs::read_to_string(&attrs)?;
    fs::write(&attrs, format!("{text}\n*.md filter=fixture\n"))?;
    fs::write(root.join(NOTE), "before\n")?;
    git(&sandbox, &root, &["add", ".gitattributes", NOTE])?;
    git(&sandbox, &root, &["commit", "-m", "Add filtered text"])?;
    let canary = root.join("filter-canary");
    assert_eq!(fs::read_to_string(&canary)?, "x");
    fs::remove_file(&canary)?;
    fs::write(root.join(NOTE), "after\n")?;
    let status = history_json(&sandbox, &root, &["status", "--json", "--no-write"])?;
    assert_eq!(status["external_filters_disabled"], true);
    assert_eq!(status["changes"][0]["path"], NOTE);
    assert_eq!(status["changes"][0]["worktree"], "M");
    assert!(!canary.exists());
    Ok(())
}

#[test]
fn frozen_index_bytes_and_legacy_ranked_ids_are_stable() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("frozen corpus");
    install_frozen_corpus(&root)?;

    let reindex = run(&sandbox, &root, &["reindex"])?;
    assert_success(&reindex, "frozen corpus reindex");
    let actual_index = fs::read(root.join("index.md"))?;
    let expected_index = fs::read(golden_root().join("index_build.txt"))?;
    assert_eq!(
        actual_index, expected_index,
        "generated index bytes changed"
    );

    let golden_text = fs::read_to_string(golden_root().join("search_ranking.json"))?;
    let expected: BTreeMap<String, Vec<String>> =
        serde_json::from_str(&golden_text).map_err(io::Error::other)?;
    assert_eq!(expected.len(), 18, "incomplete frozen ranking snapshot");
    for (query, expected_ids) in expected {
        let result = run(&sandbox, &root, &["search", &query])?;
        assert_success(&result, &query);
        let actual = note_ids(&result);
        assert_eq!(actual, expected_ids, "ranking changed for query {query:?}");
    }
    Ok(())
}

#[test]
fn frozen_lint_reports_exactly_the_legacy_findings() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("frozen lint");
    install_frozen_corpus(&root)?;
    assert_success(&run(&sandbox, &root, &["reindex"])?, "lint fixture reindex");
    let lint = command(&sandbox)
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_NO_HEARTBEAT", "1")
        .arg("lint")
        .output()?;
    let text = String::from_utf8(lint.stdout).map_err(io::Error::other)?;
    assert_eq!(lint.status.code(), Some(1), "{text}");
    let lines = |prefix: &str| {
        text.lines()
            .filter(|line| line.starts_with(prefix))
            .collect::<Vec<_>>()
    };
    let actual = serde_json::json!({
        "errors": lines("ERROR"),
        "warnings": lines("WARN"),
        "summary": text.lines().filter(|line| line.contains("across") && line.ends_with("notes")).collect::<Vec<_>>(),
    });
    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(golden_root().join("lint_report.json"))?)
            .map_err(io::Error::other)?;
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn git_backed_writers_preserve_touch_churn_and_commit_boundaries() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    assert_eq!(
        git(&sandbox, &root, &["rev-list", "--count", "HEAD"])?.trim(),
        "1"
    );
    for (title, body) in [
        ("first finding", "the old phrase"),
        ("better finding", "thermocline"),
    ] {
        let note = git_backed(
            &sandbox,
            &root,
            &[
                "new",
                "--type",
                "fact",
                "--domain",
                "testing",
                "--title",
                title,
                "--body",
                body,
                "--no-edit",
            ],
        )?;
        assert_success(&note, "new");
    }
    let before = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    assert_success(
        &git_backed(&sandbox, &root, &["touch", "kb-0001"])?,
        "touch",
    );
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, before);
    let journal = fs::read_to_string(root.join("log.md"))?;
    assert!(journal.lines().any(|line| {
        line.ends_with("update | kb-0001 | client=cli | host=test-host | session=— | touched")
    }));
    assert_success(
        &git_backed(
            &sandbox,
            &root,
            &["verify", "kb-0002", "--evidence", "checked fixture"],
        )?,
        "verify",
    );
    let original = fs::read(root.join("notes/kb-0001-first-finding.md"))?;
    assert_success(
        &git_backed(
            &sandbox,
            &root,
            &["supersede", "kb-0001", "--with", "kb-0002"],
        )?,
        "supersede",
    );
    let retired = fs::read_to_string(root.join("notes/kb-0001-first-finding.md"))?;
    assert!(retired.contains("status: superseded\n"));
    assert!(retired.contains("Superseded by [[kb-0002]]"));
    assert!(retired.ends_with("the old phrase\n"));
    assert_ne!(retired.as_bytes(), original);
    let live = fs::read_to_string(root.join("notes/kb-0002-better-finding.md"))?;
    assert!(live.contains("supersedes: [kb-0001]\n"));
    let hit = run(&sandbox, &root, &["search", "thermocline"])?;
    assert_success(&hit, "search after supersede");
    assert_eq!(note_ids(&hit), ["kb-0002"]);
    let graph = run(&sandbox, &root, &["expand", "kb-0001", "--depth", "2"])?;
    assert_success(&graph, "expand after supersede");
    assert!(String::from_utf8_lossy(&graph.stdout).contains("superseded-by"));
    assert_eq!(git(&sandbox, &root, &["status", "--porcelain"])?, "");
    assert_eq!(
        git(&sandbox, &root, &["rev-list", "--count", "HEAD"])?.trim(),
        "5"
    );
    Ok(())
}

#[test]
fn journal_keeps_event_attribution_when_a_different_client_commits() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    for title in ["First finding", "Successor"] {
        assert_success(
            &git_backed(
                &sandbox,
                &root,
                &[
                    "new",
                    "--type",
                    "fact",
                    "--domain",
                    "testing",
                    "--title",
                    title,
                    "--body",
                    "known finding",
                ],
            )?,
            "create note for attribution test",
        );
    }
    let prior_head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let touch = git_env(&mut command(&sandbox))
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_CLIENT", "ignored-client")
        .env("KNOWBOX_SESSION", "session-a")
        .args(["touch", "kb-0001", "--client", "editor-a"])
        .output()?;
    assert_success(&touch, "touch with explicit client");
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, prior_head);
    let sweep = git_env(&mut command(&sandbox))
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_CLIENT", "maintenance-b")
        .env("KNOWBOX_SESSION", "session-b")
        .arg("sweep")
        .output()?;
    assert_success(&sweep, "sweep by a different client");
    let log = git(&sandbox, &root, &["show", "HEAD:log.md"])?;
    let event = log
        .lines()
        .find(|line| line.contains("] update | kb-0001 | "))
        .ok_or_else(|| io::Error::other("missing update event"))?;
    let (time, detail) = event
        .strip_prefix("## [")
        .and_then(|line| line.split_once("] "))
        .ok_or_else(|| io::Error::other("invalid update heading"))?;
    time.parse::<jiff::Timestamp>().map_err(io::Error::other)?;
    assert_eq!(
        detail,
        "update | kb-0001 | client=editor-a | host=test-host | session=session-a | touched"
    );
    assert_eq!(
        git(&sandbox, &root, &["log", "-1", "--format=%b"])?.trim(),
        "Knowbox-Client: maintenance-b\nKnowbox-Host: test-host\nKnowbox-Session: session-b",
    );
    let supersede = git_backed(
        &sandbox,
        &root,
        &[
            "--client",
            "reviewer",
            "supersede",
            "kb-0001",
            "--with",
            "kb-0002",
            "--reason",
            "corrected value | source 2",
        ],
    )?;
    assert_success(&supersede, "supersede with reason");
    assert_eq!(supersede.stdout, b"kb-0001 superseded by kb-0002\n");
    let log = git(&sandbox, &root, &["show", "HEAD:log.md"])?;
    assert!(log.lines().any(|line| line.ends_with(
        "supersede | kb-0001 | client=reviewer | host=test-host | session=— | superseded by kb-0002 | reason=corrected value | source 2"
    )));
    assert_eq!(
        git(&sandbox, &root, &["log", "-1", "--format=%b"])?.trim(),
        "Knowbox-Client: reviewer\nKnowbox-Host: test-host\nKnowbox-Supersede-Reason: corrected value | source 2",
    );
    assert_eq!(git(&sandbox, &root, &["status", "--porcelain"])?, "");
    Ok(())
}

#[test]
fn sweep_records_manual_edits_once_even_after_an_unrelated_rebuild() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "sweep catches lost index drift")?;
    for title in ["First note", "Other note"] {
        assert_success(
            &run(
                &sandbox,
                &root,
                &[
                    "new",
                    "--type",
                    "fact",
                    "--domain",
                    "testing",
                    "--title",
                    title,
                    "--body",
                    "shared baseline",
                ],
            )?,
            "create audit fixture",
        );
        if title == "First note" {
            let first = root.join("notes/kb-0001-first-note.md");
            OpenOptions::new()
                .append(true)
                .open(first)?
                .write_all(b"\nmanualdriftcanary\n")?;
            let hit = run(&sandbox, &root, &["search", "manualdriftcanary"])?;
            assert_success(&hit, "search immediately after manual edit");
            assert_eq!(note_ids(&hit), ["kb-0001"]);
            assert!(String::from_utf8_lossy(&hit.stderr).contains("searched a temporary snapshot"));
        }
    }
    let log = root.join("log.md");
    let before = fs::read_to_string(&log)?;
    assert!(!before.contains("] edit | kb-0001 | "));
    assert_success(&run(&sandbox, &root, &["sweep"])?, "audit on a no-Git KB");
    let audited = fs::read_to_string(&log)?;
    assert_eq!(
        audited
            .lines()
            .filter(|line| line.contains("] edit | "))
            .count(),
        1
    );
    assert!(audited.lines().any(|line| line.ends_with(
        "edit | kb-0001 | client=manual | host=test-host | session=— | edited \"notes/kb-0001-first-note.md\""
    )));
    assert_success(&run(&sandbox, &root, &["sweep"])?, "repeat no-Git sweep");
    assert_eq!(fs::read_to_string(&log)?, audited);
    assert!(!root.join(".git").exists());
    assert!(fs::read_to_string(root.join(".gitignore"))?.contains("/.kb-usage/"));
    Ok(())
}

#[test]
fn git_lf_checkout_does_not_repeat_a_crlf_manual_edit() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    assert_success(
        &git_backed(
            &sandbox,
            &root,
            &[
                "new",
                "--type",
                "fact",
                "--domain",
                "testing",
                "--title",
                "Line endings",
                "--body",
                "original line",
            ],
        )?,
        "create note for linked checkout",
    );
    let relative = "notes/kb-0001-line-endings.md";
    let note = root.join(relative);
    let original = fs::read_to_string(&note)?;
    fs::write(
        &note,
        original
            .replace("original line", "a real change")
            .replace('\n', "\r\n"),
    )?;
    assert_success(
        &git_backed(&sandbox, &root, &["sweep"])?,
        "audit CRLF source",
    );
    let log = git(&sandbox, &root, &["show", "HEAD:log.md"])?;
    assert_eq!(
        log.lines()
            .filter(|line| line.contains("] edit | "))
            .count(),
        1
    );
    let second = sandbox.path().join("linked");
    let path = second
        .to_str()
        .ok_or_else(|| io::Error::other("test path must be UTF-8"))?;
    git(
        &sandbox,
        &root,
        &["worktree", "add", "--quiet", "--detach", path, "HEAD"],
    )?;
    let checked_out = fs::read_to_string(second.join(relative))?;
    assert!(checked_out.ends_with("a real change\n"));
    assert!(!checked_out.contains('\r'));
    assert_success(
        &git_backed(&sandbox, &second, &["sweep"])?,
        "sweep LF checkout",
    );
    assert_eq!(fs::read_to_string(second.join("log.md"))?, log);
    assert_eq!(git(&sandbox, &second, &["status", "--porcelain"])?, "");
    Ok(())
}

#[test]
fn same_note_touch_records_manual_preimage_and_sweep_repairs_stale_index() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "same note preimage")?;
    assert_success(
        &run(
            &sandbox,
            &root,
            &[
                "new",
                "--type",
                "fact",
                "--domain",
                "testing",
                "--title",
                "First",
                "--body",
                "base canary",
            ],
        )?,
        "create audit fixture",
    );
    let note = root.join("notes/kb-0001-first.md");
    OpenOptions::new()
        .append(true)
        .open(&note)?
        .write_all(b"\nfirstauditededit\n")?;
    assert_success(
        &run(&sandbox, &root, &["touch", "kb-0001"])?,
        "touch after external edit",
    );
    let journal = fs::read_to_string(root.join("log.md"))?;
    let edits: Vec<_> = journal
        .lines()
        .filter(|line| line.contains("] edit | ") || line.contains("] update | "))
        .collect();
    assert_eq!(edits.len(), 2);
    assert!(edits[0].contains("] edit | kb-0001 | client=manual | "));
    assert!(edits[1].contains("] update | kb-0001 | client=cli | "));
    OpenOptions::new()
        .append(true)
        .open(note)?
        .write_all(b"\nsecondauditededit\n")?;
    assert_success(&run(&sandbox, &root, &["sweep"])?, "repair and audit");
    let hit = run(&sandbox, &root, &["search", "secondauditededit"])?;
    assert_success(&hit, "search after sweep");
    assert_eq!(hit.stderr, b"");
    assert_eq!(note_ids(&hit), ["kb-0001"]);
    let journal = fs::read_to_string(root.join("log.md"))?;
    assert_eq!(
        journal
            .lines()
            .filter(|line| line.contains("] edit | "))
            .count(),
        2
    );
    Ok(())
}

#[test]
fn lint_requires_later_create_events_and_preserves_historical_waivers() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "missing create lint")?;
    fs::write(
        root.join("log.md"),
        b"## [2026-08-01T23:59:00+07:00] init | \xe2\x80\x94 | old\n",
    )?;
    fs::copy(fixture_root().join(WAL_NOTE), root.join(WAL_NOTE))?;
    assert_success(&run(&sandbox, &root, &["reindex"])?, "index hand-made note");
    let lint = || {
        command(&sandbox)
            .env("KNOWBOX_ROOT", &root)
            .env("KNOWBOX_NO_HEARTBEAT", "1")
            .arg("lint")
            .output()
    };
    let result = lint()?;
    assert_eq!(result.status.code(), Some(1));
    let result = String::from_utf8_lossy(&result.stdout);
    assert!(result.contains("ERROR notes/kb-0001-wal-mode-journal.md: no create entry in log.md"));
    let result = command(&sandbox)
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_NO_HEARTBEAT", "1")
        .args(["lint", "--json", "--no-write"])
        .output()?;
    assert_eq!(result.status.code(), Some(1));
    let lint_report: serde_json::Value =
        serde_json::from_slice(&result.stdout).map_err(io::Error::other)?;
    let findings = lint_report["findings"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing lint findings"))?;
    assert!(findings.iter().any(|finding| {
        finding["code"] == "missing_create"
            && finding["severity"] == "error"
            && finding["path"] == WAL_NOTE
            && finding["note_id"] == "kb-0001"
            && finding["domain"] == "sqlite"
            && finding["waived"] == false
    }));
    fs::write(
        root.join("lint-waivers.md"),
        b"kb-0001 created-vs-log 2026-11-22 historical import\n",
    )?;
    let result = lint()?;
    assert_success(&result, "waived hand-made note");
    let result = String::from_utf8_lossy(&result.stdout);
    assert!(result.contains("WAIVED (1)"));
    assert!(!result.contains("ERROR "));
    fs::remove_file(root.join("lint-waivers.md"))?;
    fs::write(
        root.join("log.md"),
        b"## [2026-08-02T00:01:00+07:00] init | \xe2\x80\x94 | old\n",
    )?;
    assert_success(&lint()?, "note created at journal inception stays exempt");
    Ok(())
}

#[test]
fn exact_show_opens_the_selected_search_path_with_a_recycled_id() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "exact selected hit")?;
    let source = fs::read_to_string(fixture_root().join(WAL_NOTE))?;
    fs::write(root.join("notes/kb-0001-a.md"), &source)?;
    let selected = "notes/kb-0001-Ω docs.md";
    fs::write(root.join(selected), format!("{source}\nexactpathcanary\n"))?;
    assert_success(
        &run(&sandbox, &root, &["reindex"])?,
        "index duplicate ID fixture",
    );
    let json = |args: &[&str]| -> io::Result<serde_json::Value> {
        let out = command(&sandbox)
            .env("KNOWBOX_ROOT", &root)
            .args(["--no-write"])
            .args(args)
            .arg("--json")
            .output()?;
        assert_success(&out, &args.join(" "));
        serde_json::from_slice(&out.stdout).map_err(io::Error::other)
    };
    let hit = json(&["search", "--local", "exactpathcanary"])?;
    assert_eq!(hit["hits"][0]["path"], selected);
    let id_view = json(&["show", "kb-0001"])?;
    assert_eq!(id_view["path"], "notes/kb-0001-a.md");
    let exact = json(&["show", "--path", selected])?;
    assert_eq!(exact["path"], selected);
    assert_ne!(id_view["base_sha1"], exact["base_sha1"]);
    assert!(
        exact["body"]
            .as_str()
            .is_some_and(|body| body.ends_with("exactpathcanary\n"))
    );
    for name in [
        "notes/../index.md",
        "kb-0001-Ω docs.md",
        "/notes/kb-0001-a.md",
    ] {
        let out = run(&sandbox, &root, &["show", "--path", "--no-write", name])?;
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(out.stdout, b"");
    }
    Ok(())
}

#[test]
fn usage_records_reads_and_search_outcomes_once_across_rollups() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "local usage")?;
    let before = run(&sandbox, &root, &["usage"])?;
    assert_success(&before, "usage with no recorded reads");
    assert!(String::from_utf8_lossy(&before.stdout).starts_with("Usage coverage unavailable\n"));
    assert!(!root.join(".kb-usage").exists());

    fs::copy(fixture_root().join(WAL_NOTE), root.join(WAL_NOTE))?;
    fs::write(root.join("refs/source.md"), b"## vintagemonad\n")?;
    assert_success(&run(&sandbox, &root, &["reindex"])?, "index usage fixture");
    for args in [
        &["show", "kb-0001"][..],
        &["expand", "kb-0001"],
        &["search", "kb-0001"],
        &["search", "wal mode"],
        &["search", "wal gxqblf"],
        &["search", "sqlitf"],
        &["search", "vintagemonad"],
        &["search", "quokkanoresult"],
    ] {
        let output = command(&sandbox)
            .env("KNOWBOX_ROOT", &root)
            .env("KNOWBOX_SESSION", "usage-session")
            .args(["--client", "test-agent"])
            .args(args)
            .output()?;
        assert_success(&output, &args.join(" "));
        assert_eq!(output.stderr, b"", "recording usage for {args:?} warned");
    }
    let log = fs::read_to_string(root.join(".search-misses.log"))?;
    let row = log
        .lines()
        .find(|line| !line.is_empty())
        .ok_or_else(|| io::Error::other("missing attributed search miss"))?;
    let [_, query, client, session] = miss_columns(row)?;
    assert_eq!(
        (query, client, session),
        ("quokkanoresult", "test-agent", "usage-session")
    );

    let mut rows = Vec::new();
    for file in fs::read_dir(root.join(".kb-usage"))? {
        let path = file?.path();
        if path.extension() == Some(OsStr::new("jsonl")) {
            for line in fs::read_to_string(path)?
                .lines()
                .filter(|line| !line.is_empty())
            {
                let row: serde_json::Value =
                    serde_json::from_str(line).map_err(io::Error::other)?;
                assert_eq!(row["client"], "test-agent");
                assert_eq!(row["session"], "usage-session");
                assert!(row.get("query").is_none());
                rows.push(row);
            }
        }
    }
    assert_eq!(rows.len(), 8);
    assert_eq!(rows[0]["verb"], "show");
    assert_eq!(rows[0]["note"], "kb-0001");
    assert_eq!(rows[1]["verb"], "expand");
    for (row, layer) in rows[2..]
        .iter()
        .zip(["id", "lexical", "partial", "typo", "refs", "miss"])
    {
        assert_eq!(row["verb"], "search");
        assert_eq!(row["layer"], layer);
    }
    let unrolled = run(&sandbox, &root, &["usage"])?;
    assert_success(&unrolled, "overlay unrolled events");
    let summary = String::from_utf8_lossy(&unrolled.stdout);
    assert!(
        summary
            .lines()
            .any(|line| line.starts_with("kb-0001\topen\t1\t1\t1\t"))
    );
    assert!(
        summary
            .lines()
            .any(|line| line.starts_with("kb-0001\tgraph\t1\t1\t1\t"))
    );
    assert!(
        summary
            .lines()
            .any(|line| line.starts_with("kb-0001\tsearch\t4\t1\t1\t"))
    );
    for _ in 0..2 {
        let rolled = run(&sandbox, &root, &["usage", "--rollup"])?;
        assert_success(&rolled, "roll up recorded reads");
        assert_eq!(rolled.stdout, unrolled.stdout);
    }
    Ok(())
}

#[test]
fn structured_reads_are_repeatable_and_leave_the_kb_untouched() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "read only structured reports")?;
    fs::copy(fixture_root().join(WAL_NOTE), root.join(WAL_NOTE))?;
    fs::write(root.join("refs/reading.md"), b"# Reading\r\n\r\n## Evidence\r\n\r\n### Details\r\n\r\n```md\r\n## Example in fence\r\n```\r\n")?;
    fs::write(
        root.join("raw/proposals.md"),
        b"## First proposal\r\n> state: ingested kb-0001 (2026-11-15)\r\nFirst body.\r\n## Second proposal\r\nupdates: kb-0001\r\nUse the recorded evidence.\r\n",
    )?;
    assert_success(&run(&sandbox, &root, &["reindex"])?, "index fixtures");
    let before = file_snapshot(&root)?;
    let lock = File::options().write(true).open(root.join(".kb.lock"))?;
    lock.lock()?;
    let run_json = |args: &[&str]| -> io::Result<serde_json::Value> {
        let query = || {
            command(&sandbox)
                .env("KNOWBOX_ROOT", &root)
                .env("KNOWBOX_NO_HEARTBEAT", "1")
                .env("KNOWBOX_LOCK_TIMEOUT", "0.2")
                .args(["--no-write"])
                .args(args)
                .arg("--json")
                .output()
        };
        let first = query()?;
        assert_success(&first, &args.join(" "));
        assert_eq!(first.stderr, b"");
        let second = query()?;
        assert_eq!(second.status, first.status);
        assert_eq!(
            second.stdout, first.stdout,
            "non-deterministic JSON for {args:?}"
        );
        serde_json::from_slice(&first.stdout).map_err(io::Error::other)
    };
    let found = run_json(&["search", "--local", "wal mode"])?;
    assert_eq!(found["status"], "ok");
    assert_eq!(found["hits"][0]["id"], "kb-0001");
    assert_eq!(found["hits"][0]["bm25_rank"], 1);
    let id = found["hits"][0]["id"]
        .as_str()
        .ok_or_else(|| io::Error::other("missing hit ID"))?;
    let note = run_json(&["show", id])?;
    assert_eq!(note["state"], "parsed");
    assert_eq!(note["path"], WAL_NOTE);
    assert_eq!(note["frontmatter"]["domain"], "sqlite");
    assert_eq!(note["base_sha1"].as_str().map(str::len), Some(40));
    assert_eq!(
        run_json(&["search", "--local", "quokkanoresult"])?["layer"],
        "miss"
    );
    for args in [
        &["expand", "kb-0001"][..],
        &["graph"],
        &["list"],
        &["stale"],
        &["recent"],
        &["history", "kb-0001"],
        &["timeline"],
        &["status"],
        &["lint"],
    ] {
        assert!(run_json(args)?.is_object());
    }
    let refs = run_json(&["refs"])?;
    assert_eq!(
        refs["docs"][0]["headings"],
        serde_json::json!([
            {"level": 1, "line": 1, "text": "Reading"},
            {"level": 2, "line": 3, "text": "Evidence"},
            {"level": 3, "line": 5, "text": "Details"}
        ])
    );
    let raw = run_json(&["raw-backlog"])?;
    assert_eq!(raw["total"], 2);
    assert_eq!(raw["unreviewed"], 1);
    assert_eq!(raw["entries"][0]["updates"], "kb-0001");
    assert_eq!(raw["entries"][0]["number"], 2);
    assert_eq!(
        raw["entries"][0]["body"],
        "updates: kb-0001\r\nUse the recorded evidence.\r\n"
    );

    for verb in [&["reindex"][..], &["usage", "--rollup"], &["sweep"]] {
        let refused = command(&sandbox)
            .env("KNOWBOX_ROOT", &root)
            .arg("--no-write")
            .args(verb)
            .output()?;
        assert_eq!(refused.status.code(), Some(2));
        assert_eq!(
            refused.stderr,
            b"knowbox: --no-write requires a read command\n"
        );
    }
    let usage = run(&sandbox, &root, &["usage", "--no-write"])?;
    assert_success(&usage, "read usage without a rollup");
    assert_eq!(file_snapshot(&root)?, before);
    Ok(())
}

fn file_snapshot(root: &Path) -> io::Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn add(path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                add(&entry.path(), files)?;
            } else {
                files.insert(entry.path(), fs::read(entry.path())?);
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    add(root, &mut files)?;
    Ok(files)
}

#[test]
fn lint_reports_both_heartbeats_and_honors_the_opt_out() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "heartbeat")?;
    let missing = run(&sandbox, &root, &["lint"])?;
    if cfg!(windows) {
        assert_eq!(missing.status.code(), Some(0));
        assert_eq!(missing.stdout, b"\n0 errors, 0 warnings across 0 notes\n");
    } else {
        assert_eq!(missing.status.code(), Some(1));
        assert_eq!(
            String::from_utf8_lossy(&missing.stdout),
            concat!(
                "ERROR janitor heartbeat missing — weekly cron dead? re-run your KB maintenance script\n",
                "ERROR sweep heartbeat missing — 4-hourly sweep cron dead? check the crontab and .sweep-cron.log\n",
                "\n2 errors, 0 warnings across 0 notes\n",
            )
        );
    }
    for heartbeat in [".kb-heartbeat", ".kb-sweep-heartbeat"] {
        File::create(root.join(heartbeat))?.set_modified(SystemTime::UNIX_EPOCH)?;
    }
    let stale = run(&sandbox, &root, &["lint"])?;
    assert_eq!(stale.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&stale.stdout),
        concat!(
            "ERROR janitor heartbeat stale (older than 8d) — weekly cron dead? re-run your KB maintenance script\n",
            "ERROR sweep heartbeat stale (older than 2d) — sweep cron dead? check .sweep-cron.log\n",
            "\n2 errors, 0 warnings across 0 notes\n",
        )
    );

    let disabled = command(&sandbox)
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_NO_HEARTBEAT", " 1 ")
        .arg("lint")
        .output()?;
    assert_eq!(disabled.status.code(), Some(0));
    assert_eq!(disabled.stdout, b"\n0 errors, 0 warnings across 0 notes\n");
    Ok(())
}

#[test]
fn git_config_is_scrubbed_for_init_lint_and_writers() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("scrubbed Git environment");
    let override_file = sandbox.path().join("gitconfig");
    fs::write(&override_file, "[core]\n\tbare = true\n")?;
    let git_config = || {
        let mut child = command(&sandbox);
        git_env(&mut child)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.bare")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env("GIT_CONFIG_GLOBAL", &override_file)
            .env("GIT_CONFIG", &override_file);
        child
    };
    let initialized = git_config().arg("init").arg(&root).output()?;
    assert_success(&initialized, "init with inherited Git config");
    let lint = git_config()
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_NO_HEARTBEAT", "1")
        .arg("lint")
        .output()?;
    assert_success(&lint, "lint with inherited Git config");
    assert_eq!(lint.stderr, b"");
    let new = git_config()
        .env("KNOWBOX_ROOT", &root)
        .args([
            "new",
            "--type",
            "fact",
            "--domain",
            "tests",
            "--title",
            "Git config",
            "--no-edit",
        ])
        .output()?;
    assert_success(&new, "new with inherited Git config");
    for args in [&["verify", "kb-0001"][..], &["sweep"][..]] {
        let output = git_config()
            .env("KNOWBOX_ROOT", &root)
            .args(args)
            .output()?;
        assert_success(&output, &format!("{args:?} with inherited Git config"));
    }
    assert_eq!(
        git(&sandbox, &root, &["rev-list", "--count", "HEAD"])?,
        "3\n"
    );
    assert_eq!(git(&sandbox, &root, &["status", "--porcelain"])?, "");
    Ok(())
}

#[test]
fn lint_keeps_its_summary_when_git_inventory_cannot_run() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    fs::write(root.join("untracked.txt"), "will not back up")?;
    let lint = || {
        let mut child = command(&sandbox);
        git_env(&mut child)
            .env("KNOWBOX_ROOT", &root)
            .env("KNOWBOX_NO_HEARTBEAT", "1")
            .arg("lint");
        child
    };
    let untracked = lint().output()?;
    assert_eq!(untracked.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&untracked.stdout),
        concat!(
            "ERROR untracked top-level entry 'untracked.txt' — not in TRACKED_PATHS, so it never backs up; add it to git (TRACKED_PATHS) or .gitignore\n",
            "\n1 errors, 0 warnings across 0 notes\n",
        )
    );
    fs::remove_file(root.join("index.md"))?;
    let unknown = lint().env("GIT_DIR", root.join(".git")).output()?;
    assert_eq!(unknown.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&unknown.stdout),
        concat!(
            "WARN  could not check untracked top-level entries: refusing Git environment override GIT_DIR\n",
            "ERROR index.md missing or content drift (hand-edited?) → run `knowbox reindex`\n",
            "\n1 errors, 1 warnings across 0 notes\n",
        )
    );
    assert_eq!(unknown.stderr, b"");
    Ok(())
}

#[test]
fn staged_changes_are_refused_before_a_canonical_write() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = git_kb(&sandbox)?;
    let data = root.join("SCHEMA.md");
    fs::write(&data, b"staged schema\n")?;
    git(&sandbox, &root, &["add", "--", "SCHEMA.md"])?;
    fs::write(&data, b"different worktree schema\n")?;
    let head = git(&sandbox, &root, &["rev-parse", "HEAD"])?;
    let staged = git(&sandbox, &root, &["ls-files", "--stage"])?;
    let journal = fs::read(root.join("log.md"))?;
    let output = git_backed(
        &sandbox,
        &root,
        &[
            "new",
            "--type",
            "fact",
            "--domain",
            "test",
            "--title",
            "refused",
            "--no-edit",
        ],
    )?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("staged"));
    assert_eq!(git(&sandbox, &root, &["rev-parse", "HEAD"])?, head);
    assert_eq!(git(&sandbox, &root, &["ls-files", "--stage"])?, staged);
    assert_eq!(fs::read(root.join("log.md"))?, journal);
    assert_eq!(fs::read(&data)?, b"different worktree schema\n");
    assert!(!root.join("notes/kb-0001-refused.md").exists());
    Ok(())
}

#[test]
fn concurrent_raw_review_inserts_only_one_marker_and_keeps_crlf() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "raw review")?;
    let raw = root.join("raw/draft.md");
    fs::write(&raw, b"# Draft\r\n## Candidate\r\nBody.\r\n")?;
    let mark = || {
        let mut child = command(&sandbox);
        child.env("KNOWBOX_ROOT", &root).args([
            "raw-mark",
            "draft.md",
            "1",
            "--rejected",
            "duplicate",
        ]);
        CapturedChild::spawn(&sandbox, &mut child)
    };
    let first = mark()?;
    let second = mark()?;
    let results = [first.wait()?, second.wait()?];
    assert_eq!(results.iter().filter(|r| r.status.success()).count(), 1);
    let marked = fs::read_to_string(&raw)?;
    let lines: Vec<_> = marked.split("\r\n").collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(
        (lines[0], lines[1], lines[3], lines[4]),
        ("# Draft", "## Candidate", "Body.", "")
    );
    assert!(lines[2].starts_with("> state: rejected duplicate ("));
    let backlog = run(&sandbox, &root, &["raw-backlog"])?;
    assert_success(&backlog, "raw review backlog");
    assert_eq!(backlog.stdout, b"raw backlog: 0 unreviewed of 1 entries\n");
    Ok(())
}

#[test]
fn init_reindex_search_and_show_round_trip_without_git() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "round trip")?;
    assert!(!root.join(".git").exists(), "--no-git created a repository");

    fs::copy(fixture_root().join(WAL_NOTE), root.join(WAL_NOTE))?;
    let reindex = run(&sandbox, &root, &["reindex"])?;
    assert_success(&reindex, "round-trip reindex");
    let search = run(
        &sandbox,
        &root,
        &["search", "sqlite", "wal", "mode", "journal"],
    )?;
    assert_success(&search, "round-trip search");
    assert_eq!(note_ids(&search), vec!["kb-0001"]);

    let mut expected = fs::read(fixture_root().join(WAL_NOTE))?;
    expected.push(b'\n');
    for lookup in ["kb-0001", "wal-mode-journal"] {
        let shown = run(&sandbox, &root, &["show", lookup])?;
        assert_success(&shown, lookup);
        assert_eq!(shown.stdout, expected, "wrong note for {lookup}");
    }
    Ok(())
}

#[test]
fn parallel_init_has_one_winner_and_one_log_entry() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("parallel init");
    let first = CapturedChild::spawn(&sandbox, init(&sandbox).arg(&root))?;
    let second = CapturedChild::spawn(&sandbox, init(&sandbox).arg(&root));
    let first = first.wait();
    let second = second.and_then(CapturedChild::wait);
    let first = first?;
    let second = second?;
    let mut codes = [first.status.code(), second.status.code()];
    codes.sort_unstable();
    assert_eq!(
        codes,
        [Some(0), Some(1)],
        "first: {first:?}\nsecond: {second:?}"
    );

    let log = fs::read_to_string(root.join("log.md"))?;
    let entries: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("## ["))
        .collect();
    assert_eq!(entries.len(), 1, "parallel init log: {log}");
    assert!(
        entries[0].contains("] init | — | "),
        "parallel init log: {log}"
    );
    Ok(())
}

#[test]
fn init_refuses_a_lock_only_directory() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("lock only");
    fs::create_dir_all(&root)?;
    fs::write(root.join(".kb.lock"), b"preexisting lock only\n")?;
    let output = init(&sandbox).arg(&root).output()?;
    assert_eq!(output.status.code(), Some(1), "lock-only init: {output:?}");
    let entries = fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    assert_eq!(entries, [OsStr::new(".kb.lock")]);
    assert_eq!(fs::read(root.join(".kb.lock"))?, b"preexisting lock only\n");
    Ok(())
}

#[test]
fn bad_explicit_root_exits_two_without_falling_back_to_home() -> io::Result<()> {
    let sandbox = sandbox()?;
    let setup = init(&sandbox).output()?;
    assert_success(&setup, "default init in isolated home");
    let fallback_index = sandbox.path().join("home/knowbox/index.md");
    fs::write(&fallback_index, b"fallback canary\n")?;
    let file = sandbox.path().join("not a directory");
    File::create(&file)?;
    let empty = sandbox.path().join("not a KB");
    fs::create_dir(&empty)?;
    for bad_root in [
        Path::new(""),
        &sandbox.path().join("missing root"),
        &file,
        &empty,
    ] {
        let result = run(&sandbox, bad_root, &["reindex"])?;
        assert_eq!(result.status.code(), Some(2), "{bad_root:?}: {result:?}");
        assert_eq!(fs::read(&fallback_index)?, b"fallback canary\n");
    }
    assert_eq!(fs::read_dir(&empty)?.count(), 0);
    Ok(())
}

#[test]
fn search_uses_a_private_snapshot_for_note_drift_while_a_writer_holds_the_lock() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("stale corpus");
    install_frozen_corpus(&root)?;
    let initial = run(&sandbox, &root, &["reindex"])?;
    assert_success(&initial, "initial stale-corpus reindex");
    let expected_index = fs::read(root.join("index.md"))?;
    let old_db = fs::read(root.join(".kb.db"))?;
    let lock = OpenOptions::new().write(true).open(root.join(".kb.lock"))?;
    lock.lock()?;
    let search = |query| {
        command(&sandbox)
            .env("KNOWBOX_ROOT", &root)
            .env("KNOWBOX_LOCK_TIMEOUT", "0.2")
            .args(["search", query])
            .output()
    };
    let fresh = search("checkpointing")?;
    assert_success(&fresh, "fresh search while writer holds the lock");
    assert_eq!(fresh.stderr, b"");

    let note = root.join(WAL_NOTE);
    OpenOptions::new()
        .append(true)
        .open(note)?
        .write_all(b"\nquokkarebuildneedle\n")?;
    let stale = search("quokkarebuildneedle")?;
    assert_success(&stale, "stale search while writer holds the lock");
    assert_eq!(note_ids(&stale), vec!["kb-0001"]);
    assert_eq!(stale.stderr, STALE.as_bytes());
    assert_eq!(
        fs::read(root.join(".kb.db"))?,
        old_db,
        "read replaced the shared DB"
    );
    assert_eq!(fs::read(root.join("index.md"))?, expected_index);
    drop(lock);

    assert_success(&run(&sandbox, &root, &["reindex"])?, "explicit reindex");
    assert_ne!(fs::read(root.join(".kb.db"))?, old_db);
    let fresh = search("quokkarebuildneedle")?;
    assert_success(&fresh, "search after explicit reindex");
    assert_eq!(note_ids(&fresh), vec!["kb-0001"]);
    assert_eq!(fresh.stderr, b"");
    Ok(())
}

#[test]
fn stale_index_markdown_is_left_for_explicit_reindex() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "stale index only")?;
    fs::copy(fixture_root().join(WAL_NOTE), root.join(WAL_NOTE))?;
    let reindex = run(&sandbox, &root, &["reindex"])?;
    assert_success(&reindex, "initial reindex");
    let index = root.join("index.md");
    let expected_index = fs::read(&index)?;
    let query = ["search", "checkpointing"];
    let current = run(&sandbox, &root, &query)?;
    assert_success(&current, "search with current derived artifacts");
    assert_eq!(note_ids(&current), vec!["kb-0001"]);
    assert_eq!(current.stderr, b"");

    fs::write(&index, b"hand-edited stale index\n")?;
    let stale = run(&sandbox, &root, &query)?;
    assert_success(&stale, "search with current DB but stale index.md");
    assert_eq!(stale.stdout, current.stdout);
    assert_eq!(stale.stderr, STALE.as_bytes());
    assert_eq!(fs::read(&index)?, b"hand-edited stale index\n");
    assert_success(&run(&sandbox, &root, &["reindex"])?, "explicit reindex");
    assert_eq!(fs::read(&index)?, expected_index);

    let fresh = run(&sandbox, &root, &query)?;
    assert_success(&fresh, "search after stale index.md was repaired");
    assert_eq!(fresh.stdout, current.stdout);
    assert_eq!(fresh.stderr, b"");
    Ok(())
}

#[test]
fn concurrent_search_misses_append_complete_lines_while_a_writer_holds_the_lock() -> io::Result<()>
{
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "overlapping misses")?;
    let lock = OpenOptions::new().write(true).open(root.join(".kb.lock"))?;
    lock.lock()?;
    let search = |query| {
        CapturedChild::spawn(
            &sandbox,
            command(&sandbox)
                .env("KNOWBOX_ROOT", &root)
                .env("KNOWBOX_LOCK_TIMEOUT", "0.2")
                .args(["search", query]),
        )
    };
    let first = search("quokka\talpha\nomega")?;
    let second = search("quokka\rbeta")?;
    for result in [first.wait()?, second.wait()?] {
        assert_success(&result, "search miss while writer holds the lock");
        assert_eq!(result.stdout, MISS.as_bytes());
        assert_eq!(result.stderr, b"");
    }
    let log = fs::read_to_string(root.join(".search-misses.log"))?;
    let mut queries = Vec::new();
    for line in log.lines().filter(|line| !line.is_empty()) {
        let [time, query, client, session] = miss_columns(line)?;
        time.parse::<jiff::civil::DateTime>()
            .map_err(io::Error::other)?;
        assert_eq!((client, session), ("cli", ""));
        queries.push(query);
    }
    queries.sort_unstable();
    assert!(log.ends_with('\n'));
    assert_eq!(queries, ["quokka alpha omega", "quokka beta"]);
    Ok(())
}

#[test]
fn search_miss_keeps_the_next_record_separate_after_an_incomplete_append() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "incomplete miss")?;
    let log = root.join(".search-misses.log");
    for partial in [
        b"2026-11-21T10:00:00\tinterrupted query".as_slice(),
        b"2026-11-21T10:00:00\tinterrupted \xe2\x82".as_slice(),
    ] {
        fs::write(&log, partial)?;
        let output = run(&sandbox, &root, &["search", "quokkaafterpartial"])?;
        assert_success(&output, "search miss after incomplete log write");
        let bytes = fs::read(&log)?;
        let mut rows = bytes.split(|byte| *byte == b'\n');
        assert_eq!(rows.next(), Some(partial));
        let next = rows
            .next()
            .ok_or_else(|| io::Error::other("missing search miss record"))?;
        let next = std::str::from_utf8(next).map_err(io::Error::other)?;
        let [time, query, client, session] = miss_columns(next)?;
        time.parse::<jiff::civil::DateTime>()
            .map_err(io::Error::other)?;
        assert_eq!(query, "quokkaafterpartial");
        assert_eq!((client, session), ("cli", ""));
        assert_eq!(rows.next(), Some(b"".as_slice()));
        assert_eq!(rows.next(), None);
    }
    Ok(())
}

fn miss_columns(line: &str) -> io::Result<[&str; 4]> {
    line.split('\t')
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| io::Error::other(format!("expected four miss columns in {line:?}")))
}

#[test]
fn search_miss_succeeds_when_the_log_target_cannot_be_appended() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "miss log targets")?;
    let log = root.join(".search-misses.log");
    let canary = sandbox.path().join("canary");
    fs::write(&canary, b"other file\n")?;
    fs::hard_link(&canary, &log)?;
    let query = ["search", "quokkanoresult"];
    let hard_linked = run(&sandbox, &root, &query)?;
    assert_success(&hard_linked, "search miss with hard-linked log");
    assert_eq!(hard_linked.stdout, MISS.as_bytes());
    assert_eq!(fs::read(&canary)?, b"other file\n");
    fs::remove_file(&log)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&canary, &log)?;
        let symlinked = run(&sandbox, &root, &query)?;
        assert_success(&symlinked, "search miss with symlinked log");
        assert_eq!(symlinked.stdout, MISS.as_bytes());
        assert_eq!(fs::read(&canary)?, b"other file\n");
        fs::remove_file(&log)?;
    }

    fs::create_dir(&log)?;
    let directory = run(&sandbox, &root, &query)?;
    assert_success(&directory, "search miss with directory at log path");
    assert_eq!(directory.stdout, MISS.as_bytes());
    Ok(())
}

#[test]
fn search_refuses_live_sqlite_sidecars_without_touching_them() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "sqlite sidecar")?;
    let wal = root.join(".kb.db-wal");
    fs::write(&wal, b"live WAL canary")?;
    let old_db = fs::read(root.join(".kb.db"))?;
    let output = run(&sandbox, &root, &["search", "checkpointing"])?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(fs::read(&wal)?, b"live WAL canary");
    assert_eq!(fs::read(root.join(".kb.db"))?, old_db);
    Ok(())
}

#[test]
fn malformed_note_is_raw_showable_but_excluded_from_search() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "malformed")?;
    let malformed = "---\r\nid: kb-9999\r\ntitle: Broken note\r\n\r\nquokkamalformedneedle\r\n";
    fs::write(root.join("notes/kb-9999-malformed.md"), malformed)?;
    let reindex = run(&sandbox, &root, &["reindex"])?;
    assert_success(&reindex, "malformed reindex");

    let shown = run(&sandbox, &root, &["show", "kb-9999"])?;
    assert_success(&shown, "malformed show");
    assert_eq!(
        shown.stdout,
        format!("{}\n", malformed.replace("\r\n", "\n")).as_bytes()
    );
    let search = run(&sandbox, &root, &["search", "quokkamalformedneedle"])?;
    assert_success(&search, "malformed search");
    assert_eq!(note_ids(&search), Vec::<String>::new());
    Ok(())
}

#[test]
fn unicode_space_hash_and_percent_paths_work_for_every_command() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "KB space # percent % Δ")?;
    let note = "---\nid: kb-1234\ntitle: Unicode path note\ntype: fact\ndomain: test\nstatus: active\ncreated: 2026-01-01\nupdated: 2026-01-01\nsummary: Unicode path search token\n---\n\nUnicode path search token.\n";
    fs::write(root.join("notes/kb-1234-unicode-space.md"), note)?;
    let reindex = run(&sandbox, &root, &["reindex"])?;
    assert_success(&reindex, "unicode-path reindex");
    let search = run(&sandbox, &root, &["search", "Unicode path search token"])?;
    assert_success(&search, "unicode-path search");
    assert_eq!(note_ids(&search), vec!["kb-1234"]);
    let shown = run(&sandbox, &root, &["show", "unicode-space"])?;
    assert_success(&shown, "unicode-path show");
    assert_eq!(shown.stdout, format!("{note}\n").as_bytes());
    Ok(())
}

struct RedirectGuard {
    path: PathBuf,
    active: bool,
}

impl RedirectGuard {
    fn cleanup(&mut self) -> io::Result<()> {
        if self.active {
            #[cfg(windows)]
            fs::remove_dir(&self.path)?;
            #[cfg(unix)]
            fs::remove_file(&self.path)?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for RedirectGuard {
    fn drop(&mut self) {
        if self.active {
            #[cfg(windows)]
            let _ = fs::remove_dir(&self.path);
            #[cfg(unix)]
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn redirect_directory(root: &Path, name: &str, external: &Path) -> io::Result<RedirectGuard> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;

        let system_root = std::env::var_os("SystemRoot")
            .ok_or_else(|| io::Error::other("Windows SystemRoot is missing"))?;
        // cmd.exe does not use standard argv quoting; inject the target with one expansion.
        let output = Command::new(PathBuf::from(system_root).join("System32/cmd.exe"))
            .current_dir(root)
            .env("KB_TEST_JUNCTION_NAME", name)
            .env("KB_TEST_JUNCTION_TARGET", external)
            .stdin(Stdio::null())
            .args(["/D", "/V:OFF", "/C"])
            .raw_arg(r#"mklink /J "%KB_TEST_JUNCTION_NAME%" "%KB_TEST_JUNCTION_TARGET%""#)
            .output()?;
        assert_success(&output, "create test junction");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(external, root.join(name))?;
    Ok(RedirectGuard {
        path: root.join(name),
        active: true,
    })
}

#[test]
fn redirected_notes_are_refused_without_modifying_any_artifact_or_canary() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "redirected")?;
    let external = sandbox.path().join("external notes # %");
    fs::create_dir_all(&external)?;
    fs::remove_dir_all(root.join("notes"))?;
    let external_note = external.join("kb-4321-redirected.md");
    fs::write(
        &external_note,
        "---\nid: kb-4321\ntitle: Redirected note\ntype: fact\ndomain: test\nstatus: active\ncreated: 2026-01-01\nupdated: 2026-01-01\nsummary: redirected-native-token\n---\n\nredirected-native-token\n",
    )?;
    let old_index = fs::read(root.join("index.md"))?;
    let old_db = fs::read(root.join(".kb.db"))?;
    let old_log = fs::read(root.join("log.md"))?;
    let old_external_note = fs::read(&external_note)?;

    let mut redirect = redirect_directory(&root, "notes", &external)?;
    let redirected_note = fs::read(root.join("notes/kb-4321-redirected.md"));
    let reindex = run(&sandbox, &root, &["reindex"]);
    let search = run(&sandbox, &root, &["search", "redirected-native-token"]);
    let show = run(&sandbox, &root, &["show", "kb-4321"]);
    // Unlink the redirect itself before any assertion can unwind and drop the temp directory.
    redirect.cleanup()?;

    assert_eq!(
        redirected_note?, old_external_note,
        "canary setup did not redirect notes/"
    );
    for command in [reindex?, search?, show?] {
        assert_eq!(
            command.status.code(),
            Some(2),
            "redirected notes/ was not refused: {command:?}"
        );
        assert_eq!(command.stdout, b"", "redirected notes/ leaked data");
    }
    assert_eq!(fs::read(root.join("index.md"))?, old_index);
    assert_eq!(fs::read(root.join(".kb.db"))?, old_db);
    assert_eq!(fs::read(root.join("log.md"))?, old_log);
    assert_eq!(fs::read(&external_note)?, old_external_note);
    assert_eq!(fs::read_dir(&external)?.count(), 1);
    Ok(())
}

#[cfg(unix)]
#[test]
fn reads_skip_linked_notes_and_non_utf8_filenames_while_writes_refuse() -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "mixed inventory")?;
    fs::copy(fixture_root().join(WAL_NOTE), root.join(WAL_NOTE))?;
    assert_success(&run(&sandbox, &root, &["reindex"])?, "initial reindex");
    let old_db = fs::read(root.join(".kb.db"))?;
    let old_index = fs::read(root.join("index.md"))?;
    let old_log = fs::read(root.join("log.md"))?;
    let canary = sandbox.path().join("outside.md");
    let canary_note = "---\nid: kb-9998\ntitle: external-canary-contents\n---\ncheckpointing";
    fs::write(&canary, canary_note)?;
    let notes_dir = root.join("notes");
    symlink(&canary, notes_dir.join("kb-9998-linked.md"))?;
    let bad_name = OsStr::from_bytes(b"kb-9999-bad-\xff.md");
    let mut invalid_name_supported = false;
    match fs::write(notes_dir.join(bad_name), "invalid-name-canary-contents") {
        Ok(()) => {
            for entry in fs::read_dir(&notes_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                if name == bad_name {
                    invalid_name_supported = true;
                } else if name.as_encoded_bytes().starts_with(b"kb-9999-bad-") {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Err(error)
            if error.kind() == io::ErrorKind::InvalidInput
                || error.raw_os_error() == Some(rustix::io::Errno::ILSEQ.raw_os_error()) => {}
        Err(error) => return Err(error),
    }
    let note_count = fs::read_dir(&notes_dir)?.count();

    for args in [
        &["search", "checkpointing"][..],
        &["list"][..],
        &["show", "kb-0001"][..],
        &["expand", "kb-0001"][..],
    ] {
        let output = run(&sandbox, &root, args)?;
        assert_success(&output, args[0]);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("kb-0001"));
        assert!(!stdout.contains("canary-contents"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr
                .matches("refusing symbolic link or reparse point")
                .count(),
            1
        );
        assert_eq!(
            stderr.matches("non-UTF-8 filename").count(),
            usize::from(invalid_name_supported)
        );
    }
    let direct = run(&sandbox, &root, &["show", "kb-9998-linked.md"])?;
    assert_eq!(direct.status.code(), Some(1));
    assert_eq!(direct.stdout, b"");

    let new = run(
        &sandbox,
        &root,
        &[
            "new",
            "--type",
            "fact",
            "--domain",
            "test",
            "--title",
            "must refuse",
            "--no-edit",
        ],
    )?;
    assert_eq!(new.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&new.stderr);
    assert!(
        stderr.contains("refusing symbolic link or reparse point")
            || stderr.contains("non-UTF-8 filename")
    );
    assert_eq!(fs::read_dir(root.join("notes"))?.count(), note_count);
    assert_eq!(fs::read(root.join(".kb.db"))?, old_db);
    assert_eq!(fs::read(root.join("index.md"))?, old_index);
    assert_eq!(fs::read(root.join("log.md"))?, old_log);
    assert_eq!(fs::read_to_string(&canary)?, canary_note);
    Ok(())
}

#[test]
fn redirected_optional_directories_are_skipped_by_readers_and_refused_by_writers() -> io::Result<()>
{
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "optional directories")?;
    let external = sandbox.path().join("outside # %");
    fs::create_dir(&external)?;
    fs::write(external.join("canary.md"), "## externalcanary\n")?;
    fs::remove_dir_all(root.join("refs"))?;
    fs::remove_dir_all(root.join("raw"))?;
    let mut refs_redirect = redirect_directory(&root, "refs", &external)?;
    let mut raw_redirect = redirect_directory(&root, "raw", &external)?;

    let search = run(&sandbox, &root, &["search", "externalcanary"]);
    let refs = run(&sandbox, &root, &["refs"]);
    let backlog = run(&sandbox, &root, &["raw-backlog"]);
    let lint = command(&sandbox)
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_NO_HEARTBEAT", "1")
        .arg("lint")
        .output();
    let reindex = run(&sandbox, &root, &["reindex"]);
    refs_redirect.cleanup()?;
    raw_redirect.cleanup()?;

    for (result, expected) in [
        (search?, MISS),
        (
            refs?,
            "knowbox refs: no refs/ directory — nothing ingested yet\n",
        ),
        (backlog?, "raw backlog: 0 unreviewed of 0 entries\n"),
    ] {
        assert_success(&result, "read with redirected optional directory");
        assert_eq!(result.stdout, expected.as_bytes());
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("refusing symbolic link or reparse point")
        );
    }
    let lint = lint?;
    assert_success(&lint, "lint with redirected optional directories");
    let stdout = String::from_utf8_lossy(&lint.stdout);
    assert_eq!(
        stdout
            .matches("WARN  refusing symbolic link or reparse point")
            .count(),
        2
    );
    assert!(stdout.ends_with("\n0 errors, 2 warnings across 0 notes\n"));
    assert_eq!(lint.stderr, b"");
    assert_eq!(reindex?.status.code(), Some(2));
    assert_eq!(
        fs::read(external.join("canary.md"))?,
        b"## externalcanary\n"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn lint_escapes_control_characters_in_skipped_inventory_paths() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = init_target(&sandbox, "lint warnings")?.canonicalize()?;
    let canary = sandbox.path().join("outside.md");
    fs::write(&canary, "outside the KB")?;
    let linked = root.join("notes/kb-4321-unsafe\nERROR forged.md");
    std::os::unix::fs::symlink(canary, &linked)?;

    let lint = command(&sandbox)
        .env("KNOWBOX_ROOT", &root)
        .env("KNOWBOX_NO_HEARTBEAT", "1")
        .arg("lint")
        .output()?;
    assert_success(&lint, "lint with a linked path containing a newline");
    assert_eq!(
        String::from_utf8_lossy(&lint.stdout),
        format!(
            "WARN  refusing symbolic link or reparse point {linked:?} (skipped)\n\n0 errors, 1 warnings across 0 notes\n"
        )
    );
    Ok(())
}

struct CapturedChild {
    child: Child,
    stdout: File,
    stderr: File,
    deadline: Instant,
}

impl CapturedChild {
    fn spawn(sandbox: &TempDir, command: &mut Command) -> io::Result<Self> {
        // Files allow the child to exit even if it emits more than a pipe buffer.
        let stdout = tempfile_in(sandbox.path())?;
        let stderr = tempfile_in(sandbox.path())?;
        let child = command
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?)
            .spawn()?;
        Ok(Self {
            child,
            stdout,
            stderr,
            deadline: Instant::now() + Duration::from_secs(10),
        })
    }

    fn wait(mut self) -> io::Result<Output> {
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.stdout.rewind()?;
                self.stderr.rewind()?;
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                self.stdout.read_to_end(&mut stdout)?;
                self.stderr.read_to_end(&mut stderr)?;
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            if Instant::now() >= self.deadline {
                self.child.kill()?;
                self.child.wait()?;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "knowbox child exceeded ten-second test bound",
                ));
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_reader_holds_old_db_until_bounded_rebuild_failure() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("reader-held");
    install_frozen_corpus(&root)?;
    let initial = run(&sandbox, &root, &["reindex"])?;
    assert_success(&initial, "reader-held initial reindex");
    let db_path = root.join(".kb.db");
    let old_db = fs::read(&db_path)?;
    let connection = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(io::Error::other)?;
    let count: u32 = connection
        .query_row("SELECT count(*) FROM kb WHERE id = 'kb-0001'", [], |row| {
            row.get(0)
        })
        .map_err(io::Error::other)?;
    assert_eq!(count, 1);
    OpenOptions::new()
        .append(true)
        .open(root.join(WAL_NOTE))?
        .write_all(b"\nquokkareaderheldneedle\n")?;
    let failed = CapturedChild::spawn(
        &sandbox,
        command(&sandbox).env("KNOWBOX_ROOT", &root).arg("reindex"),
    )?
    .wait()?;
    assert_eq!(
        failed.status.code(),
        Some(1),
        "reader-held reindex: {failed:?}"
    );
    assert_eq!(fs::read(&db_path)?, old_db, "failed rebuild damaged old DB");
    connection
        .close()
        .map_err(|(_, error)| io::Error::other(error))?;
    let rebuilt = run(&sandbox, &root, &["reindex"])?;
    assert_success(&rebuilt, "reader-held reindex after close");
    assert_ne!(
        fs::read(&db_path)?,
        old_db,
        "old DB survived successful rebuild"
    );
    let matched = run(&sandbox, &root, &["search", "quokkareaderheldneedle"])?;
    assert_success(&matched, "search after reader-held DB was rebuilt");
    assert_eq!(note_ids(&matched), vec!["kb-0001"]);
    assert_eq!(matched.stderr, b"");
    Ok(())
}
