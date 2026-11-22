mod cache;
mod conflicts;
mod map;
mod provider;
mod rank;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use jiff::civil::Date;

use crate::config::KbRoot;
use crate::notes::{self, Note, NoteEntry};
use crate::search::Hit;
use crate::store::{Inventory, WriteLock};

use cache::{Cache, CacheError, StoredVector, read_cache, required_cache};
pub(crate) use conflicts::{conflict_report, conflicts};
pub(crate) use map::{MissProjection, map_report};
use provider::Provider;

const INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(10);
const EXPLICIT_TIMEOUT: Duration = Duration::from_secs(90);
const BATCH_SIZE: usize = 16;

#[derive(Debug)]
pub(crate) struct SemanticHit {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) score: f64,
}

#[derive(Debug)]
pub(crate) struct Fusion {
    pub(crate) rows: Vec<Hit>,
    pub(crate) changed: bool,
    pub(crate) recency_changed: bool,
}

impl Fusion {
    fn unchanged(lexical: &[Hit]) -> Self {
        Self {
            rows: lexical.to_vec(),
            changed: false,
            recency_changed: false,
        }
    }
}

type Live<'a> = BTreeMap<&'a str, (&'a str, &'a Note)>;

fn live_notes(entries: &[NoteEntry]) -> Live<'_> {
    entries
        .iter()
        .filter_map(|entry| {
            let Ok(note) = &entry.parsed else { return None };
            if note.scalar("status") == Some("superseded") {
                return None;
            }
            let id = note.scalar("id").filter(|id| notes::is_id(id))?;
            Some((id, (entry.relative.as_str(), note)))
        })
        .collect()
}

fn excerpt(body: &str, limit: usize) -> String {
    body.split_whitespace()
        .enumerate()
        .flat_map(|(i, word)| (i != 0).then_some(' ').into_iter().chain(word.chars()))
        .take(limit)
        .collect()
}

fn embedding_text(note: &Note) -> String {
    note.search_text_parts()
        .into_iter()
        .chain([Cow::Owned(note.body.chars().take(2_000).collect())])
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn setting_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => !value.trim().is_empty(),
        Err(_) => false,
    }
}

struct Pending {
    id: String,
    text: String,
    hash: String,
}

fn pending(live: &Live<'_>, existing: Option<&Cache>) -> Vec<Pending> {
    live.iter()
        .filter_map(|(&id, &(_, note))| {
            let text = embedding_text(note);
            let hash = notes::sha1_hex(text.as_bytes());
            let current = existing
                .and_then(|cache| cache.notes.get(id))
                .is_some_and(|entry| entry.h == hash);
            (!current).then(|| Pending {
                id: id.into(),
                text,
                hash,
            })
        })
        .collect()
}

pub(crate) fn embed(root: &KbRoot, force: bool) -> Result<String> {
    let model = provider::model();
    let provider = Provider::from_env(&model, EXPLICIT_TIMEOUT)?;
    let existing = cache_for_refresh(root, force)?;
    let warmup = provider
        .vectors(&["warmup"])
        .context("checking embedding endpoint")?;
    let dim = warmup
        .first()
        .context("embedding warmup returned no vector")?
        .len();
    let snapshot = notes::snapshot(root)?;
    let live = live_notes(&snapshot);
    let matching = existing
        .as_ref()
        .filter(|cache| cache.model == model && !force);
    if let Some(old_dim) = matching.and_then(Cache::dimension) {
        ensure!(
            dim == old_dim,
            "provider vector dimension changed; run `knowbox embed --force`"
        );
    }
    let todo = pending(&live, matching);
    if todo.is_empty()
        && existing.as_ref().is_none_or(|cache| {
            cache.model == model && cache.notes.keys().all(|id| live.contains_key(id.as_str()))
        })
    {
        return Ok(format!(
            "embeddings up to date ({} notes, model={model})",
            live.len()
        ));
    }
    drop(snapshot);

    cache::check_component_budget(todo.len(), dim)?;
    let mut updates = BTreeMap::new();
    for batch in todo.chunks(BATCH_SIZE) {
        let texts: Vec<_> = batch.iter().map(|item| item.text.as_str()).collect();
        let vectors = provider.vectors(&texts)?;
        ensure!(
            vectors.len() == batch.len() && vectors.iter().all(|v| v.len() == dim),
            "provider returned inconsistent vector dimensions or count"
        );
        for (entry, vector) in batch.iter().zip(vectors) {
            updates.insert(
                entry.id.clone(),
                StoredVector {
                    h: entry.hash.clone(),
                    v: vector,
                },
            );
        }
    }

    let lock = WriteLock::acquire(root)?;
    let latest = cache_for_refresh(lock.root(), force)?;
    let previous_model = existing.as_ref().map(|cache| cache.model.as_str());
    if let Some(cache) = &latest {
        ensure!(
            cache.model == model || Some(cache.model.as_str()) == previous_model,
            "embedding cache model changed during refresh; rerun `knowbox embed`"
        );
    }
    let mut cache = latest
        .filter(|cache| cache.model == model)
        .unwrap_or_else(|| Cache::empty(&model));
    if cache
        .dimension()
        .is_some_and(|cached_dim| cached_dim != dim)
    {
        ensure!(
            force,
            "embedding cache dimension changed; run `knowbox embed --force`"
        );
        cache.notes.clear();
    }
    let snapshot = notes::snapshot(lock.root())?;
    let latest_live = live_notes(&snapshot);
    let attempted = updates.len();
    // An edit or supersession during HTTP makes that update unusable. Check the
    // embedding projection, not the file hash: a date-only touch does not matter.
    updates.retain(|id, entry| {
        latest_live
            .get(id.as_str())
            .is_some_and(|(_, note)| notes::sha1_hex(embedding_text(note).as_bytes()) == entry.h)
    });
    let refreshed = updates.len();
    cache
        .notes
        .retain(|id, _| latest_live.contains_key(id.as_str()));
    cache.notes.extend(updates);
    cache.save(&lock)?;
    let skipped = attempted - refreshed;
    let mut message = format!(
        "embedded {refreshed} note(s); {} total (model={model})",
        cache.notes.len()
    );
    if skipped > 0 {
        message.push_str(&format!(
            "; {skipped} changed during refresh — rerun `knowbox embed`"
        ));
    }
    Ok(message)
}

