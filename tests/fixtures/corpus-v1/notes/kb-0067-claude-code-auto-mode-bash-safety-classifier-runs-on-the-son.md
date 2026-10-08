---
id: kb-0067
title: "Claude Code auto-mode Bash safety classifier runs on the sonnet tier — no override"
type: gotcha
domain: tooling
tags: [claude-code, model-routing, auto-mode, safety-classifier]
aliases: [safety classifier model, auto mode classifier, bash classifier]
triggers:
  - "non-read-only Bash freezing or erroring in Claude Code auto mode"
  - "wondering which model the Claude Code safety classifier uses, or how to override it"
status: verified
confidence: high
created: 2026-08-22
updated: 2026-08-26
supersedes: []
related: [kb-0018, kb-0017]
source: "Claude Code official docs (model-config + auto-mode-config pages), verified 2026-08-22"
summary: "Claude Code's Bash auto-mode safety classifier runs on the sonnet tier (falls back to session/opus); there is NO classifier env var — a flapping sonnet-tier proxy channel freezes all non-read-only Bash."
---
The Bash auto-mode safety classifier uses the sonnet tier — Claude Sonnet 5 by default, here whatever `ANTHROPIC_DEFAULT_SONNET_MODEL` resolves to ([[kb-0018]]); it falls back to the session model, or opus when the session runs on fable. **There is no classifier-specific env var** — per official docs, Claude Code selects the classifier model and it "is not something you configure".

**Live confirmation of the fallback (2026-08-26)**: a fail-closed denial surfaced as "ZHIPU/GLM-5.3 is temporarily unavailable, so auto mode cannot determine the safety of Bash right now" — the error text names the model the classifier was actually using. Chain: sonnet-tier call unavailable → fable-session fallback → opus (`ZHIPU/GLM-5.3`) → GLM channel mid-flap → no verdict → denial. All channels self-healed within minutes; retry succeeded. So during a flap you may see the *opus-tier* model named in classifier denials even though sonnet holds the slot — that's the fallback, not a misconfig. (First probe showed two-stage classifier errors too: "Stage 2 classifier error - blocking based on stage 1 assessment".)

**Operational consequence on this box**: the sonnet tier runs over ai-box.vn proxy channels that intermittently flap ([[kb-0017]]). While the sonnet channel is down, every non-read-only Bash call in auto mode blocks on the classifier — all shell work freezes until the channel self-heals. Seen three times on 2026-08-22: twice with kimi-k2.7-code (~minutes each), and once with kimi-k3 (the sonnet slot after the ladder reshuffle) — the denial surfaces as "Stage 2 classifier error - blocking based on stage 1 assessment (usually transient — retrying often succeeds)". Happens across whichever model holds the sonnet slot → it's the proxy channel flapping, not a model-specific bug. This is an operational gotcha, not a config bug.

**Mitigations during a flap:**
- Keep the sonnet slot on the most stable proxy model available (tier ladder: [[kb-0018]]).
- `Shift+Tab` to Manual permission mode (`--permission-mode default`) — prompts instead of classifying.
- Or `acceptEdits` — auto-approves edits, prompts for shell commands instead of erroring.

**Why coding on the same model still works during a flap**: classifier calls are *separate, independent API requests* from the conversation calls — the proxy can drop one while the other gets through, and the two sides handle errors differently. The main conversation loop retries and the user just sees slowness; the classifier is **fail-closed** — no verdict means the Bash action is denied outright, which is impossible to miss. The classifier also gets a stripped transcript (user messages + non-read-only tool calls + CLAUDE.md; tool results removed) and must return a parseable verdict; unparseable → retry, repeated failure → denial. So a flapping channel manifests as "classifier broken, coding fine". Verified 2026-08-22: a sonnet-tier (kimi-k3) probe agent answered in ~4 s minutes after classifier denials. (Docs: errors page + permission-modes/auto-mode-config pages; 90 s data-free banner threshold on classifier calls vs 20 s for the main loop.)

**Not the classifier** (common mis-guesses): the haiku tier / `ANTHROPIC_SMALL_FAST_MODEL`, and `CLAUDE_CODE_SUBAGENT_MODEL` (subagents only).

## Related
- [[kb-0018]] — the tier→model mapping this depends on; classifier = whatever the sonnet slot holds
- [[kb-0017]] — ai-box.vn proxy channel flapping (the failure mode this gotcha exposes)
