---
id: kb-0012
title: "logind suspends on lid close unless HandleLidSwitch is changed"
type: gotcha
domain: linux
tags: []
aliases: []
triggers:
  - "when working on linux topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-13
updated: 2026-08-13
supersedes: []
related: []
source: "synthetic fixture"
summary: "logind suspends on lid close unless HandleLidSwitch is changed"
---
systemd-logind suspends a laptop on lid close by default via HandleLidSwitch=suspend.

Set HandleLidSwitch=ignore in logind.conf for headless lid-closed operation.
Docked and on-external-power variants are separate keys: HandleLidSwitchExternalPower.
