use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;

use anyhow::{Context, bail};
use serde::Serialize;

use crate::config::KbRoot;
use crate::notes::{self, NoteEntry};
use crate::store::Inventory;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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

struct Node<'a> {
    path: &'a str,
    title: Cow<'a, str>,
    superseded: bool,
    domain: Option<&'a str>,
    status: Option<&'a str>,
    outgoing: BTreeMap<&'a str, Edge>,
    incoming: BTreeMap<&'a str, Edge>,
}

impl<'a> Node<'a> {
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

struct GraphIndex<'a> {
    nodes: BTreeMap<&'a str, Node<'a>>,
    edges: BTreeMap<(&'a str, &'a str), Edge>,
    duplicates: BTreeMap<&'a str, Vec<&'a str>>,
}

fn build_graph(entries: &[NoteEntry]) -> GraphIndex<'_> {
    let mut nodes = BTreeMap::new();
    let mut duplicates = BTreeMap::<_, Vec<_>>::new();
    let mut edges = BTreeMap::new();
    for (path, note) in parsed_notes(entries) {
        let id = note
            .scalar("id")
            .map(str::trim)
            .filter(|id| notes::is_id(id))
            .or_else(|| path.rsplit('/').next().and_then(notes::id_prefix));
        let Some(id) = id else { continue };
        if let Some(previous) = nodes.insert(
            id,
            Node {
                path,
                title: truthy_text(note, "title").unwrap_or(Cow::Borrowed(id)),
                superseded: note.scalar("status") == Some("superseded"),
                domain: note.scalar("domain"),
                status: note.scalar("status"),
                outgoing: BTreeMap::new(),
                incoming: BTreeMap::new(),
            },
        ) {
            duplicates
                .entry(id)
                .or_insert_with(|| vec![previous.path])
                .push(path);
        }
        let links = note
            .items("related")
            .into_iter()
            .map(|id| (id.trim(), Edge::Related))
            .chain(
                note.items("supersedes")
                    .into_iter()
                    .map(|id| (id.trim(), Edge::Supersedes)),
            )
            .chain(notes::body_links(&note.body).map(|id| (id, Edge::Mention)));
        for (target, edge) in links {
            if !target.is_empty() {
                edges
                    .entry((id, target))
                    .and_modify(|prev: &mut Edge| *prev = (*prev).min(edge))
                    .or_insert(edge);
            }
        }
    }
    for (&(source, target), &edge) in &edges {
        if target != source
            && let Some(node) = nodes.get_mut(target)
        {
            node.incoming.insert(source, edge);
            let Some(node) = nodes.get_mut(source) else {
                unreachable!("graph source was inserted")
            };
            node.outgoing.insert(target, edge);
        }
    }
    GraphIndex {
        nodes,
        edges,
        duplicates,
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphReport {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    isolates: Vec<String>,
    duplicate_ids: Vec<DuplicateId>,
}

pub(crate) fn authored_edges(entries: &[NoteEntry]) -> Vec<(&str, &str)> {
    build_graph(entries).edges.into_keys().collect()
}

#[derive(Debug, Serialize)]
struct GraphNode {
    id: String,
    path: String,
    title: String,
    domain: Option<String>,
    status: Option<String>,
    superseded: bool,
}

impl GraphNode {
    fn new(id: &str, node: &Node<'_>) -> Self {
        Self {
            id: id.to_owned(),
            path: node.path.to_owned(),
            title: node.title.to_string(),
            domain: node.domain.map(str::to_owned),
            status: node.status.map(str::to_owned),
            superseded: node.superseded,
        }
    }
}

#[derive(Debug, Serialize)]
struct GraphEdge {
    source: String,
    target: String,
    kind: Edge,
    dangling: bool,
}

#[derive(Debug, Serialize)]
struct DuplicateId {
    id: String,
    paths: Vec<String>,
}

pub(crate) fn graph(root: &KbRoot, policy: &mut Inventory<'_>) -> anyhow::Result<GraphReport> {
    let entries = notes::snapshot_with(root, policy).context("building link graph")?;
    let graph = build_graph(&entries);
    let connected: HashSet<_> = graph
        .edges
        .keys()
        .flat_map(|&(source, target)| [source, target])
        .collect();
    Ok(GraphReport {
        nodes: graph
            .nodes
            .iter()
            .map(|(id, node)| GraphNode::new(id, node))
            .collect(),
        edges: graph
            .edges
            .into_iter()
            .map(|((source, target), kind)| GraphEdge {
                source: source.to_owned(),
                target: target.to_owned(),
                kind,
                dangling: !graph.nodes.contains_key(target),
            })
            .collect(),
        isolates: graph
            .nodes
            .keys()
            .copied()
            .filter(|id| !connected.contains(id))
            .map(str::to_owned)
            .collect(),
        duplicate_ids: graph
            .duplicates
            .into_iter()
            .map(|(id, paths)| DuplicateId {
                id: id.to_owned(),
                paths: paths.into_iter().map(str::to_owned).collect(),
            })
            .collect(),
    })
}

impl GraphReport {
    pub(crate) fn render(&self, out: &mut impl Write) -> anyhow::Result<()> {
        writeln!(
            out,
            "{} nodes, {} directed edges",
            self.nodes.len(),
            self.edges.len()
        )?;
        for node in &self.nodes {
            writeln!(out, "{}  {}  {}", node.id, node.path, node.title)?;
        }
        for edge in &self.edges {
            let tag = if edge.dangling { " (dangling)" } else { "" };
            writeln!(
                out,
                "{}  {}  {}{tag}",
                edge.source,
                edge.kind.name(),
                edge.target
            )?;
        }
        for duplicate in &self.duplicate_ids {
            writeln!(
                out,
                "duplicate {}: {}",
                duplicate.id,
                duplicate.paths.join(", ")
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SearchNeighbor {
    pub(crate) id: String,
    path: String,
    title: String,
    superseded: bool,
    via: String,
    depth: u8,
    direction: Direction,
    #[serde(rename = "kind")]
    edge: Edge,
}

impl SearchNeighbor {
    fn new(link: Neighbor<'_>, node: &Node<'_>, via: &str, depth: u8) -> Self {
        Self {
            id: link.id.to_owned(),
            path: node.path.to_owned(),
            title: node.title.to_string(),
            superseded: node.superseded,
            via: via.to_owned(),
            depth,
            direction: link.direction,
            edge: link.edge,
        }
    }

    fn label(&self) -> &'static str {
        match (self.direction, self.edge) {
            (Direction::In, Edge::Supersedes) => "superseded-by",
            (Direction::In | Direction::Out, edge) => edge.name(),
        }
    }

    fn search_provenance(&self) -> String {
        let seed = &self.via;
        match (self.direction, self.edge) {
            (Direction::Out, edge) => format!("via {}: {seed}", edge.name()),
            (Direction::In, Edge::Supersedes) => format!("supersedes: {seed}"),
            (Direction::In, Edge::Related | Edge::Mention) => format!("← backlink: {seed}"),
        }
    }
    fn tag(&self) -> &'static str {
        if self.superseded { " (superseded)" } else { "" }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Expansion {
    seed: GraphNode,
    neighbors: Vec<SearchNeighbor>,
}

/// Resolve an author-written link neighborhood in CLI display order.
pub(crate) fn expand(
    root: &KbRoot,
    selector: &str,
    depth: Depth,
    policy: &mut Inventory<'_>,
) -> anyhow::Result<Expansion> {
    let path = notes::find_with(root, selector, policy)?;
    let id = path
        .as_deref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .and_then(notes::id_prefix);
    let Some(id) = id else {
        bail!("knowbox expand: no note for {}", quoted(selector))
    };
    let entries = notes::snapshot_with(root, policy).context("building link graph")?;
    let graph = build_graph(&entries);
    let Some(seed) = graph.nodes.get(id) else {
        bail!("knowbox expand: no note for {}", quoted(selector))
    };
    let mut neighbors = Vec::new();
    let mut frontier = Vec::new();
    for direction in [Direction::Out, Direction::In] {
        for link in seed.neighbors(direction).take(LEVEL_CAP) {
            neighbors.push(SearchNeighbor::new(link, &graph.nodes[link.id], id, 1));
            frontier.push(link.id);
        }
    }
    if depth == Depth::Two {
        // A direct neighbor hidden by the display cap still belongs to depth one.
        let mut seen: HashSet<_> = seed.all_neighbors().map(|link| link.id).collect();
        seen.insert(id);
        let second_hop = frontier
            .into_iter()
            .flat_map(|via| {
                graph.nodes[via]
                    .all_neighbors()
                    .map(move |link| (via, link))
            })
            .filter(|(_, link)| seen.insert(link.id))
            .take(LEVEL_CAP);
        for (via, link) in second_hop {
            neighbors.push(SearchNeighbor::new(link, &graph.nodes[link.id], via, 2));
        }
    }
    Ok(Expansion {
        seed: GraphNode::new(id, seed),
        neighbors,
    })
}

impl Expansion {
    pub(crate) fn displayed_ids(&self) -> Vec<String> {
        std::iter::once(&self.seed.id)
            .chain(self.neighbors.iter().map(|row| &row.id))
            .cloned()
            .collect()
    }

    pub(crate) fn render(&self, out: &mut impl Write) -> anyhow::Result<()> {
        let seed = &self.seed;
        if seed.superseded {
            writeln!(
                out,
                "(note: {} is superseded — showing its links; prefer the successor)",
                seed.id
            )?;
        }
        writeln!(out, "{} — {}\n  {}", seed.id, seed.title, seed.path)?;
        if self.neighbors.is_empty() {
            writeln!(out, "\n(no author-written links)")?;
        }
        let mut previous = None;
        for row in &self.neighbors {
            let header = match (row.depth, row.direction) {
                (1, Direction::Out) => "links to:",
                (1, Direction::In) => "linked from (backlinks):",
                _ => "depth 2 (new neighbours, excluding the above):",
            };
            if previous != Some(header) {
                writeln!(out, "\n  {header}")?;
                previous = Some(header);
            }
            write!(
                out,
                "    {:<13} {}  {}{}",
                row.label(),
                row.id,
                row.title,
                row.tag()
            )?;
            if row.depth == 2 {
                write!(out, " (via {})", row.via)?;
            }
            writeln!(out)?;
        }
        Ok(())
    }
}

pub(crate) fn search_expansion(
    root: &KbRoot,
    ids: &[&str],
    policy: &mut Inventory<'_>,
) -> anyhow::Result<Vec<SearchNeighbor>> {
    let seeds = ids
        .iter()
        .copied()
        .take(SEARCH_SEEDS)
        .filter(|id| !id.is_empty());
    if seeds.clone().next().is_none() {
        return Ok(Vec::new());
    }
    // Search has already returned valid hits; optional graph I/O cannot invalidate them.
    let entries = notes::snapshot_with(root, policy)?;
    let graph = build_graph(&entries);
    let mut seen: HashSet<_> = ids.iter().copied().collect();
    let mut displayed = Vec::new();
    for seed in seeds {
        let Some(node) = graph.nodes.get(seed) else {
            continue;
        };
        let capacity = SEARCH_PER_SEED.min(SEARCH_TOTAL - displayed.len());
        for neighbor in node
            .all_neighbors()
            .filter(|neighbor| seen.insert(neighbor.id))
            .take(capacity)
        {
            displayed.push(SearchNeighbor::new(
                neighbor,
                &graph.nodes[neighbor.id],
                seed,
                1,
            ));
        }
        if displayed.len() == SEARCH_TOTAL {
            break;
        }
    }
    Ok(displayed)
}

pub(crate) fn render_search_expansion(
    rows: &[SearchNeighbor],
    out: &mut impl Write,
) -> anyhow::Result<()> {
    if !rows.is_empty() {
        writeln!(
            out,
            "(graph neighbours of top hits — depth 1, opt-in --expand:)"
        )?;
    }
    for row in rows {
        writeln!(out, "    {}  {}{}", row.id, row.path, row.tag())?;
        writeln!(out, "        {} — {}", row.search_provenance(), row.title)?;
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
    fn graph_keeps_folded_dangling_edges_isolated_nodes_and_duplicate_paths() -> anyhow::Result<()>
    {
        let (_dir, root) = fixture()?;
        write_note(
            &root,
            "kb-0001",
            "related: [kb-0002, kb-9999]",
            "[[kb-0002]]",
        )?;
        fs::write(
            root.path().join("notes/duplicate.md"),
            "---\nid: kb-0001\nsupersedes: [kb-0002, kb-9999]\n---",
        )?;
        write_note(&root, "kb-0002", "", "")?;
        write_note(&root, "kb-0003", "", "")?;
        let report = serde_json::to_value(graph(&root, &mut Inventory::Strict)?)?;
        assert_eq!(report["isolates"], serde_json::json!(["kb-0003"]));
        assert_eq!(
            report["duplicate_ids"],
            serde_json::json!([{"id":"kb-0001","paths":["notes/duplicate.md", "notes/kb-0001.md"]}])
        );
        assert_eq!(
            report["edges"],
            serde_json::json!([
                {"source": "kb-0001", "target": "kb-0002", "kind": "supersedes", "dangling": false},
                {"source": "kb-0001", "target": "kb-9999", "kind": "supersedes", "dangling": true}
            ])
        );
        assert_eq!(report["nodes"][0]["path"], "notes/kb-0001.md");
        assert_eq!(report["nodes"][2]["id"], "kb-0003");
        Ok(())
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
        expand(&root, "kb-0001", Depth::One, &mut Inventory::Strict)?.render(&mut one)?;
        let common = concat!(
            "(note: kb-0001 is superseded — showing its links; prefer the successor)\n",
            "kb-0001 — kb-0001\n  notes/kb-0001.md\n",
            "\n  links to:\n    supersedes    kb-0002  kb-0002\n",
            "\n  linked from (backlinks):\n    mention       kb-0003  kb-0003\n",
            "    superseded-by kb-0004  kb-0004\n",
        );
        assert_eq!(String::from_utf8(one)?, common);
        let mut two = Vec::new();
        let expanded = expand(&root, "kb-0001.md", Depth::Two, &mut Inventory::Strict)?;
        let provenance: Vec<_> = expanded
            .neighbors
            .iter()
            .map(|row| {
                (
                    row.id.as_str(),
                    row.via.as_str(),
                    row.depth,
                    row.direction,
                    row.edge,
                )
            })
            .collect();
        assert_eq!(
            provenance,
            [
                ("kb-0002", "kb-0001", 1, Direction::Out, Edge::Supersedes),
                ("kb-0003", "kb-0001", 1, Direction::In, Edge::Mention),
                ("kb-0004", "kb-0001", 1, Direction::In, Edge::Supersedes),
                ("kb-0005", "kb-0002", 2, Direction::Out, Edge::Related),
            ]
        );
        assert_eq!(
            expanded.displayed_ids(),
            ["kb-0001", "kb-0002", "kb-0003", "kb-0004", "kb-0005"]
        );
        expanded.render(&mut two)?;
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
        render_search_expansion(
            &search_expansion(
                &root,
                &["kb-0001", "kb-0002", "", "kb-0004"],
                &mut Inventory::Strict,
            )?,
            &mut out,
        )?;
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
        expand(&root, "kb-0001", Depth::One, &mut Inventory::Strict)?.render(&mut out)?;
        let text = String::from_utf8(out)?;
        let ids: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("    related"))
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert_eq!(ids, all[..12]);

        let mut out = Vec::new();
        render_search_expansion(
            &search_expansion(
                &root,
                &["kb-0001", "kb-0002", "kb-0003"],
                &mut Inventory::Strict,
            )?,
            &mut out,
        )?;
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
