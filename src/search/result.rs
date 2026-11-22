use std::io::{self, Write};

use anyhow::Context;
use serde::Serialize;

use super::rescue::{Correction, RefHit};
use crate::browse::{SearchNeighbor, render_search_expansion};
use crate::notes;
use crate::usage::{Layer, ReadEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MatchKind {
    Id,
    Lexical,
    Partial,
    Trigram,
    Typo,
    SemanticOnly,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub(crate) struct Ranks {
    pub(crate) bm25_rank: Option<usize>,
    pub(crate) cosine: Option<f64>,
    pub(crate) cosine_rank: Option<usize>,
    pub(crate) fused_rank: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Hit {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) snippet: String,
    pub(crate) match_kind: MatchKind,
    #[serde(flatten)]
    pub(crate) ranks: Ranks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) direct: Option<DirectMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum DirectMetadata {
    Malformed,
    // Preserve the CLI's rendering for notes with an empty or list-valued id.
    Parsed {
        superseded: bool,
        display_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IndexStatus {
    Unchecked,
    Fresh,
    TemporarySnapshot,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Rerank {
    Agreement,
    Recency,
}

#[derive(Debug, Serialize)]
pub(crate) struct Answer {
    pub(super) layer: Layer,
    pub(super) index_status: IndexStatus,
    pub(super) hits: Vec<Hit>,
    pub(super) partial: bool,
    pub(super) corrections: Vec<Correction>,
    pub(super) rerank: Option<Rerank>,
    pub(super) refs: Vec<RefHit>,
    pub(super) expanded: Vec<SearchNeighbor>,
    pub(super) expansion_error: Option<String>,
}

impl Answer {
    pub(super) fn new(index_status: IndexStatus, layer: Layer) -> Self {
        Self {
            layer,
            index_status,
            hits: Vec::new(),
            partial: false,
            corrections: Vec::new(),
            rerank: None,
            refs: Vec::new(),
            expanded: Vec::new(),
            expansion_error: None,
        }
    }

    fn render(&self, out: &mut impl Write, err: &mut impl Write) -> anyhow::Result<()> {
        if self.index_status == IndexStatus::TemporarySnapshot {
            writeln!(
                err,
                "(index was stale — searched a temporary snapshot. Run 'knowbox reindex' to refresh.)"
            )?;
        }
        if !self.corrections.is_empty() {
            let fixes: Vec<_> = self
                .corrections
                .iter()
                .map(|fix| format!("'{}'→'{}'", fix.from, fix.to))
                .collect();
            writeln!(out, "(typo-corrected: {})", fixes.join(", "))?;
        }
        if self.partial {
            writeln!(out, "(partial match — no note contains all query terms)")?;
        }
        if let Some(rerank) = self.rerank {
            let label = match rerank {
                Rerank::Agreement => "(hybrid re-rank: lexical + semantic agreement)",
                Rerank::Recency => "(hybrid re-rank: lexical + semantic + recency)",
            };
            writeln!(out, "{label}")?;
        }
        match self.layer {
            Layer::Miss => writeln!(
                out,
                "no matches. try: a synonym | broader term | `knowbox list`"
            )?,
            Layer::Refs => {
                writeln!(
                    out,
                    "refs/ mentions (not FTS-indexed — find the section-map note via `knowbox search` on the doc title):"
                )?;
                for hit in &self.refs {
                    let description = if hit.headings.is_empty() {
                        "(filename or lead section)".to_owned()
                    } else {
                        hit.headings.join(" / ")
                    };
                    writeln!(out, "    {}: {description}", hit.path)?;
                }
            }
            Layer::Semantic => {
                writeln!(
                    out,
                    "(BM25 missed — semantic matches; cos <0.45 is likely-absent territory, verify before relying on these:)"
                )?;
                for hit in &self.hits {
                    let cosine = hit
                        .ranks
                        .cosine
                        .context("missing cosine for a semantic search hit")?;
                    writeln!(
                        out,
                        "{}  {}  [cos {cosine:.3}]\n    {}\n",
                        hit.id, hit.path, hit.title
                    )?;
                }
            }
            Layer::Id | Layer::Lexical | Layer::Partial | Layer::Typo => {
                for hit in &self.hits {
                    hit.render(out)?;
                }
            }
        }
        render_search_expansion(&self.expanded, out)?;
        if let Some(error) = &self.expansion_error {
            writeln!(err, "knowbox: omitting graph expansion: {error}")?;
        }
        Ok(())
    }
}

impl Hit {
    fn render(&self, out: &mut impl Write) -> io::Result<()> {
        let (id, label) = match &self.direct {
            Some(DirectMetadata::Malformed) => {
                return writeln!(
                    out,
                    "{}  {} ⚠MALFORMED\n    unterminated frontmatter — `knowbox show` prints it raw\n",
                    self.id, self.path
                );
            }
            Some(DirectMetadata::Parsed {
                superseded,
                display_id,
            }) => (
                display_id.as_str(),
                if *superseded { " ⚠SUPERSEDED" } else { "" },
            ),
            None if self.match_kind == MatchKind::SemanticOnly => {
                (self.id.as_str(), " (semantic-only)")
            }
            None => (self.id.as_str(), ""),
        };
        writeln!(
            out,
            "{id}  {}{label}\n    {}\n    {}\n",
            self.path, self.title, self.snippet
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum SearchResult {
    Ok(Answer),
    StaleIndex { guidance: &'static str },
}

impl SearchResult {
    pub(crate) fn render(&self, out: &mut impl Write, err: &mut impl Write) -> anyhow::Result<()> {
        match self {
            Self::Ok(answer) => answer.render(out, err),
            Self::StaleIndex { guidance } => Ok(writeln!(err, "{guidance}")?),
        }
    }

    pub(crate) fn read_event(&self) -> Option<ReadEvent> {
        let Self::Ok(answer) = self else { return None };
        Some(ReadEvent::Search {
            layer: answer.layer,
            notes: answer
                .hits
                .iter()
                .filter(|hit| notes::is_id(&hit.id))
                .map(|hit| hit.id.clone())
                .collect(),
            expanded: answer.expanded.iter().map(|row| row.id.clone()).collect(),
        })
    }
}
