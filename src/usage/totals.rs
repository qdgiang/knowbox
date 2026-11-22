use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::ops::RangeInclusive;

use anyhow::Result;
use jiff::{Timestamp, civil::Date};

use super::{Layer, ReadEvent, Row, raw::Month};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Open,
    Search,
    Graph,
}

impl Kind {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Search => "search",
            Self::Graph => "graph",
        }
    }

    pub(super) fn parse(name: &str) -> Result<Self> {
        match name {
            "open" => Ok(Self::Open),
            "search" => Ok(Self::Search),
            "graph" => Ok(Self::Graph),
            _ => anyhow::bail!("invalid usage access kind {name:?}"),
        }
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct AccessKey {
    pub(super) note: String,
    pub(super) day: Date,
    pub(super) kind: Kind,
}

#[derive(Debug)]
pub(super) struct Access {
    pub(super) count: u64,
    pub(super) latest: Timestamp,
    pub(super) clients: BTreeMap<String, u64>,
    pub(super) sessions: BTreeSet<String>,
}

impl Access {
    pub(super) fn new(count: u64, latest: Timestamp) -> Self {
        Self {
            count,
            latest,
            clients: BTreeMap::new(),
            sessions: BTreeSet::new(),
        }
    }

    fn merge(&mut self, other: Self) {
        self.count += other.count;
        self.latest = self.latest.max(other.latest);
        for (client, count) in other.clients {
            *self.clients.entry(client).or_default() += count;
        }
        self.sessions.extend(other.sessions);
    }
}

#[derive(Debug, Default)]
pub(super) struct Totals {
    pub(super) coverage_start: Option<Date>,
    pub(super) access: BTreeMap<AccessKey, Access>,
    pub(super) outcomes: BTreeMap<(Date, Layer), u64>,
    pub(super) dropped: u64,
    pub(super) gaps: BTreeSet<Month>,
}

impl Totals {
    pub(super) fn observe_day(&mut self, day: Date) {
        self.coverage_start = Some(self.coverage_start.map_or(day, |start| start.min(day)));
    }

    pub(super) fn add(&mut self, row: Row, timestamp: Timestamp, day: Date) {
        self.observe_day(day);

        let mut add_access = |kind, note| {
            let access = self
                .access
                .entry(AccessKey { note, day, kind })
                .or_insert_with(|| Access::new(0, timestamp));
            access.count += 1;
            access.latest = access.latest.max(timestamp);
            if !row.client.trim().is_empty() {
                *access.clients.entry(row.client.clone()).or_default() += 1;
            }
            if let Some(session) = &row.session
                && !session.trim().is_empty()
            {
                access.sessions.insert(session.clone());
            }
        };

        match row.event {
            ReadEvent::Show { note } => add_access(Kind::Open, note),
            ReadEvent::Expand { notes } => {
                for note in unique(notes) {
                    add_access(Kind::Graph, note);
                }
            }
            ReadEvent::Search {
                layer,
                notes,
                expanded,
            } => {
                *self.outcomes.entry((day, layer)).or_default() += 1;
                for note in unique(notes) {
                    add_access(Kind::Search, note);
                }
                for note in unique(expanded) {
                    add_access(Kind::Graph, note);
                }
            }
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        if let Some(day) = other.coverage_start {
            self.observe_day(day);
        }
        self.dropped += other.dropped;
        self.gaps.extend(other.gaps);
        for (key, other) in other.access {
            self.access
                .entry(key)
                .or_insert_with(|| Access::new(0, other.latest))
                .merge(other);
        }
        for (outcome, count) in other.outcomes {
            *self.outcomes.entry(outcome).or_default() += count;
        }
    }

    pub(super) fn render(
        mut self,
        window: Option<RangeInclusive<Date>>,
        out: &mut impl Write,
    ) -> Result<()> {
        match self.coverage_start {
            Some(day) => writeln!(out, "Usage coverage since {day}")?,
            None => writeln!(out, "Usage coverage unavailable")?,
        }
        if let Some(window) = window {
            writeln!(out, "Activity since {}", window.start())?;
            self.access.retain(|key, _| window.contains(&key.day));
            self.outcomes.retain(|(day, _), _| window.contains(day));
        }
        writeln!(
            out,
            "Raw months expire 90 days after their last day once rolled up."
        )?;
        if self.dropped != 0 {
            writeln!(out, "Dropped malformed usage rows\t{}", self.dropped)?;
        }
        for month in self.gaps {
            writeln!(out, "Usage coverage gap\t{month}")?;
        }

        let mut summary = BTreeMap::new();
        if !self.access.is_empty() {
            writeln!(out, "\nDaily note access")?;
            writeln!(
                out,
                "day\tnote\tkind\tcount\tclients\tsessions\tlast_read_utc"
            )?;
            for (key, access) in self.access {
                write_row(out, Some(key.day), &key.note, key.kind, &access)?;
                summary
                    .entry((key.note, key.kind))
                    .or_insert_with(|| Access::new(0, access.latest))
                    .merge(access);
            }
            writeln!(out, "\nTotal note access")?;
            writeln!(out, "note\tkind\tcount\tclients\tsessions\tlast_read_utc")?;
            let mut ranked: Vec<_> = summary.into_iter().collect();
            ranked.sort_by(|((left, left_kind), a), ((right, right_kind), b)| {
                (left_kind, std::cmp::Reverse(a.count), left).cmp(&(
                    right_kind,
                    std::cmp::Reverse(b.count),
                    right,
                ))
            });
            for ((note, kind), access) in ranked {
                write_row(out, None, &note, kind, &access)?;
            }
        }
        if !self.outcomes.is_empty() {
            writeln!(out, "\nDaily search outcomes")?;
            writeln!(out, "day\tlayer\tcount")?;
            for ((day, layer), count) in self.outcomes {
                writeln!(out, "{day}\t{}\t{count}", layer.name())?;
            }
        }
        Ok(())
    }
}

fn write_row(
    out: &mut impl Write,
    day: Option<Date>,
    note: &str,
    kind: Kind,
    access: &Access,
) -> Result<()> {
    if let Some(day) = day {
        write!(out, "{day}\t")?;
    }
    writeln!(
        out,
        "{note}\t{}\t{}\t{}\t{}\t{}",
        kind.name(),
        access.count,
        access.clients.len(),
        access.sessions.len(),
        access.latest
    )?;
    Ok(())
}

fn unique(notes: Vec<String>) -> BTreeSet<String> {
    notes.into_iter().collect()
}
