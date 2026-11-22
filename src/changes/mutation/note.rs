use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::config::KbRoot;
use crate::journal::Stamp;
use crate::maintain::{DraftTarget, lint_draft};
use crate::notes::{self, Note};
use crate::store::{Inventory, WriteLock};

use super::{Action, MutationError};
use crate::changes::transaction::ChangedNote;
use crate::changes::{FileWrite, actual_id, new, next_id, read_note, rewrite, single_line};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum EditInput {
    Source {
        markdown: String,
    },
    Patch {
        body: Option<String>,
        fields: BTreeMap<String, String>,
    },
}

#[derive(Debug, clap::Args)]
pub(crate) struct EditArgs {
    selector: String,
    /// Expected SHA-1 of the file bytes when the draft was opened.
    #[arg(long)]
    base_sha1: Option<String>,
    /// Read the replacement body from stdin.
    #[arg(long, conflicts_with = "source_stdin")]
    body_stdin: bool,
    /// Read a complete Markdown note, including frontmatter, from stdin.
    #[arg(long, conflicts_with = "fields")]
    source_stdin: bool,
    /// Set a frontmatter field. Repeat for additional fields.
    #[arg(long = "set", value_name = "FIELD=VALUE")]
    fields: Vec<String>,
    /// Reuse this key when retrying an interrupted save.
    #[arg(long, value_name = "KEY")]
    request_id: Option<String>,
}

pub(crate) struct CliEdit {
    pub(crate) action: Action,
    pub(crate) key: Option<String>,
    pub(crate) draft: Option<TempDir>,
}

impl EditArgs {
    pub(crate) fn prepare(self, root: &KbRoot, err: &mut impl Write) -> Result<CliEdit> {
        let mut fields = BTreeMap::new();
        for assignment in self.fields {
            let (field, value) = assignment
                .split_once('=')
                .context("--set expects FIELD=VALUE")?;
            ensure!(
                fields.insert(field.to_owned(), value.to_owned()).is_none(),
                "repeated --set for {field}"
            );
        }
        let stdin = io::stdin();
        let mut base_sha1 = self.base_sha1;
        let mut draft = None;
        let input =
            if !self.body_stdin && !self.source_stdin && fields.is_empty() && stdin.is_terminal() {
                let note = Existing::load(root, &self.selector)?;
                note.check_base(base_sha1.as_deref())?;
                base_sha1 = Some(notes::sha1_hex(&note.bytes));
                let (markdown, temp) = new::editor_draft(
                    root,
                    &note.bytes,
                    "knowbox: editing a private copy of the note.",
                    err,
                )?;
                draft = Some(temp);
                EditInput::Source { markdown }
            } else if self.source_stdin {
                EditInput::Source {
                    markdown: read_stdin()?,
                }
            } else {
                let body = if self.body_stdin || fields.is_empty() {
                    let body = read_stdin()?;
                    ensure!(
                        !body.is_empty() || self.body_stdin,
                        "empty stdin; pass --body-stdin to clear the body"
                    );
                    Some(body)
                } else {
                    None
                };
                EditInput::Patch { body, fields }
            };
        Ok(CliEdit {
            action: Action::Edit {
                selector: self.selector,
                base_sha1,
                input,
                inbox: None,
                evidence: None,
            },
            key: self.request_id,
            draft,
        })
    }
}

fn read_stdin() -> Result<String> {
    let mut value = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut value)
        .context("reading UTF-8 draft from stdin")?;
    Ok(value)
}

pub(super) struct Existing {
    pub(super) path: PathBuf,
    pub(super) relative: String,
    pub(super) bytes: Vec<u8>,
    pub(super) meta: Note,
}

impl Existing {
    pub(super) fn load(root: &KbRoot, selector: &str) -> Result<Self> {
        let path = notes::find_for_write(root, selector)?
            .with_context(|| format!("not found: {selector}"))?;
        let bytes = read_note(&path)?;
        let meta = notes::parse(&bytes).with_context(|| format!("parsing {}", path.display()))?;
        let id = actual_id(&meta)?;
        ensure!(
            !notes::is_id(selector) || selector == id,
            "{selector} resolves to a file with id {id}"
        );
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("note filename is not UTF-8")?;
        Ok(Self {
            relative: format!("notes/{name}"),
            path,
            bytes,
            meta,
        })
    }

