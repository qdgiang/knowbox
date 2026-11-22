mod raw;
mod rollup;
mod totals;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io::Write;
use std::ops::RangeInclusive;

use anyhow::{Context as _, Result, ensure};
use jiff::{Timestamp, Zoned, civil::Date, fmt::temporal::DateTimeParser};
use serde::{Deserialize, Serialize};

use crate::{config::KbRoot, notes::is_id, store};

use raw::Month;
use totals::{Kind, Totals};

const DIRECTORY: &str = ".kb-usage";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Context<'a> {
    pub(crate) now: &'a Zoned,
    pub(crate) client: &'a str,
    pub(crate) session: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layer {
    Id,
    Lexical,
    Partial,
    Typo,
    Semantic,
    Refs,
    Miss,
}

impl Layer {
    fn name(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Lexical => "lexical",
            Self::Partial => "partial",
            Self::Typo => "typo",
            Self::Semantic => "semantic",
            Self::Refs => "refs",
            Self::Miss => "miss",
        }
    }

    fn parse(name: &str) -> Result<Self> {
        match name {
            "id" => Ok(Self::Id),
            "lexical" => Ok(Self::Lexical),
            "partial" => Ok(Self::Partial),
            "typo" => Ok(Self::Typo),
            "semantic" => Ok(Self::Semantic),
            "refs" => Ok(Self::Refs),
            "miss" => Ok(Self::Miss),
            _ => anyhow::bail!("invalid usage layer {name:?}"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub(crate) enum ReadEvent {
    Show {
        note: String,
    },
    Expand {
        notes: Vec<String>,
    },
    Search {
        layer: Layer,
        notes: Vec<String>,
        expanded: Vec<String>,
    },
}

impl ReadEvent {
    fn valid_ids(&self) -> bool {
        match self {
            Self::Show { note } => is_id(note),
            Self::Expand { notes } => notes.iter().all(|note| is_id(note)),
            Self::Search {
                notes, expanded, ..
            } => notes.iter().chain(expanded).all(|note| is_id(note)),
        }
    }
}

#[derive(Deserialize)]
struct Row {
    timestamp: String,
    client: String,
    session: Option<String>,
    #[serde(flatten)]
    event: ReadEvent,
}

impl Row {
    fn when(&self) -> Result<(Timestamp, Date)> {
        Ok((
            self.timestamp.parse()?,
            DateTimeParser::new().parse_date(&self.timestamp)?,
        ))
    }
}

#[derive(Serialize)]
struct PendingRow<'a> {
    timestamp: String,
    client: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<&'a str>,
    #[serde(flatten)]
    event: &'a ReadEvent,
}

/// Append a completed invocation. The caller handles recording failures independently of the read.
pub(crate) fn record(root: &KbRoot, context: &Context<'_>, event: &ReadEvent) -> Result<()> {
    ensure!(event.valid_ids(), "invalid note ID in usage event");
    let directory = root.path().join(DIRECTORY);
    let month = Month::of(context.now.date());
    let pending = PendingRow {
        timestamp: context
            .now
            .timestamp()
            .display_with_offset(context.now.offset())
            .to_string(),
        client: context.client,
        session: context.session,
        event,
    };
    let mut frame = vec![b'\n'];
    serde_json::to_writer(&mut frame, &pending)?;
    ensure!(
        frame.len() <= raw::MAX_ROW,
        "usage row exceeds the size limit"
    );
    frame.push(b'\n');

    raw::create_directory(&directory).context("creating private usage directory")?;
    raw::AppendGuard::acquire(&directory, month)?
        .append(&frame)
        .with_context(|| format!("appending usage month {month}"))
}

/// Render stored activity and the complete, unprocessed raw records visible to this report.
pub(crate) fn report(
    root: &KbRoot,
    window: Option<RangeInclusive<Date>>,
    out: &mut impl Write,
) -> Result<()> {
    snapshot(root)?.render(window, out)
}

fn snapshot(root: &KbRoot) -> Result<Totals> {
    let directory = root.path().join(DIRECTORY);
    if !store::directory(&directory).context("checking usage directory")? {
        return Ok(Totals::default());
    }
    rollup::snapshot(&directory)
}

#[derive(Debug, Serialize)]
pub(crate) struct UsageSummary {
    coverage_start: Option<String>,
    dropped_rows: u64,
    gaps: Vec<String>,
    access: Vec<DailyAccess>,
    outcomes: Vec<DailyOutcome>,
}

impl UsageSummary {
    pub(crate) fn coverage_start(&self) -> Option<&str> {
        self.coverage_start.as_deref()
    }

    pub(crate) fn has_gaps(&self) -> bool {
        self.dropped_rows != 0 || !self.gaps.is_empty()
    }

    pub(crate) fn note_counts(&self) -> BTreeMap<&str, (u64, u64)> {
        let mut counts = BTreeMap::<_, (u64, u64)>::new();
        for row in &self.access {
            let entry = counts.entry(row.note.as_str()).or_default();
            match row.kind {
                Kind::Open => entry.0 = entry.0.saturating_add(row.count),
                Kind::Search => entry.1 = entry.1.saturating_add(row.count),
                Kind::Graph => {}
            }
        }
        counts
    }
}

#[derive(Debug, Serialize)]
struct DailyAccess {
    day: String,
    note: String,
    kind: Kind,
    count: u64,
    clients: usize,
    sessions: usize,
    last_read_utc: String,
}

#[derive(Debug, Serialize)]
struct DailyOutcome {
    day: String,
    layer: Layer,
    count: u64,
}

/// Read daily counts from the checkpoint and complete raw tail without updating either.
pub(crate) fn summary(root: &KbRoot) -> Result<UsageSummary> {
    let totals = snapshot(root)?;
    Ok(UsageSummary {
        coverage_start: totals.coverage_start.map(|day| day.to_string()),
        dropped_rows: totals.dropped,
        gaps: totals
            .gaps
            .into_iter()
            .map(|month| month.to_string())
            .collect(),
        access: totals
            .access
            .into_iter()
            .map(|(key, access)| DailyAccess {
                day: key.day.to_string(),
                note: key.note,
                kind: key.kind,
                count: access.count,
                clients: access.clients.len(),
                sessions: access.sessions.len(),
                last_read_utc: access.latest.to_string(),
            })
            .collect(),
        outcomes: totals
            .outcomes
            .into_iter()
            .map(|((day, layer), count)| DailyOutcome {
                day: day.to_string(),
                layer,
                count,
            })
            .collect(),
    })
}

/// Checkpoint complete raw rows and expire confirmed, fully checkpointed old sources.
pub(crate) fn rollup(root: &KbRoot, today: Date) -> Result<()> {
    let directory = root.path().join(DIRECTORY);
    raw::create_directory(&directory).context("creating private usage directory")?;
    rollup::refresh(&directory, today)
}
