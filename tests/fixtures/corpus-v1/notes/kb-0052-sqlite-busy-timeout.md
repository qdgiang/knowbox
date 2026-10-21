---
id: kb-0052
title: "SQLITE_BUSY needs a busy_timeout, not retries"
type: gotcha
domain: sqlite
tags: []
aliases: []
triggers:
  - "when working on sqlite topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-13
updated: 2026-08-13
supersedes: []
related: []
source: "synthetic fixture"
summary: "SQLITE_BUSY needs a busy_timeout, not retries"
---
A locked SQLite database raises SQLITE_BUSY immediately unless busy_timeout is set.

Set PRAGMA busy_timeout=5000 or pass timeout= to connect; retry loops reinvent it badly.
WAL mode plus busy_timeout covers most single-writer contention.
