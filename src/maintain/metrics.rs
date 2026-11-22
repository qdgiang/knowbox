use std::collections::BTreeMap;
use std::io::{BufReader, Read, Write};

use anyhow::{Context, Result, ensure};
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::browse;
use crate::config::KbRoot;
use crate::store::{self, Inventory, InventoryWarnings, WriteLock};

pub(crate) const FILE: &str = ".kb-metrics.jsonl";
const MAX_LOG: u64 = 4 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct Row {
    day: String,
    errors: usize,
    warnings: usize,
    edited_since_verify: usize,
    raw_backlog: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    coverage_start: Option<String>,
    coverage_end: Option<String>,
    missing_days: i64,
    rows: Vec<Row>,
}

impl Report {
    pub(crate) fn render(&self, out: &mut impl Write) -> Result<()> {
        if self.rows.is_empty() {
            writeln!(out, "no daily metrics recorded")?;
            return Ok(());
        }
        writeln!(
            out,
            "date        lint errors  lint warnings  edited since verify  open proposals"
        )?;
        for row in &self.rows {
            writeln!(
                out,
                "{:10}  {:11}  {:13}  {:19}  {:14}",
                row.day, row.errors, row.warnings, row.edited_since_verify, row.raw_backlog
            )?;
        }
        Ok(())
    }
}

fn rows(root: &KbRoot) -> Result<BTreeMap<Date, Row>> {
    let path = root.path().join(FILE);
    if !store::regular_file(&path)? {
        return Ok(BTreeMap::new());
    }
    let file = store::read_log(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut result = BTreeMap::new();
    let length = file.metadata()?.len();
    ensure!(length <= MAX_LOG, "metrics log exceeds 4 MiB");
    let mut reader = BufReader::new(file.take(length));
    let mut line = Vec::new();
    let mut observed = 0;
    while let Some(size) = store::read_complete_record(&mut reader, &mut line, 2048, |_| {})? {
        observed += size;
        let row: Row = serde_json::from_slice(&line)
            .with_context(|| format!("reading a daily row in {}", path.display()))?;
        let day: Date = row
            .day
            .parse()
            .with_context(|| format!("reading a date in {}", path.display()))?;
        ensure!(
            day.to_string() == row.day,
            "noncanonical day in {}",
            path.display()
        );
        ensure!(
            result.insert(day, row).is_none(),
            "duplicate day in {}",
            path.display()
        );
    }
    ensure!(observed == length, "incomplete metrics log");
    Ok(result)
}

pub(crate) fn report(root: &KbRoot) -> Result<Report> {
    let mut values = rows(root)?;
    let coverage_start = values.first_entry().map(|entry| *entry.key());
    let coverage_end = values.last_entry().map(|entry| *entry.key());
    let missing_days = match coverage_start.zip(coverage_end) {
        Some((start, end)) => ((end.duration_since(start).as_secs() / 86_400) + 1)
            .saturating_sub(i64::try_from(values.len())?),
        None => 0,
    };
    Ok(Report {
        coverage_start: coverage_start.map(|day| day.to_string()),
        coverage_end: coverage_end.map(|day| day.to_string()),
        missing_days,
        rows: values.into_values().collect(),
    })
}

pub(crate) fn record(lock: &WriteLock, day: Date) -> Result<()> {
    let root = lock.root();
    let mut daily = rows(root)?;
    if daily.contains_key(&day) {
        return Ok(());
    }
    let mut warnings = InventoryWarnings::default();
    let read = &mut Inventory::Read(&mut warnings);
    let (errors, warnings) = super::lint(root, super::HeartbeatCheck::from_env(), read)?.counts();
    let edited_since_verify = browse::stale(root, 90, day, read)?.edited_since_verify();
    let raw_backlog = super::raw_backlog(root, read)?.unreviewed();
    daily.insert(
        day,
        Row {
            day: day.to_string(),
            errors,
            warnings,
            edited_since_verify,
            raw_backlog,
        },
    );
    let mut bytes = Vec::new();
    for row in daily.values() {
        serde_json::to_writer(&mut bytes, row)?;
        bytes.push(b'\n');
    }
    ensure!(
        u64::try_from(bytes.len())? <= MAX_LOG,
        "metrics log exceeds 4 MiB"
    );
    Ok(store::atomic_write(&root.path().join(FILE), &bytes)?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn measurements_are_serial_and_keep_real_gaps_and_first_daily_counts() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("kb");
        let stamp = crate::journal::test_stamp();
        let lock = crate::init::prepare(&root, &stamp)?;
        crate::index::rebuild(&lock)?;
        assert_eq!(report(lock.root())?.coverage_start, None);
        let first = "2026-11-19".parse()?;
        let next = "2026-11-21".parse()?;
        record(&lock, first)?;
        let before = fs::read(root.join(FILE))?;
        fs::write(
            root.join("raw/proposals.md"),
            "## New proposal\nConsider the measurements.\n",
        )?;
        record(&lock, first)?;
        assert_eq!(fs::read(root.join(FILE))?, before);
        record(&lock, next)?;
        let result = report(lock.root())?;
        assert_eq!(result.coverage_start.as_deref(), Some("2026-11-19"));
        assert_eq!(result.coverage_end.as_deref(), Some("2026-11-21"));
        assert_eq!(result.missing_days, 1);
        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| row.raw_backlog)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(fs::read_to_string(root.join(FILE))?.lines().count(), 2);
        Ok(())
    }
}
