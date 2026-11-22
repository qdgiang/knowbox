use anyhow::{Result, ensure};

use super::{Findings, LINK_FIELDS, LintReport, Location, NoteDates};
use crate::changes;
use crate::config::KbRoot;
use crate::notes::{self, is_id};
use crate::store::Inventory;

const STANDALONE_PATH: &str = "draft.md";

#[derive(Clone, Copy)]
pub(crate) enum DraftTarget<'a> {
    Standalone,
    Replace(&'a str),
    Create(&'a str),
}

pub(crate) fn lint_draft(
    root: &KbRoot,
    target: DraftTarget<'_>,
    bytes: &[u8],
    policy: &mut Inventory<'_>,
) -> Result<LintReport> {
    let (relative, replacing) = match target {
        DraftTarget::Standalone => (STANDALONE_PATH, None),
        DraftTarget::Replace(path) | DraftTarget::Create(path) => {
            ensure!(
                path.strip_prefix("notes/").is_some_and(|name| {
                    name.ends_with(".md") && !name.contains(['/', '\\', ':', '\0'])
                }),
                "draft target must be a direct file under notes/"
            );
            (
                path,
                matches!(target, DraftTarget::Replace(_)).then_some(path),
            )
        }
    };
    let mut findings = Findings::default();
    let note = match notes::parse(bytes) {
        Ok(note) => note,
        Err(error) => {
            findings.error(
                "note_parse",
                relative.into(),
                format!("{relative}: {error}"),
            );
            return Ok(findings.report(1));
        }
    };
    let location = Location::note(relative, &note);
    let text = std::str::from_utf8(bytes)?;
    if let Err(error) = changes::validate_frontmatter(text) {
        findings.error(
            "draft_frontmatter",
            location,
            format!("{relative}: {error:#}"),
        );
    }
    let id = note.text("id", "?");
    let pending = matches!(target, DraftTarget::Standalone) && id == "pending";
    if !pending {
        findings.inspect_id(relative, &id, &note);
    }
    let entries = notes::snapshot_with(root, policy)?;
    ensure!(
        replacing.is_none_or(|path| entries.iter().any(|entry| entry.relative == path)),
        "draft target {relative} no longer exists"
    );
    let others: Vec<_> = entries
        .iter()
        .filter(|entry| Some(entry.relative.as_str()) != replacing)
        .filter_map(|entry| {
            entry
                .parsed
                .as_ref()
                .ok()
                .map(|note| (&entry.relative, note))
        })
        .collect();
    for (path, other) in &others {
        if other.scalar("id") == Some(id.as_ref()) {
            findings.error(
                "duplicate_id",
                location,
                format!("{relative}: duplicate id {id} (also {path})"),
            );
        }
        if note
            .scalar("title")
            .is_some_and(|title| other.scalar("title") == Some(title))
        {
            findings.warning(
                "duplicate_title",
                location,
                format!("{relative}: duplicate title (also {path})"),
            );
        }
    }
    if !matches!(target, DraftTarget::Standalone) {
        findings.inspect_filename(relative, &id, &note);
    }
    let dates = if pending
        && note.scalar("created") == Some("pending")
        && note.scalar("updated") == Some("pending")
    {
        NoteDates::Pending
    } else {
        NoteDates::Validate
    };
    findings.inspect_note(relative, &note, dates);
    for field in ["title", "domain", "summary"] {
        if note.meta.contains_key(field) && note.scalar(field).is_none() {
            findings.error(
                "invalid_scalar",
                location,
                format!("{relative}: {field} must be a scalar"),
            );
        }
    }
    if note.scalar("type") == Some("session")
        && note
            .scalar("project")
            .is_none_or(|project| project.trim().is_empty())
    {
        findings.error(
            "missing_project",
            location,
            format!("{relative}: session notes require a nonempty project field"),
        );
    }
    for field in LINK_FIELDS {
        for link in note.items(field).into_iter().filter(|link| is_id(link)) {
            if link == id.as_ref() {
                findings.error(
                    "self_link",
                    location,
                    format!("{relative}: {field} links to itself"),
                );
            }
        }
    }
    findings.inspect_link_targets(relative, &note, |link| {
        link == id.as_ref()
            || others
                .iter()
                .any(|(_, other)| other.scalar("id") == Some(link))
    });
    for (_, message) in policy.take_warnings() {
        findings.warning("inventory_skipped", Location::default(), message);
    }
    Ok(findings.report(1))
}
