---
id: kb-0010
title: "A detached HEAD is a normal state, not data loss"
type: fact
domain: git
tags: []
aliases: []
triggers:
  - "when working on git topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-11
updated: 2026-08-11
supersedes: []
related: []
source: "synthetic fixture"
summary: "A detached HEAD is a normal state, not data loss"
---
Detached HEAD means HEAD points at a commit, not a branch — commits are safe.

Create a branch before leaving the commit and nothing is lost.
git reflog finds orphaned commits for ~90 days by default.
