---
id: kb-0001
title: "SQLite WAL mode keeps readers unblocked during writes"
type: fact
domain: sqlite
tags: []
aliases: []
triggers:
  - "when working on sqlite topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-02
updated: 2026-08-02
supersedes: []
related: []
source: "synthetic fixture"
summary: "SQLite WAL mode keeps readers unblocked during writes"
---
WAL mode appends to a -wal file so readers never block behind a writer.

Checkpointing folds the wal back into the main db; it runs automatically every 1000 pages.
Readers see the last checkpointed state, so long reads can stall checkpointing.
