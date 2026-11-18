use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;

use anyhow::{Context, bail};

use crate::config::KbRoot;
use crate::notes::{self, NoteEntry};

use super::{parsed_notes, quoted, truthy_text};

const LEVEL_CAP: usize = 12;
const SEARCH_SEEDS: usize = 3;
const SEARCH_PER_SEED: usize = 4;
const SEARCH_TOTAL: usize = 8;

/// The maximum number of hops to display in `expand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    One,
    Two,
}

// Declaration order is the precedence when multiple channels assert one edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Edge {
    Supersedes,
    Related,
    Mention,
}

impl Edge {
    fn name(self) -> &'static str {
        match self {
            Self::Supersedes => "supersedes",
            Self::Related => "related",
            Self::Mention => "mention",
        }
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Out,
    In,
}

#[derive(Clone, Copy)]
struct Neighbor<'a> {
    id: &'a str,
    edge: Edge,
    direction: Direction,
}

impl Neighbor<'_> {
    fn label(self) -> &'static str {
        match (self.direction, self.edge) {
            (Direction::In, Edge::Supersedes) => "superseded-by",
            (Direction::In | Direction::Out, edge) => edge.name(),
        }
    }

    fn search_provenance(self, seed: &str) -> String {
        match (self.direction, self.edge) {
            (Direction::Out, edge) => format!("via {}: {seed}", edge.name()),
            (Direction::In, Edge::Supersedes) => format!("supersedes: {seed}"),
            (Direction::In, Edge::Related | Edge::Mention) => format!("← backlink: {seed}"),
        }
    }
}

struct Node<'a> {
    path: &'a str,
    title: Cow<'a, str>,
    superseded: bool,
    outgoing: BTreeMap<&'a str, Edge>,
    incoming: BTreeMap<&'a str, Edge>,
}

impl<'a> Node<'a> {
    fn tag(&self) -> &'static str {
        if self.superseded { " (superseded)" } else { "" }
    }

    fn neighbors(&self, direction: Direction) -> impl Iterator<Item = Neighbor<'a>> {
        let edges = match direction {
            Direction::Out => &self.outgoing,
            Direction::In => &self.incoming,
        };
        edges.iter().map(move |(&id, &edge)| Neighbor {
            id,
            edge,
            direction,
        })
    }

    fn all_neighbors(&self) -> impl Iterator<Item = Neighbor<'a>> {
        self.neighbors(Direction::Out)
            .chain(self.neighbors(Direction::In))
    }
}

type Graph<'a> = BTreeMap<&'a str, Node<'a>>;

fn id_prefix(value: &str) -> Option<&str> {
    let id = value.get(..7)?;
    (id.starts_with("kb-") && id.as_bytes()[3..].iter().all(u8::is_ascii_digit)).then_some(id)
}

fn wikilinks(body: &str) -> impl Iterator<Item = &str> {
    body.match_indices('[').filter_map(move |(offset, _)| {
        let rest = body[offset..].strip_prefix("[[")?;
        let target = id_prefix(rest)?;
        rest[target.len()..].starts_with("]]").then_some(target)
    })
}

fn build_graph(entries: &[NoteEntry]) -> Graph<'_> {
    let mut graph = BTreeMap::new();
    let mut sources = Vec::new();
    for (path, note) in parsed_notes(entries) {
        let id = note
            .scalar("id")
            .map(str::trim)
            .filter(|id| id_prefix(id) == Some(id))
            .or_else(|| path.rsplit('/').next().and_then(id_prefix));
        let Some(id) = id else { continue };
        sources.push((id, note));
        graph.insert(
            id,
            Node {
                path,
                title: truthy_text(note, "title").unwrap_or(Cow::Borrowed(id)),
                superseded: note.scalar("status") == Some("superseded"),
                outgoing: BTreeMap::new(),
                incoming: BTreeMap::new(),
            },
        );
    }
    for (source, note) in sources {
        let edges = note
            .items("related")
            .into_iter()
            .map(|id| (id.trim(), Edge::Related))
            .chain(
                note.items("supersedes")
                    .into_iter()
                    .map(|id| (id.trim(), Edge::Supersedes)),
            )
            .chain(wikilinks(&note.body).map(|id| (id, Edge::Mention)));
        for (target, edge) in edges {
            if target == source {
                continue;
            }
            if let Some(node) = graph.get_mut(target) {
                fold_edge(&mut node.incoming, source, edge);
                let Some(node) = graph.get_mut(source) else {
                    unreachable!("graph source was inserted")
                };
                fold_edge(&mut node.outgoing, target, edge);
            }
        }
    }
    graph
}

