---
id: kb-0030
title: "Claude Code native multi-session primitives: cross-session messaging, headless resume, agent teams"
type: fact
domain: tooling
tags: [claude-code, multi-agent, orchestration]
aliases: [cross-session messaging, CLAUDE_CODE_MESSAGING_SOCKET, agent teams, claude -p resume]
triggers:
  - "making multiple Claude Code sessions talk to each other or delegate work"
  - "orchestrating headless claude -p workers from another session or script"
status: active
confidence: high
created: 2026-08-09
updated: 2026-08-09
supersedes: []
related: [kb-0023, kb-0031, kb-0027]
source: "deep-research workflow 2026-08-09 (adversarially verified vs code.claude.com/docs/en/{cross-session-messaging,headless,agent-teams})"
summary: "Claude Code native multi-session primitives, all version-gated: cross-session messaging over Unix sockets (v2.1.224+), machine-wide `claude -p --resume` (v2.1.223+), experimental agent teams."
---
Claude Code can run multiple persistent sessions that are aware of each other and delegate work with zero external tooling — via three version-gated native primitives.

1. **Cross-session messaging (v2.1.224+, macOS/Linux/WSL2)** — independent sessions discover each other with `ListAgents` and message by name with `SendMessage`. Delivery is over a per-session Unix inbox socket (shown in `/status`, never via Anthropic servers). The socket is exported to hooks and Bash as `CLAUDE_CODE_MESSAGING_SOCKET` before any hook runs — so custom hooks/scripts can post into a session directly. A headless `claude -p` worker binds the same socket; to accept messages unattended, start it with `crossSessionInbound: accept` in `--settings`.
2. **Headless + machine-wide resume (v2.1.223+)** — an orchestrator spawns workers programmatically: `session_id=$(claude -p "Start a review" --output-format json | jq -r '.session_id')`, later `claude -p "Continue that review" --resume "$session_id"`. Since v2.1.223 the resume works from *any* directory on the machine (before: current project only).
3. **Agent teams (experimental)** — `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`: a lead session spawns teammates, each with its own context window. Mailboxes are JSON files (`~/.claude/teams/{team}/inboxes/{agent}.json`), shared task list with file-locking (`~/.claude/tasks/{team}/`). Display: in-process (default since v2.1.179) or split panes (`teammateMode`, requires tmux/iTerm2). Limitations: no session resumption, one team per session, no nested teams — and not activatable on this deployment ([[kb-0023]]).

⚠ All three are recent/experimental; re-check version gates before relying on them. For tool-based orchestration on top, see [[kb-0031]].

## Related
- [[kb-0023]] — agent teams not activatable on this deployment (flag-gated)
- [[kb-0031]] — third-party orchestrator landscape (herdr, claude-squad, …)
- [[kb-0027]] — in-session subagent fleet: the simpler alternative when multi-session isn't needed
