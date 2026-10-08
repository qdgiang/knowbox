---
id: kb-0037
title: "Running multiple agents on this box: decision-tree hub for the multi-agent note cluster"
type: howto
domain: tooling
tags: [multi-agent,]
aliases: [multi-agent hub, which agents note, parallel agents routing]
triggers:
  - "running agents in parallel or as a team"
  - "choosing between subagents, fleets, and orchestrators"
  - "asking what multi-agent setup exists here"
status: active
confidence: high
created: 2026-08-11
updated: 2026-08-11
supersedes: []
related: [kb-0009, kb-0023, kb-0026, kb-0027, kb-0030, kb-0031, kb-0032]
source: "session 2026-08-11: KB review — 5-note cluster had no router"
summary: "Decision tree for multi-agent questions: in-session fleet (kb-0027), native primitives + version gates (kb-0030), third-party orchestrators (kb-0031), teams verdict (kb-0023), worktree hygiene (kb-0032)."
---
Five notes cover "multiple agents on this box"; route by the actual question:

- **Which role agents exist in my current Claude Code session, and how do I invoke them?** → [[kb-0027]] (the 13-role fleet, tiering, colors, invocation rules).
- **Can Claude Code itself run parallel sessions / teams / cross-session messaging?** → [[kb-0030]] (native primitives with version gates — re-check `claude --version` before relying on a gate).
- **Are agent teams usable on THIS deployment?** → [[kb-0023]] (verdict: not activatable as of 2.1.222; flag-gated; falls back to plain subagents).
- **What third-party orchestrators exist and which give real inter-agent messaging?** → [[kb-0031]] (landscape: claude-squad etc.; dated research snapshot).
- **How do I isolate parallel agents' file changes?** → [[kb-0032]] (herdr multi-persona; worktrees are per-task, not per-persona).
- **Which agent TOOLS are installed (not subagents)?** → [[kb-0009]].

Standing context: Gas Town / persistent agent workforce was researched and rejected 2026-08-09 ([[kb-0026]] — do not resurface); the 1–2 agent ceiling on this box is the design constraint all of the above assume.

## Related
- [[kb-0027]] — the fleet this box actually runs
- [[kb-0023]] — teams verdict
- [[kb-0030]] — native primitives
- [[kb-0031]] — orchestrator landscape
- [[kb-0032]] — worktree hygiene
- [[kb-0009]] — installed agent tools (not subagents)
- [[kb-0026]] — Gas Town rejection (standing context)
