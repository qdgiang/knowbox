use std::collections::BTreeSet;
use std::io::Write;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use super::history::{Commit, commit, head, read_cmd};
use super::{discover, regular_file, run};
use crate::config::KbRoot;

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum Repository {
    NoGit,
    Unborn {
        branch: String,
        changes: Vec<WorkingChange>,
        external_filters_disabled: bool,
    },
    Head {
        branch: Option<String>,
        detached: bool,
        head: Commit,
        changes: Vec<WorkingChange>,
        external_filters_disabled: bool,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkingChange {
    path: String,
    original_path: Option<String>,
    index: char,
    worktree: char,
    kind: &'static str,
}

impl Repository {
    pub(crate) fn render(&self, out: &mut impl Write) -> Result<()> {
        let (changes, filters_disabled) = match self {
            Self::NoGit => {
                writeln!(out, "No Git repository")?;
                return Ok(());
            }
            Self::Unborn {
                branch,
                changes,
                external_filters_disabled,
            } => {
                writeln!(out, "On unborn branch {branch}")?;
                (changes, *external_filters_disabled)
            }
            Self::Head {
                branch,
                head,
                changes,
                external_filters_disabled,
                ..
            } => {
                if let Some(branch) = branch {
                    writeln!(out, "On branch {branch}")?;
                } else {
                    writeln!(out, "Detached HEAD")?;
                }
                write!(out, "{} {}", head.oid, head.time)?;
                if let Some(client) = &head.client {
                    write!(out, " [client={client}]")?;
                }
                writeln!(out, " {}", head.subject)?;
                (changes, *external_filters_disabled)
            }
        };
        if changes.is_empty() {
            writeln!(out, "Working tree clean")?;
        }
        for change in changes {
            write!(out, "{}{} ", change.index, change.worktree)?;
            if let Some(original) = &change.original_path {
                write!(out, "{original:?} -> ")?;
            }
            writeln!(out, "{:?}", change.path)?;
        }
        if filters_disabled {
            writeln!(out, "External Git filters disabled for this status read")?;
        }
        Ok(())
    }
}

pub(crate) fn status(root: &KbRoot) -> Result<Repository> {
    let Some(repo) = discover(root)? else {
        return Ok(Repository::NoGit);
    };
    regular_file(&repo.admin.join("index"))?;
    let config = run(
        read_cmd(&repo).args([
            "config",
            "--null",
            "--name-only",
            "--get-regexp",
            "^filter\\..*\\.(clean|smudge|process|required)$",
        ]),
        "config",
    )?
    .optional()?;
    let mut status = read_cmd(&repo);
    let external_filters_disabled = config.as_ref().is_some_and(|config| !config.is_empty());
    if let Some(config) = &config {
        let config = std::str::from_utf8(config)?;
        for key in config.split_terminator('\0').collect::<BTreeSet<_>>() {
            let value = if key.ends_with(".required") {
                "false"
            } else {
                ""
            };
            status.arg("-c").arg(format!("{key}={value}"));
        }
    }
    let data = run(
        status.args([
            "status",
            "--porcelain=v2",
            "--branch",
            "--no-ahead-behind",
            "--renames",
            "--untracked-files=all",
            "--ignore-submodules=none",
            "-z",
            "--",
        ]),
        "status",
    )?
    .checked()?;
    let mut records = data.split(|&byte| byte == 0).filter(|row| !row.is_empty());
    let mut branch = None;
    let mut status_head = None;
    let mut changes = Vec::new();
    while let Some(row) = records.next() {
        if let Some(name) = row.strip_prefix(b"# branch.head ") {
            branch = Some(String::from_utf8_lossy(name).into_owned());
        } else if let Some(id) = row.strip_prefix(b"# branch.oid ") {
            status_head = Some(std::str::from_utf8(id)?);
        } else if !row.starts_with(b"# ") {
            changes.push(working_change(row, &mut records)?);
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.kind.cmp(b.kind)));
    let branch = branch.context("Git status omitted branch")?;
    let status_head = status_head.context("Git status omitted HEAD")?;
    let current = head(&repo)?;
    if status_head == "(initial)" {
        ensure!(current.is_none(), "Git HEAD changed while reading status");
        ensure!(
            branch != "(detached)",
            "Git status reported detached unborn HEAD"
        );
        return Ok(Repository::Unborn {
            branch,
            changes,
            external_filters_disabled,
        });
    }
    let oid = current.context("Git HEAD changed while reading status")?;
    ensure!(oid == status_head, "Git HEAD changed while reading status");
    let head = commit(&repo, &oid)?;
    let detached = branch == "(detached)";
    Ok(Repository::Head {
        branch: (!detached).then_some(branch),
        detached,
        head,
        changes,
        external_filters_disabled,
    })
}

fn working_change<'a>(
    row: &[u8],
    records: &mut impl Iterator<Item = &'a [u8]>,
) -> Result<WorkingChange> {
    let (kind, fields) = match row.first() {
        Some(b'1') => ("ordinary", 8),
        Some(b'2') => ("rename_or_copy", 9),
        Some(b'u') => ("unmerged", 10),
        Some(b'?') => {
            return Ok(WorkingChange {
                path: String::from_utf8_lossy(row.get(2..).context("missing untracked Git path")?)
                    .into_owned(),
                original_path: None,
                index: '?',
                worktree: '?',
                kind: "untracked",
            });
        }
        _ => bail!("invalid Git status record"),
    };
    let state = row.get(2..4).context("missing Git status codes")?;
    let [index, worktree] = state else {
        bail!("invalid Git status codes");
    };
    let path = row
        .splitn(fields + 1, |&byte| byte == b' ')
        .nth(fields)
        .context("missing changed Git path")?;
    let original_path = if kind == "rename_or_copy" {
        let original = records.next().context("missing original Git path")?;
        Some(String::from_utf8_lossy(original).into_owned())
    } else {
        None
    };
    Ok(WorkingChange {
        path: String::from_utf8_lossy(path).into_owned(),
        original_path,
        index: char::from(*index),
        worktree: char::from(*worktree),
        kind,
    })
}
