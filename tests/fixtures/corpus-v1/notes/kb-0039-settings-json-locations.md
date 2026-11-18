---
id: kb-0039
title: "Claude Code reads settings from ~/.claude/settings.json then project files"
type: howto
domain: claude-code
tags: []
aliases: []
triggers:
  - "when working on claude-code topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-20
updated: 2026-08-20
supersedes: []
related: []
source: "synthetic fixture"
summary: "Claude Code reads settings from ~/.claude/settings.json then project files"
---
Claude Code settings resolve user-level (~/.claude/settings.json) before project-level.

Project .claude/settings.json is shared; settings.local.json is personal and git-ignored.
Precedence: local project > shared project > user.
