---
id: kb-0025
title: "A tokenizer that glues paths makes path-part queries miss"
type: gotcha
domain: search
tags: []
aliases: []
triggers:
  - "when working on search topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-06
updated: 2026-08-06
supersedes: []
related: []
source: "synthetic fixture"
summary: "A tokenizer that glues paths makes path-part queries miss"
---
If the tokenizer indexes ~/.claude/settings.json as one token, querying settings.json finds nothing.

A second, splitting tokenizer (default unicode61) indexes the parts; query both and merge.
This is the dual-tokenizer pattern: exact compounds AND their parts are searchable.
