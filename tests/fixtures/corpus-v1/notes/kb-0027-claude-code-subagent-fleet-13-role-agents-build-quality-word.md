---
id: kb-0027
title: "Claude Code subagent fleet: 13 role agents (build/quality/words/product) with colors, tiering, and invocation rules"
type: project
domain: tooling
tags: [claude-code, subagents, fleet]
aliases: [agent fleet, role agents, subagent roster]
triggers:
  - "delegating work to a specialist agent"
  - "adding or modifying subagent definitions"
  - "asking what agents exist on this box"
status: active
confidence: high
created: 2026-08-06
updated: 2026-08-06
supersedes: []
related: [kb-0009, kb-0023]
source: "session"
summary: "13-agent role fleet in ~/.claude/agents: roster table, dispatch rules, color groups, memory/skills features, headless gotchas. Force with @agent-name; new agents need fresh sessions."
---
Claude Code on this box runs a 13-agent role fleet in `~/.claude/agents/`
(set up 2026-08-06). Delegation routes off the `description` field; force a
specific agent with `@agent-<name>` in the prompt or `claude --agent <name>`
for a whole session.

## Roster (color = functional group)

| Group | Agents (color) | Model |
|---|---|---|
| Build | software-engineer (blue), data-engineer (cyan), uiux-engineer (pink), devops-engineer (orange) | inherit |
| Quality | qa-engineer (green, designs tests), code-reviewer (green), test-runner (green, runs suites) | inherit / haiku |
| Words & search | tech-writer (yellow, `skills: kb` preload), researcher (yellow), kb-researcher (yellow), Explore (yellow) | inherit / haiku |
| Product | product-owner (purple, `memory: user`), security-engineer (red) | inherit |

Design rules: dispatch-rule descriptions ("Use for/to…"), least-privilege
tool allowlists (product-owner has NO shell; reviewers read-only),
haiku only for lookup/runner roles, no WebSearch anywhere (broken via
proxy), machine standing rules (uv/dnf/no-commit) embedded in each body.
Only the 8 named colors exist (red blue green yellow purple orange pink
cyan) — no hex.

## Frontmatter features that work here (2.1.222)

`name description tools model color` plus `skills:` (preload full skill
content), `memory: user|project|local` (persistent MEMORY.md per agent in
`~/.claude/agent-memory/<name>/`), `effort`, `maxTurns`,
`isolation: worktree`, `background`. `disable-model-invocation` is a SKILL
field, not an agent field. Agent definitions are cheap at startup; runs are
~15× chat tokens — reserve multi-agent fan-out for parallelizable/review
work, and give every file exactly one writer.

## Gotchas (learned by running trials)

- New/reloaded agents are only visible to FRESH sessions; a running
  session's Agent-tool registry is snapshotted at start.
- Headless (`claude -p`) sessions: files outside the session cwd need
  permission approval that can't be prompted → run from the right cwd, and
  pass `--allowedTools "Read Grep Glob Write Edit Agent"` when the agent
  must write.
- Subagents have no parent conversation history — ground every dispatch
  with explicit paths (they do load CLAUDE.md + git status).

Proven in the first trial: qa-engineer found 7 real bugs in freshly written
`kb stale`/`kb churn` (empty-corpus crash, silent `--days=N` misparse,
unreadable-file abort, git quotePath mislabel, commit-count overcount),
all fixed; product-owner wrote the Telegram-wiring spec
(`~/reports/spec-telegram-notifications.md`) and refused to invent content
when its source read was blocked.

Related: [[kb-0009]] — other agent tools on this box; [[kb-0023]] — agent
teams (the interactive sibling feature) are NOT activatable here.
