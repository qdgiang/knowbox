use std::borrow::Cow;
use std::cmp::Reverse;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, bail};
use jiff::civil::{Date, ISOWeekDate};

use crate::config::KbRoot;
use crate::notes::{self, normalize_newlines};
use crate::store;

use super::{display_list, parsed_notes, truthy_text};

const HEADING_CAP: usize = 12;
const HEADING_WIDTH: usize = 70;
const STALE_TITLE_WIDTH: usize = 58;

struct Doc {
    name: String,
    content: io::Result<DocContent>,
}

struct DocContent {
    words: usize,
    headings: Vec<String>,
    maps: Vec<String>,
}

impl Doc {
    fn matches(&self, filter: &str) -> bool {
        self.name.to_lowercase().contains(filter)
            || self.content.as_ref().is_ok_and(|content| {
                content
                    .headings
                    .iter()
                    .chain(&content.maps)
                    .any(|value| value.to_lowercase().contains(filter))
            })
    }
}

/// Report sorted Markdown reference headings and their live section-map notes.
pub(crate) fn refs(root: &KbRoot, filter: &str, out: &mut impl Write) -> anyhow::Result<()> {
    let directory = root.path().join("refs");
    if !store::directory(&directory)? {
        writeln!(
            out,
            "knowbox refs: no refs/ directory — nothing ingested yet"
        )?;
        return Ok(());
    }
    let names = markdown_names(&directory)?;
    if names.is_empty() {
        writeln!(out, "knowbox refs: refs/ is empty")?;
        return Ok(());
    }
    let entries = notes::snapshot(root)?;
    let maps: Vec<_> = parsed_notes(&entries)
        .filter(|&(_, note)| {
            note.scalar("type") == Some("reference") && note.scalar("status") != Some("superseded")
        })
        .map(|(_, note)| {
            (
                note.text("id", "?"),
                note.text("title", ""),
                note.text("summary", ""),
                note.body.as_str(),
            )
        })
        .collect();

    let mut docs = Vec::with_capacity(names.len());
    for name in names {
        let path = directory.join(&name);
        if !store::regular_file(&path)? {
            continue;
        }
        let content = fs::read(&path).map(|bytes| {
            let text = String::from_utf8_lossy(&bytes);
            let text = normalize_newlines(&text);
            let map_ids = maps
                .iter()
                .filter(|(_, title, summary, body)| {
                    title.contains(&name) || summary.contains(&name) || body.contains(&name)
                })
                .map(|(id, _, _, _)| id.to_string())
                .collect();
            DocContent {
                words: text
                    .split(python_whitespace)
                    .filter(|word| !word.is_empty())
                    .count(),
                headings: headings(&text),
                maps: map_ids,
            }
        });
        docs.push(Doc { name, content });
    }
    render_refs(
        &docs,
        &filter.trim_matches(python_whitespace).to_lowercase(),
        out,
    )
}

fn markdown_names(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.as_encoded_bytes().ends_with(b".md") {
            continue;
        }
        if entry.file_type()?.is_dir() {
            // A real directory is out of scope, but a junction must still be refused.
            store::directory(&entry.path())?;
            continue;
        }
        let name = name.into_string().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("non-UTF-8 reference name: {}", entry.path().display()),
            )
        })?;
        names.push(name);
    }
    names.sort_unstable();
    Ok(names)
}

fn python_whitespace(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')
}

fn headings(text: &str) -> Vec<String> {
    let mut in_fence = false;
    text.split([
        '\n', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}',
    ])
    .filter_map(|line| {
        if line
            .trim_start_matches(python_whitespace)
            .starts_with("```")
        {
            in_fence = !in_fence;
            return None;
        }
        if in_fence {
            return None;
        }
        line.strip_prefix("## ")
            .map(|heading| heading.trim_matches(python_whitespace).to_owned())
    })
    .collect()
}

fn render_refs(docs: &[Doc], filter: &str, out: &mut impl Write) -> anyhow::Result<()> {
    let shown: Vec<_> = docs.iter().filter(|doc| doc.matches(filter)).collect();
    writeln!(
        out,
        "knowbox refs — heading index over refs/ ({} of {} docs shown; read via the map note, section by section, never whole)",
        shown.len(),
        docs.len()
    )?;
    for doc in shown {
        let name = &doc.name;
        match &doc.content {
            Err(error) => writeln!(out, "\n{name}  [?]  map: UNREADABLE ({error})")?,
            Ok(content) => {
                let label = if content.maps.is_empty() {
                    "UNMAPPED".to_owned()
                } else {
                    content.maps.join(",")
                };
                writeln!(out, "\n{name}  [{}w]  map: {label}", content.words)?;
                for heading in content.headings.iter().take(HEADING_CAP) {
                    writeln!(out, "  - {}", truncate(heading, HEADING_WIDTH))?;
                }
                if content.headings.len() > HEADING_CAP {
                    writeln!(
                        out,
                        "  … +{} more sections",
                        content.headings.len() - HEADING_CAP
                    )?;
                }
            }
        }
    }
    let unmapped = docs
        .iter()
        .filter(|doc| {
            doc.content
                .as_ref()
                .is_ok_and(|content| content.maps.is_empty())
        })
        .count();
    writeln!(
        out,
        "\n{} doc(s), {} mapped, {unmapped} UNMAPPED",
        docs.len(),
        docs.len() - unmapped
    )?;
    if unmapped > 0 {
        writeln!(
            out,
            "act on UNMAPPED: create a type:reference section-map note (SCHEMA.md § Large docs) — unmapped docs are invisible to the need-knowledge protocol."
        )?;
    }
    Ok(())
}

