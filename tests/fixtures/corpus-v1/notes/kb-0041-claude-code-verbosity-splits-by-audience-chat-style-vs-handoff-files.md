---
id: kb-0041
title: "Claude Code verbosity splits by audience: output style for chat, self-contained handoff files for agents"
type: howto
domain: tooling
tags: [claude-code, output-styles, settings, handoffs]
aliases: [output style, verbosity config, terse style, handoff file]
status: active
confidence: high
created: 2026-08-16
updated: 2026-08-16
supersedes: []
related: [kb-0035]
triggers: ["configuring Claude Code verbosity or output styles", "writing or resuming agent-to-agent handoff files"]
source: "code.claude.com/docs/en/output-styles; deep-research wf run 2026-08-16; 3-way adversarial review 2026-08-16"
summary: "Two audiences, two mechanisms: outputStyle governs human-read chat; agent-consumed detail goes to self-contained handoff files with orchestrator duties."
---
Two audiences, two mechanisms — there is no single verbosity dial that does both.

## Human-read output: output styles

- Output styles modify the system prompt (role/tone/format). Set via `/config` or the `outputStyle` field in settings.json (e.g. `"outputStyle": "terse"`).
- Custom styles = markdown files with frontmatter: `~/.claude/output-styles/` (user) or `.claude/output-styles/` (project).
- `keep-coding-instructions: true` in frontmatter layers the style ON TOP of the built-in engineering instructions; without it they are REPLACED (which degrades coding behavior). Use it for pure communication-style changes.
- Style is fixed at session start — changes need `/clear` or a new session. Applies only to the main conversation, not subagents — exception: a FORK inherits the parent's full system prompt, including the output style (matters for `context: fork` skills).
- `/output-style` command deprecated v2.1.73, REMOVED v2.1.91 — community guides showing runtime `/output-style` switching are stale.
- Python Agent SDK has no programmatic output-style option (use settings files + setting_sources, `append`, or custom prompt); TS SDK sets `outputStyle` in the inline settings object.

## Agent-consumed output: separation, not configuration

- Dominant pattern (endorsed by Anthropic's multi-agent engineering post): full detail → file on disk; only a terse summary/pointer → conversation.
- Common shapes: plan+progress in a `.md` then `/clear` and resume by reading it; or fixed-schema JSON handoff chains (e.g. `{overview, files[{path,action}], steps, risks}`).
- `context: fork` in skill frontmatter isolates verbose work in a subagent so it never enters the main conversation.
- Headless/programmatic: `claude -p --output-format text|json|stream-json`; `--verbose` is debug-only.

## Setup on this box (2026-08-16, hardened by adversarial review)

- `~/.claude/output-styles/terse.md` (keep-coding-instructions: true) + `"outputStyle": "terse"` in `~/.claude/settings.json`. Scope note lives in `~/.claude/CLAUDE.md`; the canonical rule is the "Dual-audience output" bullet in `~/AGENTS.md`.
- Split by MOMENT: at a human approval gate (plan mode/ExitPlanMode, or any ask) the full plan is shown inline (numbered steps, files touched, risks) with no length cap; reporting to a parent agent is never an approval gate.
- Handoff files: `~/reports/handoffs/YYYY-MM-DD-<slug>.md`, header `status: active|done`, sections Goal / State / Decisions (+why) / Failed attempts (do-not-retry) / Next steps / Paths. Self-contained: a fresh agent continues from the file alone; resume = newest `active` file, mark `done` when finished; missing section → report the gap, don't guess.
- Orchestrator duties: pass the handoff path in the next spawn prompt; receiver reads it before acting; a subagent's final message states outcome + blockers + path, never a bare path. Read-only agents return the full result; the orchestrator files it if a later run needs it.
- Why each piece exists: without orchestrator duties, pointers degrade to a telephone game; without failed-attempts/state, later agents retry dead ends; chat-only paths are lost by `/clear` and autocompaction.
- Persistence: `terse.md` is chezmoi-tracked; `outputStyle` lives in `~/.claude/settings.base.json` (tracked) because `use-profile.sh` recomposes `settings.json` from base + profile — hand-edits to `settings.json` are lost on recomposition ([[kb-0035]]). `AGENTS.md` and `~/.claude/CLAUDE.md` are chezmoi-tracked too.
- Memory loading (verified vs code.claude.com/docs/en/memory, 2026-08-16): Claude Code does NOT read AGENTS.md natively — the `@~/AGENTS.md` import in `~/.claude/CLAUDE.md` is the official bridge. Memory files are concatenated, never overwritten; contradictions are NOT resolved ("Claude may pick one arbitrarily"), so the two files must stay non-overlapping (shared rules in AGENTS.md, Claude-only mechanics in CLAUDE.md).
- Known gap: nothing automatically enforces handoff completeness. Candidate fix if thin handoffs ever bite: SessionStart/Stop hook lint or a handoff-writer skill (not built — premature).

## Related
- [[kb-0035]] — settings.json composition/permissions on this box
