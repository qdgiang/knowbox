---
id: kb-0026
title: "Harness roadmap done through Phase 2 + 4 Tier-3 items; rest deferred with triggers to ~/reports/claude-harness-future-work.md"
type: decision
domain: tooling
tags: [claude-code, harness, roadmap]
aliases: [future work, backlog, tier 3 remaining, harness plan status]
triggers:
  - "continuing or extending the Claude harness setup"
  - "asked about graphify, MCP server, Superpowers, Gas Town, or skill evals"
status: active
confidence: high
created: 2026-08-06
updated: 2026-08-19
supersedes: []
related: [kb-0023, kb-0024, kb-0025, kb-0045, kb-0046]
source: "session"
summary: "Claude harness plan complete (Phases 0-2 + 4 Tier-3 items); Gas Town rejected 2026-08-09 after research; remaining items deferred with explicit triggers in ~/reports/claude-harness-future-work.md; exclusions listed."
---
The Claude harness hardening plan (2026-08-05 audit; section map [[kb-0045]],
full report in `~/kb/refs/claude-harness-audit-2026-08-05.md`) is complete
through
Phase 2 plus four Tier-3 items; everything else is deliberately deferred
with explicit triggers, tracked in `~/reports/claude-harness-future-work.md`.

## Done (2026-08-05 → 2026-08-06)

- **Phase 0** — profiles re-baselined, merge-based `use-profile.sh`
  (settings.base.json), official-profile fallback tested, KB hygiene.
- **Phase 1** — 5-subagent fleet, permission allowlists, plugins
  (security-guidance, hookify), statusline, 7 kb CLI fixes.
- **Phase 2** — SessionEnd distiller hook (proposals → `~/kb/raw/`),
  cost report + miss counter, 3 project CLAUDE.md files, `/loop` routine.
- **Tier 3 done:** embeddings net (kb-0024), agent teams trial — negative,
  not activatable (kb-0023), Hermes built-in memory verified (kb-0025),
  `kb stale` / `kb churn` tooling.
- Plan file with full designs: `~/.claude/plans/humming-noodling-flame.md`.

## Deferred (details + triggers in the backlog file)

Graphify on e2e (needs ≥50k-LOC-scale pain) · thin KB-MCP server (needs a
tool that can't shell out) · skill-creator eval loop (needs ≥2 skills +
usage data) · Superpowers trial (after fleet settles ~2026-08-20+, one-week
trial) · Telegram notification wiring (design in plan §1.1) · minor
prune-pass items (kb-0017 split, 12 lint warnings, verified-date backfill,
sparse e2e allowlist).

## Rejected (do not resurface)

**Gas Town / persistent agent workforce — rejected by user 2026-08-09** after
deep-research (103-agent workflow, 23 claims verified): designed for 20–30
agents + always-on stack vs a 1–2 agent ceiling on this box, ~$100/hr token
burn, documented bad autonomous merges and heavy review overhead. Verdict +
sources: `~/kb/refs/gastown-research-2026-08-09.md` (section map [[kb-0046]]). If "agents working while
away" is ever wanted again, the cheaper path is cloud Routines (zero local
footprint, lid-closed) + 1–2 batched local workers with a hard token budget.

## Standing exclusions — do not resurface

Backups (user handles), credential hardening (sole user), parallel memory
stores, extra routers, WebSearch in subagent tool lists, bulk plugin
installs.

Before proposing any deferred item, read the backlog file's trigger for it;
before proposing anything new, check this note and the audit report
(`~/reports/claude-harness-audit-2026-08-05.md`) so resolved decisions and
exclusions don't recirculate.

Related: [[kb-0023]] — agent teams verdict; [[kb-0025]] — Hermes memory;
[[kb-0024]] — embeddings endpoint gotcha.