fn fold_edge<'a>(edges: &mut BTreeMap<&'a str, Edge>, id: &'a str, edge: Edge) {
    edges
        .entry(id)
        .and_modify(|prev| *prev = (*prev).min(edge))
        .or_insert(edge);
}

/// Print an author-written link neighborhood; unknown or unparseable seeds are errors.
pub(crate) fn expand(
    root: &KbRoot,
    selector: &str,
    depth: Depth,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let path = notes::find(root, selector)?;
    let id = path
        .as_deref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .and_then(id_prefix);
    let Some(id) = id else {
        bail!("knowbox expand: no note for {}", quoted(selector))
    };
    let entries = notes::snapshot(root).context("building link graph")?;
    let graph = build_graph(&entries);
    let Some(seed) = graph.get(id) else {
        bail!("knowbox expand: no note for {}", quoted(selector))
    };
    render_neighborhood(&graph, id, seed, depth, out)
}

fn render_neighborhood(
    graph: &Graph<'_>,
    id: &str,
    seed: &Node<'_>,
    depth: Depth,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    if seed.superseded {
        writeln!(
            out,
            "(note: {id} is superseded — showing its links; prefer the successor)"
        )?;
    }
    writeln!(out, "{id} — {}\n  {}", seed.title, seed.path)?;
    if seed.outgoing.is_empty() && seed.incoming.is_empty() {
        writeln!(out, "\n(no author-written links)")?;
        return Ok(());
    }
    let mut frontier = Vec::new();
    for (direction, header) in [
        (Direction::Out, "links to:"),
        (Direction::In, "linked from (backlinks):"),
    ] {
        let mut neighbors = seed.neighbors(direction).take(LEVEL_CAP).peekable();
        if neighbors.peek().is_some() {
            writeln!(out, "\n  {header}")?;
        }
        for neighbor in neighbors {
            print_neighbor(out, neighbor, &graph[neighbor.id], "")?;
            frontier.push(neighbor.id);
        }
    }
    if depth == Depth::Two {
        // Exclude all direct neighbors, including the ones hidden by the level cap.
        let mut seen: HashSet<_> = seed.all_neighbors().map(|neighbor| neighbor.id).collect();
        seen.insert(id);
        let second_hop = frontier
            .into_iter()
            .flat_map(|via| {
                graph[via]
                    .all_neighbors()
                    .map(move |neighbor| (via, neighbor))
            })
            .filter(|(_, neighbor)| seen.insert(neighbor.id))
            .take(LEVEL_CAP);
        for (index, (via, neighbor)) in second_hop.enumerate() {
            if index == 0 {
                writeln!(out, "\n  depth 2 (new neighbours, excluding the above):")?;
            }
            print_neighbor(out, neighbor, &graph[neighbor.id], &format!(" (via {via})"))?;
        }
    }
    Ok(())
}

fn print_neighbor(
    out: &mut impl Write,
    neighbor: Neighbor<'_>,
    node: &Node<'_>,
    suffix: &str,
) -> std::io::Result<()> {
    writeln!(
        out,
        "    {:<13} {}  {}{}{suffix}",
        neighbor.label(),
        neighbor.id,
        node.title,
        node.tag()
    )
}

