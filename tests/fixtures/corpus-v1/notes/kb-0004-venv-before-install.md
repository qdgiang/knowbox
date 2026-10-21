---
id: kb-0004
title: "Create the virtualenv before installing anything into it"
type: fact
domain: python
tags: []
aliases: []
triggers:
  - "when working on python topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-05
updated: 2026-08-05
supersedes: []
related: []
source: "synthetic fixture"
summary: "Create the virtualenv before installing anything into it"
---
A venv only isolates packages installed after its creation.

uv venv then uv pip install keeps the system interpreter untouched.
Activating a venv only rewrites PATH; installs go by which pip runs.
