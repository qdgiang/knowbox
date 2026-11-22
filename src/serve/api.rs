use std::time::SystemTime;

use anyhow::{Context, Result};
use jiff::Zoned;
use serde::Serialize;
use serde_json::json;

use crate::config::KbRoot;
use crate::store::{Inventory, InventoryWarnings};
use crate::{browse, git, history, maintain, notes, search, usage};

use super::{Query, Reply, markdown, revision};

pub(super) fn read(root: &KbRoot, route: &str, query: &Query) -> Result<Reply> {
    let mut warnings = InventoryWarnings::default();
    let mut inventory = Inventory::Read(&mut warnings);
    let read = &mut inventory;

    let data = match route {
        "/api/search" => {
            let Some(text) = query.get("q").filter(|value| !value.trim().is_empty()) else {
                return Ok(Reply::error(400, "a search query is required"));
            };
            let settings = (|| {
                Ok::<_, anyhow::Error>(search::Options {
                    no_fix: query.flag("no_fix", false)?,
                    expand: query.flag("expand", false)?,
                    local: query.flag("local", true)?,
                    no_write: true,
                })
            })();
            let options = match settings {
                Ok(options) => options,
                Err(error) => return Ok(Reply::error(400, &error.to_string())),
            };
            value(search::run(root, text, options, Zoned::now().date(), read)?)?
        }
        "/api/note" => {
            let report = match (query.get("id"), query.get("path")) {
                (Some(id), None) => notes::read_detail(root, id, read),
                (None, Some(path)) => notes::read_detail_path(root, path, read),
                _ => return Ok(Reply::error(400, "specify exactly one note ID or path")),
            };
            let report = match report {
                Ok(report) => report,
                Err(error) if error.to_string().starts_with("not found:") => {
                    return Ok(Reply::error(404, &error.to_string()));
                }
                Err(error)
                    if matches!(
                        error.downcast_ref::<notes::LookupError>(),
                        Some(notes::LookupError::Ambiguous { .. })
                    ) =>
                {
                    return Ok(Reply::error(409, &error.to_string()));
                }
                Err(error) => return Err(error),
            };
            let mut data = value(report)?;
            if let Some(body) = data.get("body").and_then(serde_json::Value::as_str) {
                let html = markdown::render(body);
                data.insert("body_html".into(), html.into());
            }
            data
        }
        "/api/expand" => {
            let Some(id) = query.get("id").filter(|id| notes::is_id(id)) else {
                return Ok(Reply::error(400, "a note ID is required"));
            };
            let depth = match query.get("depth").unwrap_or("1") {
                "1" => browse::Depth::One,
                "2" => browse::Depth::Two,
                _ => return Ok(Reply::error(400, "depth must be 1 or 2")),
            };
            value(browse::expand(root, id, depth, read)?)?
        }
        "/api/history" => {
            let Some(id) = query.get("id").filter(|id| notes::is_id(id)) else {
                return Ok(Reply::error(400, "a note ID is required"));
            };
            value(history::read(root, id, read)?)?
        }
        "/api/list" => value(browse::list(
            root,
            query.get("domain"),
            query.get("type"),
            read,
        )?)?,
        "/api/recent" => value(browse::recent(root, 8, query.get("project"), read)?)?,
        "/api/status" => {
            let mut data = match git::status(root) {
                Ok(status) => value(status)?,
                Err(error) => {
                    value(json!({ "state": "unavailable", "message": format!("{error:#}") }))?
                }
            };
            data.insert(
                "heartbeats".into(),
                serde_json::to_value(maintain::heartbeats(root, SystemTime::now()))?,
            );
            data
        }
        "/api/index" => {
            let path = root.path().join("index.md");
            let markdown = if read.file(&path)? {
                Some(
                    String::from_utf8(crate::store::read_regular(&path)?)
                        .context("decoding index.md")?,
                )
            } else {
                None
            };
            value(json!({ "markdown": markdown }))?
        }
        "/api/lint" => value(maintain::lint(
            root,
            maintain::HeartbeatCheck::from_env(),
            read,
        )?)?,
        "/api/stale" => value(browse::stale(root, 90, Zoned::now().date(), read)?)?,
        "/api/raw-backlog" => value(maintain::raw_backlog(root, read)?)?,
        "/api/conflicts" => value(crate::semantic::conflict_report(root, 30, 0.7, read)?)?,
        "/api/misses" => value(search::misses(root)?)?,
        "/api/metrics" => value(maintain::metrics::report(root)?)?,
        "/api/lengths" => value(json!({ "notes": maintain::note_lengths(root, read)? }))?,
        "/api/map" => {
            let overlay = match query.flag("project_misses", false) {
                Ok(true) => crate::semantic::MissProjection::Provider,
                Ok(false) => crate::semantic::MissProjection::Offline,
                Err(error) => return Ok(Reply::error(400, &error.to_string())),
            };
            value(crate::semantic::map_report(root, overlay, read)?)?
        }
        "/api/context" => {
            let entered = |field| {
                query
                    .get(field)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            };
            let (Some(project), Some(host)) = (entered("project"), entered("host")) else {
                return Ok(Reply::error(400, "project and host are required"));
            };
            let mode = match query.flag("explain", false) {
                Ok(true) => browse::ContextMode::Explain,
                Ok(false) => browse::ContextMode::Brief,
                Err(error) => return Ok(Reply::error(400, &error.to_string())),
            };
            value(browse::context(
                root,
                project,
                host,
                Zoned::now().date(),
                mode,
                read,
            )?)?
        }
        "/api/usage" => value(usage::summary(root)?)?,
        "/api/graph" => value(browse::graph(root, read)?)?,
        "/api/timeline" => value(history::timeline(root, read)?)?,
        "/api/refs" => value(browse::refs(root, query.get("filter").unwrap_or(""), read)?)?,
        "/api/ref" => {
            let Some(path) = query.get("path") else {
                return Ok(Reply::error(400, "a reference path is required"));
            };
            let line = match query.get("line").unwrap_or("1").parse::<usize>() {
                Ok(line) if line != 0 => line,
                _ => return Ok(Reply::error(400, "line must be a positive integer")),
            };
            let Some(section) = browse::ref_section(root, path, line, read)? else {
                return Ok(Reply::error(404, "reference or section not found"));
            };
            let html = markdown::render(&section.markdown);
            let mut data = value(section)?;
            data.insert("body_html".into(), html.into());
            data
        }
        "/api/revision" => {
            let Some(view) = query.get("view").and_then(revision::View::parse) else {
                return Ok(Reply::error(400, "invalid Explorer view"));
            };
            value(json!({ "revision": revision::read(root, view, read)? }))?
        }
        _ => return Ok(Reply::error(404, "API endpoint not found")),
    };
    let mut data = data;
    let warnings = read.take_json_warnings();
    if !warnings.is_empty() {
        data.insert("inventory_warnings".into(), serde_json::to_value(warnings)?);
    }
    Reply::json(200, &data)
}

fn value(report: impl Serialize) -> Result<serde_json::Map<String, serde_json::Value>> {
    match serde_json::to_value(report)? {
        serde_json::Value::Object(data) => Ok(data),
        _ => anyhow::bail!("the Explorer expected a report object"),
    }
}
