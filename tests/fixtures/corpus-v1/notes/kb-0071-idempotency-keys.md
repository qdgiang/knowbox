---
id: kb-0071
title: "Idempotency keys make retried POSTs safe"
type: howto
domain: http
tags: []
aliases: []
triggers:
  - "when working on http topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-12
updated: 2026-08-12
supersedes: []
related: []
source: "synthetic fixture"
summary: "Idempotency keys make retried POSTs safe"
---
An idempotency key lets a server deduplicate retried POST requests.

Clients generate a UUID per logical operation and resend it on retry.
Servers store the first response and replay it for the same key.
