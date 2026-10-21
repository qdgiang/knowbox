---
id: kb-0072
title: "Pipes inside markdown table cells need escaping"
type: fact
domain: markdown
tags: []
aliases: []
triggers:
  - "when working on markdown topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-13
updated: 2026-08-13
supersedes: []
related: []
source: "synthetic fixture"
summary: "Pipes inside markdown table cells need escaping"
---
A literal pipe in a table cell must be \| or the column count breaks.

Code spans inside tables still need the pipe escaped.
HTML tables are the escape hatch for genuinely tabular prose.
