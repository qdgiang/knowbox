---
id: kb-0015
title: "sudo tty_tickets confines the sudo grace period to one terminal"
type: gotcha
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-16
updated: 2026-08-16
supersedes: []
related: []
source: "synthetic fixture"
summary: "sudo tty_tickets confines the sudo grace period to one terminal"
---
With tty_tickets (default), sudo's timestamp is per-tty — a second terminal re-prompts.

Disabling tty_tickets makes the grace period global across sessions.
timestamp_timeout=0 makes sudo prompt every time.
