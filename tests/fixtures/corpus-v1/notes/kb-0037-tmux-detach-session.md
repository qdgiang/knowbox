---
id: kb-0037
title: "tmux detach keeps the session alive server-side"
type: fact
domain: terminal
tags: []
aliases: []
triggers:
  - "when working on terminal topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-18
updated: 2026-08-18
supersedes: []
related: []
source: "synthetic fixture"
summary: "tmux detach keeps the session alive server-side"
---
tmux detach (prefix d) leaves the session running; tmux attach resumes it.

Sessions survive logout because the server process is detached from the tty.
tmux ls lists live sessions; attach -t picks one by name.
