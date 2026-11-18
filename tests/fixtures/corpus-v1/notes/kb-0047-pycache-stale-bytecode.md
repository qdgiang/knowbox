---
id: kb-0047
title: "Stale __pycache__ rarely bites, but zip-safe eggs and mounts change mtime"
type: gotcha
domain: python
tags: []
aliases: []
triggers:
  - "when working on python topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-08
updated: 2026-08-08
supersedes: []
related: []
source: "synthetic fixture"
summary: "Stale __pycache__ rarely bites, but zip-safe eggs and mounts change mtime"
---
Python invalidates bytecode by source mtime+size; clock skew across mounts defeats it.

Symptom: edits not taking effect — delete __pycache__ and retry.
PYTHONDONTWRITEBYTECODE=1 in containers avoids the layer bloat too.
