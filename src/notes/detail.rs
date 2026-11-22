use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::KbRoot;
use crate::store::Inventory;

use super::{
    Note, find_with, id_prefix, inventory, is_id, normalize_newlines, parse, read_with, sha1_hex,
};

#[derive(Debug, Serialize)]
pub(crate) struct Detail {
    pub(crate) id: Option<String>,
    path: String,
    base_sha1: String,
    markdown: String,
    #[serde(flatten)]
    content: Content,
    edited_since_verify: Option<VerificationDrift>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Content {
    Parsed {
        #[serde(flatten)]
        note: Note,
    },
    Malformed {
        parse_error: String,
    },
}

#[derive(Debug, Serialize)]
struct VerificationDrift {
    updated: String,
    verified: String,
}

impl Detail {
    pub(crate) fn render(&self, out: &mut impl Write, err: &mut impl Write) -> io::Result<()> {
        if let Some(VerificationDrift { updated, verified }) = &self.edited_since_verify {
            writeln!(
                err,
                "knowbox show: WARNING edited after last verification (updated {updated} > verified {verified}) — content changed since it was last confirmed; re-verify before relying on it"
            )?;
        }
        writeln!(out, "{}", self.markdown)
    }
}

pub(crate) fn read_detail(
    root: &KbRoot,
    selector: &str,
    policy: &mut Inventory<'_>,
) -> Result<Detail> {
    let missing = || format!("not found: {selector}");
    let path = find_with(root, selector, policy)?.with_context(missing)?;
    from_file(&path, selector, policy)
}

pub(crate) fn read_detail_path(
    root: &KbRoot,
    relative: &str,
    policy: &mut Inventory<'_>,
) -> Result<Detail> {
    let directory = root.path().join("notes");
    let missing = || format!("not found: {relative}");
    let name = relative
        .strip_prefix("notes/")
        .filter(|name| name.ends_with(".md") && !name.contains('/'))
        .with_context(missing)?;
    // Match the spelling emitted by the inventory, even on case-insensitive volumes.
    if !inventory(&directory, policy)?
        .iter()
        .any(|candidate| candidate == name)
    {
        anyhow::bail!("{}", missing());
    }
    from_file(&directory.join(name), relative, policy)
}

fn from_file(path: &Path, selector: &str, policy: &mut Inventory<'_>) -> Result<Detail> {
    let bytes = read_with(path, policy)?.with_context(|| format!("not found: {selector}"))?;
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("reading note name {}", path.display()))?;
    let source = std::str::from_utf8(&bytes)
        .with_context(|| format!("decoding {} as UTF-8", path.display()))?;
    let parsed = parse(&bytes);
    let note = parsed.as_ref().ok();
    let id = note
        .and_then(|note| note.scalar("id"))
        .filter(|id| is_id(id))
        .or_else(|| id_prefix(basename))
        .map(str::to_owned);
    let edited_since_verify = note.and_then(|note| {
        let verified = note.text("verified", "");
        let updated = note.text("updated", "");
        (!verified.is_empty() && updated > verified).then(|| VerificationDrift {
            updated: updated.into_owned(),
            verified: verified.into_owned(),
        })
    });
    Ok(Detail {
        id,
        path: format!("notes/{basename}"),
        base_sha1: sha1_hex(&bytes),
        markdown: normalize_newlines(source).into_owned(),
        content: match parsed {
            Ok(note) => Content::Parsed { note },
            Err(error) => Content::Malformed {
                parse_error: error.to_string(),
            },
        },
        edited_since_verify,
    })
}
