---
id: kb-0031
title: "Multi-agent orchestrator landscape: which tools give real inter-agent communication"
type: fact
domain: tooling
tags: [herdr, claude-squad, tmux, multi-agent, orchestration]
aliases: [claude-squad, thurbox, cross-agent-teams, Tmux-Orchestrator, agent orchestration tools]
triggers:
  - "choosing a tool for multi-persona / orchestrator-worker Claude setups"
  - "wondering whether claude-squad or herdr let agents delegate to each other"
status: active
confidence: high
created: 2026-08-09
updated: 2026-08-09
supersedes: []
related: [kb-0008, kb-0030, kb-0032, kb-0009]
source: "deep-research workflow 2026-08-09 (verified vs primary repos/docs; 24/25 claims survived 3-vote adversarial check)"
summary: "Orchestrator landscape for multi-persona Claude setups: herdr, thurbox, Tmux-Orchestrator and cross-agent-teams MCP give real inter-agent messaging; claude-squad is parallel-agent management only (no comms)."
---
Third-party "multi-persona" tools split cleanly by whether agents can actually talk to each other. Verified 2026-08-09.

**Real inter-agent communication:**
- **herdr** ([[kb-0008]]) — persistent named agent panes; delegation via `herdr agent prompt <name> "…" --wait` + per-pane state (working/blocked/idle) over its CLI/socket API. No message bus — messaging *is* prompt + state/events. ⚠ 0.x CLI churns between minors.
- **thurbox** (Thurbeen/thurbox) — TUI orchestrator with a genuine SQLite-persisted mailbox queue (`thurbox-cli message send/reply/inbox --claim`, atomic exactly-once claim drains), remote SSH sessions, native code-review view; any CLI agent defined in `~/.config/thurbox/agents.toml`.
- **Tmux-Orchestrator** (absmartly) — Orchestrator → PMs → Engineers, each in a tmux window; all comms via `send-claude-message.sh` = scripted `tmux send-keys` (simulated keystrokes + delivery verification). Hierarchy is convention, not enforced.
- **cross-agent-teams / xats** (jtianling) — local MCP daemon message bus in one SQLite file (`~/.cross-agent-teams-mcp/data.db`); agents register it as an MCP server in their native CLI; tools: register_agent, send_message, broadcast[_to_role], get_inbox, list_agents. Transport-only by design, not an orchestrator.

**Parallel management only — NO inter-agent comms:**
- **claude-squad** (`cs`, smtg-ai) — most popular parallel-agent manager: tmux sessions + git worktree per agent, works after closing the pane. Deliberately human-supervised: menu is operator lifecycle only (create/attach/detach/checkout/resume/kill). Verified through v1.0.19; a listicle claiming "native coordination" was refuted. ⚠ Misinformation exists in this niche — check primary repos.

**Native option:** Claude Code's own primitives ([[kb-0030]]) may make third-party tooling unnecessary for simple setups. Recurring config pattern across all tools: one git worktree per *parallel worker* (task-scoped, not per persona — see [[kb-0032]]); persona behavior comes from per-agent prompts/CLAUDE.md, never from the multiplexer.

## Related
- [[kb-0008]] — herdr guide section map (full setup/delegation recipes)
- [[kb-0030]] — Claude Code native multi-session primitives
- [[kb-0032]] — worktree hygiene for multi-persona setups
- [[kb-0009]] — agent tools installed on this box
