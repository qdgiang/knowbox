---
id: kb-0021
title: "Hermes Telegram group auth: chat whitelist authorizes every sender; group_allow_from '*' was a security hole"
type: gotcha
domain: tooling
tags: [hermes, telegram, security]
aliases: [hermes telegram whitelist, group_allow_from, TELEGRAM_GROUP_ALLOWED_CHATS]
triggers:
  - "changing hermes telegram group or DM access"
  - "reviewing hermes bot authorization or security"
status: verified
confidence: high
created: 2026-08-05
updated: 2026-08-05
supersedes: []
related: [kb-0017, kb-0022]
source: "session 2026-08-04/05: adapter code (_apply_yaml_config) + gateway.log verification"
summary: "Hermes Telegram auth semantics: GROUP_ALLOWED_CHATS whitelists ALL senders of listed chats (additive-only); config group_allow_from:'*' bridged to a wildcard = remote agent access; removed 2026-08-05. Never reintroduce."
---
Telegram platform (enabled 2026-08-04): bot `@qdgiang_hermes_agent_bot`, outbound polling (no ports/webhook), secrets in `~/.hermes/.env` (`TELEGRAM_BOT_TOKEN`, `TELEGRAM_ALLOWED_USERS`, `TELEGRAM_GROUP_ALLOWED_CHATS`). **Auth semantics (verified in adapter code + logs 2026-08-05):**

- For groups, `TELEGRAM_GROUP_ALLOWED_CHATS` is the gate — EVERY sender of a listed chat is authorized (the gateway's early chat-ID check returns before any user-ID check). Current value: `-5160338333,-5112681027` (the two real groups).
- `TELEGRAM_ALLOWED_USERS` is platform-wide (DMs + any group the bot is in). Add DM users by editing it + restart.
- `TELEGRAM_GROUP_ALLOWED_USERS` adds group-sender IDs; `"*"` there authorizes anyone in ANY group the bot is in. All three controls are additive-only, never restrictive.

**SECURITY HOLE (found + removed 2026-08-05):** `platforms.telegram.group_allow_from: "*"` in config.yaml bridges to `TELEGRAM_GROUP_ALLOWED_USERS="*"` (adapter `_apply_yaml_config`, first-writer-wins), which made the bot serve ANY user in ANY group — a member was served in unlisted group -5112681027 although the chat whitelist held only -5160338333. The bot has terminal access to this box, so that wildcard = remote agent access for anyone who adds it (Telegram lets anyone add a bot to a group by default). Fix applied: `group_allow_from` removed entirely (adapter intake then defers to the gateway checks) + explicit `TELEGRAM_GROUP_ALLOWED_CHATS`. **Do NOT reintroduce `group_allow_from: "*"`.** Optional hardening: BotFather → /setjoingroups → Disable.

Correction of a 2026-08-04 note in [[kb-0017]]'s history: "the chat whitelist alone is NOT enough" was WRONG — the single "Blocked unauthorized user" line that inspired it came from a process started before `GROUP_ALLOWED_CHATS` was in the env. DMs remain gated by `TELEGRAM_ALLOWED_USERS` alone.

Also `require_mention: true` in config.yaml so in groups it answers only @mentions/replies (default false = it would answer EVERY group message; DMs unaffected) — verified enforced by observation 2026-08-04.

## Related
- [[kb-0017]] — hermes deployment this platform runs in
- [[kb-0022]] — failure modes/diagnostics when the bot goes silent
