mod context;
mod graph;
mod refs;
mod reports;

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::Serialize;

use crate::config::KbRoot;
use crate::notes::{self, Note, NoteEntry, NoteValue};
use crate::store::Inventory;

pub(crate) use context::{ContextMode, context};
pub(crate) use graph::{
    Depth, SearchNeighbor, authored_edges, expand, graph, render_search_expansion, search_expansion,
};
pub(crate) use refs::{ref_section, refs};
pub(crate) use reports::{parse_date, stale};

fn parsed_notes(entries: &[NoteEntry]) -> impl Iterator<Item = (&str, &Note)> {
    entries.iter().filter_map(|entry| match &entry.parsed {
        Ok(note) => Some((entry.relative.as_str(), note)),
        // A parse failure must not publish a partial note or become a graph node.
        Err(_) => None,
    })
}

fn truthy_text<'a>(note: &'a Note, key: &str) -> Option<Cow<'a, str>> {
    match note.scalar(key) {
        Some("") => None,
        Some(text) => Some(Cow::Borrowed(text)),
        None => {
            let value = note.text(key, "");
            (!value.is_empty() && value != "[]").then_some(value)
        }
    }
}

fn display_list<'a>(note: &'a Note, key: &str) -> Cow<'a, str> {
    match note.scalar(key) {
        Some(text) => Cow::Borrowed(text),
        None => Cow::Owned(note.items(key).join(", ")),
    }
}

fn quoted(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for ch in value.chars() {
        match ch {
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch == quote || ch == '\\' => {
                result.push('\\');
                result.push(ch);
            }
            ch if ch.is_control() => result.extend(ch.escape_default()),
            ch => result.push(ch),
        }
    }
    result.push(quote);
    result
}

#[derive(Debug, Serialize)]
pub(crate) struct NoteList {
    notes: Vec<ListRow>,
}

#[derive(Debug, Serialize)]
struct ListRow {
    path: String,
    id: String,
    title: String,
    domain: String,
    #[serde(rename = "type")]
    kind: String,
    frontmatter: BTreeMap<String, NoteValue>,
}

/// List notes in inventory order, optionally filtered by domain and type.
pub(crate) fn list(
    root: &KbRoot,
    domain: Option<&str>,
    kind: Option<&str>,
    policy: &mut Inventory<'_>,
) -> anyhow::Result<NoteList> {
    let entries = notes::snapshot_with(root, policy)?;
    let domain = domain
        .filter(|domain| !domain.is_empty())
        .map(str::to_lowercase);
    let kind = kind.filter(|kind| !kind.is_empty());
    let mut notes = Vec::new();
    for entry in entries {
        let Ok(note) = entry.parsed else { continue };
        if domain
            .as_ref()
            .is_some_and(|filter| *filter != note.text("domain", "").to_lowercase())
            || kind.is_some_and(|filter| note.scalar("type") != Some(filter))
        {
            continue;
        }
        notes.push(ListRow {
            path: entry.relative,
            id: note.text("id", "?").into_owned(),
            title: note.text("title", "?").into_owned(),
            kind: note.text("type", "?").into_owned(),
            domain: note.text("domain", "?").into_owned(),
            frontmatter: note.meta,
        });
    }
    Ok(NoteList { notes })
}

