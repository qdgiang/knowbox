use std::collections::BTreeMap;
use std::io::{self, BufReader, Read};

use anyhow::{Context, Result, ensure};
use jiff::civil::DateTime;
use serde::Serialize;

use crate::{config::KbRoot, journal::Actor, store};

const LOG: &str = ".search-misses.log";
const MAX_RECORD: usize = 64 * 1024;
const RECENT_LIMIT: usize = 100;

pub(crate) fn log_miss(
    root: &KbRoot,
    query: &str,
    now: DateTime,
    actor: Option<&Actor>,
) -> anyhow::Result<()> {
    let path = root.path().join(LOG);
    let query = query.replace(
        |ch: char| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'),
        " ",
    );
    // A leading separator keeps this record intact after a previous short append.
    let client = actor.map_or("unknown", Actor::client);
    let session = actor.and_then(Actor::session).unwrap_or("");
    let line = format!("\n{now:.0}\t{query}\t{client}\t{session}\n");
    ensure!(
        line.len() <= MAX_RECORD + 1,
        "search miss record exceeds {MAX_RECORD} bytes"
    );
    store::append_record(&path, line.as_bytes())
        .with_context(|| format!("recording search miss in {}", path.display()))
}

#[derive(Debug, Serialize)]
pub(crate) struct Misses {
    pub(crate) entries: Vec<Miss>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Miss {
    pub(crate) timestamp: String,
    pub(crate) query: String,
    client: Option<String>,
    session: Option<String>,
}

fn parse(row: &[u8]) -> Option<(DateTime, Miss)> {
    let text = std::str::from_utf8(row.strip_suffix(b"\r").unwrap_or(row)).ok()?;
    let mut parts = text.split('\t');
    let timestamp = parts.next()?;
    let when: DateTime = timestamp.parse().ok()?;
    let query = parts.next()?;
    let (client, session) = match (parts.next(), parts.next(), parts.next()) {
        (None, None, None) => (None, None),
        (Some(client), Some(session), None) => (
            (!client.is_empty()).then(|| client.to_owned()),
            (!session.is_empty()).then(|| session.to_owned()),
        ),
        _ => return None,
    };
    Some((
        when,
        Miss {
            timestamp: timestamp.into(),
            query: query.into(),
            client,
            session,
        },
    ))
}

/// Read the latest complete legacy and attributed miss records, newest first.
pub(crate) fn misses(root: &KbRoot) -> Result<Misses> {
    let path = root.path().join(LOG);
    let file = match store::read_log(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Misses { entries: vec![] });
        }
        result => result.with_context(|| format!("opening {}", path.display()))?,
    };
    let length = file.metadata()?.len();
    let mut reader = BufReader::new(file.take(length));
    let mut row = Vec::new();
    let mut entries = BTreeMap::new();
    let mut offset = 0;
    while let Some(bytes) = store::read_complete_record(&mut reader, &mut row, MAX_RECORD, |_| {})?
    {
        offset += bytes;
        if let Some((when, miss)) = row.strip_suffix(b"\n").and_then(parse) {
            entries.insert((when, offset), miss);
            if entries.len() > RECENT_LIMIT {
                entries.pop_first();
            }
        }
    }
    Ok(Misses {
        entries: entries.into_values().rev().collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn oversized_and_torn_misses_leave_complete_neighbors_readable() -> Result<()> {
        let kb = tempfile::tempdir()?;
        let mut log = b"2026-05-01T00:00:00\tfirst\n2026-05-01T00:01:00\t".to_vec();
        log.extend(vec![b'x'; MAX_RECORD]);
        log.extend(b"\n2026-05-01T00:02:00\tlast complete\n2026-05-01T00:03:00\t");
        log.extend(vec![b'x'; MAX_RECORD]);
        fs::write(kb.path().join(LOG), log)?;
        let result = misses(&KbRoot::open(kb.path())?)?;
        let queries: Vec<_> = result
            .entries
            .iter()
            .map(|row| row.query.as_str())
            .collect();
        assert_eq!(queries, ["last complete", "first"]);
        Ok(())
    }

    #[test]
    fn oversized_miss_writes_leave_the_log_intact() -> Result<()> {
        let kb = tempfile::tempdir()?;
        let root = KbRoot::open(kb.path())?;
        let path = kb.path().join(LOG);
        let now = "2026-05-01T00:00:00".parse()?;
        log_miss(&root, "first", now, None)?;
        let contents = fs::read(&path)?;
        let error = match log_miss(&root, &"x".repeat(MAX_RECORD), now, None) {
            Err(error) => error,
            Ok(()) => anyhow::bail!("the writer accepted an oversized miss"),
        };
        assert_eq!(error.to_string(), "search miss record exceeds 65536 bytes");
        assert_eq!(fs::read(path)?, contents);
        Ok(())
    }

    #[test]
    fn report_returns_at_most_a_hundred_misses_newest_first() -> Result<()> {
        use std::fmt::Write;

        let kb = tempfile::tempdir()?;
        let mut log = String::new();
        for index in 0..102 {
            writeln!(log, "2026-05-01T00:00:00\tquery {index}")?;
        }
        fs::write(kb.path().join(LOG), log)?;
        let report = misses(&KbRoot::open(kb.path())?)?;
        let queries: Vec<_> = report.entries.into_iter().map(|row| row.query).collect();
        let expected: Vec<_> = (2..102).rev().map(|n| format!("query {n}")).collect();
        assert_eq!(queries, expected);
        Ok(())
    }
}
