use std::borrow::Cow;
use std::cmp::Reverse;
use std::io::Write;

use anyhow::bail;
use jiff::civil::{Date, ISOWeekDate};
use serde::Serialize;

use super::refs::python_whitespace;
use super::{display_list, truthy_text};
use crate::config::KbRoot;
use crate::notes::{self, NoteValue};
use crate::store::Inventory;

const STALE_TITLE_WIDTH: usize = 58;

pub(super) fn truncate(text: &str, width: usize) -> &str {
    text.char_indices()
        .nth(width)
        .map_or(text, |(index, _)| &text[..index])
}

#[derive(Debug, Serialize)]
pub(crate) struct StaleReport {
    days: u32,
    as_of: String,
    rows: Vec<StaleRow>,
}

impl StaleReport {
    pub(crate) fn edited_since_verify(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.flags.contains(&"EDITED-SINCE-VERIFY"))
            .count()
    }
}

#[derive(Debug, Serialize)]
struct StaleRow {
    path: String,
    base_sha1: String,
    age: Option<i64>,
    flags: Vec<&'static str>,
    id: String,
    status: String,
    created: Option<NoteValue>,
    updated: Option<NoteValue>,
    verified: Option<NoteValue>,
    effective_update: Option<String>,
    #[serde(skip)]
    effective_verification: Option<String>,
    title: String,
}

