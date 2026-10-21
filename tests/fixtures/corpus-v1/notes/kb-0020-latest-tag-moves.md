---
id: kb-0020
title: "The latest tag is a moving pointer, not a version"
type: gotcha
domain: docker
tags: []
aliases: []
triggers:
  - "when working on docker topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-01
updated: 2026-08-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "The latest tag is a moving pointer, not a version"
---
image:latest is just the default tag — it moves whenever anyone pushes it.

Pin by digest (@sha256:…) for reproducible deploys.
docker pull is not automatic: a running container keeps its original image forever.
