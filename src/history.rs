mod diffs;
mod journal_rows;
mod lifetimes;

use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::io::Write;

use anyhow::{Context, Result};
use jiff::Timestamp;
use serde::Serialize;

use crate::changes::audit::Transition;
use crate::config::KbRoot;
use crate::git::history::{self as git, Commit, History};
use crate::maintain::{self, RawEvidence};
use crate::{notes, store::Inventory};

use diffs::{Diff, Patch};
use journal_rows::Occurrence;
use lifetimes::{Lifetime, Lifetimes};

#[derive(Clone, Copy)]
enum Selection<'a> {
    Note(&'a str),
    Timeline,
}

impl<'a> Selection<'a> {
    fn id(self) -> Option<&'a str> {
        match self {
            Self::Note(id) => Some(id),
            Self::Timeline => None,
        }
    }

    fn includes(self, ids: &[String]) -> bool {
        self.id()
            .is_none_or(|id| ids.iter().any(|related| related == id))
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Coverage {
    GitAndJournal,
    PartialGitAndJournal,
    JournalOnly,
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    id: Option<String>,
    coverage: Coverage,
    lifetimes: Vec<Lifetime>,
    events: Vec<Event>,
    patches: Vec<Patch>,
    diagnostics: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Precision {
    Date,
    Timestamp,
}

#[derive(Debug, Serialize)]
struct Event {
    time: Option<String>,
    time_precision: Option<Precision>,
    action: String,
    ids: Vec<String>,
    detail: String,
    lifetime: Option<usize>,
    patch_refs: Vec<usize>,
    successor: Option<String>,
    reason: Option<String>,
    raw: Option<RawLink>,
    #[serde(flatten)]
    source: Source,
}

#[derive(Debug, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
enum Source {
    Journal {
        line: usize,
        current: bool,
        client: Option<String>,
        host: Option<String>,
        session: Option<String>,
        transitions: Vec<Transition>,
        publication: Publication,
        published_in: Option<Box<Commit>>,
        publishing_candidates: Vec<Commit>,
    },
    Git {
        commit: String,
    },
    Raw,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Publication {
    NoGit,
    Published,
    Unpublished,
    Uncertain,
}

#[derive(Debug, Serialize)]
struct RawLink {
    file: String,
    number: Option<usize>,
    line: Option<usize>,
    marker: Option<String>,
    date: Option<String>,
}

pub(crate) fn read(root: &KbRoot, id: &str, policy: &mut Inventory<'_>) -> Result<Report> {
    if notes::is_id(id) {
        return collect(root, policy, Selection::Note(id));
    }
    let missing = || format!("not found: {id}");
    let path = notes::find_with(root, id, policy)?.with_context(missing)?;
    let bytes = notes::read_with(&path, policy)?.with_context(missing)?;
    let basename = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid note name")?;
    let note = diffs::NoteImage::new(&format!("notes/{basename}"), &bytes);
    let note_id = note
        .id()
        .context("history requires a four-digit kb-NNNN note ID")?;
    collect(root, policy, Selection::Note(note_id))
}

pub(crate) fn timeline(root: &KbRoot, policy: &mut Inventory<'_>) -> Result<Report> {
    collect(root, policy, Selection::Timeline)
}

fn collect(root: &KbRoot, policy: &mut Inventory<'_>, selection: Selection<'_>) -> Result<Report> {
    let git = git::read(root)?;
    let mut blobs = match &git {
        Some(git) => git.blobs()?,
        None => Default::default(),
    };
    let mut diagnostics = Vec::new();
    let journal = journal_rows::read(root, policy, git.as_ref(), &mut blobs, &mut diagnostics)?;
    let diffs = match &git {
        Some(git) => diffs::collect(git, &mut blobs, selection.id())?,
        None => Vec::new(),
    };
    let links = match &git {
        Some(git) => diffs::associate(git, &journal, &diffs),
        None => vec![BTreeSet::new(); journal.len()],
    };
    let lifetimes = match selection {
        Selection::Note(id) => {
            lifetimes::read(root, policy, id, git.as_ref(), &diffs, &journal, &links)?
        }
        Selection::Timeline => Lifetimes::default(),
    };
    let mut events = Vec::new();
    let mut covered = BTreeSet::new();
    for (index, occurrence) in journal.into_iter().enumerate() {
        let event = journal_event(
            occurrence,
            git.as_ref(),
            &links[index],
            lifetimes.journal.get(index).copied().flatten(),
        );
        if selection.includes(&event.ids) {
            covered.extend(event.patch_refs.iter().copied());
            events.push(event);
        }
    }

    let (coverage, patches) = if let Some(git) = &git {
        for (index, diff) in diffs.iter().enumerate() {
            if !covered.contains(&index) {
                let lifetime = lifetimes
                    .patches
                    .get(index)
                    .filter(|groups| groups.len() == 1)
                    .and_then(|groups| groups.first().copied());
                events.push(git_event(git, diff, index, lifetime));
            }
        }
        for evidence in maintain::raw_evidence(root, policy)? {
            add_raw(&mut events, evidence, selection, &lifetimes);
        }
        let coverage = if git.truncated {
            diagnostics.push("Git history is shallow or exceeds the 2048-commit read limit".into());
            Coverage::PartialGitAndJournal
        } else {
            Coverage::GitAndJournal
        };
        (coverage, diffs::patches(git, &diffs, selection)?)
    } else {
        (Coverage::JournalOnly, Vec::new())
    };
    let mut events: Vec<_> = events.into_iter().enumerate().collect();
    events.sort_by_cached_key(|(ordinal, event)| {
        let instant = event.time.as_deref().and_then(|time| {
            time.parse::<Timestamp>()
                .ok()
                .or_else(|| format!("{time}T00:00:00Z").parse().ok())
        });
        (
            Reverse(instant),
            Reverse(event.time_precision),
            Reverse(*ordinal),
        )
    });
    diagnostics.extend(policy.take_warnings().map(|(_, warning)| warning));
    Ok(Report {
        id: selection.id().map(str::to_owned),
        coverage,
        lifetimes: lifetimes.rows,
        events: events.into_iter().map(|(_, event)| event).collect(),
        patches,
        diagnostics,
    })
}

fn journal_event(
    occurrence: Occurrence,
    git: Option<&History<'_>>,
    links: &BTreeSet<usize>,
    lifetime: Option<usize>,
) -> Event {
    let Occurrence {
        entry,
        origins,
        uncertain,
        current,
    } = occurrence;
    let heading = entry.heading;
    let mut ids: Vec<_> = heading.id.into_iter().collect();
    for state in entry
        .transitions
        .iter()
        .flat_map(|transition| transition.before.iter().chain(&transition.after))
    {
        if let Some(id) = &state.id
            && !ids.contains(id)
        {
            ids.push(id.clone());
        }
    }
    let (successor, mut reason) = if heading.action == "supersede" {
        supersede(&heading.detail)
    } else {
        (None, None)
    };
    if heading.action == "verdict"
        && let Some((other, _, _, rationale)) = verdict_fields(&heading.detail)
    {
        if !ids.iter().any(|id| id == other) {
            ids.push(other.to_owned());
        }
        reason = Some(rationale.to_owned());
    }
    if let Some(id) = &successor
        && !ids.contains(id)
    {
        ids.push(id.clone());
    }
    let (publication, published_in, publishing_candidates) = match git {
        None => (Publication::NoGit, None, Vec::new()),
        Some(git) => {
            let uncertain = uncertain || origins.len() > 1;
            let commits: BTreeSet<_> = origins.into_iter().map(|(commit, _)| commit).collect();
            let mut candidates: Vec<_> = commits
                .into_iter()
                .map(|index| git.revisions[index].commit.clone())
                .collect();
            match candidates.len() {
                1 if !uncertain => (
                    Publication::Published,
                    candidates.pop().map(Box::new),
                    candidates,
                ),
                0 if !uncertain => (Publication::Unpublished, None, candidates),
                _ => (Publication::Uncertain, None, candidates),
            }
        }
    };
    let time_precision = Some(if heading.time.contains('T') {
        Precision::Timestamp
    } else {
        Precision::Date
    });
    Event {
        action: heading.action,
        time: Some(heading.time),
        time_precision,
        ids,
        detail: heading.detail,
        lifetime,
        patch_refs: links.iter().copied().collect(),
        successor,
        reason,
        raw: None,
        source: Source::Journal {
            line: entry.line,
            current,
            client: heading.client,
            host: heading.host,
            session: heading.session,
            transitions: entry.transitions,
            publication,
            published_in,
            publishing_candidates,
        },
    }
}

fn supersede(detail: &str) -> (Option<String>, Option<String>) {
    let Some(detail) = detail.strip_prefix("superseded by ") else {
        return (None, None);
    };
    let (id, reason) = detail
        .split_once(" | reason=")
        .map_or((detail, None), |(id, reason)| (id, Some(reason)));
    if !notes::is_id(id) {
        return (None, None);
    }
    (Some(id.to_owned()), reason.map(str::to_owned))
}

fn verdict_fields(detail: &str) -> Option<(&str, &str, &str, &str)> {
    let mut parts = detail.splitn(4, " | ");
    let other = parts.next().filter(|part| notes::is_id(part))?;
    let verdict = parts.next()?;
    let file = parts.next()?.strip_prefix("raw/")?;
    Some((other, verdict, file, parts.next()?))
}

fn git_event(git: &History<'_>, diff: &Diff, patch: usize, lifetime: Option<usize>) -> Event {
    let commit = &git.revisions[diff.revision].commit;
    let ids: BTreeSet<_> = diff
        .images()
        .filter_map(|image| image.id().map(str::to_owned))
        .collect();
    Event {
        time: Some(commit.time.clone()),
        time_precision: Some(Precision::Timestamp),
        action: diff.action().to_owned(),
        ids: ids.into_iter().collect(),
        detail: commit.subject.clone(),
        lifetime,
        patch_refs: vec![patch],
        successor: None,
        reason: None,
        raw: None,
        source: Source::Git {
            commit: commit.oid.clone(),
        },
    }
}

fn add_raw(
    events: &mut Vec<Event>,
    evidence: RawEvidence,
    selection: Selection<'_>,
    lifetimes: &Lifetimes,
) {
    let (ids, action, detail, raw) = match evidence {
        RawEvidence::Ingested {
            file,
            number,
            id,
            marker,
            date,
        } => {
            let mut candidates = events.iter_mut().filter(|event| {
                event.action == "raw-mark"
                    && event.ids == [id.as_str()]
                    && raw_location(&event.detail) == Some((file.as_str(), number))
            });
            let first = candidates.next();
            if let Some(first) = first
                && candidates.next().is_none()
            {
                first.raw = Some(RawLink {
                    file,
                    number: Some(number),
                    line: None,
                    marker: Some(marker),
                    date,
                });
                return;
            }
            (
                vec![id],
                "ingest",
                marker.clone(),
                RawLink {
                    file,
                    number: Some(number),
                    line: None,
                    marker: Some(marker),
                    date,
                },
            )
        }
        RawEvidence::Verdict {
            file,
            line,
            ids,
            verdict,
            rationale,
            date,
        } => {
            let raw = RawLink {
                file,
                number: None,
                line: Some(line),
                marker: Some(verdict),
                date,
            };
            let mut candidates = events.iter_mut().filter(|event| {
                event.action == "verdict"
                    && event.ids.len() == ids.len()
                    && ids.iter().all(|id| event.ids.contains(id))
                    && verdict_fields(&event.detail).is_some_and(|(_, kind, file, reason)| {
                        file == raw.file
                            && Some(kind) == raw.marker.as_deref()
                            && reason == rationale
                    })
            });
            let first = candidates.next();
            if let Some(event) = first
                && candidates.next().is_none()
            {
                event.raw = Some(raw);
                return;
            }
            (Vec::from(ids), "verdict", rationale, raw)
        }
    };
    if selection.includes(&ids) {
        events.push(Event {
            time: raw.date.clone(),
            time_precision: raw.date.as_ref().map(|_| Precision::Date),
            action: action.to_owned(),
            ids,
            detail,
            lifetime: lifetimes.by_date(raw.date.as_deref()),
            patch_refs: Vec::new(),
            raw: Some(raw),
            successor: None,
            reason: None,
            source: Source::Raw,
        });
    }
}

fn raw_location(detail: &str) -> Option<(&str, usize)> {
    let (location, _) = detail
        .strip_prefix("raw/")?
        .rsplit_once(" | state: ingested ")?;
    let (file, number) = location.rsplit_once(" #")?;
    Some((file, number.parse().ok()?))
}

impl Report {
    pub(crate) fn render(&self, out: &mut impl Write) -> Result<()> {
        match &self.id {
            Some(id) => write!(out, "History for {id}")?,
            None => write!(out, "Timeline")?,
        }
        match self.coverage {
            Coverage::GitAndJournal => writeln!(out, " (Git and journal)")?,
            Coverage::PartialGitAndJournal => writeln!(out, " (partial Git history and journal)")?,
            Coverage::JournalOnly => writeln!(out, " (journal only, no Git repository)")?,
        }
        let mut shown = BTreeSet::new();
        for event in &self.events {
            write!(
                out,
                "{} {} {}",
                event.time.as_deref().unwrap_or("undated"),
                event.action,
                event.ids.join(", ")
            )?;
            match &event.source {
                Source::Journal {
                    client,
                    host,
                    session,
                    published_in,
                    line,
                    ..
                } => {
                    write!(out, " [log.md:{line}")?;
                    if let Some(client) = client {
                        write!(out, " client={client}")?;
                    }
                    if let Some(host) = host {
                        write!(out, " host={host}")?;
                    }
                    if let Some(session) = session {
                        write!(out, " session={session}")?;
                    }
                    if let Some(commit) = published_in {
                        write!(out, " published={}", commit.oid)?;
                    }
                    write!(out, "]")?;
                }
                Source::Git { commit } => write!(out, " [git {commit}]")?,
                Source::Raw => {
                    if let Some(raw) = &event.raw {
                        write!(out, " [raw/{}]", raw.file)?;
                    }
                }
            }
            if let Some(number) = event.lifetime
                && self.lifetimes.len() > 1
            {
                write!(out, " [lifetime {}]", number + 1)?;
            }
            writeln!(out, " {}", event.detail)?;
            for number in &event.patch_refs {
                if let Some(patch) = self.patches.get(*number)
                    && let Some(text) = &patch.text
                {
                    if shown.insert(*number) {
                        writeln!(out, "  patch {} ({})", number + 1, patch.commit.oid)?;
                        write!(out, "{text}")?;
                    } else {
                        writeln!(out, "  shares patch {}", number + 1)?;
                    }
                }
            }
        }
        for diagnostic in &self.diagnostics {
            writeln!(out, "warning: {diagnostic}")?;
        }
        Ok(())
    }
}
