---
id: kb-0008
title: "git rebase --onto moves a branch stack without replaying everything"
type: howto
domain: git
tags: []
aliases: []
triggers:
  - "when working on git topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-09
updated: 2026-08-09
supersedes: []
related: []
source: "synthetic fixture"
summary: "git rebase --onto moves a branch stack without replaying everything"
---
git rebase --onto NEW OLD BRANCH replays commits after OLD onto NEW.

This is the fix when a feature branch was cut from another feature branch.
The old branch tip is untouched; only BRANCH moves.
