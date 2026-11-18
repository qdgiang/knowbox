---
id: kb-0027
title: "Build throwaway sandboxes for CLI tests, never touch production data"
type: howto
domain: testing
tags: []
aliases: []
triggers:
  - "when working on testing topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-08
updated: 2026-08-08
supersedes: []
related: []
source: "synthetic fixture"
summary: "Build throwaway sandboxes for CLI tests, never touch production data"
---
CLI tests should build a synthetic sandbox per run and point the tool at it.

A hard guard that refuses to run against the real data directory is cheap insurance.
Deterministic fixture content makes goldens reproducible across machines.
