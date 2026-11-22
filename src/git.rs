mod churn;
pub(crate) mod history;
mod process;
mod status;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use tempfile::{NamedTempFile, TempPath};
use thiserror::Error;

use crate::config::KbRoot;
use crate::journal::{self, Actor};
use crate::store::{WriteLock, directory, regular_file};
pub(crate) use churn::churn;
use process::{Output, command, run};
pub(crate) use status::status;

const MANAGED: [&str; 9] = [
    "notes",
    "refs",
    "raw",
    "index.md",
    "log.md",
    "SCHEMA.md",
    ".gitignore",
    ".gitattributes",
    "lint-waivers.md",
];

#[derive(Debug, Error)]
pub(crate) enum GitError {
    #[error("git filesystem operation failed")]
    Io(#[from] io::Error),
    #[error("starting git {op}")]
    Spawn { op: &'static str, source: io::Error },
    #[error("git {op} failed ({status}): {detail}")]
    Failed {
        op: &'static str,
        status: ExitStatus,
        detail: String,
    },
    #[error("git {0} timed out")]
    Timeout(&'static str),
    #[error("refusing Git environment override {0}")]
    Override(String),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("repo mid-merge/rebase or other operation ({0}); resolve it before writing")]
    InProgress(&'static str),
    #[error("repo has unresolved conflicts; resolve them before writing")]
    Conflicts,
    #[error("repo has pre-existing staged changes; commit or unstage them before writing")]
    Staged,
    #[error("git index is locked at {0}; refusing to stage")]
    IndexLocked(PathBuf),
    #[error(
        "committed locally, but could not publish the updated Git index; recover it from {path}"
    )]
    Publish { path: PathBuf, source: io::Error },
}

#[derive(Debug)]
pub(crate) struct GitRepo<'a> {
    root: &'a KbRoot,
    admin: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CommitOutcome {
    pub(crate) committed: bool,
    pub(crate) push: PushOutcome,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum PushOutcome {
    NoRemote,
    UpToDate,
    Pushed,
    Failed { warning: String },
}

impl PushOutcome {
    pub(crate) fn warning(&self) -> Option<&str> {
        match self {
            Self::Failed { warning } => Some(warning),
            Self::NoRemote | Self::UpToDate | Self::Pushed => None,
        }
    }
}

pub(crate) struct Commit<'a> {
    pub(crate) subject: Option<&'a str>,
    pub(crate) actor: &'a Actor,
}

fn forbidden_env(name: &str) -> bool {
    matches!(
        name,
        "GIT_DIR"
            | "GIT_COMMON_DIR"
            | "GIT_WORK_TREE"
            | "GIT_IMPLICIT_WORK_TREE"
            | "GIT_INDEX_FILE"
            | "GIT_OBJECT_DIRECTORY"
            | "GIT_ALTERNATE_OBJECT_DIRECTORIES"
            | "GIT_QUARANTINE_PATH"
            | "GIT_SHALLOW_FILE"
            | "GIT_GRAFT_FILE"
            | "GIT_REPLACE_REF_BASE"
            | "GIT_NAMESPACE"
            | "GIT_ATTR_SOURCE"
            | "GIT_ATTR_SYSTEM"
            | "GIT_EXEC_PATH"
            | "GIT_TEMPLATE_DIR"
            | "GIT_ICASE_PATHSPECS"
            | "GIT_GLOB_PATHSPECS"
            | "GIT_NOGLOB_PATHSPECS"
            | "GIT_CEILING_DIRECTORIES"
            | "GIT_DISCOVERY_ACROSS_FILESYSTEM"
            | "GIT_SUPER_PREFIX"
            | "GIT_PREFIX"
            | "GIT_INTERNAL_SUPER_PREFIX"
    )
}

fn check_vars() -> Result<(), GitError> {
    for (key, _) in env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if forbidden_env(&name) {
            return Err(GitError::Override(name));
        }
    }
    Ok(())
}

pub(crate) fn check_environment() -> Result<(), GitError> {
    check_vars()?;
    run(command(Path::new(".")).arg("--version"), "--version")?.checked()?;
    Ok(())
}

fn git_cmd(root: &KbRoot) -> Command {
    let mut cmd = command(root.path());
    // Bind every command, not just discovery, to the explicit root. Never search its ancestors.
    cmd.args(["--git-dir=.git", "-c", "maintenance.auto=false"]);
    cmd
}

fn strip_lf(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\n").unwrap_or(line)
}

fn native_path(line: &[u8]) -> Result<&Path, GitError> {
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        Ok(Path::new(OsStr::from_bytes(line)))
    }
    #[cfg(not(unix))]
    {
        std::str::from_utf8(line)
            .map(Path::new)
            .map_err(|_| GitError::Invalid("invalid native path in git output"))
    }
}

