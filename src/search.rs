mod misses;
mod rescue;
mod result;

use std::collections::HashSet;

use anyhow::Context;
use jiff::civil::Date;
use rusqlite::{Connection, Row, params};

use crate::config::KbRoot;
use crate::index;
use crate::notes::{self, ParseError};
use crate::semantic;
use crate::store;
use crate::usage::Layer;
use result::{Answer, DirectMetadata, IndexStatus, Rerank};
pub(crate) use result::{Hit, MatchKind, Ranks, SearchResult};

pub(crate) use misses::{log_miss, misses};

const STOP: &str = "a an and are as at be but by can did do does for from has have how i if in is it its just me my no not of on or so than that the then this to too very was we were when will with won t you your something anything thing stuff way kind sort quite really actually maybe please need needs needed want wants wanted";
const LIMIT: usize = 6;

fn tokens(lower: &str) -> impl Iterator<Item = &str> {
    lower
        .split(|c: char| !c.is_alphanumeric() && !matches!(c, '_' | '\'' | '.' | '/' | '#' | '-'))
        .filter(|s| !s.is_empty())
}

fn query_tokens(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let kept: Vec<_> = tokens(&lower)
        .filter(|t| !STOP.split_ascii_whitespace().any(|s| s == *t))
        .map(str::to_owned)
        .collect();
    if kept.is_empty() {
        tokens(&lower).map(str::to_owned).collect()
    } else {
        kept
    }
}

fn read_hit(row: &Row<'_>, match_kind: MatchKind) -> rusqlite::Result<Hit> {
    Ok(Hit {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        snippet: row.get(3)?,
        match_kind,
        ranks: Ranks::default(),
        direct: None,
    })
}

fn lexical(db: &Connection, query: &str, kind: MatchKind) -> rusqlite::Result<Vec<Hit>> {
    let mut ranked = Vec::new();
    // Scores from distinct tokenizers are not comparable: merge by rank, primary table first on ties.
    for (order, table) in ["kb", "kb_d"].into_iter().enumerate() {
        let mut stmt = db.prepare(&format!("SELECT id, path, title, snippet({table}, 9, '>>>', '<<<', '…', 12), bm25({table}, 0.0, 0.0, 10.0, 1.0, 1.0, 5.0, 8.0, 6.0, 4.0, 1.0) AS score FROM {table} WHERE {table} MATCH ? ORDER BY score LIMIT ?"))?;
        for (rank, row) in stmt
            .query_map(params![query, 6], |r| read_hit(r, kind))?
            .enumerate()
        {
            ranked.push((rank, order, row?));
        }
    }
    ranked.sort_unstable_by_key(|(rank, order, _)| (*rank, *order));
    let mut seen = HashSet::new();
    Ok(ranked
        .into_iter()
        .map(|(_, _, hit)| hit)
        .filter(|hit| {
            seen.insert(
                if hit.path.is_empty() {
                    &hit.id
                } else {
                    &hit.path
                }
                .clone(),
            )
        })
        .take(LIMIT)
        .collect())
}

enum Matches {
    All(Vec<Hit>),
    Partial(Vec<Hit>),
    None,
}

fn search(db: &Connection, terms: &[String]) -> rusqlite::Result<Matches> {
    if terms.is_empty() {
        return Ok(Matches::None);
    }
    // Terms only contain characters accepted by tokens(); no FTS5 quote/operator can escape them.
    let quoted: Vec<_> = terms
        .iter()
        .map(|t| {
            let star = if t.chars().count() >= 3 { "*" } else { "" };
            format!("\"{t}\"{star}")
        })
        .collect();
    let all = lexical(db, &quoted.join(" AND "), MatchKind::Lexical)?;
    if !all.is_empty() {
        return Ok(Matches::All(all));
    }
    let any = lexical(db, &quoted.join(" OR "), MatchKind::Partial)?;
    if !any.is_empty() {
        return Ok(Matches::Partial(any));
    }
    let tq = terms
        .iter()
        .filter(|t| t.chars().count() >= 3)
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    if tq.is_empty() {
        return Ok(Matches::None);
    }
    let mut stmt = db.prepare("SELECT id, path, title, snippet(kb_tri, 3, '>>>', '<<<', '…', 12), bm25(kb_tri) AS score FROM kb_tri WHERE kb_tri MATCH ? ORDER BY score LIMIT ?")?;
    let tri = stmt
        .query_map(params![tq, 6], |r| read_hit(r, MatchKind::Trigram))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(if tri.is_empty() {
        Matches::None
    } else {
        Matches::Partial(tri)
    })
}

