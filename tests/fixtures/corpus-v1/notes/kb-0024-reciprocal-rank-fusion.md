---
id: kb-0024
title: "Reciprocal rank fusion merges rankings with 1/(k+rank)"
type: howto
domain: search
tags: []
aliases: []
triggers:
  - "when working on search topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-05
updated: 2026-08-05
supersedes: []
related: []
source: "synthetic fixture"
summary: "Reciprocal rank fusion merges rankings with 1/(k+rank)"
---
RRF scores each document as the sum of 1/(k+rank) across rankers, k=60 canonical.

It needs no score calibration — only rank order — which is why it suits hybrid search.
Fixed k beats adaptive weighting; adaptive RRF was refuted in benchmarks.