fn pointer(path: &Path) -> Result<Vec<u8>, GitError> {
    if !regular_file(path)? {
        return Err(GitError::Invalid("missing git worktree pointer"));
    }
    let mut data = Vec::new();
    File::open(path)?.take(32769).read_to_end(&mut data)?;
    if data.len() > 32768 {
        return Err(GitError::Invalid("oversize git worktree pointer"));
    }
    if data.ends_with(b"\n") {
        data.pop();
        if data.ends_with(b"\r") {
            data.pop();
        }
    }
    Ok(data)
}

fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn discover(root: &KbRoot) -> Result<Option<GitRepo<'_>>, GitError> {
    check_vars()?;
    let dot = root.path().join(".git");
    if !exists(&dot)? {
        return Ok(None);
    }
    let admin = if fs::symlink_metadata(&dot)?.is_dir() {
        directory(&dot)?;
        dot.canonicalize()?
    } else {
        let pointer_bytes = pointer(&dot)?;
        let target = pointer_bytes
            .strip_prefix(b"gitdir: ")
            .ok_or(GitError::Invalid("invalid .git worktree pointer"))?;
        let admin = root.path().join(native_path(target)?);
        if !directory(&admin)? {
            return Err(GitError::Invalid("missing git worktree directory"));
        }
        let admin = admin.canonicalize()?;
        // --show-toplevel alone accepts a copied pointer to an unrelated repo as OUR worktree.
        let back = pointer(&admin.join("gitdir"))?;
        if admin.join(native_path(&back)?).canonicalize()? != dot.canonicalize()? {
            return Err(GitError::Invalid(
                "git worktree pointer does not point back to this KB",
            ));
        }
        admin
    };
    let top = run(
        git_cmd(root).args(["rev-parse", "--show-toplevel"]),
        "rev-parse",
    )?
    .checked()?;
    if native_path(strip_lf(&top))?.canonicalize()? != root.path() {
        return Err(GitError::Invalid("KB is not the exact Git worktree root"));
    }
    Ok(Some(GitRepo { root, admin }))
}

pub(crate) fn init(lock: &WriteLock) -> Result<GitRepo<'_>, GitError> {
    check_vars()?;
    let root = lock.root();
    fs::create_dir(root.path().join(".git"))?;
    run(
        command(root.path()).args(["init", "-q", "--template=", "-b", "main", "."]),
        "init",
    )?
    .checked()?;
    let repo = discover(root)?.ok_or(GitError::Invalid(
        "git init did not create a local repository",
    ))?;
    repo.exec("config", &["--local", "user.name", "kb"])?
        .checked()?;
    repo.exec("config", &["--local", "user.email", "kb@localhost"])?
        .checked()?;
    Ok(repo)
}

pub(crate) fn preflight(lock: &WriteLock) -> Result<Option<GitRepo<'_>>, GitError> {
    let repo = discover(lock.root())?;
    if let Some(repo) = &repo {
        repo.guard()?;
        let path = repo.admin.join("index.lock");
        if exists(&path)? {
            return Err(GitError::IndexLocked(path));
        }
    }
    Ok(repo)
}

