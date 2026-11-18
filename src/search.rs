mod rescue;

use std::collections::HashSet;
use std::fs;
use std::io::Write;

use anyhow::Context;
use jiff::civil::DateTime;
use rusqlite::{Connection, Row, params};

use crate::config::KbRoot;
use crate::index;
use crate::notes::{self, ParseError};
use crate::semantic::{self, RankedHit};
use crate::store;

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

fn read_hit(row: &Row<'_>) -> rusqlite::Result<RankedHit> {
    Ok(RankedHit {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        snippet: row.get(3)?,
        semantic_only: false,
    })
}

fn lexical(db: &Connection, query: &str) -> rusqlite::Result<Vec<RankedHit>> {
    let mut ranked = Vec::new();
    // Scores from distinct tokenizers are not comparable: merge by rank, primary table first on ties.
    for (order, table) in ["kb", "kb_d"].into_iter().enumerate() {
        let mut stmt = db.prepare(&format!("SELECT id, path, title, snippet({table}, 9, '>>>', '<<<', '…', 12), bm25({table}, 0.0, 0.0, 10.0, 1.0, 1.0, 5.0, 8.0, 6.0, 4.0, 1.0) AS score FROM {table} WHERE {table} MATCH ? ORDER BY score LIMIT ?"))?;
        for (rank, row) in stmt.query_map(params![query, 6], read_hit)?.enumerate() {
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
    All(Vec<RankedHit>),
    Partial(Vec<RankedHit>),
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
    let all = lexical(db, &quoted.join(" AND "))?;
    if !all.is_empty() {
        return Ok(Matches::All(all));
    }
    let any = lexical(db, &quoted.join(" OR "))?;
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
        .query_map(params![tq, 6], read_hit)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(if tri.is_empty() {
        Matches::None
    } else {
        Matches::Partial(tri)
    })
}

enum DirectHit {
    Missing,
    Malformed,
    Note(String),
}

fn print_own_id(root: &KbRoot, query: &str, out: &mut impl Write) -> anyhow::Result<DirectHit> {
    let id = query.to_ascii_lowercase();
    if !id
        .strip_prefix("kb-")
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    {
        return Ok(DirectHit::Missing);
    }
    let Some(path) = notes::find(root, &id)? else {
        return Ok(DirectHit::Missing);
    };
    let rel = path
        .strip_prefix(root.path())?
        .to_str()
        .context("note path is not UTF-8")?
        .replace(std::path::MAIN_SEPARATOR, "/");
    store::regular_file(&path)?;
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let note = match notes::parse(&bytes) {
        Err(ParseError::Malformed) => {
            writeln!(
                out,
                "{id}  {rel} ⚠MALFORMED\n    unterminated frontmatter — `knowbox show` prints it raw\n"
            )?;
            return Ok(DirectHit::Malformed);
        }
        result => result.with_context(|| format!("parsing {rel}"))?,
    };
    let label = if note.scalar("status") == Some("superseded") {
        " ⚠SUPERSEDED"
    } else {
        ""
    };
    let snippet: String = note
        .body
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect();
    writeln!(
        out,
        "{}  {rel}{label}\n    {}\n    {snippet}\n",
        note.text("id", &id),
        note.text("title", "?")
    )?;
    let graph_id = note.scalar("id").filter(|id| !id.is_empty()).unwrap_or(&id);
    Ok(DirectHit::Note(graph_id.to_owned()))
}

/// Search by ID or lexical rank; the semantic provider is never called on an exact or typo hit.
pub(crate) fn run(
    root: &KbRoot,
    query: &str,
    expand: bool,
    now: DateTime,
    out: &mut impl Write,
    err: &mut impl Write,
) -> anyhow::Result<()> {
    match print_own_id(root, query, out)? {
        DirectHit::Missing => {}
        DirectHit::Malformed => return Ok(()),
        DirectHit::Note(id) => {
            if expand {
                expand_hits(root, &[&id], out, err)?;
            }
            return Ok(());
        }
    }
    let terms = query_tokens(query);
    let ((matches, fixes), rebuilt) = index::with_current(root, |db, notes| {
        let matches = search(db, &terms)?;
        if matches!(matches, Matches::None) && !terms.is_empty() {
            let (fixed, fixes) = rescue::typo(&terms, notes);
            if !fixes.is_empty() {
                return Ok((search(db, &query_tokens(&fixed.join(" ")))?, fixes));
            }
        }
        Ok((matches, Vec::new()))
    })
    .context("searching the SQLite index")?;
    if rebuilt {
        writeln!(err, "(index was stale — rebuilt from current notes)")?;
    }
    if !matches!(matches, Matches::None) && !fixes.is_empty() {
        writeln!(out, "(typo-corrected: {})", fixes.join(", "))?;
    }
    let rows = match matches {
        Matches::All(rows) => rows,
        Matches::Partial(rows) => {
            writeln!(out, "(partial match — no note contains all query terms)")?;
            if fixes.is_empty() {
                let fused = semantic::fuse(query, &rows, root, now.date());
                if fused.changed {
                    let label = if fused.recency_changed {
                        "(hybrid re-rank: lexical + semantic + recency)"
                    } else {
                        "(hybrid re-rank: lexical + semantic agreement)"
                    };
                    writeln!(out, "{label}")?;
                }
                fused.rows
            } else {
                rows
            }
        }
        Matches::None => {
            let hits = semantic::rescue(root, query);
            if !hits.is_empty() {
                writeln!(
                    out,
                    "(BM25 missed — semantic matches; cos <0.45 is likely-absent territory, verify before relying on these:)"
                )?;
                for hit in hits {
                    writeln!(
                        out,
                        "{}  {}  [cos {:.3}]\n    {}\n",
                        hit.id, hit.path, hit.score, hit.title
                    )?;
                }
                return Ok(());
            }
            let refs = rescue::refs(root, &terms)?;
            if refs.is_empty() {
                rescue::log_miss(root, query, now)?;
                writeln!(
                    out,
                    "no matches. try: a synonym | broader term | `knowbox list`"
                )?;
            } else {
                writeln!(
                    out,
                    "refs/ mentions (not FTS-indexed — find the section-map note via `knowbox search` on the doc title):"
                )?;
                for hit in refs {
                    writeln!(out, "    {hit}")?;
                }
            }
            return Ok(());
        }
    };
    for hit in &rows {
        let snippet = hit.snippet.split_whitespace().collect::<Vec<_>>().join(" ");
        let label = if hit.semantic_only {
            " (semantic-only)"
        } else {
            ""
        };
        writeln!(
            out,
            "{}  {}{label}\n    {}\n    {}\n",
            hit.id, hit.path, hit.title, snippet
        )?;
    }
    if expand && fixes.is_empty() {
        let ids: Vec<_> = rows.iter().map(|hit| hit.id.as_str()).collect();
        expand_hits(root, &ids, out, err)?;
    }
    Ok(())
}

fn expand_hits(
    root: &KbRoot,
    ids: &[&str],
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<()> {
    if let Err(problem) = crate::browse::search_expansion(root, ids, out) {
        writeln!(err, "knowbox: omitting graph expansion: {problem:#}")?;
    }
    Ok(())
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
