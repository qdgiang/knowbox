use std::fs;
use std::io::Write;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::Serialize;

use crate::config::KbRoot;
use crate::notes::{self, normalize_newlines};
use crate::store::{EntryKind, Inventory};

use super::{parsed_notes, reports::truncate};

const HEADING_CAP: usize = 12;
const HEADING_WIDTH: usize = 70;

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum RefsReport {
    Missing,
    Empty,
    Ready {
        docs: Vec<RefDoc>,
        total: usize,
        unmapped: usize,
        unreadable: usize,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct RefDoc {
    name: String,
    path: String,
    #[serde(flatten)]
    content: RefContent,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum RefContent {
    Unreadable {
        error: String,
    },
    Readable {
        words: usize,
        headings: Vec<Heading>,
        maps: Vec<String>,
        #[serde(skip)]
        text_headings: Vec<String>,
    },
}

#[derive(Debug, Serialize)]
struct Heading {
    level: usize,
    line: usize,
    text: String,
}

impl RefDoc {
    fn matches(&self, filter: &str) -> bool {
        self.name.to_lowercase().contains(filter)
            || matches!(&self.content, RefContent::Readable { maps, text_headings, .. }
                if text_headings.iter().chain(maps).any(|s| s.to_lowercase().contains(filter)))
    }
}

pub(crate) fn refs(
    root: &KbRoot,
    filter: &str,
    policy: &mut Inventory<'_>,
) -> anyhow::Result<RefsReport> {
    let directory = root.path().join("refs");
    if !policy.directory(&directory)? {
        return Ok(RefsReport::Missing);
    }
    let names = policy.names(&directory, EntryKind::MarkdownFile)?;
    if names.is_empty() {
        return Ok(RefsReport::Empty);
    }
    let entries = notes::snapshot_with(root, policy)?;
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

    let filter = filter.trim_matches(python_whitespace).to_lowercase();
    let mut docs = Vec::new();
    let mut unmapped = 0;
    let mut unreadable = 0;
    let mut total = 0;
    for name in names {
        let path = directory.join(&name);
        if !policy.file(&path)? {
            continue;
        }
        let content = match fs::read(&path) {
            Err(error) => {
                unreadable += 1;
                RefContent::Unreadable {
                    error: error.to_string(),
                }
            }
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let text = normalize_newlines(&text);
                let map_ids: Vec<_> = maps
                    .iter()
                    .filter(|(_, title, summary, body)| {
                        title.contains(&name) || summary.contains(&name) || body.contains(&name)
                    })
                    .map(|(id, _, _, _)| id.to_string())
                    .collect();
                unmapped += usize::from(map_ids.is_empty());
                RefContent::Readable {
                    words: text
                        .split(python_whitespace)
                        .filter(|word| !word.is_empty())
                        .count(),
                    headings: headings(&text),
                    maps: map_ids,
                    text_headings: text_headings(&text),
                }
            }
        };
        total += 1;
        let doc = RefDoc {
            path: format!("refs/{name}"),
            name,
            content,
        };
        if doc.matches(&filter) {
            docs.push(doc);
        }
    }
    Ok(RefsReport::Ready {
        docs,
        total,
        unmapped,
        unreadable,
    })
}

pub(super) fn python_whitespace(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')
}

// This matches the original CLI's Python splitlines, H2 syntax and simple fence toggle.
fn text_headings(text: &str) -> Vec<String> {
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

fn headings(text: &str) -> Vec<Heading> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut result = Vec::new();
    let mut current: Option<Heading> = None;
    let mut start = 0;
    let mut line = 1;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                line += text[start..range.start]
                    .bytes()
                    .filter(|&b| b == b'\n')
                    .count();
                start = range.start;
                current = Some(Heading {
                    level: level as usize,
                    line,
                    text: String::new(),
                });
            }
            Event::End(TagEnd::Heading(_)) => result.extend(current.take()),
            Event::Text(value) | Event::Code(value) if let Some(heading) = &mut current => {
                heading.text.push_str(&value);
            }
            Event::SoftBreak | Event::HardBreak if let Some(heading) = &mut current => {
                heading.text.push(' ');
            }
            _ => {}
        }
    }
    result
}

#[derive(Debug, Serialize)]
pub(crate) struct RefSection {
    path: String,
    start_line: usize,
    end_line: usize,
    pub(crate) markdown: String,
}

