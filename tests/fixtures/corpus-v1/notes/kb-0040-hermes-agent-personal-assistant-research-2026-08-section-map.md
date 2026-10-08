---
id: kb-0040
title: "Hermes Agent personal-assistant research (2026-08): section map for refs/hermes-personal-assistant-research.md"
type: reference
domain: tooling
tags: [hermes, cron, telegram, slack, automation]
aliases: [hermes use cases, hermes workflows, hermes community patterns]
triggers:
  - "choosing what to delegate to the Hermes gateway (cron jobs, digests, reminders)"
  - "wiring Hermes channels (Telegram home channel, Slack Socket Mode)"
  - "Hermes cron/scheduler behavior or delivery questions"
status: active
confidence: high
created: 2026-08-14
updated: 2026-08-14
supersedes: []
related: [kb-0017, kb-0022, kb-0025]
source: "session: deep-research workflow 2026-08-14 (106 agents, 24 sources, 3-vote adversarial verification)"
summary: "Section map: 2026-08 community research on Hermes as personal assistant — hosting, channels, cron workflows, coding delegation, 7 verified gotchas. Read refs/hermes-personal-assistant-research.md by section."
---
Community-verified map of how people run Hermes Agent as a 24/7 personal assistant — consult `refs/hermes-personal-assistant-research.md` BY SECTION via the map below, never whole. All claims passed 3-vote adversarial verification unless flagged; ⚠ fast-moving project — re-verify specifics before relying on them.

## Section map

1. Summary — TL;DR: self-hosted daemon, chat-app-first, cron is the workhorse.
2. Hosting patterns — Pi/VPS/old laptop/Termux/k8s; hermes-homelab kit (pinned Compose + SearXNG); loopback-only hardening pattern.
3. Channels — Telegram home channel (`/sethome`, TELEGRAM_HOME_CHANNEL, threaded DMs), Slack Bolt+Socket Mode DM-driven model, multi-platform shared sessions.
4. Recurring workflows people actually delegate — cron mechanics (60s tick, jobs.json, deliver targets) + 8 confirmed real examples (inbox→Slack, HN digest, dev digest, zero-LLM email pipeline, `[SILENT]` prefix).
5. Coding delegation pattern — Telegram → Hermes spawns Claude Code session → returns result (bundled skill).
6. Community gotchas — 7 verified: context compression drops constraints ~30 turns, memory char limits, weak-model skill degradation, approval-gate violations (#17619), sudo no-TTY (NOPASSWD/CLI), config.yaml bind-mount write failures (#5214 + follow-ons), "start with one boring workflow" advice.
7. Caveats and refuted claims — vendor-marketing flags + 2 claims killed by verification.
8. Open questions — memory value over time, email triage reliability, token costs, security of chat-reachable shell access.
9. Sources — primary docs/issues, Reddit/HN threads, blog-tier corroboration.

## Related
- [[kb-0017]] — our Hermes deployment; gotcha §6 matches the config-rewrite warning in project CLAUDE.md
- [[kb-0022]] — Telegram bot silence triage; this note adds home-channel + group-auth context
- [[kb-0025]] — Hermes built-in memory; gotchas §6.2 (char limits) qualify its long-term value
