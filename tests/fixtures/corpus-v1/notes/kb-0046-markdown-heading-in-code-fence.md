---
id: kb-0046
title: "Headings inside code fences are not headings"
type: gotcha
domain: markdown
tags: []
aliases: []
triggers:
  - "when working on markdown topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-07
updated: 2026-08-07
supersedes: []
related: []
source: "synthetic fixture"
summary: "Headings inside code fences are not headings"
---
A ## line inside a fenced code block is literal text, not a section heading.

Parsers that ignore fences index phantom sections — fence-awareness matters.
This fixture's refs test pins exactly that behavior.
