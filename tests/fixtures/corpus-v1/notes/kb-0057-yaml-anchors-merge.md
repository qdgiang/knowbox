---
id: kb-0057
title: "YAML anchors and merge keys deduplicate mappings"
type: fact
domain: yaml
tags: []
aliases: []
triggers:
  - "when working on yaml topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-18
updated: 2026-08-18
supersedes: []
related: []
source: "synthetic fixture"
summary: "YAML anchors and merge keys deduplicate mappings"
---
YAML anchors (&x) and aliases (*x) reuse subtrees; <<: *x merges mappings.

Merge keys compose base configs; later keys override merged ones.
Aliases are references at parse time — no runtime identity is kept.
