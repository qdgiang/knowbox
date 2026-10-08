---
id: kb-0009
title: "Agent tools on this box: Claude Code, OpenCode, Copilot, Herdr — and how each loads context"
type: fact
domain: tooling
tags: [agents, claude-code, opencode, copilot, agents-md]
aliases: [which AI tools are installed, agent context loading]
triggers:
  - "which AI coding tools exist on this machine"
  - "making a convention/instruction visible to all agent tools"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0001, kb-0008, kb-0012]
source: "filesystem survey 2026-08-02"
summary: "Multi-agent-tool box: Claude Code, OpenCode, Copilot, Herdr. Shared conventions in ~/AGENTS.md; shared knowledge in ~/kb. Installed TOOLS, not subagents (fleet: kb-0027)."
---
This machine runs several AI coding tools in parallel — any cross-tool convention or knowledge must live in shared files, not one tool's config ("shared content, per-tool wiring — not N copies").

| Tool | State dir / presence | Loads conventions from |
|---|---|---|
| Claude Code | `~/.claude/` (settings, auto-memory in `~/.claude/projects/-home-giang/memory/`) | `~/.claude/CLAUDE.md` (user-level, always) + project `CLAUDE.md` + skills |
| OpenCode | `~/.opencode/` (npm-installed binary) | `AGENTS.md` |
| Copilot | `~/.copilot/` | `AGENTS.md` |
| Herdr | multiplexer that hosts the above in panes — see [[kb-0008]] | n/a |

**The wiring (verified 2026-08-02):**
- `~/AGENTS.md` holds the shared KB query/write protocol **once**.
- Claude Code: `~/.claude/CLAUDE.md` starts with `@~/AGENTS.md` (import must be `@~/…` — a bare `@AGENTS.md` would resolve against `~/.claude/` and silently load nothing). Plus a `/kb` skill at `~/.claude/skills/kb/SKILL.md` for maintenance ops.
- OpenCode: `~/.config/opencode/AGENTS.md` is a **symlink** to `~/AGENTS.md` (applied to all sessions per its docs); it also walks up from cwd finding `~/AGENTS.md`, and falls back to project `CLAUDE.md` where no project `AGENTS.md` exists. Config `~/.config/opencode/opencode.jsonc` holds models only.
- The knowledge itself lives in `~/kb` (see [[kb-0001]]); `kb` is on PATH via `.zshrc`/`.bashrc` and runs under uv-managed Python ([[kb-0012]]).

Claude's auto-memory (`~/.claude/projects/-home-giang/memory/`) remains for Claude-internal session preferences; durable machine/project facts are canonical in `~/kb` — memory notes on those topics are pointers into the KB.

## Related
- [[kb-0001]] — KB design rationale
- [[kb-0008]] — Herdr guide map
