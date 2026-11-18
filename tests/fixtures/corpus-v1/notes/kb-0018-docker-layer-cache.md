---
id: kb-0018
title: "Docker layer cache invalidates at the first changed instruction"
type: howto
domain: docker
tags: []
aliases: []
triggers:
  - "when working on docker topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: []
source: "synthetic fixture"
summary: "Docker layer cache invalidates at the first changed instruction"
---
Every instruction after the first changed one rebuilds — order Dockerfile steps by change frequency.

COPY package files before source so dependency installs stay cached.
BuildKit cache mounts (RUN --mount=type=cache) survive invalidation for package downloads.
