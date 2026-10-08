---
id: kb-0044
title: "Claude Code: /model <name> persists as default; session-only switch needs picker, flag, or env var"
type: gotcha
domain: tooling
tags: [claude-code, model, settings, session]
aliases: [model switch, session model, /model persists, ANTHROPIC_MODEL]
triggers:
  - "switching the Claude Code model for one session only"
  - "default model changed unexpectedly after /model"
  - "overriding the model without editing settings.json"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0035]
source: "code.claude.com docs (commands, cli-reference, env-vars, settings), 2026-08-19"
summary: "In Claude Code, `/model <name>` (with argument) switches AND saves the model as the new default. Session-only: `/model` picker + press `s`, or `claude --model X` at launch, or ANTHROPIC_MODEL env var."
---
`/model <name>` (with a model argument) switches the model AND persists it as the default for all new sessions — this is documented current behavior, not a bug. To switch for one session without touching the default, use the picker, the CLI flag, or the env var.

**Session-only switch (three ways):**
1. In-session: run `/model` with *no* argument to open the interactive picker, then press **`s`** on the desired model row — applies to the current session only, nothing persisted.
2. At launch: `claude --model <model>` (e.g. `claude --model sonnet`) — applies to that session only, writes no settings file.
3. Env var: `ANTHROPIC_MODEL=<model> claude` — overrides the settings key for that shell.

**Precedence (highest → lowest):** CLI `--model` / in-session `/model` > `ANTHROPIC_MODEL` env var > settings files (managed > local > project > user).

**Reverting a persisted default:** `/model <name>` wrote the `model` key into `~/.claude/settings.json`. Either run `/model <preferred>` to overwrite it, or edit/remove the `"model"` key in `~/.claude/settings.json` directly (read at session start, not hot-reloaded — restart after editing). A project-level `.claude/settings.json` `model` key can also override the user-scope default without touching global settings.

Notes: no `/model persist` subcommand exists (web-search hallucination). `claude -p --model X "…"` (print mode) is also session-only. Verified against code.claude.com docs 2026-08-19; the picker-`s` behavior is version-dependent — re-check docs if the picker has no `s` hint.

## Related
- [[kb-0035]] — use-profile.sh composes settings.json; relevant if it ever manages the `model` key
