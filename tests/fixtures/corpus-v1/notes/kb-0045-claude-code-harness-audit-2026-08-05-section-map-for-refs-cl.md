---
id: kb-0045
title: "Claude Code harness audit 2026-08-05: section map for refs/claude-harness-audit-2026-08-05.md"
type: reference
domain: tooling
tags: [claude-code,harness,audit]
aliases: [harness audit, claude audit, 2026-08-05 audit]
triggers:
  - "reviewing the Claude harness setup or questioning why a piece exists"
  - "asked about hooks, MCP servers, plugins, graphify, or model-routing decisions and their rationale"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0026]
source: "session: 9-agent audit/research fleet 2026-08-05 (~197k tokens, all findings disk-verified)"
summary: "Section map: 2026-08-05 Claude Code harness audit — state of the box, findings by area (KB, subagents, graphify, hooks, MCP, routing), Tier 0-3 roadmap. Status of items in kb-0026."
---
Nine-agent audit of the Claude Code harness on fedora-server — consult `refs/claude-harness-audit-2026-08-05.md` BY SECTION via the map below, never whole. ⚠ §2–§4 are a point-in-time snapshot (2026-08-05); most roadmap items were executed or deferred since — execution status lives in [[kb-0026]] and `~/reports/claude-harness-future-work.md`, not here.

## Section map

1. Executive summary — "unusually well-designed, unusually under-wired"; headline verdicts: KB foundation excellent, build native subagent fleet first, code-graph tooling not yet.
2. Current state (2026-08-05) — settings/profiles incl. aibox tier mapping, KB at 22 notes, per-project Claude integration table, "the zeros" (no hooks/agents/MCP/plugins/backups). ⚠ stale by design — superseded by Phases 0–2 work.
3. Findings by area — 3.1 KB lifecycle gaps (lint cadence, CLI bugs, link drift); 3.2 the four orchestration layers (subagents / `claude --bg` background sessions / agent teams ≈7× tokens / dynamic workflows) + token economics; 3.3 graphify = not yet (payoff >50k–150k LOC vs 104 files; CLAUDE.md is the free equivalent); 3.4 hooks = biggest structural gap (hooks deny even in bypassPermissions) with 6 recipes; 3.5 MCP shortlist (Context7, Playwright-via-podman, GitHub, Fetch; ≤4–5 servers because aibox disables tool search) + plugin picks + tool-poisoning security notes; 3.6 model routing: keep aibox + profiles, native knobs over a router, hardening env block, LiteLLM supply-chain warning; 3.7 critic pass: backups biggest real gap, `permissions.deny` for secrets, untested official fallback, accidental update policy, no measurement.
4. Proposed roadmap — Tier 0 hygiene/safety, Tier 1 wiring (Telegram hook, 5-agent fleet, allowlists, plugins, statusline, kb CLI fixes), Tier 2 knowledge-loop automation (SessionEnd distiller, KB-MCP, instrumentation, churn/stale tooling), Tier 3 bets with kill criteria. ⚠ check [[kb-0026]] for what was done vs deferred.
5. What NOT to do — resolved contradictions, kept as standing exclusions: no code-graph at current size, no extra router, no parallel memory store, no WebSearch via proxy, no bulk plugins, no agent teams for coding.
6. Measuring success — baselines: $87.16 metered cost, 0 search misses at 22 notes, 132 queued permission prompts.
Appendix A/B — source list; raw fleet reports pointer.

## Related
- [[kb-0026]] — execution status of this audit's roadmap (done/deferred/rejected)
