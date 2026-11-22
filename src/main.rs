mod browse;
mod changes;
mod cli;
mod config;
mod git;
mod history;
mod index;
mod init;
mod journal;
mod maintain;
mod notes;
mod search;
mod semantic;
mod serve;
mod store;
mod usage;
mod write_cli;

use std::io::{self, Read, Write};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use jiff::{Span, Zoned};

use crate::changes::mutation::Action;
use crate::cli::Command;
use crate::config::{KbRoot, RootError};
use crate::journal::Actor;
use crate::notes::LookupError;
use crate::store::{Inventory, InventoryWarning, InventoryWarnings};
use crate::usage::{Layer, ReadEvent};

// Cargo uses a three-component package version. Releases use only major and minor.
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR")
);

fn main() -> ExitCode {
    let mut out = io::stdout().lock();
    let mut err = io::stderr().lock();
    let mut warnings = InventoryWarnings::default();
    let result = execute(&mut Inventory::Read(&mut warnings), &mut out, &mut err);
    if warnings.report(&mut err).is_err() {
        return ExitCode::FAILURE;
    }
    let result = result.and_then(|code| {
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

fn execute(
    read: &mut Inventory<'_>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<ExitCode> {
    let cli::Cli {
        command,
        client,
        no_write,
    } = match cli::parse() {
        Ok(cli) => cli,
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
    if no_write && command.writes() {
        writeln!(err, "knowbox: --no-write requires a read command")?;
        return Ok(ExitCode::from(2));
    }
    let actor = || Actor::resolve(client.as_deref(), "cli");
    match command {
        Command::Version => writeln!(out, "{VERSION}")?,
        Command::Context {
            project,
            host,
            explain,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let mode = if explain {
                browse::ContextMode::Explain
            } else {
                browse::ContextMode::Brief
            };
            let report = browse::context(&root, &project, &host, Zoned::now().date(), mode, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Map {
            project_misses,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let mode = if project_misses {
                semantic::MissProjection::Provider
            } else {
                semantic::MissProjection::Offline
            };
            let report = semantic::map_report(&root, mode, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Metrics { json } => {
            let report = maintain::metrics::report(&KbRoot::resolve(read)?)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Init { dir, no_git } => {
            KbRoot::validate_init_override()?;
            let dir = match dir {
                Some(dir) => dir,
                None => config::default_root()?,
            };
            write_cli::init(&dir, no_git, &actor()?, out, err)?;
        }
        Command::Reindex => write_cli::reindex(
            &KbRoot::resolve(&mut Inventory::Strict)?,
            &actor()?,
            out,
            err,
        )?,
        Command::Search {
            query,
            expand,
            local,
            no_fix,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let now = Zoned::now();
            let query = query.join(" ");
            let report = search::run(
                &root,
                &query,
                search::Options {
                    expand,
                    local,
                    no_fix,
                    no_write,
                },
                now.date(),
                read,
            )?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out, err)?;
            }
            if !no_write && let Some(event) = report.read_event() {
                let actor = actor();
                if matches!(
                    event,
                    ReadEvent::Search {
                        layer: Layer::Miss,
                        ..
                    }
                ) && let Err(error) =
                    search::log_miss(&root, &query, now.datetime(), actor.as_ref().ok())
                {
                    writeln!(
                        err,
                        "knowbox search: warning: could not record search miss: {error:#}"
                    )?;
                }
                record_usage(&root, &now, actor, &event, err)?;
            }
            if matches!(report, search::SearchResult::StaleIndex { .. }) {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::Show {
            id_or_slug,
            path,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let detail = if path {
                notes::read_detail_path(&root, &id_or_slug, read)?
            } else {
                notes::read_detail(&root, &id_or_slug, read)?
            };
            if json {
                write_json(&detail, read, out)?;
            } else {
                detail.render(out, err)?;
            }
            if !no_write && let Some(note) = detail.id {
                record_usage(
                    &root,
                    &Zoned::now(),
                    actor(),
                    &ReadEvent::Show { note },
                    err,
                )?;
            }
        }
        Command::Expand {
            id_or_slug,
            depth,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let report = browse::expand(&root, &id_or_slug, depth, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
            if !no_write {
                record_usage(
                    &root,
                    &Zoned::now(),
                    actor(),
                    &ReadEvent::Expand {
                        notes: report.displayed_ids(),
                    },
                    err,
                )?;
            }
        }
        Command::Graph { json } => {
            let report = browse::graph(&KbRoot::resolve(read)?, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::History { id_or_slug, json } => {
            let report = history::read(&KbRoot::resolve(read)?, &id_or_slug, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Timeline { json } => {
            let report = history::timeline(&KbRoot::resolve(read)?, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Status { json } => {
            let report = git::status(&KbRoot::resolve(read)?)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Serve(mut options) => {
            options.read_only |= no_write;
            let root = KbRoot::resolve(read)?;
            serve::run(&root, &options, client.as_deref(), out, err)?;
        }
        Command::List { domain, kind, json } => {
            let report = browse::list(
                &KbRoot::resolve(read)?,
                domain.as_deref(),
                kind.as_deref(),
                read,
            )?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Recent {
            limit,
            here,
            project,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let project = match project.filter(|s| !s.is_empty()) {
                None if here => {
                    Some(browse::project_from_cwd().context("detecting current project")?)
                }
                project => project,
            };
            let report = browse::recent(&root, limit, project.as_deref(), read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Refs { filter, json } => {
            let report = browse::refs(&KbRoot::resolve(read)?, &filter.join(" "), read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Stale { days, json } => {
            let report = browse::stale(&KbRoot::resolve(read)?, days, Zoned::now().date(), read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
        Command::Churn { days } => git::churn(&KbRoot::resolve(read)?, days, read, out)?,
        Command::New(args) => write_cli::create(
            &KbRoot::resolve(&mut Inventory::Strict)?,
            *args,
            &actor()?,
            out,
            err,
        )?,
        Command::Edit(args) => write_cli::edit(
            &KbRoot::resolve(&mut Inventory::Strict)?,
            args,
            &actor()?,
            out,
            err,
        )?,
        Command::Link { a, b, request_id } => {
            let root = KbRoot::resolve(&mut Inventory::Strict)?;
            write_cli::mutate(
                &root,
                Action::Link { a, b },
                request_id.as_deref(),
                &actor()?,
            )?
            .render(&root, out, err)?;
        }
        Command::Verdict {
            a,
            b,
            verdict,
            reason,
            link,
            winner,
            request_id,
        } => {
            let root = KbRoot::resolve(&mut Inventory::Strict)?;
            write_cli::mutate(
                &root,
                Action::Verdict {
                    a,
                    b,
                    verdict,
                    reason,
                    link,
                    winner,
                    bases: None,
                },
                request_id.as_deref(),
                &actor()?,
            )?
            .render(&root, out, err)?;
        }
        Command::NextId => writeln!(out, "{}", changes::next_id(&KbRoot::resolve(read)?)?)?,
        Command::Touch { id_or_slug } => write_cli::touch(
            &KbRoot::resolve(&mut Inventory::Strict)?,
            &id_or_slug,
            &actor()?,
            out,
        )?,
        Command::Verify { ids, evidence } => {
            write_cli::verify(
                &KbRoot::resolve(&mut Inventory::Strict)?,
                &ids,
                &evidence,
                &actor()?,
                out,
                err,
            )?;
        }
        Command::Supersede { old, with, reason } => {
            write_cli::supersede(
                &KbRoot::resolve(&mut Inventory::Strict)?,
                &old,
                &with,
                reason.as_deref(),
                &actor()?,
                out,
                err,
            )?;
        }
        Command::Sweep => write_cli::sweep(
            &KbRoot::resolve(&mut Inventory::Strict)?,
            &Actor::resolve(client.as_deref(), "sweep")?,
            err,
        )?,
        Command::Lint {
            file,
            stdin,
            against,
            json,
        } => {
            let root = KbRoot::resolve(read)?;
            let report = if file.is_some() || stdin {
                let bytes = match file.as_deref() {
                    Some(path) if path.as_os_str() != "-" => std::fs::read(path)
                        .with_context(|| format!("reading draft {}", path.display()))?,
                    _ => {
                        let mut bytes = Vec::new();
                        io::stdin()
                            .lock()
                            .read_to_end(&mut bytes)
                            .context("reading draft from stdin")?;
                        bytes
                    }
                };
                let relative = against
                    .as_deref()
                    .map(|selector| {
                        let path = notes::find_for_write(&root, selector)?
                            .with_context(|| format!("not found: {selector}"))?;
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .context("note filename is not UTF-8")?;
                        Ok::<_, anyhow::Error>(format!("notes/{name}"))
                    })
                    .transpose()?;
                let target = relative.as_deref().map_or(
                    maintain::DraftTarget::Standalone,
                    maintain::DraftTarget::Replace,
                );
                maintain::lint_draft(&root, target, &bytes, read)?
            } else {
                maintain::lint(&root, maintain::HeartbeatCheck::from_env(), read)?
            };
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
            if report.has_errors() {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::Usage { days, rollup } => {
            let root = KbRoot::resolve(read)?;
            let today = Zoned::now().date();
            let since = days
                .map(|days| today.checked_sub(Span::new().try_days(i64::from(days) - 1)?))
                .transpose()
                .context("usage period starts outside the supported date range")?;
            if rollup {
                usage::rollup(&root, today)?;
            }
            usage::report(&root, since.map(|start| start..=today), out)?;
        }
        Command::RawBacklog { json } => {
            let report = maintain::raw_backlog(&KbRoot::resolve(read)?, read)?;
            if json {
                write_json(&report, read, out)?;
            } else {
                report.render(out)?;
            }
        }
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
            write_cli::raw_mark(
                &KbRoot::resolve(&mut Inventory::Strict)?,
                &file,
                number,
                mark,
                &actor()?,
                out,
                err,
            )?;
        }
        Command::Embed { force } => {
            let root = KbRoot::resolve(&mut Inventory::Strict)?;
            writeln!(out, "{}", semantic::embed(&root, force)?)?;
        }
        Command::Vsearch { query } => {
            let root = KbRoot::resolve(read)?;
            let hits = semantic::vsearch(&root, &query.join(" "), read)?;
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
            let root = KbRoot::resolve(read)?;
            if !(-1.0..=1.0).contains(&min_cos) {
                bail!("--min-cos must be between -1 and 1");
            }
            semantic::conflicts(&root, usize::try_from(top)?, min_cos, json, read, out)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn write_json<T: serde::Serialize>(
    result: &T,
    read: &mut Inventory<'_>,
    out: &mut impl Write,
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct Report<'a, T> {
        #[serde(flatten)]
        data: &'a T,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        inventory_warnings: Vec<InventoryWarning>,
    }
    serde_json::to_writer(
        &mut *out,
        &Report {
            data: result,
            inventory_warnings: read.take_json_warnings(),
        },
    )?;
    writeln!(out)?;
    Ok(())
}

fn record_usage(
    root: &KbRoot,
    now: &Zoned,
    actor: Result<Actor>,
    event: &ReadEvent,
    err: &mut impl Write,
) -> std::io::Result<()> {
    if let Err(error) = actor.and_then(|actor| {
        usage::record(
            root,
            &usage::Context {
                now,
                client: actor.client(),
                session: actor.session(),
            },
            event,
        )
    }) {
        writeln!(
            err,
            "knowbox: warning: could not record local usage: {error:#}"
        )?;
    }
    Ok(())
}
