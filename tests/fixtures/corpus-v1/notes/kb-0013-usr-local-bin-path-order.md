---
id: kb-0013
title: "/usr/local/bin precedes /usr/bin in the default PATH"
type: fact
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-14
updated: 2026-08-14
supersedes: []
related: []
source: "synthetic fixture"
summary: "/usr/local/bin precedes /usr/bin in the default PATH"
---
The default PATH puts /usr/local/bin before /usr/bin, so local installs shadow system ones.

which -a shows every match in resolution order.
Package managers own /usr/bin; /usr/local is the admin's namespace — no package writes there.
