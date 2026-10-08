---
id: kb-0051
title: "Write-denied subagent without Bash cannot persist files — have it return content as its final message; Notification hooks must exit 0"
type: gotcha
domain: tooling
tags: [claude-code,subagent,permissions,hooks,task-notification,recovery]
aliases: [write-denied subagent, return-content recovery, notification hook fail-soft]
triggers:
  - "subagent Write tool denied"
  - "task notification content HTML escaped"
  - "Notification hook hangs session"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0027, kb-0034]
source: "raw/session-distill-2026-08-06.md"
summary: "Bash-less subagent with Write denied: recover by telling it to return content as its final message; task notifications HTML-escape relayed text; Notification hooks must exit 0 fail-soft."
---
A subagent whose toolset excludes Bash (e.g. product-owner: Read, Grep, Glob, Write, Edit) has no alternate way to create a file once its Write tool is denied — even when the parent session can write the same path. Recovery (observed 2026-08-06): the parent used SendMessage to instruct the subagent to return the complete file content raw as its final message if Write was denied again; the parent then persists the content itself. Note the parent's own Write can also be denied, so be prepared to relay the content as text.

Two relay/hook gotchas surfaced in the same incident:

1. Task-notification/TaskOutput HTML-escapes relayed content (e.g. `&lt;` instead of `<`) — unescape before writing the returned content to a file.
2. Any script behind a Claude Code `Notification` hook must always exit 0 and stay fail-soft: wrap network calls in `timeout 5 ... || true` so a denied or failed send never hangs the session.

Rule of thumb: before spawning a writer subagent, check its toolset — if it has no Bash, its only egress when Write is denied is its final message, so plan the handoff up front.

## Related
- [[kb-0027]] — the subagent fleet whose role toolsets (product-owner, tech-writer) define which agents are Bash-less and thus affected
- [[kb-0034]] — tg-send/Telegram delivery: exactly the kind of network call a Notification hook must fail-soft around
