use jiff::civil::Date;

use super::{Fusion, Live, SemanticHit};
use crate::search::{Hit, MatchKind, Ranks};

pub(super) const COS_FLOOR: f64 = 0.40;
pub(super) const DISPLAY: usize = 6;

// Scaling by the largest component first keeps norms finite even for very
// large or subnormal, but valid, JSON numbers. A zero vector stays zero.
pub(super) fn normalize(mut v: Vec<f64>) -> Vec<f64> {
    let scale = v.iter().fold(0.0_f64, |s, x| s.max(x.abs()));
    if scale != 0.0 {
        for x in &mut v {
            *x /= scale;
        }
        let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// Inputs are normalized at the store/query boundary, including a common dimension check.
pub(super) fn cosine(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len(), "validated embedding dimensions");
    a.iter()
        .zip(b)
        .map(|(x, y)| x * y)
        .sum::<f64>()
        .clamp(-1.0, 1.0)
}

fn recency(age_days: Option<i64>) -> f64 {
    match age_days {
        Some(age) if age > 30 => 2.0_f64.powf(-((age - 30) as f64) / 90.0).max(0.5),
        _ => 1.0,
    }
}

fn key<'a>(path: &'a str, id: &'a str) -> &'a str {
    if path.is_empty() { id } else { path }
}

fn contribution(index: usize, weight: f64) -> f64 {
    weight / (61.0 + index as f64)
}

struct Scored {
    row: Hit,
    score: f64,
    lex_rank: usize,
}

pub(super) fn fuse_rows(
    lexical: &[Hit],
    semantic: &[SemanticHit],
    live: &Live,
    now: Date,
    recency_on: bool,
) -> Fusion {
    let mut rows: Vec<_> = lexical
        .iter()
        .enumerate()
        .map(|(i, row)| Scored {
            row: row.clone(),
            score: contribution(i, 1.0),
            lex_rank: i,
        })
        .collect();
    let mut extras = Vec::new();
    let mut participating = 0;
    for (i, sem) in semantic.iter().enumerate() {
        let k = key(&sem.path, &sem.id);
        let lexical = rows.iter_mut().find(|r| key(&r.row.path, &r.row.id) == k);
        if let Some(lex) = lexical {
            lex.row.ranks.cosine = Some(sem.score);
            lex.row.ranks.cosine_rank = Some(i + 1);
            if sem.score >= COS_FLOOR {
                lex.score += contribution(participating, 0.5);
            }
        } else if sem.score >= 0.45 {
            extras.push((sem, contribution(participating, 0.5), k, i + 1));
        }
        if sem.score >= COS_FLOOR {
            participating += 1;
        }
    }
    if participating == 0 {
        return Fusion {
            rows: rows.into_iter().map(|r| r.row).collect(),
            changed: false,
            recency_changed: false,
        };
    }
    extras.sort_by(|(a, _, ka, _), (b, _, kb, _)| b.score.total_cmp(&a.score).then(ka.cmp(kb)));
    rows.extend(
        extras
            .into_iter()
            .take(2)
            .map(|(sem, score, _, cosine_rank)| {
                let snippet = live
                    .get(sem.id.as_str())
                    .map(|(_, note)| {
                        let summary = note.text("summary", "");
                        if summary.trim().is_empty() || summary == "[]" {
                            super::excerpt(&note.body, 160)
                        } else {
                            summary.trim().to_owned()
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| sem.title.clone());
                Scored {
                    row: Hit {
                        id: sem.id.clone(),
                        path: sem.path.clone(),
                        title: sem.title.clone(),
                        snippet,
                        match_kind: MatchKind::SemanticOnly,
                        ranks: Ranks {
                            cosine: Some(sem.score),
                            cosine_rank: Some(cosine_rank),
                            ..Ranks::default()
                        },
                        direct: None,
                    },
                    score,
                    lex_rank: usize::MAX,
                }
            }),
    );
    rows.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.lex_rank.cmp(&b.lex_rank))
            .then_with(|| key(&a.row.path, &a.row.id).cmp(key(&b.row.path, &b.row.id)))
    });
    rows.truncate(DISPLAY);
    let mut recency_changed = false;
    if recency_on {
        let before: Vec<_> = rows.iter().map(|r| r.row.id.clone()).collect();
        for row in &mut rows {
            let age = live.get(row.row.id.as_str()).and_then(|(_, n)| {
                let date = n
                    .scalar("updated")
                    .filter(|s| !s.is_empty())
                    .or_else(|| n.scalar("created"))?;
                crate::browse::parse_date(date.trim())
                    .map(|d| now.duration_since(d).as_secs() / 86_400)
            });
            row.score *= recency(age);
        }
        // Stable sort preserves the prior RRF tie-break. No dated tail row can
        // cross a lexical row, even if decaying the latter inverted the scores.
        rows.sort_by(|a, b| {
            (a.row.match_kind == MatchKind::SemanticOnly)
                .cmp(&(b.row.match_kind == MatchKind::SemanticOnly))
                .then_with(|| b.score.total_cmp(&a.score))
        });
        recency_changed = before.iter().ne(rows.iter().map(|r| &r.row.id));
    }
    let changed = lexical
        .iter()
        .map(|r| &r.id)
        .ne(rows.iter().map(|r| &r.row.id));
    Fusion {
        rows: rows
            .into_iter()
            .enumerate()
            .map(|(i, mut r)| {
                r.row.ranks.fused_rank = Some(i + 1);
                r.row
            })
            .collect(),
        changed,
        recency_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn lex(id: &str) -> Hit {
        Hit {
            id: id.into(),
            path: format!("notes/{id}.md"),
            title: id.into(),
            snippet: format!("lex {id}"),
            match_kind: MatchKind::Partial,
            ranks: Ranks::default(),
            direct: None,
        }
    }
    fn sem(id: &str, score: f64) -> SemanticHit {
        SemanticHit {
            id: id.into(),
            path: format!("notes/{id}.md"),
            title: id.into(),
            score,
        }
    }

    #[test]
    fn rrf_gates_agreement_and_caps_tail_by_cosine_not_input_order() -> Result<()> {
        let lexical = [lex("A"), lex("B")];
        let semantic = [
            sem("B", 0.40),
            sem("C", 0.5),
            sem("D", 0.44),
            sem("E", 0.90),
            sem("F", 0.80),
        ];
        let now = Date::new(2026, 1, 1)?;
        let fused = fuse_rows(&lexical, &semantic, &Live::new(), now, false);
        assert_eq!(
            fused
                .rows
                .iter()
                .map(|r| (
                    r.id.as_str(),
                    r.match_kind == MatchKind::SemanticOnly,
                    r.snippet.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                ("B", false, "lex B"),
                ("A", false, "lex A"),
                ("E", true, "E"),
                ("F", true, "F")
            ]
        );
        assert!(fused.changed && !fused.recency_changed);
        let below_floor = fuse_rows(
            &lexical,
            &[sem("B", 0.399), sem("C", 0.449)],
            &Live::new(),
            now,
            false,
        );
        let mut expected = lexical;
        expected[0].ranks.fused_rank = Some(1);
        expected[1].ranks = Ranks {
            cosine: Some(0.399),
            cosine_rank: Some(1),
            fused_rank: Some(2),
            ..Ranks::default()
        };
        assert_eq!(below_floor.rows, expected);
        assert!(!below_floor.changed);
        Ok(())
    }

    #[test]
    fn half_life_has_grace_floor_and_neutral_invalid_dates() {
        for age in [None, Some(-90), Some(0), Some(30)] {
            assert_eq!(recency(age), 1.0);
        }
        assert!((recency(Some(75)) - 0.5_f64.sqrt()).abs() < 1e-12);
        assert_eq!(recency(Some(120)), 0.5);
        assert_eq!(recency(Some(900)), 0.5);
    }

    #[test]
    fn cosine_handles_zero_and_extreme_finite_vectors() {
        assert_eq!(
            cosine(&normalize(vec![0., 0.]), &normalize(vec![1., 0.])),
            0.0
        );
        assert_eq!(
            cosine(
                &normalize(vec![f64::MAX, 0.]),
                &normalize(vec![-f64::MIN_POSITIVE, 0.])
            ),
            -1.0
        );
    }
}
