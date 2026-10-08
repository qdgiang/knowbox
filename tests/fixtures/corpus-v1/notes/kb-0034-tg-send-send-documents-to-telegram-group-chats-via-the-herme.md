---
id: kb-0034
title: "tg-send: send documents to Telegram group chats via the Hermes bot token"
type: howto
domain: tooling
tags: [telegram, tg-send, claude-code, skill]
aliases: [telegram send, tg-send]
triggers:
  - "sending a file or message to Telegram from a script or skill"
status: verified
confidence: high
created: 2026-08-09
updated: 2026-08-11
supersedes: []
related: [kb-0022, kb-0021, kb-0028]
source: "session 2026-08-09: built tg-send + telegram-send skill, live-verified by sending text + document to My Workspace group; 2026-08-11: workspace upgraded to supergroup (topics enabled), topics send verified"
summary: "tg-send (~/.local/bin) sends files/messages to any chat the Hermes bot is in: tg-send doc|msg, -c workspace(default)|agentic|gialai|dm|<id>, -t topic. Skill: telegram-send. 50 MB limit; fallback IP per kb-0022."
---
`tg-send` is a bash+curl CLI that posts documents/messages to the user's Telegram chats via the Bot API, reusing the Hermes bot token — no extra bot setup needed, and sending coexists fine with the Hermes gateway's polling.

**Usage** (source chezmoi-managed: `~/dotfiles/dot_local/bin/executable_tg-send`):

```bash
tg-send doc FILE [CAPTION...]   # sendDocument, 50 MB Bot API limit
tg-send msg "TEXT"              # sendMessage
# -c/--chat ALIAS|ID (default: workspace), --silent
```

**Chat registry** (aliases → ids; current truth also in `~/.hermes/channel_directory.json`):
workspace "My Workspace" `-1004410916467` (default; upgraded to supergroup 2026-08-11 when topics were enabled — old id `-5499295312` is dead; supergroup upgrades change the id permanently, and the API returns `migrate_to_chat_id` when you hit a dead one) · agentic "Anh em tao Agentic" `-5112681027` · gialai "Bốn Kats và nguời TQ + người Gia Lai" `-5160338333` · dm (Giang) `1894749960`.

**Scope:** tg-send is NOT limited to My Workspace — `workspace` is merely the default when `-c` is omitted. `-c` accepts any registered alias or any raw numeric chat id, so any group/DM the Hermes bot is a member of works (`-c <id>`); find unknown group ids in `~/.hermes/channel_directory.json`. `-t/--topic` likewise works with any target that is a forum (topics enabled).

**Forum topics (2026-08-11):** "My Workspace" has topics enabled (`is_forum`). `tg-send` supports `-t/--topic NAME|ID` (adds `message_thread_id`); alias registry in the script: `files` → thread 4. Discovering a new topic's thread id: there is no Bot API to list topics, but once any message arrives in the topic the Hermes gateway records it in `~/.hermes/sessions/sessions.json` under `origin.thread_id` — then add an alias to the script. Enabling topics is client-side only (no Bot API method); bots need the **Manage topics admin right** to `createForumTopic` — plain membership isn't enough.

**Mechanics:** token read from env `TELEGRAM_BOT_TOKEN` or `~/.config/dotfiles/secrets.env`; on any curl connection failure it retries once with `--resolve api.telegram.org:443:149.154.166.110` (the fallback-IP trick from [[kb-0022]]). Messages appear as sent by the Hermes bot (@qdgiang_hermes_agent_bot, id 8937443080).

**Claude Code skill** `telegram-send` (`~/dotfiles/dot_claude/skills/telegram-send/SKILL.md`) makes future sessions send on request; default destination workspace. New groups: read the id from `~/.hermes/channel_directory.json`, pass as raw id, optionally add an alias to the script.

## Related
- [[kb-0022]] — fallback-IP requirement for Bot API calls from this host; Hermes polling triage
- [[kb-0021]] — group auth semantics (relevant if the bot is added to new groups)
- [[kb-0028]] — chezmoi layout; script + skill live in ~/dotfiles, secrets stay in ~/.config/dotfiles/secrets.env