impl GitRepo<'_> {
    fn exec(&self, op: &'static str, args: &[&str]) -> Result<Output, GitError> {
        run(git_cmd(self.root).arg(op).args(args), op)
    }

    fn guard(&self) -> Result<(), GitError> {
        for marker in [
            "MERGE_HEAD",
            "MERGE_MSG",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "REBASE_HEAD",
            "sequencer",
            "BISECT_LOG",
            "BISECT_START",
            "AUTO_MERGE",
        ] {
            if exists(&self.admin.join(marker))? {
                return Err(GitError::InProgress(marker));
            }
        }
        regular_file(&self.admin.join("index"))?;
        if !self
            .exec("ls-files", &["--unmerged", "-z"])?
            .checked()?
            .is_empty()
        {
            return Err(GitError::Conflicts);
        }
        if self
            .exec(
                "diff",
                &[
                    "--cached",
                    "--quiet",
                    "--ita-visible-in-index",
                    "--no-ext-diff",
                    "--ignore-submodules=none",
                    "--",
                ],
            )?
            .optional()?
            .is_none()
        {
            return Err(GitError::Staged);
        }
        Ok(())
    }

    fn managed_status(&self) -> Result<Vec<u8>, GitError> {
        run(
            git_cmd(self.root)
                .args([
                    "status",
                    "--porcelain=v1",
                    "--no-renames",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                    "-z",
                    "--",
                ])
                .args(MANAGED),
            "status",
        )?
        .checked()
    }

    pub(crate) fn tracked_dirty(&self) -> Result<bool, GitError> {
        Ok(!self.managed_status()?.is_empty())
    }

    pub(crate) fn commit(&self, intent: Commit<'_>) -> Result<CommitOutcome, GitError> {
        if !discover(self.root)?.is_some_and(|current| current.admin == self.admin) {
            return Err(GitError::Invalid(
                "KB Git repository changed since preflight",
            ));
        }
        self.guard()?;
        let changed = self.tracked_dirty()? && self.local_commit(intent)?;
        Ok(CommitOutcome {
            committed: changed,
            push: self.push_outcome(changed),
        })
    }

    pub(crate) fn has_log_line(&self, line: &str) -> Result<bool, GitError> {
        if self
            .exec("rev-parse", &["--verify", "-q", "HEAD"])?
            .optional()?
            .is_none()
            || self
                .exec("ls-tree", &["HEAD", "--", "log.md"])?
                .checked()?
                .is_empty()
        {
            return Ok(false);
        }
        let bytes = self.exec("show", &["HEAD:log.md"])?.checked()?;
        Ok(bytes
            .split(|byte| *byte == b'\n')
            .any(|value| value.strip_suffix(b"\r").unwrap_or(value) == line.as_bytes()))
    }

    pub(crate) fn retry_push(&self) -> PushOutcome {
        self.push_outcome(false)
    }

    fn push_outcome(&self, changed: bool) -> PushOutcome {
        self.push(changed)
            .unwrap_or_else(|error| PushOutcome::Failed {
                warning: format!(
                    "push failed (offline?): {error}; no new local commit is needed to retry"
                ),
            })
    }

    fn local_commit(&self, intent: Commit<'_>) -> Result<bool, GitError> {
        let index = self.admin.join("index");
        let reservation = self.admin.join("index.lock");
        let file = File::create_new(&reservation).map_err(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                GitError::IndexLocked(reservation.clone())
            } else {
                err.into()
            }
        })?;
        drop(file);
        let reserved = TempPath::try_from_path(reservation)?;
        // The real index is immutable while we hold Git's own lock. Git itself stages/commits
        // through its usual atomic rewrites, with the reservation as its private index file.
        self.guard()?;
        let private = |op: &'static str, args: &[&str]| {
            run(
                git_cmd(self.root)
                    .env("GIT_INDEX_FILE", &*reserved)
                    .arg(op)
                    .args(args),
                op,
            )?
            .checked()
        };
        if regular_file(&index)? {
            fs::copy(&index, &*reserved)?;
        } else {
            private("read-tree", &["--empty"])?;
        }
        let status = self.managed_status()?;
        let mut paths = NamedTempFile::new()?;
        for record in status.split(|&b| b == 0).filter(|r| !r.is_empty()) {
            let path = record
                .get(3..)
                .filter(|p| !p.is_empty() && !p.ends_with(b"/"))
                .ok_or(GitError::Invalid(
                    "invalid or embedded repository in managed Git paths",
                ))?;
            paths.write_all(path)?;
            paths.write_all(b"\0")?;
        }
        if status.is_empty() {
            return Ok(false);
        }
        let paths = paths.into_temp_path();
        let pathspec = |cmd: &mut Command| {
            let mut arg = std::ffi::OsString::from("--pathspec-from-file=");
            arg.push(&*paths);
            cmd.arg(arg).arg("--pathspec-file-nul");
        };
        let mut add = git_cmd(self.root);
        add.env("GIT_INDEX_FILE", &*reserved).args(["add", "-A"]);
        pathspec(&mut add);
        run(&mut add, "add")?.checked()?;
        let names = private(
            "diff",
            &[
                "--cached",
                "--name-only",
                "--no-renames",
                "--no-ext-diff",
                "-z",
                "--",
            ],
        )?;
        if names.is_empty() {
            return Ok(false);
        }
        let head = self
            .exec("rev-parse", &["--verify", "-q", "HEAD"])?
            .optional()?;
        let subject = commit_subject(intent.subject, head.is_some(), &names);
        let journal_diff = if names.split(|byte| *byte == 0).any(|path| path == b"log.md") {
            private(
                "diff",
                &[
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--text",
                    "--unified=0",
                    "--",
                    "log.md",
                ],
            )?
        } else {
            Vec::new()
        };
        // A sweep can publish a supersede recovered from an interrupted writer.
        let diff = String::from_utf8_lossy(&journal_diff);
        let reasons: Vec<_> = diff
            .lines()
            .filter_map(|line| journal::supersede_reason(line.strip_prefix('+')?))
            .collect();
        let message = intent.actor.commit_message(&subject, &reasons);
        let mut commit = git_cmd(self.root);
        commit
            .env("GIT_INDEX_FILE", &*reserved)
            .args(["commit", "--only", "-q", "-m", &message]);
        pathspec(&mut commit);
        run(&mut commit, "commit")?.checked()?;
        if let Err(err) = reserved.persist(index) {
            let mut path = err.path;
            path.disable_cleanup(true);
            return Err(GitError::Publish {
                path: path.to_path_buf(),
                source: err.error,
            });
        }
        Ok(true)
    }

    fn push(&self, changed: bool) -> Result<PushOutcome, GitError> {
        if self.exec("remote", &[])?.checked()?.is_empty() {
            return Ok(PushOutcome::NoRemote);
        }
        let branch = self
            .exec("symbolic-ref", &["-q", "HEAD"])?
            .optional()?
            .ok_or(GitError::Invalid(
                "detached HEAD has no configured push destination",
            ))?;
        let branch = std::str::from_utf8(strip_lf(&branch))
            .map_err(|_| GitError::Invalid("non-UTF-8 Git branch"))?;
        let fields = self
            .exec(
                "for-each-ref",
                &[
                    "--format=%(upstream)%00%(upstream:remotename)%00%(upstream:remoteref)",
                    "--",
                    branch,
                ],
            )?
            .checked()?;
        let fields = std::str::from_utf8(strip_lf(&fields))
            .map_err(|_| GitError::Invalid("non-UTF-8 Git upstream"))?;
        let mut fields = fields.split('\0');
        let (Some(tracking), Some(remote), Some(target), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(GitError::Invalid("no configured branch upstream"));
        };
        if !tracking.starts_with("refs/")
            || remote.is_empty()
            || remote == "."
            || !target.starts_with("refs/heads/")
        {
            return Err(GitError::Invalid("no configured remote branch upstream"));
        }
        if !changed
            && self
                .exec("rev-parse", &["--verify", "-q", tracking])?
                .optional()?
                .is_some()
        {
            let range = format!("{tracking}..{branch}");
            if self
                .exec("rev-list", &["--max-count=1", &range, "--"])?
                .checked()?
                .is_empty()
            {
                return Ok(PushOutcome::UpToDate);
            }
        }
        let mirror_off = format!("remote.{remote}.mirror=false");
        let refspec = format!("{branch}:{target}");
        run(
            git_cmd(self.root).args([
                "-c",
                &mirror_off,
                "push",
                "--no-force",
                "--no-follow-tags",
                "--recurse-submodules=no",
                "-q",
                "--",
                remote,
                &refspec,
            ]),
            "push",
        )?
        .checked()?;
        Ok(PushOutcome::Pushed)
    }
}

