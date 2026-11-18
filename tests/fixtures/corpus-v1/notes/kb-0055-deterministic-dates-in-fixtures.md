---
id: kb-0055
title: "Fixture dates must be absolute and fixed"
type: fact
domain: testing
tags: []
aliases: []
triggers:
  - "when working on testing topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-16
updated: 2026-08-16
supersedes: []
related: []
source: "synthetic fixture"
summary: "Fixture dates must be absolute and fixed"
---
Relative dates in fixtures (yesterday, last week) rot the fixture as time passes.

Absolute ISO dates keep goldens stable forever.
Age-based logic under test gets its dates injected, never from the clock.
