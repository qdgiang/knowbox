---
id: kb-0022
title: "Silent Hermes Telegram bot: check the log trap first, then suspect a hung polling loop"
type: gotcha
domain: tooling
tags: [hermes, telegram, debugging]
aliases: [hermes bot silent, hermes telegram hung, hermes polling loop]
triggers:
  - "hermes telegram bot does not respond in a group"
  - "a group mention produced zero inbound log lines"
status: verified
confidence: high
created: 2026-08-05
updated: 2026-08-19
supersedes: []
related: [kb-0017, kb-0021]
source: "session 2026-08-04/05: live debugging + telegram adapter code read"
summary: "Hermes Telegram silence triage: adapter strips @mentions before logging; zero inbound log lines after a tag = hung polling loop → docker restart hermes. Bot API from this host needs the fallback IP."
---
When the Hermes Telegram bot goes silent in a group, triage in this order — the two failure signatures are easy to confuse ([[kb-0021]] for auth semantics):

**Log trap first:** the adapter strips the bot @mention from message text (`_clean_bot_trigger_text`) BEFORE logging and dispatch, so tagged messages appear UNTAGGED in `~/.hermes/logs/gateway.log` inbound lines and session transcripts. Never use log text to judge mention-gating; the `'[Replying to: ...]'` quoted context retains the original mention and is the reliable evidence.

**Zero inbound lines after a tag = hung polling loop, not auth.** Observed 2026-08-04: tag sent, no inbound line anywhere while the gateway process stayed alive (housekeeping mem-trim logs kept appearing); the update queued at Telegram and flushed 2s after a fresh polling connection from `docker restart hermes`, then processed normally. `getChatMember` showed plain `member` and mentions delivered fine after restart — BotFather group privacy was not the blocker.

**Third branch — Telegram privacy mode:** a bot that is a regular `member` of a group (not administrator) operates in privacy mode: it only receives `/commands`, replies to its own messages, and service messages — plain @mentions are NEVER delivered, giving the same zero-log-line signature. Diagnose with `getChatMember` (fallback-IP curl below): `"status":"member"` confirms it. Two fixes: (1) promote the bot to group admin — admins always receive all messages, no specific rights needed; or (2) BotFather `/setprivacy` → Disable, then remove and re-add the bot (privacy changes only take effect on re-add). Once it sees all group messages, set `require_mention: true` in the Hermes config or it will answer every group message.

Known upstream adapter bugs: #63309 (gateway hangs on "Connecting attempt 1/8" with no further output) and #66006 (fallback-IP discovery stall when DNS is blocked). Telegram reachability from this network is flaky (httpx.ReadError; DoH fallback-IP discovery; fallback IP 149.154.166.110).

**Runbook:** check `gateway.log` for new `inbound message` lines; none after a tag → `docker restart hermes`. Bot API calls from this host need the fallback IP, e.g.:
`curl --resolve api.telegram.org:443:149.154.166.110 https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/getChatMember?...`
Bot identity: id=8937443080.

## Related
- [[kb-0021]] — auth semantics to rule out before blaming the polling loop
- [[kb-0017]] — hermes deployment + log locations
