---
id: kb-0038
title: "tmux clipboard needs OSC52 or set-clipboard for remote copies"
type: gotcha
domain: terminal
tags: []
aliases: []
triggers:
  - "when working on terminal topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: []
source: "synthetic fixture"
summary: "tmux clipboard needs OSC52 or set-clipboard for remote copies"
---
Copying inside a remote tmux needs OSC52 pass-through or set-clipboard on.

Without it, mouse copy lands in the terminal's buffer, not the remote clipboard.
Some terminals cap OSC52 payload size; large selections silently truncate.