/// Report live notes ranked by day age at the supplied local calendar date.
pub(crate) fn stale(
    root: &KbRoot,
    days: u32,
    today: Date,
    policy: &mut Inventory<'_>,
) -> anyhow::Result<StaleReport> {
    if days < 1 {
        bail!("knowbox stale: --days must be >= 1");
    }
    let entries = notes::snapshot_with(root, policy)?;
    let mut rows: Vec<_> = entries
        .into_iter()
        .filter_map(|entry| {
            let Ok(mut note) = entry.parsed else {
                return None;
            };
            if note.scalar("status") == Some("superseded") {
                return None;
            }
            let effective_update =
                truthy_text(&note, "updated").or_else(|| truthy_text(&note, "created"));
            let verified = truthy_text(&note, "verified");
            let age = effective_update.as_deref().and_then(|date| {
                parse_date(date).map(|date| today.duration_since(date).as_secs() / 86_400)
            });
            let mut flags = Vec::new();
            if age.is_some_and(|age| age >= i64::from(days)) {
                flags.push("STALE");
            }
            if let Some(verified) = verified.as_deref() {
                if effective_update
                    .as_deref()
                    .is_some_and(|updated| updated > verified)
                {
                    flags.push("EDITED-SINCE-VERIFY");
                }
            } else if age.is_some_and(|age| age >= i64::from(days / 2)) {
                flags.push("NEVER-VERIFIED");
            }
            Some(StaleRow {
                path: entry.relative,
                base_sha1: entry.sha1,
                age,
                flags,
                id: note.text("id", "?").into_owned(),
                status: note.text("status", "?").into_owned(),
                effective_update: effective_update.map(Cow::into_owned),
                effective_verification: verified.map(Cow::into_owned),
                title: display_list(&note, "title").into_owned(),
                created: note.meta.remove("created"),
                updated: note.meta.remove("updated"),
                verified: note.meta.remove("verified"),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        (Reverse(a.age.unwrap_or(-1)), &a.id).cmp(&(Reverse(b.age.unwrap_or(-1)), &b.id))
    });
    Ok(StaleReport {
        days,
        as_of: today.to_string(),
        rows,
    })
}

impl StaleReport {
    pub(crate) fn render(&self, out: &mut impl Write) -> anyhow::Result<()> {
        let days = self.days;
        let rows = &self.rows;
        let Some(oldest) = rows.first() else {
            writeln!(out, "knowbox stale: no live notes found")?;
            return Ok(());
        };
        writeln!(
            out,
            "knowbox stale — live notes ranked by staleness (threshold: {days}d, never-verified flag at {}d; EDITED-SINCE-VERIFY = updated > verified, re-verify before relying)",
            days / 2
        )?;
        writeln!(
            out,
            "{:16} {:8} {:9} {:11} {:>4} {:11} title",
            "flag", "id", "status", "updated", "age", "verified"
        )?;
        for row in rows {
            writeln!(
                out,
                "{:16} {:8} {:9} {:11} {:>3}d {:11} {}",
                row.flags.join("+"),
                row.id,
                row.status,
                row.effective_update.as_deref().unwrap_or("?"),
                row.age.unwrap_or(-1),
                row.effective_verification.as_deref().unwrap_or("never"),
                truncate(&row.title, STALE_TITLE_WIDTH)
            )?;
        }
        let flagged = rows.iter().filter(|row| !row.flags.is_empty()).count();
        writeln!(
            out,
            "\n{} live note(s); oldest update {}d ago; {flagged} flagged",
            rows.len(),
            oldest.age.unwrap_or(-1)
        )?;
        if flagged > 0 {
            writeln!(
                out,
                "act on flags: re-verify with 'knowbox verify ID --evidence …', update, or supersede."
            )?;
        }
        Ok(())
    }
}

pub(crate) fn parse_date(value: &str) -> Option<Date> {
    let value = value.trim_matches(python_whitespace);
    let year = value.get(..4)?;
    if year == "0000" || !year.bytes().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if let Some(week) = value.strip_prefix(year)?.strip_prefix(['W', '-'])
        && (value.as_bytes()[4] == b'W' || week.starts_with('W'))
    {
        let date = match (value.as_bytes()[4], week.len()) {
            (b'W', 2) => format!("{value}1").parse::<ISOWeekDate>(),
            (b'-', 3) => format!("{value}-1").parse::<ISOWeekDate>(),
            _ => value.parse::<ISOWeekDate>(),
        };
        return match date {
            Ok(date) if date.date().year() > 0 => Some(date.date()),
            _ => None,
        };
    }
    let mut bytes = value.bytes();
    let calendar_date = match value.len() {
        8 => bytes.all(|ch| ch.is_ascii_digit()),
        10 => bytes.enumerate().all(|(index, ch)| {
            if index == 4 || index == 7 {
                ch == b'-'
            } else {
                ch.is_ascii_digit()
            }
        }),
        _ => false,
    };
    if !calendar_date {
        return None;
    }
    let date = match value.parse::<Date>() {
        Ok(date) => date,
        // Malformed dates rank as unknown rather than excluding a live note.
        Err(_) => return None,
    };
    (date.year() > 0).then_some(date)
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture;
    use super::*;
    use std::fs;

    #[test]
    fn stale_uses_calendar_age_fallbacks_and_lexical_verification_order() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        for (file, contents) in [
            (
                "a.md",
                "id: kb-0002\ncreated: 2024-02-28\ntitle: Two\nstatus: active",
            ),
            (
                "b.md",
                "id: kb-0001\nupdated: 2024-02-28\nverified: 2024-02-27\ntitle: One\nstatus: verified",
            ),
            (
                "c.md",
                "id: kb-0003\nupdated: 2024-02-30\ntitle: Invalid\nstatus: draft",
            ),
            (
                "d.md",
                "id: kb-0004\ncreated: 2020-01-01\nstatus: superseded",
            ),
        ] {
            fs::write(
                root.path().join("notes").join(file),
                format!("---\n{contents}\n---"),
            )?;
        }
        let mut out = Vec::new();
        let report = stale(&root, 2, Date::new(2024, 3, 1)?, &mut Inventory::Strict)?;
        let json = serde_json::to_value(&report)?;
        assert_eq!(
            json["rows"][0]["flags"],
            serde_json::json!(["STALE", "EDITED-SINCE-VERIFY"])
        );
        assert_eq!(json["rows"][1]["age"], 2);
        assert_eq!(json["rows"][1]["updated"], serde_json::Value::Null);
        assert_eq!(json["rows"][1]["created"], "2024-02-28");
        assert_eq!(json["rows"][2]["age"], serde_json::Value::Null);
        report.render(&mut out)?;
        assert_eq!(
            String::from_utf8(out)?,
            concat!(
                "knowbox stale — live notes ranked by staleness (threshold: 2d, never-verified flag at 1d; EDITED-SINCE-VERIFY = updated > verified, re-verify before relying)\n",
                "flag             id       status    updated      age verified    title\n",
                "STALE+EDITED-SINCE-VERIFY kb-0001  verified  2024-02-28    2d 2024-02-27  One\n",
                "STALE+NEVER-VERIFIED kb-0002  active    2024-02-28    2d never       Two\n",
                "                 kb-0003  draft     2024-02-30   -1d never       Invalid\n",
                "\n3 live note(s); oldest update 2d ago; 2 flagged\n",
                "act on flags: re-verify with 'knowbox verify ID --evidence …', update, or supersede.\n",
            )
        );
        Ok(())
    }

    #[test]
    fn stale_reports_original_empty_date_fields_and_effective_fallback() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        fs::write(
            root.path().join("notes/kb-0007.md"),
            "---\nid: kb-0007\nupdated: []\ncreated: 2025-02-03\nverified: ''\n---",
        )?;
        let rows = serde_json::to_value(stale(
            &root,
            2,
            Date::new(2025, 2, 5)?,
            &mut Inventory::Strict,
        )?)?;
        assert_eq!(rows["rows"][0]["age"], 2);
        assert_eq!(rows["rows"][0]["updated"], serde_json::json!([]));
        assert_eq!(rows["rows"][0]["verified"], "");
        assert_eq!(rows["rows"][0]["effective_update"], "2025-02-03");
        Ok(())
    }

    #[test]
    fn stale_date_parser_supports_calendar_and_iso_week_formats() -> anyhow::Result<()> {
        for text in [
            "2024-01-01",
            "20240101",
            "2024-W01-1",
            "2024W011",
            "2024-W01",
            "2024W01",
        ] {
            assert_eq!(
                parse_date(text),
                Some(Date::new(2024, 1, 1)?),
                "date {text:?}"
            );
        }
        for text in [
            "0000-01-01",
            "2024-02-30",
            "2024-1-01",
            "2024-w01-1",
            "2024-01-01T00:00:00",
        ] {
            assert_eq!(parse_date(text), None, "date {text:?}");
        }
        Ok(())
    }
}
