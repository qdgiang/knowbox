---
id: kb-0023
title: "BM25 saturates term frequency — repetition stops helping"
type: fact
domain: search
tags: []
aliases: []
triggers:
  - "when working on search topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-04
updated: 2026-08-04
supersedes: []
related: []
source: "synthetic fixture"
summary: "BM25 saturates term frequency — repetition stops helping"
---
BM25's k1 parameter saturates term frequency: the 10th repetition adds almost nothing.

Document-length normalization (b) keeps long documents from dominating.
IDF means rare query terms dominate ranking over common ones.
