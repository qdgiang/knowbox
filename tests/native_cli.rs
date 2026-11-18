use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::{TempDir, tempdir, tempfile_in};

#[cfg(windows)]
use rusqlite::{Connection, OpenFlags};

const STALE: &str = "(index was stale — rebuilt from current notes)\n";
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
    let lint = run(&sandbox, &root, &["lint"])?;
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
    assert!(
        journal
            .lines()
            .any(|line| line.ends_with("update | kb-0001 | touched"))
    );
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
    for bad_root in [Path::new(""), &sandbox.path().join("missing root"), &file] {
        let result = run(&sandbox, bad_root, &["reindex"])?;
        assert_eq!(result.status.code(), Some(2), "{bad_root:?}: {result:?}");
        assert_eq!(fs::read(&fallback_index)?, b"fallback canary\n");
    }
    Ok(())
}

#[test]
fn external_note_edit_rebuilds_once_even_with_intact_index() -> io::Result<()> {
    let sandbox = sandbox()?;
    let root = sandbox.path().join("stale corpus");
    install_frozen_corpus(&root)?;
    let initial = run(&sandbox, &root, &["reindex"])?;
    assert_success(&initial, "initial stale-corpus reindex");
    let expected_index = fs::read(root.join("index.md"))?;
    let old_db = fs::read(root.join(".kb.db"))?;

    let note = root.join(WAL_NOTE);
    OpenOptions::new()
        .append(true)
        .open(note)?
        .write_all(b"\nquokkarebuildneedle\n")?;
    let query = ["search", "quokkarebuildneedle"];
    let rebuilt = run(&sandbox, &root, &query)?;
    assert_success(&rebuilt, "stale search");
    assert_eq!(note_ids(&rebuilt), vec!["kb-0001"]);
    assert_eq!(rebuilt.stderr, STALE.as_bytes());
    assert_ne!(
        fs::read(root.join(".kb.db"))?,
        old_db,
        "current DB was not rebuilt"
    );
    assert_eq!(fs::read(root.join("index.md"))?, expected_index);

    let fresh = run(&sandbox, &root, &query)?;
    assert_success(&fresh, "search after note edit was indexed");
    assert_eq!(note_ids(&fresh), vec!["kb-0001"]);
    assert_eq!(fresh.stderr, b"");
    Ok(())
}

#[test]
fn stale_index_rebuilds_once_even_with_current_database() -> io::Result<()> {
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
    let repaired = run(&sandbox, &root, &query)?;
    assert_success(&repaired, "search with current DB but stale index.md");
    assert_eq!(repaired.stdout, current.stdout);
    assert_eq!(repaired.stderr, STALE.as_bytes());
    assert_eq!(fs::read(&index)?, expected_index);

    let fresh = run(&sandbox, &root, &query)?;
    assert_success(&fresh, "search after stale index.md was repaired");
    assert_eq!(fresh.stdout, current.stdout);
    assert_eq!(fresh.stderr, b"");
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

fn redirect_notes(root: &Path, external: &Path) -> io::Result<RedirectGuard> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;

        let system_root = std::env::var_os("SystemRoot")
            .ok_or_else(|| io::Error::other("Windows SystemRoot is missing"))?;
        // cmd.exe does not use standard argv quoting; inject the target with one expansion.
        let output = Command::new(PathBuf::from(system_root).join("System32/cmd.exe"))
            .current_dir(root)
            .env("KB_TEST_JUNCTION_TARGET", external)
            .stdin(Stdio::null())
            .args(["/D", "/V:OFF", "/C"])
            .raw_arg(r#"mklink /J "notes" "%KB_TEST_JUNCTION_TARGET%""#)
            .output()?;
        assert_success(&output, "create test junction");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(external, root.join("notes"))?;
    Ok(RedirectGuard {
        path: root.join("notes"),
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

    let mut redirect = redirect_notes(&root, &external)?;
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
