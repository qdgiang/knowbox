mod note;

use std::io::Write;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::changes::{Event, append_note_entry, read_log, supersede, transaction, write_entry};
use crate::config::KbRoot;
use crate::journal::Stamp;
use crate::maintain::raw::{self, Inbox, Mark, PreparedRaw, Verdict, VerdictDecision};
use crate::notes::{self, sha1_hex};
use crate::store::WriteLock;

pub(crate) use crate::changes::new::keep_draft;
pub(crate) use note::EditArgs;
pub(in crate::changes) use note::links_with;
use note::{EditInput, Existing, NoteWrite};

const RECEIPT: &str = "<!-- knowbox-write-v1 ";

#[derive(Debug, Error)]
pub(crate) enum MutationError {
    #[error("draft has new lint errors: {}", errors.join("; "))]
    Lint { errors: Vec<String> },
    #[error("request key already belongs to a different operation")]
    ReusedRequest,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    Create {
        markdown: String,
        #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
        cli_title: Option<String>,
        #[serde(default)]
        related: Vec<String>,
        inbox: Option<Inbox>,
        followup: Option<Followup>,
    },
    Edit {
        selector: String,
        base_sha1: Option<String>,
        input: EditInput,
        inbox: Option<Inbox>,
        evidence: Option<String>,
    },
    Link {
        a: String,
        b: String,
    },
    Verdict {
        a: String,
        b: String,
        verdict: Verdict,
        reason: String,
        #[serde(default)]
        link: bool,
        winner: Option<String>,
        bases: Option<PairBases>,
    },
    Reject {
        inbox: Inbox,
        reason: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairBases {
    pub(crate) a: String,
    pub(crate) b: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Followup {
    Split {
        path: String,
        base_sha1: String,
        remaining: String,
    },
    Supersede {
        path: String,
        base_sha1: String,
        reason: String,
    },
}

impl Followup {
    fn load(&self, root: &KbRoot) -> Result<Existing> {
        let (path, hash) = match self {
            Self::Split {
                path, base_sha1, ..
            }
            | Self::Supersede {
                path, base_sha1, ..
            } => (path, base_sha1),
        };
        ensure!(
            path.starts_with("notes/"),
            "choose an exact note path for the original"
        );
        let note = Existing::load(root, path)?;
        note.check_base(Some(hash))?;
        supersede::require_live(&note.meta, note.id()?)?;
        Ok(note)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ResultNote {
    pub(crate) id: String,
    pub(crate) path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EditorSource {
    #[serde(flatten)]
    pub(crate) note: ResultNote,
    pub(crate) base_sha1: String,
    pub(crate) markdown: String,
    #[serde(flatten)]
    pub(crate) content: notes::Note,
}

pub(crate) fn editor_source(root: &KbRoot, selector: &str) -> Result<EditorSource> {
    let file = Existing::load(root, selector)?;
    Ok(EditorSource {
        note: ResultNote {
            id: file.id()?.to_owned(),
            path: file.relative,
        },
        base_sha1: sha1_hex(&file.bytes),
        markdown: String::from_utf8(file.bytes)?,
        content: file.meta,
    })
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Saved {
    pub(crate) event: Event,
    pub(crate) note: Option<ResultNote>,
    pub(crate) index_changed: bool,
    pub(crate) changed: bool,
}

pub(crate) struct Operation<'a> {
    key: &'a str,
    digest: String,
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    digest: String,
    saved: Saved,
}

impl<'a> Operation<'a> {
    pub(crate) fn new(key: &'a str, action: &Action) -> Result<Self> {
        ensure!(
            !key.is_empty()
                && key.len() <= 128
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
            "request key must contain 1–128 letters, digits, hyphens or underscores"
        );
        Ok(Self {
            key,
            digest: sha1_hex(&serde_json::to_vec(action)?),
        })
    }

    pub(crate) fn replay(&self, root: &KbRoot) -> Result<Option<(Saved, String)>> {
        let prefix = self.prefix();
        let log = read_log(root)?;
        let mut lines = log.lines().filter(|line| line.starts_with(&prefix));
        let Some(line) = lines.next() else {
            return Ok(None);
        };
        ensure!(
            lines.next().is_none(),
            "duplicate receipt for request {}",
            self.key
        );
        let json = line
            .strip_prefix(&prefix)
            .and_then(|text| text.strip_suffix(" -->"))
            .context("invalid journal write receipt")?;
        let receipt: Receipt =
            serde_json::from_str(json).context("reading journal write receipt")?;
        if receipt.digest != self.digest {
            return Err(MutationError::ReusedRequest.into());
        }
        Ok(Some((receipt.saved, line.to_owned())))
    }

    fn prefix(&self) -> String {
        format!("{RECEIPT}{} ", self.key)
    }
}

struct NoteEvent {
    action: &'static str,
    id: String,
    detail: String,
    writes: Vec<NoteWrite>,
}

pub(crate) struct Published {
    pub(crate) saved: Saved,
    pub(crate) receipt: Option<String>,
}

struct Plan {
    notes: Vec<NoteEvent>,
    raw: Option<PreparedRaw>,
    saved: Saved,
}

impl Plan {
    fn new(
        subject: String,
        stdout: String,
        note: Option<ResultNote>,
        notes: Vec<NoteEvent>,
        raw: Option<PreparedRaw>,
    ) -> Self {
        let index_changed = notes.iter().any(|event| !event.writes.is_empty());
        let changed = !notes.is_empty() || raw.is_some();
        Self {
            notes,
            raw,
            saved: Saved {
                event: Event { subject, stdout },
                note,
                index_changed,
                changed,
            },
        }
    }

    fn publish(
        self,
        lock: &WriteLock,
        stamp: &Stamp<'_>,
        operation: Option<&Operation<'_>>,
    ) -> Result<Published> {
        let Plan {
            notes,
            raw,
            mut saved,
        } = self;
        if notes.is_empty() && raw.is_none() && operation.is_none() {
            return Ok(Published {
                saved,
                receipt: None,
            });
        }
        let mut log = read_log(lock.root())?.into_bytes();
        let mut files = Vec::new();
        for event in &notes {
            let writes: Vec<_> = event.writes.iter().map(NoteWrite::file_write).collect();
            if writes.is_empty() {
                write_entry(&mut log, stamp, event.action, &event.id, &event.detail, &[])?;
            } else {
                append_note_entry(
                    lock,
                    &mut log,
                    stamp,
                    event.action,
                    &event.id,
                    &event.detail,
                    &writes,
                )?;
                files.extend(writes);
            }
        }
        if let Some(raw) = &raw {
            write_entry(&mut log, stamp, raw.action, &raw.id, &raw.detail, &[])?;
            files.push(raw.file_write());
        }
        let receipt = if let Some(operation) = operation {
            let payload = Receipt {
                digest: operation.digest.clone(),
                saved,
            };
            let json = serde_json::to_string(&payload)?
                .replace('<', "\\u003c")
                .replace('>', "\\u003e");
            let line = format!("{}{json} -->", operation.prefix());
            if !log.is_empty() && !log.ends_with(b"\n") {
                log.push(b'\n');
            }
            writeln!(log, "{line}")?;
            saved = payload.saved;
            Some(line)
        } else {
            None
        };
        transaction::apply(lock, &log, &files)?;
        Ok(Published { saved, receipt })
    }
}

pub(crate) fn apply(
    lock: &WriteLock,
    action: Action,
    stamp: &Stamp<'_>,
    operation: Option<&Operation<'_>>,
) -> Result<Published> {
    let plan = match action {
        Action::Create {
            markdown,
            cli_title,
            related,
            inbox,
            followup,
        } => {
            let draft = match cli_title.as_deref() {
                Some(title) => note::CreateDraft::Cli {
                    markdown: &markdown,
                    title,
                },
                None => note::CreateDraft::Browser(&markdown),
            };
            let source = followup
                .as_ref()
                .map(|item| item.load(lock.root()))
                .transpose()?;
            let primary = source.as_ref().map(Existing::id).transpose()?;
            let supersedes = if matches!(followup, Some(Followup::Supersede { .. })) {
                primary
            } else {
                None
            };
            let note::NewNote {
                id,
                title,
                relative,
                mut writes,
            } = note::create(
                lock,
                draft,
                note::CreateLinks {
                    related: &related,
                    primary,
                    supersedes,
                },
                stamp,
            )?;
            let related_writes = writes.split_off(1);
            let mut notes = vec![NoteEvent {
                action: "create",
                id: id.clone(),
                detail: title.clone(),
                writes,
            }];
            if !related_writes.is_empty() {
                notes.push(NoteEvent {
                    action: "link",
                    id: id.clone(),
                    detail: format!("linked with {}", related.join(", ")),
                    writes: related_writes,
                });
            }
            if let Some((original, followup)) = source.zip(followup) {
                let old = original.id()?.to_owned();
                let date = stamp.date().to_string();
                let event = match followup {
                    Followup::Split { remaining, .. } => {
                        let input = EditInput::Patch {
                            body: Some(remaining),
                            fields: Default::default(),
                        };
                        let edited = note::edit(lock, &original, &input, None, stamp)?;
                        let after = note::patch_link(&edited, &notes::parse(&edited)?, &id, &date)?;
                        NoteEvent {
                            action: "edit",
                            id: old,
                            detail: format!("split body into {id}"),
                            writes: vec![original.changed(after)],
                        }
                    }
                    Followup::Supersede { reason, .. } => {
                        let detail = supersede::detail(&id, Some(&reason))?;
                        let after =
                            supersede::mark_retired(&original.bytes, &original.meta, &id, &date)?;
                        NoteEvent {
                            action: "supersede",
                            id: old,
                            detail,
                            writes: vec![original.changed(after)],
                        }
                    }
                };
                notes.push(event);
            }
            let raw = inbox
                .as_ref()
                .map(|item| item.prepare(lock, Mark::Ingested(&id), stamp))
                .transpose()?;
            Plan::new(
                format!("knowbox: create {id} — {title}"),
                format!("created {id}"),
                Some(ResultNote { id, path: relative }),
                notes,
                raw,
            )
        }
        Action::Edit {
            selector,
            base_sha1,
            input,
            inbox,
            evidence,
        } => {
            let original = Existing::load(lock.root(), &selector)?;
            original.check_base(base_sha1.as_deref())?;
            let id = original.id()?.to_owned();
            let relative = original.relative.clone();
            let edited = note::edit(lock, &original, &input, evidence.as_deref(), stamp)?;
            let mut events = Vec::new();
            if edited != original.bytes {
                let mut fields = changed_fields(&original.meta, &notes::parse(&edited)?);
                if let Some(inbox) = &inbox {
                    fields.push(format!("applied raw/{} #{}", inbox.file, inbox.number));
                }
                let detail = fields.join(", ");
                events.push(NoteEvent {
                    action: "edit",
                    id: id.clone(),
                    detail,
                    writes: vec![original.changed(edited)],
                });
            }
            if let Some(evidence) = evidence {
                events.push(NoteEvent {
                    action: "verify",
                    id: id.clone(),
                    detail: evidence,
                    writes: Vec::new(),
                });
            }
            let raw = inbox
                .as_ref()
                .map(|item| item.prepare(lock, Mark::Ingested(&id), stamp))
                .transpose()?;
            let message = if events.is_empty() && raw.is_none() {
                format!("{id}: no changes")
            } else {
                format!("saved {id}")
            };
            Plan::new(
                format!("knowbox: edit {id}"),
                message,
                Some(ResultNote { id, path: relative }),
                events,
                raw,
            )
        }
        Action::Link { a, b } => {
            let (a, b) = pair(lock, &a, &b)?;
            let subject = format!("knowbox: link {} and {}", a.id()?, b.id()?);
            let message = format!("{} linked with {}", a.id()?, b.id()?);
            let events = link(a, b, stamp)?.into_iter().collect();
            Plan::new(subject, message, None, events, None)
        }
        Action::Verdict {
            a,
            b,
            verdict,
            reason,
            link: link_notes,
            winner,
            bases,
        } => {
            ensure!(
                !link_notes || matches!(verdict, Verdict::Complementary),
                "only a complementary verdict can link both notes"
            );
            ensure!(
                winner.is_none() || matches!(verdict, Verdict::Duplicate | Verdict::Contradiction),
                "a winner applies to a duplicate or contradiction verdict"
            );
            let (a, b) = pair(lock, &a, &b)?;
            if let Some(bases) = bases {
                a.check_base(Some(&bases.a))?;
                b.check_base(Some(&bases.b))?;
            }
            let ids = [a.id()?, b.id()?];
            let subject = format!("knowbox: verdict {} and {}", ids[0], ids[1]);
            let (raw, message) = match raw::prepare_verdict(lock, ids, verdict, &reason, stamp)? {
                VerdictDecision::Prepared(raw) => (
                    Some(raw),
                    format!(
                        "recorded {} verdict for {} and {}",
                        verdict.as_str(),
                        ids[0],
                        ids[1]
                    ),
                ),
                VerdictDecision::AlreadyRecorded => (
                    None,
                    format!(
                        "{} verdict already recorded for {} and {}",
                        verdict.as_str(),
                        ids[0],
                        ids[1]
                    ),
                ),
            };
            let events = if link_notes {
                link(a, b, stamp)?.into_iter().collect()
            } else if let Some(winner) = winner {
                let (old, kept) = if winner == ids[0] {
                    (b, a)
                } else if winner == ids[1] {
                    (a, b)
                } else {
                    anyhow::bail!("choose one of the reviewed IDs as the winner")
                };
                retire_duplicate(old, kept, &reason, stamp)?
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            };
            Plan::new(subject, message, None, events, raw)
        }
        Action::Reject { inbox, reason } => {
            let raw = inbox.prepare(lock, Mark::Rejected(&reason), stamp)?;
            let subject = format!("knowbox: raw-mark {} #{}", inbox.file, inbox.number);
            let message = format!("rejected {} #{}", inbox.file, inbox.number);
            Plan::new(subject, message, None, Vec::new(), Some(raw))
        }
    };
    plan.publish(lock, stamp, operation)
}

fn retire_duplicate(
    old: Existing,
    kept: Existing,
    reason: &str,
    stamp: &Stamp<'_>,
) -> Result<Option<NoteEvent>> {
    let (old_id, kept_id) = (old.id()?.to_owned(), kept.id()?.to_owned());
    supersede::require_live(&kept.meta, &kept_id)?;
    if old.meta.scalar("status") == Some("superseded")
        && kept.meta.items("supersedes").contains(&old_id.as_str())
        && old.meta.items("related").contains(&kept_id.as_str())
    {
        return Ok(None);
    }
    supersede::require_live(&old.meta, &old_id)?;
    let date = stamp.date().to_string();
    let detail = supersede::detail(&kept_id, Some(reason))?;
    let retired = supersede::mark_retired(&old.bytes, &old.meta, &kept_id, &date)?;
    let successor = supersede::mark_successor(&kept.bytes, &kept.meta, &old_id, &date)?;
    Ok(Some(NoteEvent {
        action: "supersede",
        id: old_id,
        detail,
        writes: vec![old.changed(retired), kept.changed(successor)],
    }))
}

fn pair(lock: &WriteLock, first: &str, second: &str) -> Result<(Existing, Existing)> {
    let a = Existing::load(lock.root(), first)?;
    let b = Existing::load(lock.root(), second)?;
    ensure!(a.id()? != b.id()?, "choose two different notes");
    Ok((a, b))
}

fn link(a: Existing, b: Existing, stamp: &Stamp<'_>) -> Result<Option<NoteEvent>> {
    let (a_id, b_id) = (a.id()?.to_owned(), b.id()?.to_owned());
    let date = stamp.date().to_string();
    let mut writes = Vec::new();
    for (note, target) in [(a, &b_id), (b, &a_id)] {
        if let Some(bytes) = note::add_link(&note, target, &date)? {
            writes.push(note.changed(bytes));
        }
    }
    Ok((!writes.is_empty()).then(|| NoteEvent {
        action: "link",
        id: a_id,
        detail: format!("linked with {b_id}"),
        writes,
    }))
}

fn changed_fields(before: &notes::Note, after: &notes::Note) -> Vec<String> {
    let mut fields = Vec::new();
    if before.body != after.body {
        fields.push("body".into());
    }
    let keys: std::collections::BTreeSet<_> = before.meta.keys().chain(after.meta.keys()).collect();
    for key in keys {
        if key != "updated" && before.meta.get(key) != after.meta.get(key) {
            fields.push(key.clone());
        }
    }
    if fields.is_empty() {
        fields.push("formatting".into());
    }
    fields
}