fn direct_hit(
    root: &KbRoot,
    query: &str,
    policy: &mut store::Inventory<'_>,
) -> anyhow::Result<Option<Hit>> {
    let id = query.to_ascii_lowercase();
    if !id
        .strip_prefix("kb-")
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    {
        return Ok(None);
    }
    let Some(path) = notes::find_with(root, &id, policy)? else {
        return Ok(None);
    };
    let rel = path
        .strip_prefix(root.path())?
        .to_str()
        .context("note path is not UTF-8")?
        .replace(std::path::MAIN_SEPARATOR, "/");
    let Some(bytes) = notes::read_with(&path, policy)? else {
        return Ok(None);
    };
    let mut hit = Hit {
        id: id.clone(),
        path: rel,
        title: String::new(),
        snippet: String::new(),
        match_kind: MatchKind::Id,
        ranks: Ranks::default(),
        direct: None,
    };
    let meta = match notes::parse(&bytes) {
        Err(ParseError::Malformed) => DirectMetadata::Malformed,
        parsed => {
            let note = parsed.with_context(|| format!("parsing {}", hit.path))?;
            hit.id = note
                .scalar("id")
                .filter(|s| !s.is_empty())
                .unwrap_or(&id)
                .to_owned();
            hit.title = note.text("title", "?").into_owned();
            hit.snippet = note
                .body
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(160)
                .collect();
            DirectMetadata::Parsed {
                superseded: note.scalar("status") == Some("superseded"),
                display_id: note.text("id", &id).into_owned(),
            }
        }
    };
    hit.direct = Some(meta);
    Ok(Some(hit))
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Options {
    pub(crate) expand: bool,
    pub(crate) local: bool,
    pub(crate) no_fix: bool,
    pub(crate) no_write: bool,
}

pub(crate) fn run(
    root: &KbRoot,
    query: &str,
    options: Options,
    today: Date,
    policy: &mut store::Inventory<'_>,
) -> anyhow::Result<SearchResult> {
    if let Some(hit) = direct_hit(root, query, policy)? {
        let expand = options.expand && matches!(hit.direct, Some(DirectMetadata::Parsed { .. }));
        let mut answer = Answer::new(IndexStatus::Unchecked, Layer::Id);
        answer.hits.push(hit);
        if expand {
            expand_hits(root, &mut answer, policy);
        }
        return Ok(SearchResult::Ok(answer));
    }
    let terms = query_tokens(query);
    let stale = if options.no_write {
        index::StalePolicy::Report
    } else {
        index::StalePolicy::TemporarySnapshot
    };
    let indexed = index::with_current(root, policy, stale, |db, notes| {
        let matches = search(db, &terms)?;
        if !options.no_fix && matches!(matches, Matches::None) && !terms.is_empty() {
            let (fixed, fixes) = rescue::typo(&terms, notes);
            if !fixes.is_empty() {
                return Ok((search(db, &query_tokens(&fixed.join(" ")))?, fixes));
            }
        }
        Ok((matches, Vec::new()))
    })
    .context("searching the SQLite index")?;
    let (index_status, (matches, fixes)) = match indexed {
        index::IndexRead::Stale => {
            return Ok(SearchResult::StaleIndex {
                guidance: "search index is stale. Run 'knowbox reindex' to refresh.",
            });
        }
        index::IndexRead::Current(answer) => (IndexStatus::Fresh, answer),
        index::IndexRead::Temporary(answer) => (IndexStatus::TemporarySnapshot, answer),
    };

    let (mut rows, partial) = match matches {
        Matches::All(rows) => (rows, false),
        Matches::Partial(rows) => (rows, true),
        Matches::None => {
            return Ok(SearchResult::Ok(fallback(
                root,
                query,
                &terms,
                index_status,
                options,
                policy,
            )?));
        }
    };
    let layer = if !fixes.is_empty() {
        Layer::Typo
    } else if partial {
        Layer::Partial
    } else {
        Layer::Lexical
    };
    let mut answer = Answer::new(index_status, layer);
    answer.partial = partial;
    for (i, hit) in rows.iter_mut().enumerate() {
        hit.ranks.bm25_rank = Some(i + 1);
        if layer == Layer::Typo {
            hit.match_kind = MatchKind::Typo;
        }
    }
    answer.corrections = fixes;
    if partial && layer != Layer::Typo && !options.local {
        let fused = semantic::fuse(query, &rows, root, today, policy);
        if fused.changed {
            answer.rerank = Some(if fused.recency_changed {
                Rerank::Recency
            } else {
                Rerank::Agreement
            });
        }
        rows = fused.rows;
    }
    for hit in &mut rows {
        hit.snippet = hit.snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    answer.hits = rows;
    if options.expand && layer != Layer::Typo {
        expand_hits(root, &mut answer, policy);
    }
    Ok(SearchResult::Ok(answer))
}

fn fallback(
    root: &KbRoot,
    query: &str,
    terms: &[String],
    index_status: IndexStatus,
    options: Options,
    policy: &mut store::Inventory<'_>,
) -> anyhow::Result<Answer> {
    if !options.local {
        let hits = semantic::rescue(root, query, policy);
        if !hits.is_empty() {
            let mut answer = Answer::new(index_status, Layer::Semantic);
            answer.hits = hits
                .into_iter()
                .enumerate()
                .map(|(i, hit)| Hit {
                    id: hit.id,
                    path: hit.path,
                    title: hit.title,
                    snippet: String::new(),
                    match_kind: MatchKind::SemanticOnly,
                    ranks: Ranks {
                        cosine: Some(hit.score),
                        cosine_rank: Some(i + 1),
                        ..Ranks::default()
                    },
                    direct: None,
                })
                .collect();
            return Ok(answer);
        }
    }
    let refs = rescue::refs(root, terms, policy)?;
    let layer = if refs.is_empty() {
        Layer::Miss
    } else {
        Layer::Refs
    };
    let mut answer = Answer::new(index_status, layer);
    answer.refs = refs;
    Ok(answer)
}

fn expand_hits(root: &KbRoot, answer: &mut Answer, policy: &mut store::Inventory<'_>) {
    let ids: Vec<_> = answer.hits.iter().map(|hit| hit.id.as_str()).collect();
    match crate::browse::search_expansion(root, &ids, policy) {
        Ok(rows) => answer.expanded = rows,
        Err(problem) => answer.expansion_error = Some(format!("{problem:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwords_are_filtered_unless_the_entire_query_is_stopwords() {
        assert_eq!(
            query_tokens("How is /FiLe.c #Δ's AND foo_bar-baz?"),
            ["/file.c", "#δ's", "foo_bar-baz"]
        );
        assert_eq!(query_tokens("How is this?"), ["how", "is", "this"]);
        assert_eq!(query_tokens("!?"), Vec::<String>::new());
    }
}
