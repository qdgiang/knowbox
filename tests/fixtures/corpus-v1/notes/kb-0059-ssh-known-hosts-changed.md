---
id: kb-0059
title: "CHANGED host key means rebuild or MITM — verify out of band"
type: gotcha
domain: ssh
tags: []
aliases: []
triggers:
  - "when working on ssh topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-20
updated: 2026-08-20
supersedes: []
related: []
source: "synthetic fixture"
summary: "CHANGED host key means rebuild or MITM — verify out of band"
---
A REMOTE HOST IDENTIFICATION HAS CHANGED warning means the key differs from known_hosts.

After a legitimate OS reinstall, remove the stale line; otherwise investigate.
ssh-keygen -R host removes the entry without hand-editing the file.
