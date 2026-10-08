---
id: kb-0086
title: "KB hardening plan (composed-leaping-origami): tooling+protocol+content, phased over ~17 sessions"
type: project
domain: kb
tags: [kb, hardening, plan, testing, cron, lint, conflicts]
aliases: [composed-leaping-origami,kb hardening plan,kb hardening phases]
triggers:
  - "resuming the KB hardening plan"
  - "checking which hardening phase is next"
  - "updating hardening plan state"
  - "a session asks what is pending on the KB audit fixes"
status: active
confidence: high
created: 2026-09-05
updated: 2026-09-05
supersedes: []
related: [kb-0085, kb-0038, kb-0014]
source: "~/.claude/plans/composed-leaping-origami.md"
summary: "Pointer note for the multi-session KB hardening plan (audit+research 2026-09-05). State line tracks the current phase; full plan at ~/.claude/plans/composed-leaping-origami.md."
---
**State: approved 2026-09-05. Next: P1 (KB_ROOT env fix + pytest harness + goldens).**

Full plan (phases, anchors, tests, risks): `~/.claude/plans/composed-leaping-origami.md`.
Ground rules for every executing session are in plan §0 — read them first. Serialize:
never run two plan sessions concurrently (git_commit stages `bin` wholesale).

## Phase checklist

- [ ] P1 KB_ROOT + pytest harness + characterization goldens [M] — prereq for all tooling phases
- [ ] P2 janitor revival + heartbeat + Telegram alerts [S/M] — target before Mon Sep 8; updates kb-0014
- [ ] P3 content quick wins: zellij alignment kb-0010/0077, kb-0065 summary, domain tools→tooling [S] — URGENT; asks D4
- [ ] P4 write-path safety: frontmatter guards, parse robustness [M] — GATES P5
- [ ] P5 verified-dates sweep, 24 notes [M] — **DEADLINE ≤ Sep 16**
- [ ] P6 branch rename master→main [S]
- [ ] P7 concurrency: lock coverage + unique tmp paths [M]
- [ ] P8 staleness detection: sha1 kb_meta, drift/orphan lint [M/L] — new decision note
- [ ] P9 bookkeeping: id high-water, churn-by-id, reindex no-op [S/M] — new gotcha note
- [ ] P10 search quality: own-id, dual tokenizer, typo, refs-grep [L, split 10a/10b] — new gotcha note
- [ ] P11 protocol+docs: precedence rule, edit-vs-supersede, SKILL.md supersede, ⚠→[[NEW]] flag, kb-0055 [M]
- [ ] P12 kb conflicts (similarity report) + judge-pass skill [M]
- [ ] P13 distill triage, ~33 proposals → 0 [M]
- [ ] P14 oversized splits kb-0018/0076/0077 → lint 0 errors [M]
- [ ] P15 kb-0048 + kb-0023 reconciliation [S]
- [ ] P16 zellij decision execution [S, D4-gated]
- [ ] P17 final pass: suite green, lint 0E, this note → done [S]

## Decisions

D1 pyproject+uv.lock pytest (adopted) · D2 git_commit stages tests/pyproject/uv.lock (adopted) ·
D3 triggers optional→WARN, final call P11 · **D4 zellij exception vs COPR migration — ASK USER at P3** ·
D5 conflicts reviews → raw/conflicts-DATE.md · D6 search auto-rebuilds stale index + stderr note.

## Baselines

- Lint at approval: **3 errors / 24 warnings / 85 notes** (errors: kb-0018, kb-0076, kb-0077 oversized — cleared at P14).
- Last recorded commit: 2727a61 (P0 create; the P0 touch commit follows it — session-start gate: this sha must appear in `git log --oneline -5`).

## Deferred leftovers (fill at P17)

(none yet)

## Related
- [[kb-0085]] — the research whose mechanisms this plan implements (heartbeat, sha1 freshness, conflicts report).
- [[kb-0038]] — prior KB review; its deferred "contradiction lint" is P12.
- [[kb-0014]] — janitor cron note; P2 rewrites it after the revival.
