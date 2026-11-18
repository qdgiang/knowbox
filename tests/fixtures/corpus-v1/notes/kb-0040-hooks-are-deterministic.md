---
id: kb-0040
title: "Hooks fire on tool events deterministically, unlike instructions"
type: fact
domain: claude-code
tags: []
aliases: []
triggers:
  - "when working on claude-code topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-01
updated: 2026-08-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "Hooks fire on tool events deterministically, unlike instructions"
---
A hook configured in settings.json runs on every matching tool call — no model discretion.

Use hooks for invariants (always run tests after edits), prompts for guidance.
Hook output shown to the model steers the next action; keep it short.
