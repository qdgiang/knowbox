---
id: kb-0002
title: "FTS5 bm25() returns negative scores — more negative is better"
type: gotcha
domain: sqlite
tags: []
aliases: []
triggers:
  - "when working on sqlite topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-03
updated: 2026-08-03
supersedes: []
related: []
source: "synthetic fixture"
summary: "FTS5 bm25() returns negative scores — more negative is better"
---
In SQLite FTS5, bm25() scores are negative and SMALLER (more negative) means a better match.

ORDER BY bm25(tbl) ascending gives best-first ranking; forgetting this inverts results.
Column weights via bm25(tbl, 10, 1) boost title-like columns over body columns.
