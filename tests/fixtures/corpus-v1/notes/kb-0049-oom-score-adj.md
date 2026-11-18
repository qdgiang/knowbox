---
id: kb-0049
title: "oom_score_adj biases which process the OOM killer picks"
type: fact
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-10
updated: 2026-08-10
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "oom_score_adj biases which process the OOM killer picks"
---
oom_score_adj (-1000..1000) shifts a process's OOM-kill priority; -1000 is immune.

The killer sums badness heuristics; the adjustment is a multiplier offset.
systemd units set it via OOMScoreAdjust= in the service file.
