---
id: kb-0011
title: "git bisect run automates the search for a breaking commit"
type: howto
domain: git
tags: []
aliases: []
triggers:
  - "when working on git topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-12
updated: 2026-08-12
supersedes: []
related: []
source: "synthetic fixture"
summary: "git bisect run automates the search for a breaking commit"
---
git bisect run <script> binary-searches history with the script's exit code as verdict.

Exit 0 = good, 1 = bad, 125 = skip (untestable commit like a broken build).
Start with git bisect start, mark known bad and good endpoints first.
