---
id: kb-0045
title: "Agent forwarding exposes your keys to the remote host's root"
type: fact
domain: ssh
tags: []
aliases: []
triggers:
  - "when working on ssh topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-06
updated: 2026-08-06
supersedes: []
related: []
source: "synthetic fixture"
summary: "Agent forwarding exposes your keys to the remote host's root"
---
With ForwardAgent, the remote root can use your agent for any key it holds.

ProxyJump gives the same convenience without exposing the agent socket.
Per-key confirm extensions make each use prompt locally.
