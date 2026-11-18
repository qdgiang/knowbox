use super::*;
use anyhow::{Context, Result};

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    Ok(run(command(root).args(args), "test")?.checked()?)
}

fn lock(root: &Path) -> Result<WriteLock> {
    Ok(WriteLock::acquire(&KbRoot::open(root)?)?)
}

fn seeded(lock: &WriteLock) -> Result<GitRepo<'_>> {
    let repo = init(lock)?;
    fs::create_dir(repo.root.path().join("notes"))?;
    fs::write(repo.root.path().join("log.md"), b"initial\n")?;
    assert_eq!(repo.commit(Some("init"))?.push_warning, None);
    Ok(repo)
}

#[test]
fn refuses_any_existing_staging_without_touching_index_or_head() -> Result<()> {
    for path in ["unrelated", "log.md"] {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        let lock = lock(root)?;
        let repo = seeded(&lock)?;
        let head = git(root, &["rev-parse", "HEAD"])?;
        fs::write(root.join(path), b"stage only this\n")?;
        git(root, &["add", "--", path])?;
        fs::write(root.join(path), b"working bytes differ\n")?;
        let index = fs::read(root.join(".git/index"))?;
        assert!(matches!(preflight(&lock), Err(GitError::Staged)));
        // Also catch staging by a human after the original preflight.
        assert!(matches!(repo.commit(None), Err(GitError::Staged)));
        assert_eq!(fs::read(root.join(".git/index"))?, index);
        assert_eq!(git(root, &["rev-parse", "HEAD"])?, head);
        assert_eq!(fs::read(root.join(path))?, b"working bytes differ\n");
    }
    Ok(())
}

#[test]
fn intent_to_add_and_merge_marker_block_preflight() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let lock = lock(root)?;
    seeded(&lock)?;
    fs::write(root.join(".git/MERGE_MSG"), b"pending\n")?;
    assert!(matches!(
        preflight(&lock),
        Err(GitError::InProgress("MERGE_MSG"))
    ));
    fs::remove_file(root.join(".git/MERGE_MSG"))?;
    fs::write(root.join("foreign"), b"unstaged\n")?;
    git(root, &["add", "--intent-to-add", "--", "foreign"])?;
    assert!(matches!(preflight(&lock), Err(GitError::Staged)));
    Ok(())
}

#[test]
fn unresolved_index_without_operation_markers_blocks_writes() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let lock = lock(root)?;
    seeded(&lock)?;
    fs::write(root.join("log.md"), b"pending A\n")?;
    git(root, &["stash", "push", "-qm", "saved"])?;
    fs::write(root.join("log.md"), b"pending B\n")?;
    git(root, &["commit", "-qam", "other change"])?;
    assert!(
        matches!(git(root, &["stash", "pop"]), Err(e) if matches!(e.downcast_ref::<GitError>(), Some(GitError::Failed { .. })))
    );
    // Some newer Git versions leave an AUTO_MERGE tree; exercise the markerless case.
    let auto_merge = root.join(".git/AUTO_MERGE");
    if exists(&auto_merge)? {
        fs::remove_file(auto_merge)?;
    }
    let index = fs::read(root.join(".git/index"))?;
    assert!(matches!(preflight(&lock), Err(GitError::Conflicts)));
    assert_eq!(fs::read(root.join(".git/index"))?, index);
    Ok(())
}

#[test]
fn absent_dot_git_never_falls_back_to_parent_and_fake_pointer_cannot_adopt_it() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let parent = lock(dir.path())?;
    seeded(&parent)?;
    let index = fs::read(dir.path().join(".git/index"))?;
    let head = git(dir.path(), &["rev-parse", "HEAD"])?;
    let inner = dir.path().join("inner");
    fs::create_dir(&inner)?;
    let inner_lock = lock(&inner)?;
    assert!(preflight(&inner_lock)?.is_none());
    assert_eq!(untracked_toplevel(inner_lock.root())?, Vec::<String>::new());
    fs::write(inner.join(".git"), b"gitdir: ../.git\n")?;
    assert!(matches!(
        preflight(&inner_lock),
        Err(GitError::Invalid("missing git worktree pointer"))
    ));
    assert_eq!(fs::read(dir.path().join(".git/index"))?, index);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"])?, head);
    Ok(())
}

