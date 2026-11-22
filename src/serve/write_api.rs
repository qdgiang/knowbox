use std::io::Read;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use tiny_http::Request;

use crate::changes::mutation::{self, Action, MutationError};
use crate::changes::transaction::{ChangedNote, PendingWrite};
use crate::config::KbRoot;
use crate::git::GitError;
use crate::journal::Actor;
use crate::maintain::{DraftTarget, lint_draft, raw::ReviewError};
use crate::store::{Inventory, InventoryWarnings, StoreError, WriteLock};
use crate::write_cli::{self, IncompleteWrite};

use super::{Query, Reply, header, markdown};

const MAX_BODY: usize = 1024 * 1024;

pub(super) enum Route {
    Write,
    Draft,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteRequest {
    request_id: String,
    operation: Action,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    path: Option<String>,
    markdown: String,
}

pub(super) fn source(root: &KbRoot, query: &Query) -> Reply {
    let Some(target) = query.get("target") else {
        return Reply::error(400, "choose a note to open in the Editor");
    };
    match mutation::editor_source(root, target).and_then(|data| Reply::json(200, &data)) {
        Ok(reply) => reply,
        Err(error) if error.to_string().starts_with("not found:") => {
            Reply::error(404, &error.to_string())
        }
        Err(error) => failure(&error),
    }
}

pub(super) fn post(root: &KbRoot, actor: &Actor, route: Route, request: &mut Request) -> Reply {
    if !matches!(header(request, "Content-Type"),
        Ok(Some(value)) if value.split(';').next().is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json")))
    {
        return Reply::error(415, "send an application/json body");
    }
    let Some(length) = request.body_length() else {
        return Reply::error(411, "send a bounded request body");
    };
    if length > MAX_BODY {
        return Reply::error(413, "draft request exceeds 1 MiB");
    }
    let mut bytes = Vec::with_capacity(length);
    if let Err(error) = request
        .as_reader()
        .take(u64::try_from(length).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
    {
        return Reply::error(400, &format!("reading the request body: {error}"));
    }
    if bytes.len() != length {
        return Reply::error(400, "request body ended early");
    }
    let result = match route {
        Route::Write => write(root, actor, &bytes),
        Route::Draft => preview(root, &bytes),
    };
    result.unwrap_or_else(|error| failure(&error))
}

fn write(root: &KbRoot, actor: &Actor, bytes: &[u8]) -> Result<Reply> {
    let WriteRequest {
        request_id,
        operation,
    } = serde_json::from_slice(bytes).context("reading the write request")?;
    if let Action::Edit {
        selector,
        base_sha1,
        ..
    } = &operation
    {
        ensure!(
            selector.starts_with("notes/") && base_sha1.is_some(),
            "browser edits require the note path and its original SHA-1"
        );
    }
    if let Action::Verdict { a, b, bases, .. } = &operation {
        ensure!(
            a.starts_with("notes/") && b.starts_with("notes/") && bases.is_some(),
            "browser verdicts require both note paths and original SHA-1 hashes"
        );
    }
    Reply::json(
        200,
        &write_cli::mutate_locked(
            &WriteLock::try_acquire(root)?,
            operation,
            Some(&request_id),
            actor,
        )?,
    )
}

fn preview(root: &KbRoot, bytes: &[u8]) -> Result<Reply> {
    let Draft {
        path,
        markdown: text,
    } = serde_json::from_slice(bytes).context("reading the draft request")?;
    let target = path
        .as_deref()
        .map_or(DraftTarget::Standalone, DraftTarget::Replace);
    let mut warnings = InventoryWarnings::default();
    let lint = lint_draft(
        root,
        target,
        text.as_bytes(),
        &mut Inventory::Read(&mut warnings),
    )?;
    let body_html = crate::notes::parse(text.as_bytes())
        .map_or_else(|_| String::new(), |note| markdown::render(&note.body));
    Reply::json(200, &json!({ "lint": lint, "body_html": body_html }))
}

fn failure(error: &anyhow::Error) -> Reply {
    let status;
    let value = match error.downcast_ref::<MutationError>() {
        None if let Some(ChangedNote {
            path,
            current_sha1,
            current_markdown,
        }) = error.downcast_ref::<ChangedNote>() =>
        {
            status = 409;
            json!({ "error": error.to_string(), "code": "changed", "path": path,
                "current_sha1": current_sha1, "current_markdown": current_markdown })
        }
        Some(MutationError::Lint { errors }) => {
            status = 422;
            json!({ "error": "draft has new lint errors", "code": "lint", "findings": errors })
        }
        Some(MutationError::ReusedRequest) => {
            status = 409;
            json!({ "error": error.to_string(), "code": "request_key_reused" })
        }
        None if let Some(incomplete) = error.downcast_ref::<IncompleteWrite>() => {
            status = 500;
            let committed = matches!(
                incomplete.source.downcast_ref::<GitError>(),
                Some(GitError::Publish { .. })
            );
            json!({ "error": format!("{error:#}"), "code": "incomplete", "step": incomplete.step,
                "committed_locally": committed, "saved": incomplete.saved })
        }
        None if error.is::<PendingWrite>() => {
            status = 500;
            json!({ "error": format!("{error:#}"), "code": "incomplete" })
        }
        None if error.is::<ReviewError>() => {
            status = 409;
            json!({ "error": format!("{error:#}"), "code": "review_changed" })
        }
        None if matches!(
            error.downcast_ref::<StoreError>(),
            Some(StoreError::Contended { .. })
        ) =>
        {
            status = 409;
            json!({ "error": format!("{error:#}"), "code": "writer_busy" })
        }
        None if let Some(git) = error.downcast_ref::<GitError>() => {
            status = 409;
            let code = match git {
                GitError::Staged => "git_staged",
                GitError::InProgress(_) | GitError::Conflicts => "git_in_progress",
                GitError::IndexLocked(_) => "git_index_locked",
                _ => "git_error",
            };
            json!({ "error": format!("{error:#}"), "code": code })
        }
        None => {
            status = 422;
            json!({ "error": format!("{error:#}"), "code": "invalid_draft" })
        }
    };
    Reply::json(status, &value).unwrap_or_else(|error| Reply::error(500, &error.to_string()))
}
