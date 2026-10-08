---
id: kb-0025
title: "Hermes built-in memory (MEMORY.md/USER.md) is live and persistent: no external provider or memory_20250818 needed"
type: fact
domain: tooling
tags: [hermes, memory]
aliases: [hermes memory, memory_20250818, MEMORY.md, USER.md]
triggers:
  - "considering persistent memory for the Hermes gateway, or the Anthropic memory tool"
status: active
confidence: high
created: 2026-08-06
updated: 2026-08-06
supersedes: []
related: [kb-0017]
source: "session"
summary: "Hermes built-in memory (MEMORY.md + USER.md in ~/.hermes/memories, bind-mounted) is enabled, verified end-to-end; external memory providers and memory_20250818 are unnecessary."
---
Hermes Agent has a working built-in memory system — the audit's proposed
`memory_20250818` handler and any external memory provider are unnecessary
here.

## What is live (verified 2026-08-06)

- Built-in memory = `MEMORY.md` (agent memories) + `USER.md` (user profile)
  in `~/.hermes/memories/`, inside the bind-mounted `~/.hermes` volume →
  persistent across container restarts/rebuilds.
- Config block `memory:` in `~/.hermes/config.yaml`: `memory_enabled: true`,
  `user_profile_enabled: true`, `memory_char_limit: 2200`,
  `user_char_limit: 1375`, `nudge_interval: 10`, `flush_min_turns: 6`.
- `hermes memory status` shows injection, user profile, and the memory WRITE
  tool all enabled. External providers (mem0, honcho, hindsight, …) come
  installed as plugins but stay inactive (`plugins: enabled: []`) — keep it
  that way; a parallel memory store duplicates ~/kb (audit "what NOT to do").
- End-to-end checks: a fresh one-shot session recalled a fact present only in
  USER.md (Telegram username); a one-shot instruction to merge two duplicate
  MEMORY.md entries did so via the memory tool (2 tool calls, nothing else
  touched).

## Operations

- Inspect: `docker exec hermes hermes memory status`; files readable directly
  under `~/.hermes/memories/`.
- The gateway REWRITES config.yaml but does not touch the memory files —
  direct reads are safe; for writes prefer letting the agent use its memory
  tool (it holds `MEMORY.md.lock`).
- `hermes memory reset` ERASES both files — never run unthinkingly.
- `memory_20250818` (Anthropic API memory tool) is moot: Hermes reaches the
  proxy over the OpenAI wire (glm-5.2), and the built-in system already
  provides the same file-backed persistent memory.

Related: [[kb-0017]] — the Hermes deployment this belongs to.
