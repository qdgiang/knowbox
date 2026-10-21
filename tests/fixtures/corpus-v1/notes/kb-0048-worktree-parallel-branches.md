---
id: kb-0048
title: "git worktree checks out a second branch in a second directory"
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
summary: "git worktree checks out a second branch in a second directory"
---
git worktree add ../repo-feature feature gives a clean tree without stashing.

Each worktree has its own index and HEAD but shares the object store.
Remove with git worktree remove; prune cleans stale administrative files.
