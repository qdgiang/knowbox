---
id: kb-0053
title: "inotify watch limits break file watchers on big trees"
type: fact
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-14
updated: 2026-08-14
supersedes: []
related: []
source: "synthetic fixture"
summary: "inotify watch limits break file watchers on big trees"
---
fs.inotify.max_user_watches caps watched paths; big repos or monorepos hit it.

Symptoms are silent non-reloading dev servers, not errors.
Raise via sysctl; watch tools dedupe poorly by default.
