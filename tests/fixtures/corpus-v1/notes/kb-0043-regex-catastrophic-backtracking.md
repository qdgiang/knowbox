---
id: kb-0043
title: "Nested quantifiers cause catastrophic regex backtracking"
type: howto
domain: regex
tags: []
aliases: []
triggers:
  - "when working on regex topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-04
updated: 2026-08-04
supersedes: []
related: []
source: "synthetic fixture"
summary: "Nested quantifiers cause catastrophic regex backtracking"
---
(a+)+ style nested quantifiers explode exponentially on near-miss input.

Possessive quantifiers or atomic groups cut the search tree.
RE2-style engines trade backreferences for linear-time guarantees.
