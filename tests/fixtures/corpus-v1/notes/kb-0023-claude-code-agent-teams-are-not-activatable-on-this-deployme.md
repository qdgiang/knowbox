---
id: kb-0023
title: "Claude Code agent teams are not activatable on this deployment: flag-gated, falls back to plain subagents"
type: fact
domain: tooling
tags: [claude-code, agent-teams, subagents]
aliases: [CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS, tengu_amber_flint, teammate spawn]
triggers:
  - "considering agent teams or multi-agent orchestration beyond subagents in Claude Code"
status: active
confidence: high
created: 2026-08-06
updated: 2026-08-22
supersedes: []
related: [kb-0009, kb-0030, kb-0066]
source: "session"
summary: "Agent teams (CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS) do not activate here: remote-flag gated; spawn falls back to plain Agent calls. Keep subagent fleet + Workflow fan-out."
---
As of 2026-08-06 (Claude Code 2.1.222) agent teams cannot be activated on
this deployment. The env var is recognized but the team machinery never
starts; a lead asked to spawn teammates silently falls back to a plain
Agent-tool subagent.

## Evidence (all verified on this box)

- `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` is confirmed in the installed
  binary, but enablement is ALSO gated by remote feature flag
  `tengu_amber_flint` (statsig-style, server-side; unverifiable from here —
  evidently off for this account/proxy).
- Headless spawn probe with the env var set claimed "the teammate completed",
  but the session transcript shows exactly ONE ordinary `Agent` tool_use
  (`subagent_type: Explore`), zero team tools (no SendMessage, no shared task
  list), and no `~/.claude/tasks/{team}/` or `~/.claude/teams/` dirs created.
- Hidden CLI flag: `claude --agent-teams` → `error: unknown option` (the
  2.1.222 parser rejects it even though it appears in binary strings).
- Teams are interactive-first by design (shared task list, mailboxes,
  SendMessage); headless use is undocumented anyway. **2026-08-22 update:**
  official docs now state this explicitly — "In non-interactive mode with the
  -p flag, including Agent SDK sessions, Claude doesn't spawn teammates"
  (code.claude.com/docs/en/agent-teams, verified 3-0 in kb-0066 survey);
  issue anthropics/claude-code#72343 (unfixed) confirms the headless path.
  The local remote-flag gating above is therefore moot for headless use even
  if the flag were enabled.

## Verdict and revisit trigger

Keep the parallelism path that works here: the subagent fleet + Workflow
fan-out. Nothing was persisted to settings (the env var was only ever set
per-invocation), so there is nothing to roll back. Revisit only if Anthropic
enables the flag for this account, or a release ships teams GA — check the
CHANGELOG for "agent teams", or whether the hidden flag starts parsing.

Related: [[kb-0009]] — the agent-tool inventory this verdict extends.
[[kb-0030]] — native multi-session primitives that landed right after this
note (cross-session messaging v2.1.224+, machine-wide `--resume` v2.1.223+);
if this box is updated past those versions, that path may work here even
while agent teams stay flag-gated — worth probing before revisiting teams.
[[kb-0066]] — Aug-2026 orchestrator survey; its headless-gap finding
corroborates this note and keeps external orchestrators relevant here.