    pub(super) fn id(&self) -> Result<&str> {
        actual_id(&self.meta)
    }

    pub(super) fn check_base(&self, expected: Option<&str>) -> Result<()> {
        if let Some(expected) = expected {
            ensure!(
                expected.len() == 40 && expected.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "base SHA-1 must have 40 hexadecimal digits"
            );
            let actual = notes::sha1_hex(&self.bytes);
            if !expected.eq_ignore_ascii_case(&actual) {
                return Err(ChangedNote {
                    path: self.relative.clone(),
                    current_sha1: actual,
                    current_markdown: String::from_utf8_lossy(&self.bytes).into_owned(),
                }
                .into());
            }
        }
        Ok(())
    }

    pub(super) fn changed(self, after: Vec<u8>) -> NoteWrite {
        NoteWrite {
            path: self.path,
            before: Some(self.bytes),
            after,
        }
    }
}

pub(super) struct NoteWrite {
    pub(super) path: PathBuf,
    pub(super) before: Option<Vec<u8>>,
    pub(super) after: Vec<u8>,
}

impl NoteWrite {
    pub(super) fn file_write(&self) -> FileWrite<'_> {
        FileWrite {
            path: &self.path,
            before: self.before.as_deref(),
            after: &self.after,
        }
    }
}

pub(super) fn edit(
    lock: &WriteLock,
    original: &Existing,
    input: &EditInput,
    evidence: Option<&str>,
    stamp: &Stamp<'_>,
) -> Result<Vec<u8>> {
    let mut edited = match input {
        EditInput::Source { markdown } => {
            let parsed = notes::parse(markdown.as_bytes())?;
            for key in ["id", "created"] {
                ensure!(
                    parsed.scalar(key) == original.meta.scalar(key),
                    "edit cannot change {key}"
                );
            }
            markdown.as_bytes().to_vec()
        }
        EditInput::Patch { body, fields } => {
            let mut updates = Vec::new();
            for (key, value) in fields {
                ensure!(
                    !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_'),
                    "invalid field {key:?}"
                );
                ensure!(
                    !matches!(key.as_str(), "id" | "created" | "updated"),
                    "edit cannot set {key}"
                );
                single_line(key, value)?;
                updates.push((key.as_str(), value.as_str()));
            }
            rewrite::patch(&original.bytes, &updates, body.as_deref())?
        }
    };
    if edited == original.bytes && evidence.is_none() {
        return Ok(edited);
    }
    let date = stamp.date().to_string();
    let mut fields = vec![("updated", date.as_str())];
    if let Some(evidence) = evidence {
        single_line("evidence", evidence)?;
        ensure!(!evidence.trim().is_empty(), "evidence must not be blank");
        ensure!(
            original.meta.scalar("status") != Some("superseded"),
            "cannot verify a superseded note"
        );
        fields.extend([("status", "verified"), ("verified", date.as_str())]);
    } else if original.meta.scalar("status") == Some("verified")
        && original.meta.scalar("verified") == Some(date.as_str())
        && notes::parse(&edited)?.scalar("status") == Some("verified")
    {
        // With day-granularity dates, a same-day edit needs an explicit status change.
        fields.push(("status", "active"));
    }
    edited = rewrite::patch(&edited, &fields, None)?;
    reject_new_errors(
        lock,
        DraftTarget::Replace(&original.relative),
        &edited,
        Some(&original.bytes),
    )?;
    Ok(edited)
}

pub(super) struct NewNote {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) relative: String,
    pub(super) writes: Vec<NoteWrite>,
}

pub(super) struct CreateLinks<'a> {
    pub(super) related: &'a [String],
    pub(super) primary: Option<&'a str>,
    pub(super) supersedes: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub(super) enum CreateDraft<'a> {
    Browser(&'a str),
    Cli { markdown: &'a str, title: &'a str },
}

