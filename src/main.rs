mod browse;
mod changes;
mod cli;
mod config;
mod git;
mod index;
mod init;
mod maintain;
mod notes;
mod search;
mod semantic;
mod store;
mod write_cli;

use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use jiff::Zoned;

use crate::cli::Command;
use crate::config::{KbRoot, RootError};
use crate::notes::LookupError;

fn main() -> ExitCode {
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    let result = execute(&mut out, &mut err).and_then(|code| {
        out.flush().context("flushing stdout")?;
        Ok(code)
    });
    match result {
        Ok(code) => code,
        Err(error) => {
            if writeln!(err, "knowbox: {error:#}").is_err() {
                return ExitCode::FAILURE;
            }
            if error.is::<RootError>()
                || matches!(
                    error.downcast_ref::<LookupError>(),
                    Some(LookupError::Ambiguous { .. })
                )
            {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn execute(out: &mut impl Write, err: &mut impl Write) -> Result<ExitCode> {
    let command = match cli::parse() {
        Ok(command) => command,
        Err(diagnostic) => {
            let code = if diagnostic.use_stderr() {
                write!(err, "{diagnostic}")?;
                ExitCode::from(2)
            } else {
                write!(out, "{diagnostic}")?;
                ExitCode::SUCCESS
            };
            return Ok(code);
        }
    };
    match command {
        Command::Version => writeln!(out, "{}", env!("CARGO_PKG_VERSION"))?,
        Command::Init { dir, no_git } => {
            if std::env::var_os("KNOWBOX_ROOT").is_some() {
                KbRoot::resolve()?;
            }
            let dir = match dir {
                Some(dir) => dir,
                None => config::default_root()?,
            };
            write_cli::init(&dir, no_git, out, err)?;
        }
        Command::Reindex => write_cli::reindex(&KbRoot::resolve()?, out, err)?,
        Command::Search { query, expand } => {
            let root = KbRoot::resolve()?;
            search::run(
                &root,
                &query.join(" "),
                expand,
                Zoned::now().datetime(),
                out,
                err,
            )?;
        }
        Command::Show { id_or_slug } => show(&KbRoot::resolve()?, &id_or_slug, out, err)?,
        Command::Expand { id_or_slug, depth } => {
            browse::expand(&KbRoot::resolve()?, &id_or_slug, depth, out)?;
        }
        Command::List { domain, kind } => {
            browse::list(&KbRoot::resolve()?, domain.as_deref(), kind.as_deref(), out)?;
        }
        Command::Recent {
            limit,
            here,
            project,
        } => {
            let root = KbRoot::resolve()?;
            let project = match project.filter(|s| !s.is_empty()) {
                None if here => {
                    Some(browse::project_from_cwd().context("detecting current project")?)
                }
                project => project,
            };
            browse::recent(&root, limit, project.as_deref(), out)?;
        }
        Command::Refs { filter } => browse::refs(&KbRoot::resolve()?, &filter.join(" "), out)?,
        Command::Stale { days } => {
            browse::stale(&KbRoot::resolve()?, days, Zoned::now().date(), out)?
        }
        Command::Churn { days } => git::churn(&KbRoot::resolve()?, days, out)?,
        Command::New(args) => write_cli::create(&KbRoot::resolve()?, *args, out, err)?,
        Command::NextId => writeln!(out, "{}", changes::next_id(&KbRoot::resolve()?)?)?,
        Command::Touch { id_or_slug } => write_cli::touch(&KbRoot::resolve()?, &id_or_slug, out)?,
        Command::Verify { ids, evidence } => {
            write_cli::verify(&KbRoot::resolve()?, &ids, &evidence, out, err)?;
        }
        Command::Supersede { old, with } => {
            write_cli::supersede(&KbRoot::resolve()?, &old, &with, out, err)?;
        }
        Command::Sweep => write_cli::sweep(&KbRoot::resolve()?, err)?,
        Command::Lint => {
            if maintain::lint(&KbRoot::resolve()?, out)? {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::RawBacklog => maintain::raw_backlog(&KbRoot::resolve()?, out)?,
        Command::RawMark {
            file,
            number,
            note,
            rejected,
        } => {
            let mark = match (note.as_deref(), rejected.as_deref()) {
                (Some(id), None) => maintain::Mark::Ingested(id),
                (None, Some(reason)) => maintain::Mark::Rejected(reason),
                _ => bail!("raw-mark requires exactly one of --note or --rejected"),
            };
            write_cli::raw_mark(&KbRoot::resolve()?, &file, number, mark, out, err)?;
        }
        Command::Embed { force } => {
            let root = KbRoot::resolve()?;
            writeln!(out, "{}", semantic::embed(&root, force)?)?;
        }
        Command::Vsearch { query } => {
            let root = KbRoot::resolve()?;
            let hits = semantic::vsearch(&root, &query.join(" "))?;
            if hits.is_empty() {
                writeln!(out, "no semantic matches.")?;
            }
            for hit in hits {
                writeln!(
                    out,
                    "{}  {}  [{:.3}]\n    {}\n",
                    hit.id, hit.path, hit.score, hit.title
                )?;
            }
        }
        Command::Conflicts { top, min_cos, json } => {
            let root = KbRoot::resolve()?;
            if !(-1.0..=1.0).contains(&min_cos) {
                bail!("--min-cos must be between -1 and 1");
            }
            semantic::conflicts(&root, usize::try_from(top)?, min_cos, json, out)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn show(root: &KbRoot, selector: &str, out: &mut impl Write, err: &mut impl Write) -> Result<()> {
    let Some(path) = notes::find(root, selector)? else {
        bail!("not found: {selector}");
    };
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("decoding {} as UTF-8", path.display()))?;
    if let Ok(note) = notes::parse(&bytes) {
        let verified = note.text("verified", "");
        let updated = note.text("updated", "");
        if !verified.is_empty() && updated > verified {
            writeln!(
                err,
                "knowbox show: WARNING edited after last verification (updated {updated} > verified {verified}) — content changed since it was last confirmed; re-verify before relying on it"
            )?;
        }
    }
    // Match Python's text-mode read plus print, without reserializing frontmatter.
    writeln!(out, "{}", notes::normalize_newlines(text))?;
    Ok(())
}
