---
id: kb-0046
title: "Gas Town rejected 2026-08-09: verdict + verified evidence in refs/gastown-research-2026-08-09.md"
type: reference
domain: tooling
tags: [claude-code,harness,multi-agent,gastown]
aliases: [gas town, gastown, persistent agent workforce, yegge gastown]
triggers:
  - "asked about Gas Town, always-on agent fleets, or a persistent agent workforce"
  - "revisiting how to run agents while away from the box"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0026, kb-0045]
source: "session: deep-research workflow 2026-08-09 (103 agents, 21 sources, 25 claims verified -> 23 confirmed)"
summary: "Section map: Gas Town deep-research verdict — REJECTED 2026-08-09 (scale mismatch, ~$100/hr, bad autonomous merges); cheaper alternative = cloud Routines + batched local workers. Do not resurface."
---
Deep-research verdict on Gas Town (Steve Yegge's persistent-agent-workforce orchestrator) — **REJECTED by user 2026-08-09, do not resurface**. Consult `refs/gastown-research-2026-08-09.md` BY SECTION via the map below.

## Section map

1. Verdict — not worth it on fedora-server (1–2 agent ceiling vs 20–30 design point); if "agents working while away" is ever wanted: cloud Routines + 1–2 batched local workers with hard token budget; no merge queue without a reliable test gate.
2. What Gas Town is — Mayor/Polecat/Witness/Refinery roles, Beads-in-git state, Bors-style merge queue, `gt up` always-on stack.
3. Why rejected (verified findings) — scale mismatch (141 orphaned processes in one trial); ~$100/hr token burn (n=1) + multi-account arbitrage now blocked by Anthropic; first-person failure modes (queue merged a failing PR, bugs fixed 2–3× with fixes lost); thin maturity ("100% vibe coded", repeated do-not-use warning).
4. Viable alternatives (vendor-documented, verified) — cloud Routines (scheduled ≥1h, lid-closed OK, subscription-bounded), desktop scheduled tasks, /loop + cron.
5. Caveats — most negative evidence from Jan 2026 (tool was weeks old); $100/hr is n=1; non-Claude alternatives produced no verified claims. ⚠ if ever resurfaced, re-verify current state first.
6. Sources — Yegge post, gastown repo/docs, DoltHub hands-on, Hartcher, wangdhen, Anthropic routines docs.

## Related
- [[kb-0026]] — harness roadmap; records this rejection under its A.5 item
- [[kb-0045]] — the 2026-08-05 audit whose Tier-3 list originally proposed Gas Town
