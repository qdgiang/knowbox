---
id: kb-0029
title: "Large reference docs get a section-map companion note"
type: howto
domain: docs
tags: []
aliases: []
triggers:
  - "when working on docs topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-10
updated: 2026-08-10
supersedes: []
related: []
source: "synthetic fixture"
summary: "Large reference docs get a section-map companion note"
---
Keep large docs whole and give each a companion note mapping its sections.

Agents grep the map, then read one section with an offset — never the whole file.
A doc without a map is flagged so the gap is visible.
