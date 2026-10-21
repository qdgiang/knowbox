---
id: kb-0070
title: "Typo correction fires only on a unique edit-distance-1 candidate"
type: fact
domain: search
tags: []
aliases: []
triggers:
  - "when working on search topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-11
updated: 2026-08-11
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "Typo correction fires only on a unique edit-distance-1 candidate"
---
A conservative typo layer rewrites a query token only when exactly one vocabulary word is one edit away.

Ambiguous corrections are left alone for an honest miss.
The correction is announced in the output so hits are read critically.
