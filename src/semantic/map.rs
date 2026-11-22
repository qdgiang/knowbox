use std::collections::{BTreeMap, VecDeque};
use std::io::Write;

use anyhow::Result;
use serde::Serialize;

use crate::config::KbRoot;
use crate::notes;
use crate::search;
use crate::store::Inventory;

use super::cache::{Cache, required_cache};
use super::rank::normalize;
use super::{
    BATCH_SIZE, INTERACTIVE_TIMEOUT, Live, Provider, embedding_text, live_notes, provider,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissProjection {
    Offline,
    Provider,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Embedding,
    Graph,
}

#[derive(Debug, Serialize)]
struct NotePoint {
    id: String,
    title: String,
    domain: Option<String>,
    path: String,
    x: Option<f64>,
    y: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Link {
    source: String,
    target: String,
}

#[derive(Debug, Serialize)]
struct MissPoint {
    timestamp: String,
    query: String,
    x: f64,
    y: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum MissState {
    Offline,
    Empty,
    Unavailable,
    Projected,
}

#[derive(Debug, Serialize)]
struct MissOverlay {
    state: MissState,
    message: String,
    points: Vec<MissPoint>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MapReport {
    mode: Mode,
    message: String,
    nodes: Vec<NotePoint>,
    edges: Vec<Link>,
    misses: MissOverlay,
}

impl MapReport {
    pub(crate) fn render(&self, out: &mut impl Write) -> Result<()> {
        writeln!(out, "{}", self.message)?;
        for note in &self.nodes {
            match (note.x, note.y) {
                (Some(x), Some(y)) => writeln!(out, "{}  {x:.3}  {y:.3}  {}", note.id, note.title)?,
                _ => writeln!(out, "{}  unprojected  {}", note.id, note.title)?,
            }
        }
        writeln!(out, "{}", self.misses.message)?;
        for miss in &self.misses.points {
            writeln!(
                out,
                "{}  {:.3}  {:.3}  {}",
                miss.timestamp, miss.x, miss.y, miss.query
            )?;
        }
        Ok(())
    }
}

fn current_vectors<'a>(live: &Live<'a>, mut cache: Cache) -> BTreeMap<&'a str, Vec<f64>> {
    live.iter()
        .filter_map(|(&id, &(_, note))| {
            let stored = cache.notes.remove(id)?;
            (stored.h == notes::sha1_hex(embedding_text(note).as_bytes())
                && stored.v.iter().any(|&v| v != 0.0))
            .then(|| (id, normalize(stored.v)))
        })
        .collect()
}

struct Projection {
    center: Vec<f64>,
    horizontal: Vec<f64>,
    vertical: Vec<f64>,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

impl Projection {
    // Two orthogonal landmark directions, selected in ID order on ties. Fitting uses
    // linear time in the total number of vector components and O(dimension) scratch.
    fn fit(vectors: &BTreeMap<&str, Vec<f64>>) -> Option<Self> {
        if vectors.len() < 2 {
            return None;
        }
        let origin = vectors.values().next()?;
        let dimension = origin.len();
        let mut center = vec![0.; dimension];
        for point in vectors.values() {
            for ((sum, value), base) in center.iter_mut().zip(point).zip(origin) {
                *sum += value - base;
            }
        }
        for (sum, base) in center.iter_mut().zip(origin) {
            *sum = base + *sum / vectors.len() as f64;
        }
        let axis = |previous: Option<&[f64]>| {
            let mut best = vec![0.; dimension];
            let mut residual = vec![0.; dimension];
            let mut largest = 1e-24;
            for point in vectors.values() {
                for ((part, value), mean) in residual.iter_mut().zip(point).zip(&center) {
                    *part = value - mean;
                }
                if let Some(axis) = previous {
                    let along = dot(&residual, axis);
                    for (part, direction) in residual.iter_mut().zip(axis) {
                        *part -= along * direction;
                    }
                }
                let squared = dot(&residual, &residual);
                if squared > largest {
                    best.copy_from_slice(&residual);
                    largest = squared;
                }
            }
            normalize(best)
        };
        let horizontal = axis(None);
        if horizontal.iter().all(|&v| v == 0.) {
            return None;
        }
        let vertical = axis(Some(&horizontal));
        Some(Self {
            center,
            horizontal,
            vertical,
        })
    }

    fn coordinates(&self, vector: &[f64]) -> [f64; 2] {
        let mut x = 0.;
        let mut y = 0.;
        for (((&v, &center), &h), &w) in vector
            .iter()
            .zip(&self.center)
            .zip(&self.horizontal)
            .zip(&self.vertical)
        {
            let offset = v - center;
            x += offset * h;
            y += offset * w;
        }
        [x, y]
    }
}

fn graph_positions(ids: &[&str], edges: &[Link]) -> Vec<[f64; 2]> {
    let indices: BTreeMap<_, _> = ids.iter().copied().zip(0..).collect();
    let mut neighbors = vec![Vec::new(); ids.len()];
    for link in edges {
        let (a, b) = (indices[link.source.as_str()], indices[link.target.as_str()]);
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    // Lay out each component by hop distance from its lowest ID; tile components,
    // including isolates, so a KB without links still produces distinct points.
    let mut visited = vec![false; ids.len()];
    let mut positions = vec![[0., 0.]; ids.len()];
    let mut components = Vec::new();
    for root in 0..ids.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut queue = VecDeque::from([(root, 0)]);
        let mut members = Vec::new();
        let mut widths = Vec::new();
        while let Some((node, depth)) = queue.pop_front() {
            if depth == widths.len() {
                widths.push(0);
            }
            positions[node] = [depth as f64, widths[depth] as f64];
            widths[depth] += 1;
            members.push(node);
            for &adjacent in &neighbors[node] {
                if !visited[adjacent] {
                    visited[adjacent] = true;
                    queue.push_back((adjacent, depth + 1));
                }
            }
        }
        components.push((members, widths.len(), widths.into_iter().max().unwrap_or(1)));
    }
    let area: f64 = components
        .iter()
        .map(|(_, w, h)| (*w as f64 + 1.) * (*h as f64 + 1.))
        .sum();
    let row_limit = area.sqrt();
    let (mut left, mut top, mut tallest) = (0., 0., 0usize);
    for (members, width, height) in components {
        if left > 0. && left + width as f64 > row_limit {
            left = 0.;
            top += tallest as f64 + 1.;
            tallest = 0;
        }
        for node in members {
            positions[node][0] += left;
            positions[node][1] += top;
        }
        left += width as f64 + 1.;
        tallest = tallest.max(height);
    }
    positions
}

fn miss_overlay(
    root: &KbRoot,
    access: MissProjection,
    projection: Option<&Projection>,
    embed: impl FnOnce(&[&str]) -> Result<Vec<Vec<f64>>, provider::ProviderError>,
) -> MissOverlay {
    let without_points = |state, message: &str| MissOverlay {
        state,
        message: message.to_owned(),
        points: Vec::new(),
    };
    if access == MissProjection::Offline {
        return without_points(
            MissState::Offline,
            "Miss queries are local. Enable provider projection to place recent misses.",
        );
    }
    let Some(projection) = projection else {
        return without_points(
            MissState::Unavailable,
            "Misses need a usable note embedding projection.",
        );
    };
    let Ok(log) = search::misses(root) else {
        return without_points(
            MissState::Unavailable,
            "The local miss log could not be read.",
        );
    };
    let entries: Vec<_> = log
        .entries
        .into_iter()
        .filter(|row| !row.query.trim().is_empty())
        .take(BATCH_SIZE)
        .collect();
    if entries.is_empty() {
        return without_points(MissState::Empty, "No recent miss queries to project.");
    }
    let texts: Vec<_> = entries.iter().map(|row| row.query.as_str()).collect();
    let vectors = match embed(&texts) {
        Ok(vectors)
            if vectors.len() == entries.len()
                && vectors
                    .iter()
                    .all(|v| v.len() == projection.center.len() && v.iter().any(|&x| x != 0.)) =>
        {
            vectors
        }
        Ok(_) => {
            return without_points(
                MissState::Unavailable,
                "Miss embeddings are incompatible with the local note projection.",
            );
        }
        Err(error) => {
            return without_points(
                MissState::Unavailable,
                &format!("Miss projection unavailable: {error}."),
            );
        }
    };
    let points: Vec<_> = entries
        .into_iter()
        .zip(vectors)
        .map(|(row, vector)| {
            let [x, y] = projection.coordinates(&normalize(vector));
            MissPoint {
                timestamp: row.timestamp,
                query: row.query,
                x,
                y,
            }
        })
        .collect();
    MissOverlay {
        state: MissState::Projected,
        message: format!(
            "Projected {} recent miss queries (limit {BATCH_SIZE}).",
            points.len()
        ),
        points,
    }
}

/// Build a deterministic note map from the current snapshot and compatible local vectors.
pub(crate) fn map_report(
    root: &KbRoot,
    access: MissProjection,
    policy: &mut Inventory<'_>,
) -> Result<MapReport> {
    let entries = notes::snapshot_with(root, policy)?;
    let live = live_notes(&entries);
    let edges: Vec<_> = crate::browse::authored_edges(&entries)
        .into_iter()
        .filter(|(a, b)| a != b && live.contains_key(a) && live.contains_key(b))
        .map(|(a, b)| Link {
            source: a.to_owned(),
            target: b.to_owned(),
        })
        .collect();
    let model = provider::model();
    let (vectors, cache_gap) = match required_cache(root, &model) {
        Ok(cache) => (current_vectors(&live, cache), None),
        Err(error) => (BTreeMap::new(), Some(error.to_string())),
    };
    let projection = Projection::fit(&vectors);
    let ids: Vec<_> = live.keys().copied().collect();
    let graph_points = projection.is_none().then(|| graph_positions(&ids, &edges));
    let nodes = live
        .iter()
        .enumerate()
        .map(|(index, (&id, &(path, note)))| {
            let position = match &projection {
                Some(axes) => vectors.get(id).map(|v| axes.coordinates(v)),
                None => graph_points.as_ref().map(|points| points[index]),
            };
            NotePoint {
                id: id.to_owned(),
                title: note.text("title", id).into_owned(),
                path: path.to_owned(),
                domain: note.scalar("domain").map(str::to_owned),
                x: position.map(|[x, _]| x),
                y: position.map(|[_, y]| y),
            }
        })
        .collect();
    let (mode, message) = if projection.is_some() {
        (
            Mode::Embedding,
            format!(
                "Embedding projection of {} of {} live notes; {} unprojected.",
                vectors.len(),
                live.len(),
                live.len() - vectors.len()
            ),
        )
    } else {
        let reason = cache_gap
            .as_deref()
            .unwrap_or("current embeddings do not contain distinct directions");
        (
            Mode::Graph,
            format!("Link-graph layout of {} live notes: {reason}.", live.len()),
        )
    };
    let misses = miss_overlay(root, access, projection.as_ref(), |texts| {
        Provider::from_env(&model, INTERACTIVE_TIMEOUT)?.vectors(texts)
    });
    Ok(MapReport {
        mode,
        message,
        nodes,
        edges,
        misses,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use anyhow::Context;
    use serde_json::json;

    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, KbRoot)> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("notes"))?;
        let root = KbRoot::open(directory.path())?;
        Ok((directory, root))
    }

    fn note(root: &KbRoot, id: &str, fields: &str) -> Result<()> {
        fs::write(
            root.path().join("notes").join(format!("{id}.md")),
            format!("---\nid: {id}\ntitle: {id}\n{fields}\n---"),
        )?;
        Ok(())
    }

    #[test]
    fn only_current_nonzero_live_vectors_place_notes_in_id_order() -> Result<()> {
        let (_directory, root) = fixture()?;
        for id in [
            "kb-0008", "kb-0006", "kb-0005", "kb-0004", "kb-0003", "kb-0002", "kb-0001",
        ] {
            note(&root, id, "")?;
        }
        note(&root, "kb-0007", "status: superseded")?;
        let mut cache = Cache::empty(&provider::model());
        for (id, vector) in [
            ("kb-0001", [8., 0.]),
            ("kb-0002", [-8., 0.]),
            ("kb-0003", [0., 8.]),
            ("kb-0004", [0., -8.]),
            ("kb-0005", [1., 1.]),
            ("kb-0006", [0., 0.]),
            ("kb-0007", [2., 2.]),
        ] {
            let source = if id == "kb-0005" { "old title" } else { id };
            cache.notes.insert(
                id.into(),
                super::super::StoredVector {
                    h: notes::sha1_hex(source.as_bytes()),
                    v: vector.to_vec(),
                },
            );
        }
        fs::write(
            root.path().join(".kb-embed.json"),
            serde_json::to_vec(&cache)?,
        )?;
        let report = map_report(&root, MissProjection::Offline, &mut Inventory::Strict)?;
        assert_eq!(serde_json::to_value(&report.mode)?, "embedding");
        let points: Vec<_> = report
            .nodes
            .iter()
            .map(|note| (&*note.id, note.x, note.y))
            .collect();
        assert_eq!(
            points,
            [
                ("kb-0001", Some(1.), Some(0.)),
                ("kb-0002", Some(-1.), Some(0.)),
                ("kb-0003", Some(0.), Some(1.)),
                ("kb-0004", Some(0.), Some(-1.)),
                ("kb-0005", None, None),
                ("kb-0006", None, None),
                ("kb-0008", None, None),
            ]
        );
        assert_eq!(serde_json::to_value(&report.misses.state)?, "offline");
        assert_eq!(
            serde_json::to_vec(&report)?,
            serde_json::to_vec(&map_report(
                &root,
                MissProjection::Offline,
                &mut Inventory::Strict
            )?)?
        );
        Ok(())
    }

    #[test]
    fn missing_and_unusable_caches_follow_authored_links_and_preserve_isolates() -> Result<()> {
        let (_directory, root) = fixture()?;
        note(
            &root,
            "kb-0001",
            "related: [kb-0003, kb-0001, kb-0004, kb-9999]",
        )?;
        note(&root, "kb-0002", "")?;
        note(&root, "kb-0003", "")?;
        note(&root, "kb-0004", "status: superseded")?;
        let cache_path = root.path().join(".kb-embed.json");
        let mut constant = Cache::empty(&provider::model());
        for id in ["kb-0001", "kb-0003"] {
            constant.notes.insert(
                id.into(),
                super::super::StoredVector {
                    h: notes::sha1_hex(id.as_bytes()),
                    v: vec![1., 1.],
                },
            );
        }
        for cache in [
            None,
            Some(b"invalid json".to_vec()),
            Some(serde_json::to_vec(
                &json!({"model": format!("{}-other", provider::model()), "notes":{"kb-0001":{"h":"old","v":[1]}}}),
            )?),
            Some(serde_json::to_vec(&constant)?),
        ] {
            if let Some(cache) = cache {
                fs::write(&cache_path, cache)?;
            }
            let report = map_report(&root, MissProjection::Provider, &mut Inventory::Strict)?;
            assert_eq!(serde_json::to_value(&report.mode)?, "graph");
            assert_eq!(
                serde_json::to_value(&report.edges)?,
                json!([{"source":"kb-0001","target":"kb-0003"}])
            );
            assert_eq!(serde_json::to_value(&report.misses.state)?, "unavailable");
            assert_eq!(
                report
                    .nodes
                    .iter()
                    .map(|note| (&*note.id, note.x, note.y))
                    .collect::<Vec<_>>(),
                [
                    ("kb-0001", Some(0.), Some(0.)),
                    ("kb-0002", Some(0.), Some(2.)),
                    ("kb-0003", Some(1.), Some(0.)),
                ]
            );
        }
        Ok(())
    }

    #[test]
    fn miss_batch_uses_the_note_center_and_axes_and_caps_disclosure() -> Result<()> {
        let (_directory, root) = fixture()?;
        let mut log = fs::File::create(root.path().join(".search-misses.log"))?;
        for n in 1..=17 {
            writeln!(log, "2026-11-10T00:00:{n:02}\tquery {n}")?;
        }
        let axes = Projection::fit(&BTreeMap::from([
            ("a", vec![1., 0., 0.]),
            ("b", vec![0., 1., 0.]),
        ]))
        .context("fixture needs a nonzero horizontal axis")?;
        let expected_queries: Vec<_> = (2..=17).rev().map(|n| format!("query {n}")).collect();
        let overlay = miss_overlay(&root, MissProjection::Provider, Some(&axes), |texts| {
            assert_eq!(texts, expected_queries);
            Ok(vec![vec![40., 0., 0.]; texts.len()])
        });
        assert_eq!(serde_json::to_value(&overlay.state)?, "projected");
        let timestamps: Vec<_> = overlay
            .points
            .iter()
            .map(|p| p.timestamp.as_str())
            .collect();
        assert_eq!(
            timestamps,
            (2..=17)
                .rev()
                .map(|n| format!("2026-11-10T00:00:{n:02}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            overlay
                .points
                .iter()
                .map(|p| p.query.as_str())
                .collect::<Vec<_>>(),
            expected_queries
        );
        for point in overlay.points {
            assert!((point.x - 0.5_f64.sqrt()).abs() < 1e-12);
            assert_eq!(point.y, 0.);
        }
        let failure = miss_overlay(&root, MissProjection::Provider, Some(&axes), |_| {
            Err(provider::ProviderError::Transport)
        });
        assert_eq!(
            serde_json::to_value(failure)?,
            json!({
                "state": "unavailable", "message": "Miss projection unavailable: embedding request failed.", "points": []
            })
        );
        let wrong_dim = miss_overlay(&root, MissProjection::Provider, Some(&axes), |texts| {
            Ok(vec![vec![1.]; texts.len()])
        });
        assert_eq!(
            serde_json::to_value(wrong_dim)?,
            json!({
                "state": "unavailable", "message": "Miss embeddings are incompatible with the local note projection.", "points": []
            })
        );
        Ok(())
    }

    #[test]
    fn offline_and_empty_log_skip_the_provider() -> Result<()> {
        let (_directory, root) = fixture()?;
        let axes = Projection::fit(&BTreeMap::from([("a", vec![1.]), ("b", vec![-1.])]))
            .context("fixture axis")?;
        for (access, state) in [
            (MissProjection::Offline, "offline"),
            (MissProjection::Provider, "empty"),
        ] {
            let report = miss_overlay(&root, access, Some(&axes), |_| {
                unreachable!("no provider request")
            });
            assert_eq!(serde_json::to_value(report.state)?, state);
            assert_eq!(report.points.len(), 0);
        }
        Ok(())
    }
}
