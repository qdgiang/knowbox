use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::io::Write;

use anyhow::bail;
use serde::Serialize;

use crate::config::KbRoot;
use crate::notes::{self, Note};

use super::rank::{cosine, normalize};
use super::{live_notes, provider, required_cache};

struct Embedded<'a> {
    id: &'a str,
    path: &'a str,
    note: &'a Note,
    vector: Vec<f64>,
}

// Indices refer to notes in ID order. A max-heap then keeps the worst retained
// candidate at the root; sort order for rendering is best first.
#[derive(Debug, Clone, Copy)]
struct Pair {
    score: f64,
    a: usize,
    b: usize,
}

impl Ord for Pair {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.score + 0.0)
            .total_cmp(&(self.score + 0.0))
            .then_with(|| (self.a, self.b).cmp(&(other.a, other.b)))
    }
}

impl PartialOrd for Pair {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Pair {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Pair {}

fn best_pairs(embedded: &[Embedded<'_>], top: usize, min_cos: f64) -> Vec<Pair> {
    let mut pairs = BinaryHeap::new();
    for (a, first) in embedded.iter().enumerate() {
        for (b, second) in embedded.iter().enumerate().skip(a + 1) {
            let score = cosine(&first.vector, &second.vector);
            if score < min_cos {
                continue;
            }
            let pair = Pair { score, a, b };
            if pairs.len() < top {
                pairs.push(pair);
            } else if let Some(mut worst) = pairs.peek_mut()
                && pair < *worst
            {
                *worst = pair;
            }
        }
    }
    pairs.into_sorted_vec()
}

#[derive(Serialize)]
struct JsonPair<'a> {
    a: &'a str,
    b: &'a str,
    cos: f64,
    a_status: Cow<'a, str>,
    b_status: Cow<'a, str>,
    a_updated: Cow<'a, str>,
    b_updated: Cow<'a, str>,
    a_summary: Cow<'a, str>,
    b_summary: Cow<'a, str>,
}

/// Report cached similarity candidates for live notes, without contacting the provider.
pub(crate) fn conflicts(
    root: &KbRoot,
    top: usize,
    min_cos: f64,
    json: bool,
    out: &mut impl Write,
) -> anyhow::Result<()> {
    let mut cache = required_cache(root, &provider::model())?;
    let entries = notes::snapshot(root)?;
    let embedded: Vec<_> = live_notes(&entries)
        .into_iter()
        .filter_map(|(id, (path, note))| {
            let vector = normalize(cache.notes.remove(id)?.v);
            Some(Embedded {
                id,
                path,
                note,
                vector,
            })
        })
        .collect();
    if embedded.len() < 2 {
        bail!(
            "knowbox conflicts: only {} embedded live note(s) — run `knowbox embed` first",
            embedded.len()
        );
    }
    let pairs = best_pairs(&embedded, top, min_cos);
    if json {
        write_json(out, &embedded, &pairs)
    } else {
        write_text(out, &embedded, &pairs, min_cos)
    }
}

fn write_json(
    out: &mut impl Write,
    embedded: &[Embedded<'_>],
    pairs: &[Pair],
) -> anyhow::Result<()> {
    let rows: Vec<_> = pairs
        .iter()
        .map(|pair| {
            let (a, b) = (&embedded[pair.a], &embedded[pair.b]);
            Ok(JsonPair {
                a: a.id,
                b: b.id,
                // Round only for display, not ranking or filtering.
                cos: format!("{:.4}", pair.score).parse()?,
                a_status: a.note.text("status", ""),
                b_status: b.note.text("status", ""),
                a_updated: a.note.text("updated", ""),
                b_updated: b.note.text("updated", ""),
                a_summary: a.note.text("summary", ""),
                b_summary: b.note.text("summary", ""),
            })
        })
        .collect::<Result<_, std::num::ParseFloatError>>()?;
    serde_json::to_writer_pretty(&mut *out, &rows)?;
    writeln!(out)?;
    Ok(())
}

fn write_text(
    out: &mut impl Write,
    embedded: &[Embedded<'_>],
    pairs: &[Pair],
    min_cos: f64,
) -> anyhow::Result<()> {
    if pairs.is_empty() {
        writeln!(
            out,
            "no live-note pairs at cos >= {min_cos:.2} ({} embedded notes scanned)",
            embedded.len()
        )?;
        return Ok(());
    }
    writeln!(
        out,
        "knowbox conflicts — {} candidate pair(s) at cos >= {min_cos:.2} ({} embedded live notes).",
        pairs.len(),
        embedded.len()
    )?;
    writeln!(
        out,
        "Candidates, NOT verdicts — judge pass: SKILL.md 'Conflicts judge pass' → raw/conflicts-DATE.md\n"
    )?;
    for pair in pairs {
        writeln!(out, "  cos {:.4}", pair.score)?;
        for entry in [&embedded[pair.a], &embedded[pair.b]] {
            let summary = entry.note.items("summary").join(", ");
            let summary = summary
                .split(|ch: char| ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}'))
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let stop = summary
                .char_indices()
                .nth(110)
                .map_or(summary.len(), |(n, _)| n);
            writeln!(
                out,
                "    {}  [{} · upd {}]  {}\n        {}",
                entry.id,
                entry.note.text("status", "?"),
                entry.note.text("updated", "?"),
                entry.path,
                &summary[..stop]
            )?;
        }
        writeln!(out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_pairs_rank_unrounded_scores_then_break_ties_by_ids() -> anyhow::Result<()> {
        let note = notes::parse(b"")?;
        let embedded: Vec<_> = [
            ("kb-0001", vec![1.0, 0.0]),
            ("kb-0002", vec![1.0, 0.001]),
            ("kb-0003", vec![1.0, 0.0]),
            ("kb-0004", vec![0.0, 1.0]),
        ]
        .into_iter()
        .map(|(id, vector)| Embedded {
            id,
            path: "",
            note: &note,
            vector: normalize(vector),
        })
        .collect();
        let ids = |pairs: Vec<Pair>| {
            pairs
                .iter()
                .map(|p| (embedded[p.a].id, embedded[p.b].id))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(best_pairs(&embedded, 2, 0.7)),
            [("kb-0001", "kb-0003"), ("kb-0001", "kb-0002")]
        );
        assert_eq!(ids(best_pairs(&embedded, 8, 1.0)), [("kb-0001", "kb-0003")]);
        Ok(())
    }

    #[test]
    fn json_preserves_order_and_metadata_whitespace() -> anyhow::Result<()> {
        let a = notes::parse(
            "---\nstatus: active\nupdated: 2026-11-18\nsummary: 'résumé  détaillé'\n---".as_bytes(),
        )?;
        let b = notes::parse(b"---\nstatus: draft\nsummary: [one, two]\n---")?;
        let embedded = [
            Embedded {
                id: "kb-0001",
                path: "notes/a.md",
                note: &a,
                vector: vec![1.0, 0.0],
            },
            Embedded {
                id: "kb-0002",
                path: "notes/b.md",
                note: &b,
                vector: normalize(vec![4.0, 3.0]),
            },
        ];
        let mut out = Vec::new();
        write_json(&mut out, &embedded, &best_pairs(&embedded, 1, 0.7))?;
        assert_eq!(
            String::from_utf8(out)?,
            concat!(
                "[\n  {\n",
                "    \"a\": \"kb-0001\",\n    \"b\": \"kb-0002\",\n    \"cos\": 0.8,\n",
                "    \"a_status\": \"active\",\n    \"b_status\": \"draft\",\n",
                "    \"a_updated\": \"2026-11-18\",\n    \"b_updated\": \"\",\n",
                "    \"a_summary\": \"résumé  détaillé\",\n    \"b_summary\": \"['one', 'two']\"\n",
                "  }\n]\n",
            )
        );
        Ok(())
    }

    #[test]
    fn text_snippet_collapses_whitespace_and_caps_unicode_characters() -> anyhow::Result<()> {
        let summary = format!("---\nsummary: 'a  {}'\n---", "見".repeat(111));
        let a = notes::parse(summary.as_bytes())?;
        let b = notes::parse(b"")?;
        let embedded = [
            Embedded {
                id: "kb-0001",
                path: "notes/a.md",
                note: &a,
                vector: vec![1.0],
            },
            Embedded {
                id: "kb-0002",
                path: "notes/b.md",
                note: &b,
                vector: vec![1.0],
            },
        ];
        let mut out = Vec::new();
        write_text(&mut out, &embedded, &best_pairs(&embedded, 1, 0.7), 0.7)?;
        assert_eq!(
            String::from_utf8(out)?,
            format!(
                concat!(
                    "knowbox conflicts — 1 candidate pair(s) at cos >= 0.70 (2 embedded live notes).\n",
                    "Candidates, NOT verdicts — judge pass: SKILL.md 'Conflicts judge pass' → raw/conflicts-DATE.md\n\n",
                    "  cos 1.0000\n    kb-0001  [? · upd ?]  notes/a.md\n        a {}\n",
                    "    kb-0002  [? · upd ?]  notes/b.md\n        \n\n",
                ),
                "見".repeat(108)
            )
        );
        Ok(())
    }
}
