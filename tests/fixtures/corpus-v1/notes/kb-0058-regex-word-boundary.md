---
id: kb-0058
title: "\b matches between word and non-word characters"
type: fact
domain: regex
tags: []
aliases: []
triggers:
  - "when working on regex topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: []
source: "synthetic fixture"
summary: "\b matches between word and non-word characters"
---
The regex \b is zero-width: it matches at word/non-word transitions.

\bcat\b misses category but matches 'a cat'; unicode word chars vary by engine.
In URLs and paths, hyphens are boundaries — \b often surprises there.
