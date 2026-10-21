---
id: kb-0016
title: "consoleblank blanks the virtual console after N seconds"
type: fact
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-17
updated: 2026-08-17
supersedes: []
related: []
source: "synthetic fixture"
summary: "consoleblank blanks the virtual console after N seconds"
---
The consoleblank kernel parameter blanks text consoles after the given seconds; 0 disables.

It only affects virtual consoles, not graphical sessions.
setterm -blank N does the same from userspace for the current console.