#[test]
fn accepts_real_linked_worktree_but_not_copied_gitfile() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let main = dir.path().join("main");
    fs::create_dir(&main)?;
    let parent = lock(&main)?;
    seeded(&parent)?;
    git(
        &main,
        &["worktree", "add", "-q", "-b", "linked", "../linked"],
    )?;
    let wt = dir.path().join("linked");
    let linked = lock(&wt)?;
    let repo = preflight(&linked)?.context("linked repo missing")?;
    fs::write(wt.join("log.md"), b"edited in linked\n")?;
    assert_eq!(repo.commit(None)?.push_warning, None);
    assert_eq!(git(&main, &["show", "HEAD:log.md"])?, b"initial\n");
    assert_eq!(git(&wt, &["show", "HEAD:log.md"])?, b"edited in linked\n");
    let fake = dir.path().join("fake");
    fs::create_dir(&fake)?;
    fs::copy(wt.join(".git"), fake.join(".git"))?;
    assert!(matches!(
        preflight(&lock(&fake)?),
        Err(GitError::Invalid(
            "git worktree pointer does not point back to this KB"
        ))
    ));
    Ok(())
}

#[test]
fn soft_failed_push_is_retried_on_clean_sweep_to_exact_upstream() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join("kb");
    fs::create_dir(&root)?;
    let lock = lock(&root)?;
    let repo = seeded(&lock)?;
    git(
        dir.path(),
        &[
            "init",
            "--bare",
            "--template=",
            "-b",
            "backup",
            "remote.git",
        ],
    )?;
    git(&root, &["branch", "-m", "writer"])?;
    git(&root, &["remote", "add", "archive", "../remote.git"])?;
    git(&root, &["push", "-qu", "archive", "writer:backup"])?;
    let bare = dir.path().join("remote.git");
    let initial = git(&bare, &["rev-parse", "refs/heads/backup"])?;
    git(&root, &["remote", "set-url", "archive", "../offline.git"])?;
    fs::write(root.join("notes/kb-0024-x.md"), b"edited\n")?;
    assert!(
        matches!(repo.commit(None)?.push_warning.as_deref(), Some(text) if text.starts_with("push failed (offline?): git push failed"))
    );
    let local = git(&root, &["rev-parse", "HEAD"])?;
    assert_ne!(local, initial);
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/backup"])?, initial);
    assert!(!repo.tracked_dirty()?);
    git(&root, &["remote", "set-url", "archive", "../remote.git"])?;
    assert_eq!(repo.commit(None)?.push_warning, None);
    assert_eq!(git(&root, &["rev-parse", "HEAD"])?, local);
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/backup"])?, local);
    assert_eq!(
        git(
            &bare,
            &["for-each-ref", "--format=%(refname)", "refs/heads"]
        )?,
        b"refs/heads/backup\n"
    );
    Ok(())
}

#[test]
fn no_upstream_warns_without_guessing_branch_or_remote() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join("kb");
    fs::create_dir(&root)?;
    let lock = lock(&root)?;
    let repo = seeded(&lock)?;
    git(
        dir.path(),
        &["init", "--bare", "--template=", "-b", "main", "remote.git"],
    )?;
    git(&root, &["remote", "add", "origin", "../remote.git"])?;
    fs::write(root.join("log.md"), b"committed only locally\n")?;
    assert_eq!(
        repo.commit(None)?.push_warning.as_deref(),
        Some(
            "push failed (offline?): no configured remote branch upstream; no new local commit is needed to retry"
        )
    );
    assert_eq!(git(&dir.path().join("remote.git"), &["for-each-ref"])?, b"");
    Ok(())
}

