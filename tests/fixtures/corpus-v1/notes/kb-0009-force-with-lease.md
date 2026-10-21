---
id: kb-0009
title: "git push --force-with-lease refuses to overwrite unseen commits"
type: gotcha
domain: git
tags: []
aliases: []
triggers:
  - "when working on git topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-10
updated: 2026-08-10
supersedes: []
related: []
source: "synthetic fixture"
summary: "git push --force-with-lease refuses to overwrite unseen commits"
---
--force-with-lease fails the push if the remote ref moved since your last fetch.

Plain --force silently clobbers a teammate's push; lease makes the clobber explicit.
It still overwrites — just only the commits you have actually seen.