pub(super) fn create(
    lock: &WriteLock,
    draft: CreateDraft<'_>,
    links: CreateLinks<'_>,
    stamp: &Stamp<'_>,
) -> Result<NewNote> {
    let (markdown, title) = match draft {
        CreateDraft::Browser(markdown) => (markdown, new::edited_title(markdown)?),
        CreateDraft::Cli { markdown, title } => (markdown, title.to_owned()),
    };
    let id = next_id(lock.root())?;
    let date = stamp.date().to_string();
    let relative = format!("notes/{id}-{}.md", new::slug(&title));
    let mut after = rewrite::patch(
        markdown.as_bytes(),
        &[("id", &id), ("created", &date), ("updated", &date)],
        None,
    )?;
    let mut writes = Vec::new();
    let mut ids: Vec<_> = links
        .related
        .iter()
        .map(String::as_str)
        .chain(links.primary)
        .collect();
    if !ids.is_empty() {
        let meta = notes::parse(&after)?;
        ensure!(
            ids.iter().all(|id| notes::is_id(id)),
            "--related requires exact kb-IDs"
        );
        ids.sort_unstable();
        ids.dedup();
        let value = links_with(&meta, "related", &ids)?;
        after = rewrite::patch(&after, &[("related", &value)], None)?;
        for target in ids
            .into_iter()
            .filter(|target| Some(*target) != links.primary)
        {
            let original = Existing::load(lock.root(), target)?;
            if let Some(bytes) = add_link(&original, &id, &date)? {
                writes.push(original.changed(bytes));
            }
        }
    }
    if let Some(old) = links.supersedes {
        let meta = notes::parse(&after)?;
        crate::changes::supersede::require_live(&meta, &id)?;
        let supersedes = links_with(&meta, "supersedes", &[old])?;
        after = rewrite::patch(&after, &[("supersedes", &supersedes)], None)?;
    }
    if matches!(draft, CreateDraft::Browser(_)) {
        reject_new_errors(lock, DraftTarget::Create(&relative), &after, None)?;
    }
    writes.insert(
        0,
        NoteWrite {
            path: lock.root().path().join(&relative),
            before: None,
            after,
        },
    );
    Ok(NewNote {
        id,
        title,
        relative,
        writes,
    })
}

pub(super) fn add_link(note: &Existing, id: &str, date: &str) -> Result<Option<Vec<u8>>> {
    if note.meta.items("related").contains(&id) {
        return Ok(None);
    }
    Ok(Some(patch_link(&note.bytes, &note.meta, id, date)?))
}

pub(super) fn patch_link(bytes: &[u8], meta: &Note, id: &str, date: &str) -> Result<Vec<u8>> {
    let value = links_with(meta, "related", &[id])?;
    rewrite::patch(bytes, &[("related", &value), ("updated", date)], None)
}

pub(in crate::changes) fn links_with(
    note: &Note,
    key: &str,
    additional: &[&str],
) -> Result<String> {
    let mut links = note.items(key);
    ensure!(
        links.iter().all(|link| notes::is_id(link)),
        "non-ID item in {key}; refusing to rewrite"
    );
    for &id in additional {
        if !links.contains(&id) {
            links.push(id);
        }
    }
    Ok(format!("[{}]", links.join(", ")))
}

fn reject_new_errors(
    lock: &WriteLock,
    target: DraftTarget<'_>,
    after: &[u8],
    before: Option<&[u8]>,
) -> Result<()> {
    let report = lint_draft(lock.root(), target, after, &mut Inventory::Strict)?;
    if report.has_errors() {
        let baseline = before
            .map(|bytes| lint_draft(lock.root(), target, bytes, &mut Inventory::Strict))
            .transpose()?;
        let errors = report.introduced_errors(baseline.as_ref());
        if !errors.is_empty() {
            return Err(MutationError::Lint { errors }.into());
        }
    }
    Ok(())
}
