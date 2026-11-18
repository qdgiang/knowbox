---
id: kb-0028
title: "Mutating a session-scoped fixture leaks state between tests"
type: gotcha
domain: testing
tags: []
aliases: []
triggers:
  - "when working on testing topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-09
updated: 2026-08-09
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "Mutating a session-scoped fixture leaks state between tests"
---
A session-scoped fixture shared by tests must be treated as read-only.

If a test must mutate, take a copy or drop to function scope.
Order-dependent passes are the symptom; pytest -p no:randomly helps diagnose.