fn cache_for_refresh(root: &KbRoot, force: bool) -> Result<Option<Cache>, CacheError> {
    match read_cache(root) {
        Err(CacheError::Invalid | CacheError::Budget) if force => Ok(None),
        result => result,
    }
}

struct Query {
    entries: Vec<NoteEntry>,
    hits: Vec<SemanticHit>,
}

fn query_cache(
    root: &KbRoot,
    query: &str,
    cache: Cache,
    timeout: Duration,
    limit: usize,
    policy: &mut Inventory<'_>,
) -> Result<Query> {
    let provider = Provider::from_env(&cache.model, timeout)?;
    let vector = provider
        .vectors(&[query])?
        .into_iter()
        .next()
        .context("embedding provider returned no query vector")?;
    ensure!(
        cache.dimension() == Some(vector.len()),
        "query and cached vectors have different dimensions; run `knowbox embed --force`"
    );
    let vector = rank::normalize(vector);
    let entries = notes::snapshot_with(root, policy)?;
    let live = live_notes(&entries);
    let mut hits: Vec<_> = cache
        .notes
        .into_iter()
        .filter_map(|(id, entry)| {
            let &(path, note) = live.get(id.as_str())?;
            Some(SemanticHit {
                id,
                path: path.into(),
                title: note.text("title", "").into_owned(),
                score: rank::cosine(&vector, &rank::normalize(entry.v)),
            })
        })
        .collect();
    hits.sort_unstable_by(|a, b| b.score.total_cmp(&a.score).then_with(|| b.id.cmp(&a.id)));
    hits.truncate(limit);
    Ok(Query { entries, hits })
}

pub(crate) fn vsearch(
    root: &KbRoot,
    query: &str,
    policy: &mut Inventory<'_>,
) -> Result<Vec<SemanticHit>> {
    let cache = required_cache(root, &provider::model())?;
    Ok(query_cache(root, query, cache, EXPLICIT_TIMEOUT, rank::DISPLAY, policy)?.hits)
}

fn interactive_query(
    root: &KbRoot,
    query: &str,
    limit: usize,
    policy: &mut Inventory<'_>,
) -> Result<Query> {
    let cache = required_cache(root, &provider::model())?;
    query_cache(root, query, cache, INTERACTIVE_TIMEOUT, limit, policy)
}

pub(crate) fn rescue(root: &KbRoot, query: &str, policy: &mut Inventory<'_>) -> Vec<SemanticHit> {
    match interactive_query(root, query, 3, policy) {
        Ok(result) => result
            .hits
            .into_iter()
            .filter(|hit| hit.score >= rank::COS_FLOOR)
            .collect(),
        // Implicit search is fail-soft. Explicit vsearch reports provider errors.
        Err(_) => Vec::new(),
    }
}

pub(crate) fn fuse(
    query: &str,
    lexical: &[Hit],
    root: &KbRoot,
    now: Date,
    policy: &mut Inventory<'_>,
) -> Fusion {
    if !setting_enabled("KNOWBOX_NO_FUSION")
        && let Ok(result) = interactive_query(root, query, rank::DISPLAY, policy)
    {
        return rank::fuse_rows(
            lexical,
            &result.hits,
            &live_notes(&result.entries),
            now,
            !setting_enabled("KNOWBOX_NO_RECENCY"),
        );
    }
    Fusion::unchanged(lexical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_hashes_only_the_first_two_thousand_body_characters() -> Result<()> {
        let document = format!(
            "---\nid: kb-0001\ntitle: Café\nsummary: Route\ntags: [one, two]\naliases: [alt]\ntriggers:\n  - when testing\n---\n{}discard",
            "🦀".repeat(2_000)
        );
        let note = notes::parse(document.as_bytes())?;
        assert_eq!(
            embedding_text(&note),
            format!(
                "Café\nRoute\none two\nalt\nwhen testing\n{}",
                "🦀".repeat(2_000)
            )
        );
        Ok(())
    }
}