impl NoteList {
    pub(crate) fn render(&self, out: &mut impl Write) -> anyhow::Result<()> {
        for note in &self.notes {
            writeln!(
                out,
                "{:8} [{:9}] {:12} {}",
                note.id, note.kind, note.domain, note.title
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Recent {
    project: Option<String>,
    total: usize,
    sessions: Vec<Session>,
}

#[derive(Debug, Serialize)]
struct Session {
    path: String,
    id: String,
    project: String,
    created: String,
    title: String,
    summary: String,
}

/// List sessions newest first, breaking date ties by descending ID.
pub(crate) fn recent(
    root: &KbRoot,
    limit: usize,
    project: Option<&str>,
    policy: &mut Inventory<'_>,
) -> anyhow::Result<Recent> {
    let project = project.filter(|project| !project.is_empty());
    let entries = notes::snapshot_with(root, policy)?;
    let mut sessions: Vec<_> = parsed_notes(&entries)
        .filter_map(|(path, note)| {
            if note.scalar("type") != Some("session") {
                return None;
            }
            let name = note.text("project", "?");
            if project.is_some_and(|project| project != name) {
                return None;
            }
            Some(Session {
                path: path.to_owned(),
                id: note.text("id", "?").into_owned(),
                project: name.into_owned(),
                created: note.text("created", "").into_owned(),
                title: display_list(note, "title").into_owned(),
                summary: display_list(note, "summary").into_owned(),
            })
        })
        .collect();
    sessions.sort_by(|a, b| Reverse((&a.created, &a.id)).cmp(&Reverse((&b.created, &b.id))));
    let total = sessions.len();
    sessions.truncate(limit);
    Ok(Recent {
        project: project.map(str::to_owned),
        total,
        sessions,
    })
}

impl Recent {
    pub(crate) fn render(&self, out: &mut impl Write) -> anyhow::Result<()> {
        let project = self.project.as_deref();
        if self.total == 0 {
            let scope =
                project.map_or_else(String::new, |name| format!(" for project {}", quoted(name)));
            writeln!(
                out,
                "no session notes{scope} yet. Save one on request: knowbox new --type session --domain sessions --title \"<date> <topic>\" (template: SCHEMA.md 'Session notes')"
            )?;
            return Ok(());
        }
        let scope = project.map_or_else(
            || " (all projects)".to_owned(),
            |name| format!(" (project: {name})"),
        );
        writeln!(out, "knowbox recent — newest session notes first{scope}")?;
        for Session {
            id,
            created,
            project,
            title,
            summary,
            path,
        } in &self.sessions
        {
            writeln!(
                out,
                "{id}  {created}  [{project}]  {title}\n    {summary}\n    {path}"
            )?;
        }
        Ok(())
    }
}

/// The nearest repository basename, or the current directory basename outside Git.
pub(crate) fn project_from_cwd() -> io::Result<String> {
    let cwd = env::current_dir()?.canonicalize()?;
    let home = match env::home_dir() {
        Some(path) => match path.canonicalize() {
            Ok(path) => Some(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        },
        None => None,
    };
    detect_project(&cwd, home.as_deref())
}

fn detect_project(cwd: &Path, home: Option<&Path>) -> io::Result<String> {
    let name = |path: &Path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    for path in cwd.ancestors() {
        match fs::symlink_metadata(path.join(".git")) {
            Ok(meta) if meta.is_dir() || meta.is_file() => return Ok(name(path)),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(if home == Some(cwd) {
        "machine".to_owned()
    } else {
        name(cwd)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn fixture() -> anyhow::Result<(tempfile::TempDir, KbRoot)> {
        let dir = tempfile::tempdir()?;
        fs::create_dir(dir.path().join("notes"))?;
        let root = KbRoot::open(dir.path())?;
        Ok((dir, root))
    }

    #[test]
    fn list_preserves_inventory_order_and_filters_domain_case_insensitively() -> anyhow::Result<()>
    {
        let (_dir, root) = fixture()?;
        for (file, text) in [
            (
                "a.md",
                "---\nid: kb-9999\ntitle: First\ndomain: NiÑo\ntype: fact\nsource: [refs/a.md, refs/b.md]\n---",
            ),
            (
                "b.md",
                "---\nid: kb-0001\ntitle: Second\ndomain: niño\ntype: fact\n---",
            ),
            ("c.md", "---\ntype: Fact\ndomain: niño\n---"),
            ("d.md", "---\ntype: fact\ndomain: niño"),
        ] {
            fs::write(root.path().join("notes").join(file), text)?;
        }
        let mut output = Vec::new();
        let result = list(&root, Some("NIÑO"), Some("fact"), &mut Inventory::Strict)?;
        let json = serde_json::to_value(&result)?;
        assert_eq!(json["notes"][0]["path"], "notes/a.md");
        assert_eq!(
            json["notes"][0]["frontmatter"]["source"],
            serde_json::json!(["refs/a.md", "refs/b.md"])
        );
        result.render(&mut output)?;
        assert_eq!(
            String::from_utf8(output)?,
            concat!(
                "kb-9999  [fact     ] NiÑo         First\n",
                "kb-0001  [fact     ] niño         Second\n",
            )
        );
        Ok(())
    }

    #[test]
    fn recent_limits_after_project_and_descending_id_tie_break() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        for (id, project, created) in [
            ("kb-0001", "app", "2025-11-10"),
            ("kb-0002", "app", "2025-11-10"),
            ("kb-0003", "App", "2025-11-11"),
        ] {
            fs::write(
                root.path().join("notes").join(format!("{id}.md")),
                format!(
                    "---\nid: {id}\nproject: {project}\ntype: session\ncreated: {created}\ntitle: [one, two]\nsummary: state\n---"
                ),
            )?;
        }
        let mut output = Vec::new();
        recent(&root, 1, Some("app"), &mut Inventory::Strict)?.render(&mut output)?;
        assert_eq!(
            String::from_utf8(output)?,
            concat!(
                "knowbox recent — newest session notes first (project: app)\n",
                "kb-0002  2025-11-10  [app]  one, two\n",
                "    state\n",
                "    notes/kb-0002.md\n",
            )
        );
        Ok(())
    }

    #[test]
    fn project_detection_respects_worktree_marker_and_home() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let nested = dir.path().join("outer/inner/subdir");
        fs::create_dir_all(&nested)?;
        assert_eq!(detect_project(&nested, Some(&nested))?, "machine");
        fs::create_dir(dir.path().join("outer/.git"))?;
        fs::write(
            dir.path().join("outer/inner/.git"),
            "gitdir: ../../somewhere",
        )?;
        assert_eq!(detect_project(&nested, Some(&nested))?, "inner");
        Ok(())
    }
}
