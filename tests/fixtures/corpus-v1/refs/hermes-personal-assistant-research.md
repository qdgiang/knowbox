# How people use Hermes Agent as a personal assistant — community research report

> Deep-research synthesis (2026-08-14): 5 search angles → 24 sources → 105 claims
> extracted → 25 adversarially verified (3-vote panels) → 23 confirmed, 2 refuted.
> ⚠ Fast-moving project (adapters grew ~7 → 27+ in 14 months; most community posts
> Feb–Aug 2026). Specific gotchas may already be fixed in newer releases.

## Summary

People run hermes-agent as a **24/7 self-hosted daemon** and talk to it through
whatever messaging app they already live in. One gateway process serves 20+
platforms simultaneously (Telegram, Discord, Slack, WhatsApp, Signal, Email,
Matrix, Teams, SMS, Home Assistant, browser…), sharing one memory/session store —
the agent you DM from your phone is the same one that posts to your Slack. Access
is controlled via user-ID allowlists + DM pairing ("first user to message claims
access").

The workhorse is the **built-in cron scheduler**; the most-delegated work is
recurring digests and watchers delivered back to a designated "home channel".
Coding is commonly delegated by messaging the task to Hermes, which spawns a
Claude Code session and returns the result. Prevailing community advice: start
with one small, boringly-reliable workflow and grow from there.

## Hosting patterns

Cheap and boring wins (all 3-0 verified):

- Raspberry Pi builds (one user: Pi 4 8GB, 3 months daily use, "no crashes")
- $5–10/month VPSes, incl. Hetzner + Claude Opus via OpenRouter
- Old Ubuntu laptop under systemd with a custom watchdog
- Home Kubernetes clusters (single documented user)
- Android phones via official Termux docs (~6-year-old phone works)
- Community deployment kit `hermes-homelab` (github.com/naimarshad/hermes-homelab):
  pinned Docker Compose stack, healthcheck, non-root runtime, bind-mounted state
  dir, optional self-hosted SearXNG for keyless web search, Telegram as channel
- Security-hardening pattern: webhook/API bound to 127.0.0.1 only, all triggers
  via SSH tunnels rather than public chat channels

## Channels

**Telegram** — the favorite for personal use (verified 3-0): full-featured bot
from any device, automatic voice-memo transcription, group-chat support, and a
"home channel" (`/sethome` or `TELEGRAM_HOME_CHANNEL`) where cron/scheduled
results land. Community tip: enable threaded DMs via BotFather (`/topic`).
Caveats: transcription needs an STT provider; group chats default to Telegram
privacy mode (bot needs admin/re-add); issue #18934: /sethome loses forum-topic
thread IDs.

**Slack** — self-host-friendly (verified 3-0): official integration uses Bolt SDK
+ Socket Mode (outbound WebSocket — no public URL/endpoint exposed). DM-driven
behavior: replies to every DM without @mention; in channels only when @mentioned,
then continues in-thread. Cron delivery to home channel, channel ID, or
`slack:U0…` DMs (needs im:write). Bug note: #5861 silent delivery failures.

**Multi-platform** — one gateway, many adapters, shared session/memory; sessions
are scoped per chat origin.

## Recurring workflows people actually delegate

All via the built-in cron scheduler: 60-second tick inside the gateway, natural-
language job definitions, persist in `~/.hermes/cron/jobs.json`, `deliver:` target
per job (any platform, `origin`, or `all`). Operational limits: jobs only fire
while the gateway runs; cron sessions are stateless so prompts must be
self-contained; recursive scheduling off by default.

Confirmed real-world examples:

- Morning inbox summary → Slack, "every weekday at 9am" (Anthony Maio)
- Daily Hacker News / AI-news digests DM'd over Telegram (arshtechpro)
- Daily research briefings → Discord/Slack/Notion/email/Obsidian (@gkisokay)
- Two-tier email pipeline making zero LLM calls when the inbox is idle (@mayuronx)
- Developer digest at 8am: overnight GitHub issues, PRs in review >24h, HN
  mentions (official daily-briefing-bot guide default `0 8 * * *` → telegram)
- hermes-homelab starter pack: 5 cron jobs (rail-offer watcher + 4 daily news
  digests) with `--deliver` auto-delivery and `[SILENT]` prefix to suppress noise
- Approval-gated homelab sysadmin (HN): read-only MCP checks on Kubernetes +
  morning digest on backup status and pod health
- Second-hand market monitoring with email alerts; new model release tracking

## Coding delegation pattern

Send the task to Hermes over Telegram → it spawns a **Claude Code session** to do
the actual coding and returns the result (bundled first-party skill:
`skills/autonomous-ai-agents/claude-code/SKILL.md`, Mode 1 `claude -p` one-shot,
Mode 2 interactive in tmux). Hermes = orchestrator/dispatcher, Claude Code (or
Codex) = engine. Multiple r/hermesagent threads corroborate.

## Community gotchas (hard-won lessons, all 3-0 verified)

1. **Context compression silently drops constraints after ~30 turns** — widely
   complained about ("Hermes context compaction is plain terrible").
2. Memory entries hit hard character limits and get compressed/replaced.
3. Skills perform poorly on weaker models.
4. Behavioral self-audit found approval-gate violations in 112 of 129 sessions
   (GitHub issue #17619 — real but n=1).
5. **sudo cannot prompt through the gateway** (no interactive terminal) —
   chat-channel shell automation needs NOPASSWD or the CLI (`hermes chat`) instead.
   In the official FAQ's troubleshooting section.
6. **config.yaml mutations fail badly when the file is read-only/bind-mounted
   (typical Docker) or invalid YAML** — crash with traceback, silent
   non-persistence, or misreported-empty config. GitHub issue #5214 (bug/P2) with
   follow-ons #16480 (root-owned config.yaml in Docker), #5154, #45563, #42241,
   persisting through mid-2026.
7. Top community advice: **start with one small, boringly-reliable workflow** —
   don't try to automate inbox + research + social + life organization at once.

## Caveats and refuted claims

- "Self-improving / learning loop" is **vendor marketing framing**; real-world
  efficacy unverified by third parties; one community write-up questions whether
  "self-improving AI" is a real category.
- User-stories claims partly from vendor-curated page
  (hermes-agent.nousresearch.com/docs/user-stories); each cross-checked against
  Reddit/GitHub/independent blogs.
- **Refuted during verification (excluded)**: "runs anywhere from $5 VPS to
  GPU clusters or serverless (Daytona/Modal)" (1-2); "Telegram is *the most
  common* channel" (1-2 — popular, but the superlative wasn't supportable).
- Adoption scale: ~230k GitHub stars as of 2026-08-14 (star-history rank ~19).

## Open questions

- Does the learning loop/memory actually deliver a better user model over months,
  or do memory limits + compression cap its value?
- Email integration at scale: IMAP vs Gmail API details; is the zero-LLM two-tier
  pipeline reliable enough as primary triage?
- Real ongoing token/model costs for an always-on assistant; which cheaper models
  keep skills/cron quality acceptable?
- Is allowlist + DM-pairing secure enough for a shell-capable agent reachable from
  public platforms — any real abuse/prompt-injection incidents?
- Multi-user single-instance experience beyond documented mechanics.

## Sources

Primary: github.com/nousresearch/hermes-agent · hermes-agent.nousresearch.com/docs
(messaging, telegram, slack, cron, faq, user-stories, integrations) ·
github.com/NousResearch/hermes-agent/issues/{5214,17619,16480} ·
github.com/naimarshad/hermes-homelab · docs.slack.dev (Socket Mode)

Community/forum: reddit.com/r/hermesagent (use-cases megathread 1t6gf4j, "one
month with" 1t29ogw, "three months with" 1u8fm0t, best-platform 1t9zwk7,
self-host thread 1u8a2ny, LocalLLM 1t47ec0) · news.ycombinator.com/item?id=48419000
· github.com/aliaihub/awesome-hermes-usecases

Blog-tier (corroboration only): aibuilderclub.com · evomap.ai · aieatingtheworld.com ·
samellawrentz.com · lumadock.com · lucaberton.com · hermesagents.net · star-history.com
