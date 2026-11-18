---
id: kb-0006
title: "pytest fixture scope decides how often setup runs"
type: howto
domain: python
tags: []
aliases: []
triggers:
  - "when working on python topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-07
updated: 2026-08-07
supersedes: []
related: []
source: "synthetic fixture"
summary: "pytest fixture scope decides how often setup runs"
---
pytest fixture scope=function reruns setup per test; scope=session runs it once per run.

Session-scoped fixtures must stay read-only or tests leak state into each other.
tmp_path_factory is the session-scoped counterpart of tmp_path.
