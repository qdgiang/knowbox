---
id: kb-0018
title: "Claude Code tier routing: qwen3.8-max-0902 fable, glm-5.2 opus, qwen3.8-flash sonnet+haiku via ai-box.vn"
type: decision
domain: tooling
tags: [claude-code, llm-proxy, model-routing, ai-box]
aliases: [model tier mapping, which model per tier, sonnet opus haiku mapping]
triggers:
  - "changing or debugging Claude Code model tiers on this box"
  - "choosing a model from the ai-box.vn pool"
status: verified
confidence: high
created: 2026-08-05
updated: 2026-09-05
supersedes: []
related: [kb-0017, kb-0009, kb-0035, kb-0067]
source: "web research 2026-08-05 (vendor benchmarks + Artificial Analysis + Agent Arena) + ~/.claude/settings.json"
summary: "Claude Code tier→model mapping on this box: fable=qwen3.8-max-0902, opus=glm-5.2, sonnet=qwen3.8-flash (classifier slot), haiku=qwen3.8-flash, all via ai-box.vn; rationale + revisit criteria."
---
Claude Code's model tiers on this box are mapped to ai-box.vn proxy models in the `env` block of `~/.claude/settings.json` (`ANTHROPIC_BASE_URL=https://api.ai-box.vn`):

| Tier | Env var | Model | Role |
|---|---|---|---|
| fable | `ANTHROPIC_DEFAULT_FABLE_MODEL` | `qwen3.8-max-0902` | top reasoning |
| opus | `ANTHROPIC_DEFAULT_OPUS_MODEL` | `glm-5.2` | planning, sub-agents, hard tasks |
| sonnet | `ANTHROPIC_DEFAULT_SONNET_MODEL` | `qwen3.8-flash` | safety classifier (+ any explicit sonnet-tier work) |
| haiku | `ANTHROPIC_DEFAULT_HAIKU_MODEL` | `qwen3.8-flash` | cheap utility calls (`ANTHROPIC_SMALL_FAST_MODEL` same) |

**2026-09-05 swap**: fable `qwen3.8-max` → `qwen3.8-max-0902` (user request: update the claude qwen3.8 slot to the newer version). Pool check first: `/v1/models` shows `qwen3.8-max-0902` live (new since the 08-26 drift check, along with non-wired `zai-org/glm-5.3-flash`); **no newer qwen3.8-flash revision exists**, so sonnet/haiku slots stay on `qwen3.8-flash`. Smoke-tested HTTP 200 in ~1.9 s before wiring. Flow: all three files edited to the same value (chezmoi source `~/dotfiles/dot_claude/private_settings.aibox.json` + `~/.claude/settings.aibox.json` + `~/.claude/settings.json`) → `chezmoi apply` (no-op) → `use-profile.sh status` = `aibox`, clean. Hook grep found only the `haiku` tier alias (08-27 lesson already applied) — no hardcoded fable model IDs anywhere. Takes effect for NEW sessions. Operational note: during this swap the `qwen3.8-flash` classifier channel flapped repeatedly (~15 min, gating Bash AND Edit intermittently, [[kb-0067]]) — same failure mode as 08-27; escape hatch unchanged (revert flash slots to deepseek-v4-pro-0813 / v4-flash-0731).

