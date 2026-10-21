---
id: kb-0005
title: "Python default arguments are evaluated once at def time"
type: gotcha
domain: python
tags: []
aliases: []
triggers:
  - "when working on python topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-06
updated: 2026-08-06
supersedes: []
related: []
source: "synthetic fixture"
summary: "Python default arguments are evaluated once at def time"
---
def f(x=[]) shares one list across all calls — the classic mutable-default trap.

Use None as the sentinel and create the list inside the function body.
The same trap applies to dict and set defaults, and to datetime.now() defaults.
