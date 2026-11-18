---
id: kb-0017
title: "The docker group is root-equivalent on the host"
type: gotcha
domain: docker
tags: []
aliases: []
triggers:
  - "when working on docker topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-18
updated: 2026-08-18
supersedes: []
related: []
source: "synthetic fixture"
summary: "The docker group is root-equivalent on the host"
---
Docker group membership is root-equivalent: -v /:/host gives a container the whole filesystem.

There is no permission boundary between the docker socket and host root.
Rootless docker or sudo-only access avoids the silent escalation path.