/// Append a graph block for displayed search hits; unavailable graph data is nonfatal.
pub(crate) fn search_expansion(
    root: &KbRoot,
    ids: &[&str],
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let seeds = ids
        .iter()
        .copied()
        .take(SEARCH_SEEDS)
        .filter(|id| !id.is_empty());
    if seeds.clone().next().is_none() {
        return Ok(());
    }
    // Search has already returned valid hits; optional graph I/O cannot invalidate them.
    let Ok(entries) = notes::snapshot(root) else {
        return Ok(());
    };
    let graph = build_graph(&entries);
    let mut seen: HashSet<_> = ids.iter().copied().collect();
    let mut printed = 0;
    for seed in seeds {
        let Some(node) = graph.get(seed) else {
            continue;
        };
        let capacity = SEARCH_PER_SEED.min(SEARCH_TOTAL - printed);
        for neighbor in node
            .all_neighbors()
            .filter(|neighbor| seen.insert(neighbor.id))
            .take(capacity)
        {
            if printed == 0 {
                writeln!(
                    out,
                    "(graph neighbours of top hits — depth 1, opt-in --expand:)"
                )?;
            }
            let node = &graph[neighbor.id];
            writeln!(out, "    {}  {}{}", neighbor.id, node.path, node.tag())?;
            writeln!(
                out,
                "        {} — {}",
                neighbor.search_provenance(seed),
                node.title
            )?;
            printed += 1;
        }
        if printed == SEARCH_TOTAL {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::tests::fixture;
    use super::*;

    fn write_note(root: &KbRoot, id: &str, fields: &str, body: &str) -> std::io::Result<()> {
        fs::write(
            root.path().join("notes").join(format!("{id}.md")),
            format!("---\nid: {id}\n{fields}\n---\n{body}"),
        )
    }

    #[test]
    fn expansion_folds_precedence_and_keeps_first_depth_two_provenance() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        write_note(
            &root,
            "kb-0001",
            "status: superseded\nrelated: [kb-0002, kb-9999, kb-0001, kb-0006]\nsupersedes: kb-0002",
            "[[kb-0002]]",
        )?;
        write_note(&root, "kb-0002", "related: kb-0005", "")?;
        write_note(
            &root,
            "kb-0003",
            "related: kb-0005",
            "See [[[kb-0001]] and [[kb-00021]].",
        )?;
        write_note(&root, "kb-0004", "supersedes: kb-0001", "")?;
        write_note(&root, "kb-0005", "title: shared", "")?;
        fs::write(root.path().join("notes/kb-0006.md"), "---\nid: kb-0006")?;
        let mut one = Vec::new();
        expand(&root, "kb-0001", Depth::One, &mut one)?;
        let common = concat!(
            "(note: kb-0001 is superseded — showing its links; prefer the successor)\n",
            "kb-0001 — kb-0001\n  notes/kb-0001.md\n",
            "\n  links to:\n    supersedes    kb-0002  kb-0002\n",
            "\n  linked from (backlinks):\n    mention       kb-0003  kb-0003\n",
            "    superseded-by kb-0004  kb-0004\n",
        );
        assert_eq!(String::from_utf8(one)?, common);
        let mut two = Vec::new();
        expand(&root, "kb-0001.md", Depth::Two, &mut two)?;
        assert_eq!(
            String::from_utf8(two)?,
            format!(
                "{common}\n  depth 2 (new neighbours, excluding the above):\n    related       kb-0005  shared (via kb-0002)\n"
            )
        );
        Ok(())
    }

    #[test]
    fn search_expansion_dedups_all_displayed_ids_and_keeps_first_seed_label() -> anyhow::Result<()>
    {
        let (_dir, root) = fixture()?;
        write_note(&root, "kb-0001", "related: [kb-0003, kb-0004]", "")?;
        write_note(&root, "kb-0002", "related: [kb-0003, kb-0006]", "")?;
        write_note(&root, "kb-0003", "status: superseded", "")?;
        write_note(&root, "kb-0004", "title: displayed", "")?;
        write_note(&root, "kb-0005", "supersedes: kb-0001", "")?;
        write_note(&root, "kb-0006", "title: other", "")?;
        let mut out = Vec::new();
        search_expansion(&root, &["kb-0001", "kb-0002", "", "kb-0004"], &mut out)?;
        assert_eq!(
            String::from_utf8(out)?,
            concat!(
                "(graph neighbours of top hits — depth 1, opt-in --expand:)\n",
                "    kb-0003  notes/kb-0003.md (superseded)\n",
                "        via related: kb-0001 — kb-0003\n",
                "    kb-0005  notes/kb-0005.md\n",
                "        supersedes: kb-0001 — kb-0005\n",
                "    kb-0006  notes/kb-0006.md\n",
                "        via related: kb-0002 — other\n",
            )
        );
        Ok(())
    }

    #[test]
    fn visible_neighbor_caps_do_not_change_sort_order() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let all: Vec<_> = (100..114).map(|n| format!("kb-{n:04}")).collect();
        for id in &all {
            write_note(&root, id, "", "")?;
        }
        let fields = format!("related: [{}]", all.join(", "));
        for id in ["kb-0001", "kb-0002", "kb-0003"] {
            write_note(&root, id, &fields, "")?;
        }
        let mut out = Vec::new();
        expand(&root, "kb-0001", Depth::One, &mut out)?;
        let text = String::from_utf8(out)?;
        let ids: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("    related"))
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert_eq!(ids, all[..12]);

        let mut out = Vec::new();
        search_expansion(&root, &["kb-0001", "kb-0002", "kb-0003"], &mut out)?;
        let text = String::from_utf8(out)?;
        let ids: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("    kb-"))
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert_eq!(
            ids,
            [
                "0100", "0101", "0102", "0103", "0104", "0105", "0106", "0107"
            ]
        );
        Ok(())
    }
}
