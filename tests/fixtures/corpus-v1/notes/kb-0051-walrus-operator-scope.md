---
id: kb-0051
title: "The walrus operator binds in comprehensions without leaking the loop var"
type: fact
domain: python
tags: []
aliases: []
triggers:
  - "when working on python topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-12
updated: 2026-08-12
supersedes: []
related: []
source: "synthetic fixture"
summary: "The walrus operator binds in comprehensions without leaking the loop var"
---
The walrus operator (:=) assigns inside expressions, including comprehension filters.

In comprehensions the walrus target binds in the enclosing scope by design.
