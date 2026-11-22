use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::changes::mutation::{self, Action, EditArgs, Saved};
use crate::changes::{self, Event, NewArgs};
use crate::config::KbRoot;
use crate::git::{self, Commit, GitRepo};
use crate::journal::{Actor, Stamp};
use crate::maintain::{self, Mark};
use crate::store::{Inventory, WriteLock};
use crate::{index, init, notes, usage};

pub(crate) fn init(
    path: &Path,
    no_git: bool,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    if !no_git {
        git::check_environment()?;
    }
    let lock = init::prepare(path, &Stamp::now(actor))?;
    changes::seed_audit(&lock)?;
    index::rebuild(&lock)
        .context("KB skeleton exists; building its initial derived index failed")?;
    let suffix = if no_git {
        " (no git repo)"
    } else {
        let repo = git::init(&lock)
            .context("KB skeleton exists; initializing its Git repository failed")?;
        commit(
            &repo,
            Commit {
                subject: Some("knowbox: init — empty KB skeleton"),
                actor,
            },
            err,
        )
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

pub(crate) fn reindex(
    root: &KbRoot,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = preflight_note_write(&lock)?;
    let stamp = Stamp::now(actor);
    changes::audit_manual(&lock, &stamp, &notes::snapshot(root)?)?;
    let count = index::rebuild(&lock)?;
    if let Some(repo) = repo {
        if !repo
            .tracked_dirty()
            .context("index rebuilt; checking Git changes failed")?
        {
            writeln!(out, "reindex: no changes")?;
            return Ok(());
        }
        changes::append_entry(&lock, &stamp, "reindex", "—", &format!("{count} notes"))
            .context("index rebuilt; recording the reindex in log.md failed")?;
        commit(
            &repo,
            Commit {
                subject: Some("knowbox: reindex"),
                actor,
            },
            err,
        )
        .context("index and log.md updated; Git operation did not complete")?;
    }
    writeln!(out, "index.md + FTS rebuilt from {count} notes")?;
    Ok(())
}

pub(crate) fn create(
    root: &KbRoot,
    mut args: NewArgs,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let related = std::mem::take(&mut args.related);
    let key = args.request_id.take();
    let mut draft = changes::prepare_new(root, args, err)?;
    if !related.is_empty() || key.is_some() {
        let action = Action::Create {
            markdown: draft.take_source(),
            cli_title: Some(draft.title.clone()),
            related,
            inbox: None,
            followup: None,
        };
        return match mutate(root, action, key.as_deref(), actor) {
            Ok(outcome) => outcome.render(root, out, err),
            Err(error) => Err(draft.on_publication_error(error)),
        };
    }
    let lock = match WriteLock::acquire(root) {
        Ok(lock) => lock,
        Err(error) => return Err(draft.on_publication_error(error.into())),
    };
    let repo = match preflight_note_write(&lock) {
        Ok(repo) => repo,
        Err(error) => return Err(draft.on_publication_error(error)),
    };
    let event = changes::create(&lock, draft, &Stamp::now(actor))?;
    finalize(&lock, repo, event, actor, out, err)
}

pub(crate) fn edit(
    root: &KbRoot,
    args: EditArgs,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let prepared = args.prepare(root, err)?;
    match mutate(root, prepared.action, prepared.key.as_deref(), actor) {
        Ok(outcome) => outcome.render(root, out, err),
        Err(error) => Err(match prepared.draft {
            Some(draft) => mutation::keep_draft(draft, error),
            None => error,
        }),
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct WriteOutcome {
    #[serde(flatten)]
    pub(crate) saved: Saved,
    pub(crate) replayed: bool,
    pub(crate) git: WriteGit,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum WriteGit {
    NoRepository,
    Unchanged,
    Committed { new: bool, push: git::PushOutcome },
}

#[derive(Debug, thiserror::Error)]
#[error("data changes are on disk; {step} did not complete")]
pub(crate) struct IncompleteWrite {
    pub(crate) step: &'static str,
    pub(crate) saved: Saved,
    #[source]
    pub(crate) source: anyhow::Error,
}

impl WriteOutcome {
    pub(crate) fn render(
        &self,
        root: &KbRoot,
        out: &mut impl Write,
        err: &mut impl Write,
    ) -> Result<()> {
        if let WriteGit::Committed { push, .. } = &self.git
            && let Some(warning) = push.warning()
        {
            writeln!(
                err,
                "knowbox: push failed; local Git history was kept: {warning}"
            )?;
        }
        if let Some(note) = &self.saved.note {
            writeln!(out, "{}", root.path().join(&note.path).display())?;
        } else {
            writeln!(out, "{}", self.saved.event.stdout)?;
        }
        Ok(())
    }
}

pub(crate) fn mutate(
    root: &KbRoot,
    action: Action,
    key: Option<&str>,
    actor: &Actor,
) -> Result<WriteOutcome> {
    mutate_locked(&WriteLock::acquire(root)?, action, key, actor)
}

pub(crate) fn mutate_locked(
    lock: &WriteLock,
    action: Action,
    key: Option<&str>,
    actor: &Actor,
) -> Result<WriteOutcome> {
    let root = lock.root();
    let operation = key
        .map(|key| mutation::Operation::new(key, &action))
        .transpose()?;
    let repo = preflight_note_write(lock)?;
    let replay = operation
        .as_ref()
        .map(|intent| intent.replay(root))
        .transpose()?
        .flatten();
    let (published, replayed) = if let Some((saved, receipt)) = replay {
        (
            mutation::Published {
                saved,
                receipt: Some(receipt),
            },
            true,
        )
    } else {
        (
            mutation::apply(lock, action, &Stamp::now(actor), operation.as_ref())?,
            false,
        )
    };
    let mutation::Published { saved, receipt } = published;
    if saved.index_changed
        && let Err(error) = refresh_after_write(lock, replayed)
    {
        return Err(IncompleteWrite {
            step: "rebuilding the index",
            saved,
            source: error,
        }
        .into());
    }
    if replayed && let (Some(repo), Some(receipt)) = (&repo, &receipt) {
        match repo.has_log_line(receipt) {
            Ok(true) => {
                return Ok(WriteOutcome {
                    saved,
                    replayed,
                    git: WriteGit::Committed {
                        new: false,
                        push: repo.retry_push(),
                    },
                });
            }
            Ok(false) => {}
            Err(error) => {
                return Err(IncompleteWrite {
                    step: "checking the Git receipt",
                    saved,
                    source: error.into(),
                }
                .into());
            }
        }
    }
    let git = match repo {
        Some(repo) if saved.changed || receipt.is_some() => {
            match repo.commit(Commit {
                subject: Some(&saved.event.subject),
                actor,
            }) {
                Ok(result) => WriteGit::Committed {
                    new: result.committed,
                    push: result.push,
                },
                Err(error) => {
                    return Err(IncompleteWrite {
                        step: "publishing Git changes",
                        saved,
                        source: error.into(),
                    }
                    .into());
                }
            }
        }
        Some(_) => WriteGit::Unchanged,
        None => WriteGit::NoRepository,
    };
    Ok(WriteOutcome {
        saved,
        replayed,
        git,
    })
}

fn refresh_after_write(lock: &WriteLock, replayed: bool) -> Result<()> {
    if replayed {
        let observed = notes::snapshot(lock.root())?;
        if index::freshness(lock.root(), &observed, &mut Inventory::Strict)?.is_current() {
            return Ok(());
        }
    }
    index::rebuild(lock)?;
    Ok(())
}

pub(crate) fn touch(
    root: &KbRoot,
    selector: &str,
    actor: &Actor,
    out: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    preflight_note_write(&lock)?;
    let path = changes::touch(&lock, selector, &Stamp::now(actor))?;
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
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = preflight_note_write(&lock)?;
    if let Some(event) = changes::verify(&lock, ids, evidence, &Stamp::now(actor), out)? {
        finalize(&lock, repo, event, actor, out, err)?;
    }
    Ok(())
}

pub(crate) fn supersede(
    root: &KbRoot,
    old: &str,
    new: &str,
    reason: Option<&str>,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = preflight_note_write(&lock)?;
    let event = changes::supersede(&lock, old, new, reason, &Stamp::now(actor))?;
    finalize(&lock, repo, event, actor, out, err)
}

pub(crate) fn raw_mark(
    root: &KbRoot,
    file: &str,
    number: usize,
    mark: Mark<'_>,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = git::preflight(&lock)?;
    changes::transaction::recover(&lock)?;
    let event = maintain::raw_mark(&lock, file, number, mark, &Stamp::now(actor))?;
    record(repo, event, actor, out, err)
}

pub(crate) fn sweep(root: &KbRoot, actor: &Actor, err: &mut impl Write) -> Result<()> {
    let lock = WriteLock::acquire(root)?;
    let repo = preflight_note_write(&lock)?;
    let stamp = Stamp::now(actor);
    let observed = notes::snapshot(root)?;
    changes::audit_manual(&lock, &stamp, &observed)?;
    let freshness = index::freshness(root, &observed, &mut Inventory::Strict)?;
    if !freshness.is_current() {
        index::rebuild(&lock).context("rebuilding stale index during sweep")?;
    }
    if let Some(repo) = repo {
        for name in git::untracked_toplevel(root)? {
            writeln!(
                err,
                "knowbox sweep: WARNING untracked top-level entry {name:?} — not a managed path; it will not back up"
            )?;
        }
        commit(
            &repo,
            Commit {
                subject: None,
                actor,
            },
            err,
        )?;
    }
    if let Err(error) = maintain::metrics::record(&lock, stamp.date()) {
        writeln!(
            err,
            "knowbox sweep: warning: could not record local metrics: {error:#}"
        )?;
    }
    drop(lock);
    if let Err(error) = usage::rollup(root, stamp.date()) {
        writeln!(
            err,
            "knowbox sweep: warning: could not roll up local usage: {error:#}"
        )?;
    }
    Ok(())
}

fn preflight_note_write(lock: &WriteLock) -> Result<Option<GitRepo<'_>>> {
    let repo = git::preflight(lock)?;
    notes::validate_inventory(lock.root())?;
    index::ref_names(lock.root(), &mut Inventory::Strict)?;
    changes::transaction::recover(lock)?;
    Ok(repo)
}

fn finalize(
    lock: &WriteLock,
    repo: Option<GitRepo<'_>>,
    event: Event,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    index::rebuild(lock).with_context(|| {
        format!(
            "{}: data changes and log.md are on disk; rebuilding the index failed",
            event.subject
        )
    })?;
    record(repo, event, actor, out, err)
}

fn record(
    repo: Option<GitRepo<'_>>,
    event: Event,
    actor: &Actor,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    if let Some(repo) = repo {
        commit(
            &repo,
            Commit {
                subject: Some(&event.subject),
                actor,
            },
            err,
        )
        .with_context(|| {
            format!(
                "{}: data changes are on disk; Git operation did not complete",
                event.subject
            )
        })?;
    }
    writeln!(out, "{}", event.stdout)?;
    Ok(())
}

fn commit(repo: &GitRepo<'_>, intent: Commit<'_>, err: &mut impl Write) -> Result<()> {
    if let Some(warning) = repo.commit(intent)?.push.warning() {
        writeln!(
            err,
            "knowbox: push failed; local Git history was kept: {warning}"
        )?;
    }
    Ok(())
}