#[test]
fn hard_commit_failure_preserves_index_and_working_tree() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let lock = lock(root)?;
    let repo = seeded(&lock)?;
    let before = git(root, &["rev-parse", "HEAD"])?;
    let index = fs::read(root.join(".git/index"))?;
    git(root, &["config", "user.name", ""])?;
    fs::remove_file(root.join("log.md"))?;
    assert!(matches!(
        repo.commit(None),
        Err(GitError::Failed { op: "commit", .. })
    ));
    assert_eq!(git(root, &["rev-parse", "HEAD"])?, before);
    assert_eq!(fs::read(root.join(".git/index"))?, index);
    assert!(!exists(&root.join(".git/index.lock"))?);
    git(root, &["config", "user.name", "kb"])?;
    assert_eq!(repo.commit(None)?.push_warning, None);
    assert_eq!(git(root, &["ls-tree", "--name-only", "HEAD"])?, b"");
    Ok(())
}

#[test]
fn untracked_toplevel_and_subjects_match_managed_scope() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let lock = lock(root)?;
    let repo = seeded(&lock)?;
    for name in ["notes/kb-1111-a.md", ".hidden", "index.md", ".gitignore"] {
        fs::write(root.join(name), b"stray.txt\n")?;
    }
    fs::write(root.join("stray.txt"), b"ignored\n")?;
    for name in ["raw", "other"] {
        fs::create_dir(root.join(name))?;
        fs::write(root.join(name).join("x.md"), b"data\n")?;
    }
    assert_eq!(untracked_toplevel(repo.root)?, ["other"]);
    assert_eq!(repo.commit(None)?.push_warning, None);
    assert_eq!(
        git(root, &["log", "-1", "--format=%s"])?,
        "knowbox: sweep — kb-1111, raw/x.md\n".as_bytes()
    );
    assert_eq!(untracked_toplevel(repo.root)?, ["other"]);
    assert!(!repo.tracked_dirty()?);
    assert_eq!(
        commit_subject(
            Some("create kb-2000"),
            true,
            b"notes/kb-1000.md\0notes/kb-2000.md\0log.md\0refs/x\0"
        ),
        "create kb-2000 (+kb-1000, refs/x)"
    );
    assert_eq!(
        commit_subject(None, true, b"raw/1\0raw/2\0raw/3\0raw/4\0raw/5\0raw/6\0"),
        "knowbox: sweep — raw/1, raw/2, raw/3, raw/4, raw/5, +1 more"
    );
    Ok(())
}

#[test]
fn churn_counts_content_not_date_stamps_or_renames() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let lock = lock(root)?;
    let repo = seeded(&lock)?;
    let original = root.join("notes/kb-0123-x.md");
    let renamed = root.join("notes/kb-0123-y.md");
    fs::write(
        &original,
        b"---\nid: kb-0123\ntitle: churn title\nverified: 2026-01-01\n---\nbody\n",
    )?;
    repo.commit(None)?;
    fs::rename(&original, &renamed)?;
    repo.commit(None)?;
    fs::write(
        &renamed,
        fs::read_to_string(&renamed)?.replace("2026-01-01", "2026-02-01"),
    )?;
    repo.commit(None)?;
    fs::write(
        &renamed,
        fs::read_to_string(&renamed)?.replace("body", "-- new body"),
    )?;
    repo.commit(None)?;
    let mut out = Vec::new();
    churn(repo.root, 90, &mut out)?;
    let actual = String::from_utf8(out)?;
    let mut lines = actual.lines();
    assert_eq!(
        lines.next(),
        Some(
            "knowbox churn — content edits per note id, last 90 days (HIGH-CHURN at >=5; date-only bumps skipped, renames aggregated)"
        )
    );
    assert_eq!(lines.next(), Some("commits  days  note"));
    // Crossing local midnight affects days, not edit count.
    assert!(matches!(
        lines.next(),
        Some("      2     1  kb-0123  churn title" | "      2     2  kb-0123  churn title")
    ));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(
        lines.next(),
        Some("1 note id(s) edited across 2 commit(s); high-churn: 0")
    );
    assert_eq!(lines.next(), None);
    Ok(())
}

#[test]
fn refuses_environment_redirects() {
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_KEY_0",
        "GIT_CONFIG_VALUE_0",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG",
    ] {
        assert!(forbidden_env(var), "{var}");
    }
    assert!(!forbidden_env("GIT_CONFIG_NOSYSTEM"));
}