**2026-08-27 distill-hook fix**: the small residual trickle of bare `deepseek-v4-flash` calls after the env swaps (which the 08-24 entry claimed couldn't happen) came from `~/.claude/hooks/session-distill-run.sh` hardcoding `--model deepseek-v4-flash` for the SessionEnd KB-distillation worker — one small call per session ended. Tier env vars do NOT reach hardcoded model IDs in hook scripts. Fixed at line 90: `--model deepseek-v4-flash` → `--model haiku` (tier alias, resolves via `ANTHROPIC_DEFAULT_HAIKU_MODEL` at runtime), so future swaps propagate automatically. Lesson: after any tier swap, grep `~/.claude/hooks/` and sibling tool configs for hardcoded model IDs. Remaining intentional deepseek consumers: opencode `small_model` + `aibox-worker` agent in `~/.config/opencode/opencode.jsonc` (user decision 08-27: leave as-is).

**2026-08-27 distill-hook fix**: the small residual trickle of bare `deepseek-v4-flash` calls after the env swaps (which the 08-24 entry declared impossible) came from `~/.claude/hooks/session-distill-run.sh` hardcoding `--model deepseek-v4-flash` for the SessionEnd KB-distillation worker (one call per session ended; tier env vars don't reach hardcoded model IDs in hooks/scripts). Fixed at line 90: `--model deepseek-v4-flash` → `--model haiku` (tier alias → resolves via `ANTHROPIC_DEFAULT_HAIKU_MODEL`, so it tracks future swaps automatically). Lesson: after any tier swap, grep `~/.claude/hooks/` + sibling tool configs for hardcoded model IDs. Remaining intentional deepseek consumers: opencode `small_model` + `aibox-worker` agent (`~/.config/opencode/opencode.jsonc`) — left as-is per user decision 2026-08-27.

**2026-08-27 swap**: sonnet `deepseek-v4-pro-0813` → `qwen3.8-flash` and haiku `deepseek-v4-flash-0731` → `qwen3.8-flash` (user decision; `qwen3.8-flash` confirmed live in `/v1/models` before wiring). The change was hand-applied to `~/.claude/settings.json` first (~11:09), leaving the profile dirty ("Active profile: custom (matches neither)"), then made official via the usual flow: edit `~/dotfiles/dot_claude/private_settings.aibox.json` → `chezmoi apply ~/.claude/settings.aibox.json` → `use-profile.sh status` = `aibox` (the hand-edit was exactly the two env lines, so no recompose was needed and no snapshot created). Takes effect for NEW sessions. Operational note: within minutes of the hand-swap the new sonnet slot itself flapped ("qwen3.8-flash is temporarily unavailable, so auto mode cannot determine the safety of Bash") — the [[kb-0067]] failure mode is model-agnostic; qwen channels flap too. Escape hatch if the qwen3.8-flash slots keep flapping: revert to deepseek-v4-pro-0813 / v4-flash-0731 (both still live in the pool). No subagent-fleet file edits needed: tier aliases resolve via the env vars (below).

**2026-08-26 revert**: opus `ZHIPU/GLM-5.3` → `glm-5.2` (user decision). Immediate trigger: the GLM-5.3 channel went "temporarily unavailable" mid-session this morning, freezing the auto-mode Bash safety classifier (it falls back to the opus/session model, [[kb-0067]]) — glm-5.2 is the battle-tested predecessor. Endpoint consulted first: live `/v1/models` lists `glm-5.2`, `ZHIPU/GLM-5.2` (alt route), `glm-5.2-fast-preview`; bare `glm-5.2` smoke-tested HTTP 200 in ~1 s before wiring. Flow: edit `~/dotfiles/dot_claude/private_settings.aibox.json` → `chezmoi apply --force` (the 08-24 hand-wiring had left the target dirty vs chezmoi's state hash — only-opus-line diff confirmed first) → `bash ~/.claude/use-profile.sh aibox --force`; all three files agree again; takes effect for NEW sessions. Note `use-profile.sh` is NOT on PATH from a Claude Bash tool shell — invoke it as `bash ~/.claude/use-profile.sh`.

**2026-08-24 swap (second)**: sonnet `deepseek-v4-flash-0731` → `deepseek-v4-pro-0813` (user decision, same session as the haiku swap). Immediate trigger: the flash-0731 channel went "temporarily unavailable" mid-session, which froze the Bash auto-mode safety classifier ([[kb-0067]]) — pro-0813 confirmed live in the pool during the 08-22 drift check. Wired by hand (Edit tool, Bash frozen): chezmoi source + `~/.claude/settings.aibox.json` + `~/.claude/settings.json` all set to the same value, so `use-profile.sh status` still matches the aibox profile; `chezmoi apply`/recompose deferred — nothing to redo unless one of the three files is touched independently.

**2026-08-24 swap**: haiku `deepseek-v4-flash` → `deepseek-v4-flash-0731` (user decision: unify both flash slots on the 0731 re-post-trained model; plain v4-flash was still showing in call logs because only the sonnet slot had been swapped on 08-22). Changed in `~/dotfiles/dot_claude/private_settings.aibox.json` (chezmoi source) → `chezmoi apply ~/.claude/settings.aibox.json` → `use-profile.sh aibox --force`; takes effect for NEW Claude Code sessions. Bare `deepseek-v4-flash` should no longer appear in call logs.

**2026-08-22 swap**: opus glm-5.2 → `ZHIPU/GLM-5.3`, sonnet kimi-k2.7-code → `kimi-k3` (user decision after the pool-drift check below; both smoke-tested HTTP 200 before wiring). Endpoint ID for GLM-5.3 is exactly `ZHIPU/GLM-5.3` (slash included) — no bare `glm-5.3` exists on the proxy. Changed in `~/dotfiles/dot_claude/private_settings.aibox.json` (chezmoi source) + recomposed via `use-profile.sh aibox --force`; takes effect for NEW Claude Code sessions. Predecessors glm-5.2/kimi-k2.7-code stay in the Hermes alias pool.

**2026-08-22 second swap (same day)**: sonnet kimi-k3 → `deepseek-v4-flash-0731` (user decision, smoke-tested HTTP 200 in 0.8 s before wiring). Reasons: (1) cost — kimi-k3 is expensive, and session-transcript audit showed the sonnet tier does ZERO coding work on this box (daily work runs on fable/qwen3.8-max; 248× qwen3.8-max vs 0× kimi-k3 across the 10 newest sessions) — its only real consumer is the auto-mode safety classifier, tiny payloads; (2) stability — the kimi channel flapped three times that day, fail-closing the classifier ([[kb-0067]]), while deepseek channels are the documented stable fallback. Escape hatch if the classifier misjudges: bump sonnet to `kimi-k2.7-code`, or Shift+Tab to Manual mode.

Decided 2026-08-05 after comparing glm-5.2, kimi-k2.7-code, deepseek-v4-flash (the pool is listed in [[kb-0017]]).

## Rationale
- **opus = glm-5.2**: strongest of the three on independent signals — Artificial Analysis Intelligence Index 51 (kimi 42, v4-flash 40) and highest coding index; 2nd-highest confirmed task success on Agent Arena and tool-hallucination rate tied-lowest (1.86%), which matters most for planning/sub-agents; 1M context, MIT weights. Known weak spots: bash error-recovery, hardest long-horizon tasks (SWE-Marathon).
- **sonnet = kimi-k2.7-code**: the main loop consumes the most tokens; kimi's ~30% fewer thinking tokens cuts compounding cost, strong MCP/tool use. **Catch: 256K context** — Claude Code auto-compacts past it; for very long sessions consider glm-5.2 in both slots instead.
- **haiku = deepseek-v4-flash**: cheapest ($0.14/$0.28 per M, cache ~$0.003); the July-2026 0731 re-post-training roughly doubled its agent scores. Rejected for sonnet: it is already haiku (no tier separation) and weakest of the three on independent measures.

## Revisit when
- Independent SWE-bench Verified re-runs appear for deepseek-v4-flash 0731 or kimi-k2.7-code (mid-2026 numbers were vendor-reported).
- ai-box.vn proxy channels flap persistently on glm-5.2/kimi-k2.7-code (known intermittent 503s, [[kb-0017]]) — deepseek models are the stable fallback.
- New model added to the ai-box.vn pool; re-check the tier ladder then.

Tool-call format caveat for all three: OpenRouter-style Anthropic shims can mis-translate non-Anthropic tool conventions a few steps into agentic work; ai-box.vn is the direct route here.

## Pool drift check 2026-08-26
Live `/v1/models`: all 4 tier models live; chat pool unchanged (incl. `ZHIPU/GLM-5.2` alt route + `glm-5.2-fast-preview`). New since 08-22 is all non-chat: `happyhorse-*` (video edit/i2v/r2v/t2v), `wan2.7-*`/`wan3.0-video*` (video+image), `qwen-image-2.0/3.0/3.0-pro`, `qwen3.7-text-embedding`, `qwen3-rerank`. No tier-ladder action (not wired into Claude Code/Hermes per chat-only policy).

## Pool drift check 2026-08-22 (revisit criterion triggered)
Live `/v1/models` check confirmed all 4 tier models live and surfaced newcomers `ZHIPU/GLM-5.3`, `kimi-k3` (+ `kimi/kimi-k3` route), `deepseek-v4-pro-0813`, `deepseek-v4-flash-0731` — plus two kimi-k2.7-code channel flaps. Outcome (user decision): the tier swap above, and Hermes alias sync — dead aliases (`deepseek-v4-pro`, `deepseek-v4-pro[1m]`, `deepseek-v4-flash[1m]`) pruned, the four newcomers added, Hermes default → `deepseek-v4-flash-0731`. Current alias truth lives in `~/.hermes/config.yaml` ([[kb-0017]]).

## Subagent fleet routing (2026-08-22)
Custom agents in `~/.claude/agents/` (chezmoi-managed source: `~/dotfiles/dot_claude/agents/`): haiku (qwen3.8-flash since 2026-08-27) = Explore, kb-researcher, test-runner; **opus (glm-5.2) = tech-writer, product-owner, researcher** (writing/specs/docs don't need top reasoning; this finally uses the opus slot for its stated "sub-agents" role); `inherit` (= session model, fable/qwen3.8-max-0902) = all engineer agents (software, data, devops, security, qa, uiux) + code-reviewer. No `CLAUDE_CODE_SUBAGENT_MODEL` set.

## Auto-mode safety classifier
The Bash safety classifier runs on the sonnet tier with no independent override — a flapping sonnet-tier channel freezes all non-read-only Bash in auto mode. Details + mitigations: [[kb-0067]].

## Related
- [[kb-0017]] — Hermes + ai-box.vn proxy quirks, full model pool and alias names
- [[kb-0009]] — agent tools on this box and how Claude Code is wired
- [[kb-0067]] — safety classifier runs on the sonnet tier (operational consequence of this mapping)
