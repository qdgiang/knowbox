---
id: kb-0066
title: "AI agent orchestration tools survey 2026-08-22: section map for refs/2026-08-22-ai-agent-orchestration-tools-survey.md"
type: reference
domain: claude-code
tags: [orchestration, multi-session, fleet, herdr, orca, gas-town]
aliases: [orchestrator landscape 2026, agent fleet tools, multi-session orchestration survey]
triggers:
  - "choosing or comparing multi-session AI agent orchestrators (herdr, orca, claude-squad, gas town, conductor, composio)"
  - "deciding what external orchestration adds beyond Claude Code native features"
  - "looking up verified facts about agent teams, headless fleets, or worktree tooling"
status: active
confidence: high
created: 2026-08-22
updated: 2026-08-22
supersedes: []
related: [kb-0031, kb-0023, kb-0030, kb-0032]
source: "deep-research workflow wf_c6ce2d74-b81 (103 agents, 21 sources, 3-vote verification)"
summary: "Verified Aug-2026 survey of multi-session agent orchestrators (herdr, orca, claude-squad, gas town, conductor, composio) vs Claude Code native agent teams/worktrees; headless gap is the key finding."
---
Adversarially verified (3-vote panels, 22/25 confirmed) landscape survey of multi-session AI coding-agent orchestration tools as of 2026-08-22. ⚠ Fast-moving space: all coverage is June–Aug 2026; star counts and gating flags are snapshots — re-verify before relying.

Read `~/kb/refs/2026-08-22-ai-agent-orchestration-tools-survey.md` by section, never whole:

## Section map
- **Executive summary** — four categories (native Claude Code orchestration / TUI multiplexers-daemons / desktop ADEs / heavy persistent frameworks); the decisive headless gap.
- **Verified findings** — 10 findings, each with confidence, vote, sources:
  1. Native agent teams (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`, lead+teammates, shared task list, JSON mailboxes at `~/.claude/teams/…`)
  2. **Agent teams is interactive-only — no teammates under `claude -p`/Agent SDK (the key headless finding)**
  3. Native `--worktree`/`-w` since v2.1.49 commoditizes worktree-fleet isolation
  4. Herdr: daemon + attachable TUI, 20 agent CLIs, agent-to-agent socket API, restart persistence
  5. Orca: cross-platform ADE, per-task worktrees, 25+ backends, fan-out-and-pick-winner, SSH remote execution
  6. Claude Squad (`cs`): canonical tmux+worktree TUI, AGPL, detach-and-persist
  7. Gas Town: daemon/mailboxes/Beads state/watchdogs/merge queue for 20–50+ agents
  8. Composio Agent Orchestrator: per-agent worktree/branch/PR + autonomous CI-repair loop
  9. Conductor: Mac-only, Melty Labs (not Anthropic)
  10. Headless PR-review loop via GitHub Actions `claude -p --output-format json` (no orchestrator needed)
- **Refuted claims** — 3 killed claims not to repeat (linear scaling, worktrees-universal, coordination-weakest).
- **Caveats** — vendor-verified ≠ efficacy-verified; notifications/cost-tracking uncovered.
- **Open questions** — headless agent-teams workaround, Gas Town vs handoff-file persistence, retention data.
- **Additional landscape signals** — secondary tools (Pane, Baton, Vibe Kanban, Grit…), practitioner signals (foreman pattern, ~4-5 agent laptop ceiling).
- **Source inventory** — primary vs secondary/forum URLs.

## Related
- [[kb-0031]] — earlier orchestrator landscape note; this survey updates/broadens it (Aug-2026, verified)
- [[kb-0023]] — agent teams not activatable here; survey confirms interactive-only limitation
- [[kb-0030]] — Claude Code native multi-session primitives (the baseline these tools extend)
- [[kb-0032]] — herdr setup recipes