fn id_prefix(s: &str) -> Option<&str> {
    s.get(..7)
        .filter(|id| id.starts_with("kb-") && id.as_bytes()[3..].iter().all(u8::is_ascii_digit))
}

fn commit_subject(subject: Option<&str>, has_head: bool, names: &[u8]) -> String {
    let base = subject.unwrap_or("knowbox: sweep");
    if !has_head {
        return base.to_owned();
    }
    let mut ids = Vec::new();
    let mut others = Vec::new();
    let mut seen = BTreeSet::new();
    let names = String::from_utf8_lossy(names);
    for path in names.split('\0').filter(|n| !n.is_empty()) {
        let name = path.rsplit('/').next().unwrap_or(path);
        if path.starts_with("notes/")
            && let Some(id) = id_prefix(name)
        {
            if !base.contains(id) && seen.insert(id) {
                ids.push(id);
            }
        } else if !matches!(path, "index.md" | "log.md") && !name.starts_with('.') {
            others.push(path);
        }
    }
    ids.extend(others);
    if ids.is_empty() {
        return base.to_owned();
    }
    let mut label = ids[..ids.len().min(5)].join(", ");
    if ids.len() > 5 {
        label.push_str(&format!(", +{} more", ids.len() - 5));
    }
    match subject {
        Some(_) => format!("{base} (+{label})"),
        None => format!("{base} — {label}"),
    }
}

pub(crate) fn untracked_toplevel(root: &KbRoot) -> Result<Vec<String>, GitError> {
    let Some(repo) = discover(root)? else {
        return Ok(Vec::new());
    };
    let data = run(
        history::read_cmd(&repo).args([
            "ls-files",
            "--others",
            "--exclude-standard",
            "--directory",
            "--no-empty-directory",
            "-z",
        ]),
        "ls-files",
    )?
    .checked()?;
    Ok(String::from_utf8_lossy(&data)
        .split('\0')
        .map(|s| s.trim_end_matches('/'))
        .filter(|p| {
            !p.is_empty() && !p.starts_with('.') && !p.contains('/') && !MANAGED.contains(p)
        })
        .map(str::to_owned)
        .collect())
}
