---
id: kb-0056
title: "429 responses carry Retry-After — honor it"
type: gotcha
domain: http
tags: []
aliases: []
triggers:
  - "when working on http topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-17
updated: 2026-08-17
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "429 responses carry Retry-After — honor it"
---
HTTP 429 means slow down; Retry-After says for how long, in seconds or a date.

Retrying without backoff on 429 turns throttling into an outage.
Exponential backoff with jitter handles the absence of the header.
