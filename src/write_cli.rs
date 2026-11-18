use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use jiff::Zoned;

use crate::changes::{self, Event, NewArgs};
use crate::config::KbRoot;
use crate::git::{self, GitRepo};
use crate::maintain::{self, Mark};
use crate::store::WriteLock;
use crate::{index, init};

pub(crate) fn init(
    path: &Path,
    no_git: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    if !no_git {
        git::check_environment()?;
    }
    let lock = init::prepare(path, Zoned::now().date())?;
    index::rebuild(&lock)
        .context("KB skeleton exists; building its initial derived index failed")?;
    let suffix = if no_git {
        " (no git repo)"
    } else {
        let repo = git::init(&lock)
            .context("KB skeleton exists; initializing its Git repository failed")?;
        commit(&repo, Some("knowbox: init — empty KB skeleton"), err)
            .context("KB skeleton exists; its initial Git operation did not complete")?;
        ""
    };
    writeln!(
        out,
        "knowbox: initialized empty KB at {}{suffix}",
        lock.root().path().display()
    )?;
    Ok(())
}

pub(crate) fn reindex(root: &KbRoot, out: &mut impl Write, err: &mut impl Write) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = git::preflight(&lock)?;
    let count = index::rebuild(&lock)?;
    if let Some(repo) = repo {
        if !repo
            .tracked_dirty()
            .context("index rebuilt; checking Git changes failed")?
        {
            writeln!(out, "reindex: no changes")?;
            return Ok(());
        }
        changes::append_entry(
            &lock,
            Zoned::now().date(),
            "reindex",
            "—",
            &format!("{count} notes"),
        )
        .context("index rebuilt; recording the reindex in log.md failed")?;
        commit(&repo, Some("knowbox: reindex"), err)
            .context("index and log.md updated; Git operation did not complete")?;
    }
    writeln!(out, "index.md + FTS rebuilt from {count} notes")?;
    Ok(())
}

pub(crate) fn create(
    root: &KbRoot,
    args: NewArgs,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let draft = changes::prepare_new(args, err)?;
    let lock = WriteLock::acquire(root)?;
    let repo = git::preflight(&lock)?;
    let event = changes::create(&lock, draft, Zoned::now().date())?;
    finalize(&lock, repo, event, out, err)
}

pub(crate) fn touch(root: &KbRoot, selector: &str, out: &mut impl Write) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    git::preflight(&lock)?;
    let path = changes::touch(&lock, selector, Zoned::now().date())?;
    index::rebuild(&lock).with_context(|| {
        format!(
            "{} and log.md updated; rebuilding the index failed",
            path.display()
        )
    })?;
    writeln!(out, "{}", path.display())?;
    Ok(())
}

pub(crate) fn verify(
    root: &KbRoot,
    ids: &[String],
    evidence: &str,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = git::preflight(&lock)?;
    if let Some(event) = changes::verify(&lock, ids, evidence, Zoned::now().date(), out)? {
        finalize(&lock, repo, event, out, err)?;
    }
    Ok(())
}

pub(crate) fn supersede(
    root: &KbRoot,
    old: &str,
    new: &str,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = git::preflight(&lock)?;
    let event = changes::supersede(&lock, old, new, Zoned::now().date())?;
    finalize(&lock, repo, event, out, err)
}

pub(crate) fn raw_mark(
    root: &KbRoot,
    file: &str,
    number: usize,
    mark: Mark<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = git::preflight(&lock)?;
    let event = maintain::raw_mark(&lock, file, number, mark, Zoned::now().date())?;
    record(repo, event, out, err)
}

pub(crate) fn sweep(root: &KbRoot, err: &mut impl Write) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo =
        git::preflight(&lock)?.context("cannot sweep a KB without its own Git repository")?;
    for name in git::untracked_toplevel(root)? {
        writeln!(
            err,
            "knowbox sweep: WARNING untracked top-level entry {name:?} — not a managed path; it will not back up"
        )?;
    }
    commit(&repo, None, err)
}

fn finalize(
    lock: &WriteLock,
    repo: Option<GitRepo<'_>>,
    event: Event,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    index::rebuild(lock).with_context(|| {
        format!(
            "{}: data changes and log.md are on disk; rebuilding the index failed",
            event.subject
        )
    })?;
    record(repo, event, out, err)
}

fn record(
    repo: Option<GitRepo<'_>>,
    event: Event,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    if let Some(repo) = repo {
        commit(&repo, Some(&event.subject), err).with_context(|| {
            format!(
                "{}: data changes are on disk; Git operation did not complete",
                event.subject
            )
        })?;
    }
    writeln!(out, "{}", event.stdout)?;
    Ok(())
}

fn commit(repo: &GitRepo<'_>, subject: Option<&str>, err: &mut impl Write) -> Result<()> {
    if let Some(warning) = repo.commit(subject)?.push_warning {
        writeln!(
            err,
            "knowbox: push failed; local Git history was kept: {warning}"
        )?;
    }
    Ok(())
}
