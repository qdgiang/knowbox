---
id: kb-0014
title: "journalctl is volatile until Storage=persistent is set"
type: howto
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-15
updated: 2026-08-15
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "journalctl is volatile until Storage=persistent is set"
---
The systemd journal lives in /run (RAM) unless Storage=persistent in journald.conf.

With persistence, logs land in /var/log/journal and survive reboots.
journalctl --disk-usage and --vacuum-time=2weeks manage the footprint.
