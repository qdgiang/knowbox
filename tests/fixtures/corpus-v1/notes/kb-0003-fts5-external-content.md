---
id: kb-0003
title: "FTS5 external-content tables must be rebuilt after bulk changes"
type: howto
domain: sqlite
tags: []
aliases: []
triggers:
  - "when working on sqlite topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-04
updated: 2026-08-04
supersedes: []
related: []
source: "synthetic fixture"
summary: "FTS5 external-content tables must be rebuilt after bulk changes"
---
An external-content FTS5 table does not track the source table; rebuild it after bulk edits.

INSERT INTO ft(ft) VALUES('rebuild') is the canonical rebuild.
Contentless delete tables trade a small index for not storing the text twice.
