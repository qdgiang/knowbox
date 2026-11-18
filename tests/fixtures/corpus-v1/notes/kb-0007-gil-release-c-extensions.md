---
id: kb-0007
title: "C extensions can release the GIL; pure Python cannot"
type: fact
domain: python
tags: []
aliases: []
triggers:
  - "when working on python topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-08
updated: 2026-08-08
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "C extensions can release the GIL; pure Python cannot"
---
NumPy-style C extensions release the GIL around heavy loops, so threads help there.

Pure-Python threads never run bytecode in parallel — use processes for CPU work.
The GIL protects refcounts; free-threaded builds replace that with biased locking.