pub(crate) fn ref_section(
    root: &KbRoot,
    relative: &str,
    line: usize,
    policy: &mut Inventory<'_>,
) -> anyhow::Result<Option<RefSection>> {
    let Some(name) = relative
        .strip_prefix("refs/")
        .filter(|name| name.ends_with(".md") && !name.contains('/'))
    else {
        return Ok(None);
    };
    let directory = root.path().join("refs");
    if !policy.directory(&directory)?
        || !policy
            .names(&directory, EntryKind::MarkdownFile)?
            .iter()
            .any(|candidate| candidate == name)
    {
        return Ok(None);
    }
    let path = directory.join(name);
    let bytes = crate::store::read_regular(&path)?;
    let original = String::from_utf8_lossy(&bytes);
    let text = normalize_newlines(&original);
    let sections = headings(&text);
    let selected = sections.iter().find(|heading| heading.line == line);
    if line == 0 || (line != 1 && selected.is_none()) {
        return Ok(None);
    }
    let next = sections
        .iter()
        .find(|heading| {
            heading.line > line && selected.is_none_or(|first| heading.level <= first.level)
        })
        .map(|heading| heading.line);
    let mut start = 0;
    let mut end = 0;
    let mut end_line = line;
    for (index, row) in text.split_inclusive('\n').enumerate() {
        let number = index + 1;
        if Some(number) == next {
            break;
        }
        end += row.len();
        if number < line {
            start = end;
        }
        end_line = number;
    }
    Ok(Some(RefSection {
        path: relative.to_owned(),
        start_line: line,
        end_line,
        markdown: text[start..end].to_owned(),
    }))
}

impl RefsReport {
    pub(crate) fn render(&self, out: &mut impl Write) -> anyhow::Result<()> {
        let (docs, total, unmapped) = match self {
            Self::Missing => {
                writeln!(
                    out,
                    "knowbox refs: no refs/ directory — nothing ingested yet"
                )?;
                return Ok(());
            }
            Self::Empty => {
                writeln!(out, "knowbox refs: refs/ is empty")?;
                return Ok(());
            }
            Self::Ready {
                docs,
                total,
                unmapped,
                unreadable: _,
            } => (docs, total, unmapped),
        };
        writeln!(
            out,
            "knowbox refs — heading index over refs/ ({} of {total} docs shown; read via the map note, section by section, never whole)",
            docs.len()
        )?;
        for doc in docs {
            let name = &doc.name;
            match &doc.content {
                RefContent::Unreadable { error } => {
                    writeln!(out, "\n{name}  [?]  map: UNREADABLE ({error})")?
                }
                RefContent::Readable {
                    words,
                    maps,
                    text_headings,
                    ..
                } => {
                    let label = if maps.is_empty() {
                        "UNMAPPED".to_owned()
                    } else {
                        maps.join(",")
                    };
                    writeln!(out, "\n{name}  [{words}w]  map: {label}")?;
                    for heading in text_headings.iter().take(HEADING_CAP) {
                        writeln!(out, "  - {}", truncate(heading, HEADING_WIDTH))?;
                    }
                    if text_headings.len() > HEADING_CAP {
                        writeln!(
                            out,
                            "  … +{} more sections",
                            text_headings.len() - HEADING_CAP
                        )?;
                    }
                }
            }
        }
        writeln!(
            out,
            "\n{total} doc(s), {} mapped, {unmapped} UNMAPPED",
            total - unmapped
        )?;
        if *unmapped > 0 {
            writeln!(
                out,
                "act on UNMAPPED: create a type:reference section-map note (SCHEMA.md § Large docs) — unmapped docs are invisible to the need-knowledge protocol."
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    #[cfg(unix)]
    use std::io;

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
        refs(
            &root,
            " KB-0001 ",
            &mut Inventory::Read(&mut Default::default()),
        )?
        .render(&mut out)?;
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

    #[test]
    fn refs_json_includes_full_heading_text_levels_and_source_lines_beyond_the_text_cap()
    -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let directory = root.path().join("refs");
        fs::create_dir(&directory)?;
        let mut source = String::from(
            "# Book\r\n### A *nested* [section](target)\r\n~~~~\r\n### Fenced\r\n~~~~\r\n> Setext\r\n> ======\r\n",
        );
        for n in 1..=12 {
            source.push_str(&format!("## Section {n}\r\n"));
        }
        let long = "界".repeat(78);
        source.push_str(&format!("## {long}\r\n"));
        fs::write(directory.join("guide.md"), source)?;
        let report = refs(&root, "", &mut Inventory::Strict)?;
        let json = serde_json::to_value(&report)?;
        let rows = json["docs"][0]["headings"]
            .as_array()
            .context("expected heading array")?;
        assert_eq!(rows.len(), 16);
        assert_eq!(
            rows[0],
            serde_json::json!({"text":"Book","level":1,"line":1})
        );
        assert_eq!(
            rows[1],
            serde_json::json!({"text":"A nested section","level":3,"line":2})
        );
        assert_eq!(
            rows[2],
            serde_json::json!({"text":"Setext","level":1,"line":6})
        );
        assert_eq!(
            rows[15],
            serde_json::json!({"text":long,"level":2,"line":20})
        );
        let mut out = Vec::new();
        report.render(&mut out)?;
        let text = String::from_utf8(out)?;
        assert_eq!(text.lines().filter(|s| s.starts_with("  -")).count(), 12);
        assert_eq!(
            text.lines().find(|s| s.starts_with("  …")),
            Some("  … +1 more sections")
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
        assert!(
            matches!(refs(&root, "", &mut Inventory::Strict), Err(error) if error.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::InvalidInput))
        );
        Ok(())
    }
}
