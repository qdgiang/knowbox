---
id: kb-0054
title: "git rerere records conflict resolutions and replays them"
type: fact
domain: git
tags: []
aliases: []
triggers:
  - "when working on git topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-15
updated: 2026-08-15
supersedes: []
related: []
source: "synthetic fixture"
summary: "git rerere records conflict resolutions and replays them"
---
git rerere remembers how you resolved a conflict and replays it on identical conflicts.

Long-lived parallel branches benefit most; enable with rerere.enabled=true.
The cache lives in .git/rr-cache and is local only.
