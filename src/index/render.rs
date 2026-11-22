use std::collections::{BTreeMap, HashMap};
use std::io;

use crate::config::KbRoot;
use crate::notes::{Note, NoteEntry};
use crate::store::{EntryKind, Inventory};

pub(crate) fn ref_names(root: &KbRoot, policy: &mut Inventory<'_>) -> io::Result<Vec<String>> {
    let dir = root.path().join("refs");
    if !policy.directory(&dir)? {
        return Ok(Vec::new());
    }
    policy.names(&dir, EntryKind::Visible)
}

pub(super) fn render(notes: &[NoteEntry], refs: &[String]) -> (String, usize) {
    let mut successors = HashMap::new();
    let mut domains = BTreeMap::<String, Vec<(&str, &Note)>>::new();
    let mut count = 0;
    for entry in notes {
        let Ok(note) = &entry.parsed else { continue };
        for id in note.items("supersedes") {
            successors.insert(id, note.text("id", "?"));
        }
        if note.scalar("type") == Some("session") {
            continue;
        }
        let mut domain = note.text("domain", "misc").to_lowercase();
        if domain.is_empty() {
            domain.push_str("misc");
        }
        domains
            .entry(domain)
            .or_default()
            .push((&entry.relative, note));
        count += 1;
    }
    let mut lines = vec![
        "# KB Index".to_owned(),
        String::new(),
        "> Personal knowledge base. **Read this file first, then open only the".to_owned(),
        "> note(s) you need.** Conventions: SCHEMA.md. CLI: `knowbox search/show/new`.".to_owned(),
        "> Entries: `- [id · title](path): summary`. Generated — do not edit by hand.".to_owned(),
        String::new(),
        format!(
            "**{count} notes** · domains: {}",
            domains
                .keys()
                .map(|d| format!("[{d}](#{d})"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        String::new(),
    ];
    for (domain, mut group) in domains {
        lines.extend([format!("## {domain}"), String::new()]);
        group.sort_by_cached_key(|(_, note)| note.text("id", ""));
        for (rel, note) in group {
            let id = note.text("id", "?");
            let title = note.text("title", "?");
            let summary = note.text("summary", "—");
            let mut line = format!("- [{id} · {title}]({rel}): {summary}");
            if note.scalar("status") == Some("superseded") {
                line.push_str(" ⚠SUPERSEDED");
                if let Some(next) = successors.get(note.text("id", "").as_ref())
                    && !next.is_empty()
                {
                    line.push_str(&format!("→[[{next}]]"));
                }
            }
            lines.push(line);
        }
        lines.push(String::new());
    }
    if !refs.is_empty() {
        lines.extend([
            "## refs/ — large docs (read via their section-map note)".to_owned(),
            String::new(),
        ]);
        lines.extend(refs.iter().map(|name| format!("- `refs/{name}`")));
        lines.push(String::new());
    }
    (lines.join("\n"), count)
}
