---
id: kb-0036
title: "Prefer the distribution package manager for system installs"
type: howto
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-17
updated: 2026-08-17
supersedes: []
related: []
source: "synthetic fixture"
summary: "Prefer the distribution package manager for system installs"
---
A dnf package install policy keeps the system reproducible: repo packages first.

Static binaries into /usr/local bypass updates and provenance checks.
When no package exists, a containerized deployment beats a host install.
