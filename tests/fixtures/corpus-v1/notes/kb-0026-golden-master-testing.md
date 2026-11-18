---
id: kb-0026
title: "Golden tests pin outputs byte-for-byte and regenerate explicitly"
type: fact
domain: testing
tags: []
aliases: []
triggers:
  - "when working on testing topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-07
updated: 2026-08-07
supersedes: []
related: []
source: "synthetic fixture"
summary: "Golden tests pin outputs byte-for-byte and regenerate explicitly"
---
Golden-master tests compare output to a committed expected file, byte for byte.

Regeneration must be an explicit, reviewable step — never auto-update on failure.
Pin ids and order, not scores or timestamps; those are environment-fragile.
