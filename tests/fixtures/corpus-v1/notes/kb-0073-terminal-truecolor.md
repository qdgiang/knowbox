---
id: kb-0073
title: "Truecolor terminals advertise via COLORTERM=truecolor"
type: fact
domain: terminal
tags: []
aliases: []
triggers:
  - "when working on terminal topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-14
updated: 2026-08-14
supersedes: []
related: []
source: "synthetic fixture"
summary: "Truecolor terminals advertise via COLORTERM=truecolor"
---
COLORTERM=truecolor or 24bit signals 24-bit color support to TUI apps.

Falling back to 256-color palettes mangles themes designed for truecolor.
tmux needs terminal-overrides for RGB to pass through.