fn truncate(text: &str, width: usize) -> &str {
    text.char_indices()
        .nth(width)
        .map_or(text, |(index, _)| &text[..index])
}

struct StaleRow<'a> {
    age: i64,
    flags: String,
    id: Cow<'a, str>,
    status: Cow<'a, str>,
    updated: Cow<'a, str>,
    verified: Option<Cow<'a, str>>,
    title: Cow<'a, str>,
}

/// Report live notes ranked by day age at the supplied local calendar date.
pub(crate) fn stale(
    root: &KbRoot,
    days: u32,
    today: Date,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    if days < 1 {
        bail!("knowbox stale: --days must be >= 1");
    }
    let entries = notes::snapshot(root)?;
    let mut rows: Vec<_> = parsed_notes(&entries)
        .filter_map(|(_, note)| {
            if note.scalar("status") == Some("superseded") {
                return None;
            }
            let updated = truthy_text(note, "updated").or_else(|| truthy_text(note, "created"));
            let verified = truthy_text(note, "verified");
            let age = updated.as_deref().and_then(|date| {
                parse_date(date).map(|date| today.duration_since(date).as_secs() / 86_400)
            });
            let mut flags = Vec::new();
            if age.is_some_and(|age| age >= i64::from(days)) {
                flags.push("STALE");
            }
            if let Some(verified) = verified.as_deref() {
                if updated.as_deref().is_some_and(|updated| updated > verified) {
                    flags.push("EDITED-SINCE-VERIFY");
                }
            } else if age.is_some_and(|age| age >= i64::from(days / 2)) {
                flags.push("NEVER-VERIFIED");
            }
            Some(StaleRow {
                age: age.unwrap_or(-1),
                flags: flags.join("+"),
                id: note.text("id", "?"),
                status: note.text("status", "?"),
                updated: updated.unwrap_or(Cow::Borrowed("?")),
                verified,
                title: display_list(note, "title"),
            })
        })
        .collect();
    rows.sort_by(|a, b| (Reverse(a.age), &a.id).cmp(&(Reverse(b.age), &b.id)));
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
    for row in &rows {
        writeln!(
            out,
            "{:16} {:8} {:9} {:11} {:>3}d {:11} {}",
            row.flags,
            row.id,
            row.status,
            row.updated,
            row.age,
            row.verified.as_deref().unwrap_or("never"),
            truncate(&row.title, STALE_TITLE_WIDTH)
        )?;
    }
    let flagged = rows.iter().filter(|row| !row.flags.is_empty()).count();
    writeln!(
        out,
        "\n{} live note(s); oldest update {}d ago; {flagged} flagged",
        rows.len(),
        oldest.age
    )?;
    if flagged > 0 {
        writeln!(
            out,
            "act on flags: re-verify with 'knowbox verify ID --evidence …', update, or supersede."
        )?;
    }
    Ok(())
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

    #[test]
    fn refs_filters_by_map_id_skips_fences_and_counts_unicode_characters() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let path = root.path().join("refs");
        fs::create_dir(&path)?;
        fs::create_dir(path.join("directory.md"))?;
        fs::write(
            path.join("a-guide.md"),
            format!("## {}\r\n  ```md\r## Hidden\n```\n", "見".repeat(71)),
        )?;
        fs::write(path.join("b-orphan.md"), b"## No map")?;
        fs::write(
            root.path().join("notes/kb-0001.md"),
            "---\nid: kb-0001\ntype: reference\n---\na-guide.md",
        )?;
        fs::write(
            root.path().join("notes/kb-0002.md"),
            "---\nid: kb-0002\ntype: reference\nstatus: superseded\n---\nb-orphan.md",
        )?;
        let mut out = Vec::new();
        refs(&root, " KB-0001 ", &mut out)?;
        assert_eq!(
            String::from_utf8(out)?,
            format!(
                concat!(
                    "knowbox refs — heading index over refs/ (1 of 2 docs shown; read via the map note, section by section, never whole)\n",
                    "\na-guide.md  [6w]  map: kb-0001\n  - {}\n",
                    "\n2 doc(s), 1 mapped, 1 UNMAPPED\n",
                    "act on UNMAPPED: create a type:reference section-map note (SCHEMA.md § Large docs) — unmapped docs are invisible to the need-knowledge protocol.\n",
                ),
                "見".repeat(70)
            )
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refs_refuses_linked_documents_before_reading_them() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let path = root.path().join("refs");
        fs::create_dir(&path)?;
        let external = root.path().join("elsewhere.md");
        fs::write(&external, "## Never read")?;
        std::os::unix::fs::symlink(external, path.join("external.md"))?;
        let mut out = Vec::new();
        assert!(
            matches!(refs(&root, "", &mut out), Err(error) if error.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::InvalidInput))
        );
        assert_eq!(out, b"");
        Ok(())
    }

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
        stale(&root, 2, Date::new(2024, 3, 1)?, &mut out)?;
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
