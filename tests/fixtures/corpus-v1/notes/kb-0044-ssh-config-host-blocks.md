---
id: kb-0044
title: "ssh config Host blocks apply first-match-wins per option"
type: howto
domain: ssh
tags: []
aliases: []
triggers:
  - "when working on ssh topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-05
updated: 2026-08-05
supersedes: []
related: []
source: "synthetic fixture"
summary: "ssh config Host blocks apply first-match-wins per option"
---
In ssh config, the FIRST obtained value for each option wins — put specific Host blocks first.

Host * at the bottom carries defaults; a specific block above overrides per key.
Include directives splice other files in place.
