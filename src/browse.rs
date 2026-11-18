mod graph;
mod reports;

use std::borrow::Cow;
use std::cmp::Reverse;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::config::KbRoot;
use crate::notes::{self, Note, NoteEntry};

pub(crate) use graph::{Depth, expand, search_expansion};
pub(crate) use reports::{parse_date, refs, stale};

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

/// List notes in inventory order, optionally filtered by domain and type.
pub(crate) fn list(
    root: &KbRoot,
    domain: Option<&str>,
    kind: Option<&str>,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let entries = notes::snapshot(root)?;
    let domain = domain
        .filter(|domain| !domain.is_empty())
        .map(str::to_lowercase);
    let kind = kind.filter(|kind| !kind.is_empty());
    for (_, note) in parsed_notes(&entries) {
        if domain
            .as_ref()
            .is_some_and(|filter| *filter != note.text("domain", "").to_lowercase())
            || kind.is_some_and(|filter| note.scalar("type") != Some(filter))
        {
            continue;
        }
        let id = note.text("id", "?");
        let kind = note.text("type", "?");
        let domain = note.text("domain", "?");
        let title = note.text("title", "?");
        writeln!(out, "{id:8} [{kind:9}] {domain:12} {title}")?;
    }
    Ok(())
}

struct Session<'a> {
    path: &'a str,
    note: &'a Note,
    id: Cow<'a, str>,
    project: Cow<'a, str>,
    created: Cow<'a, str>,
}

/// List sessions newest first, breaking date ties by descending ID.
pub(crate) fn recent(
    root: &KbRoot,
    limit: usize,
    project: Option<&str>,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let project = project.filter(|project| !project.is_empty());
    let entries = notes::snapshot(root)?;
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
                path,
                note,
                id: note.text("id", "?"),
                project: name,
                created: note.text("created", ""),
            })
        })
        .collect();
    if sessions.is_empty() {
        let scope =
            project.map_or_else(String::new, |name| format!(" for project {}", quoted(name)));
        writeln!(
            out,
            "no session notes{scope} yet. Save one on request: knowbox new --type session --domain sessions --title \"<date> <topic>\" (template: SCHEMA.md 'Session notes')"
        )?;
        return Ok(());
    }
    sessions.sort_by(|a, b| Reverse((&a.created, &a.id)).cmp(&Reverse((&b.created, &b.id))));
    let scope = project.map_or_else(
        || " (all projects)".to_owned(),
        |name| format!(" (project: {name})"),
    );
    writeln!(out, "knowbox recent — newest session notes first{scope}")?;
    for Session {
        path,
        note,
        id,
        project,
        created,
    } in sessions.into_iter().take(limit)
    {
        writeln!(
            out,
            "{id}  {created}  [{project}]  {}\n    {}\n    {path}",
            display_list(note, "title"),
            display_list(note, "summary")
        )?;
    }
    Ok(())
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
                "---\nid: kb-9999\ntitle: First\ndomain: NiÑo\ntype: fact\n---",
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
        list(&root, Some("NIÑO"), Some("fact"), &mut output)?;
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
        recent(&root, 1, Some("app"), &mut output)?;
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
