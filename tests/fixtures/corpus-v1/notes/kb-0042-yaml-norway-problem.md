---
id: kb-0042
title: "YAML 1.1 parses bare no as boolean false — the Norway problem"
type: gotcha
domain: yaml
tags: []
aliases: []
triggers:
  - "when working on yaml topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-03
updated: 2026-08-03
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "YAML 1.1 parses bare no as boolean false — the Norway problem"
---
In YAML 1.1, country: NO parses as false — quote two-letter codes.

yes/no/on/off are all booleans unquoted; YAML 1.2 dropped them but parsers lag.
Octal-looking values like 0755 also surprise: quote or use 0o755 explicitly.
