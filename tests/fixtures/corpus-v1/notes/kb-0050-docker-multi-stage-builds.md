---
id: kb-0050
title: "Multi-stage builds keep build tools out of the runtime image"
type: howto
domain: docker
tags: []
aliases: []
triggers:
  - "when working on docker topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-11
updated: 2026-08-11
supersedes: []
related: []
source: "synthetic fixture"
summary: "Multi-stage builds keep build tools out of the runtime image"
---
A multi-stage Dockerfile compiles in a builder stage and COPYs artifacts into a slim final stage.

The final image carries no compiler, no package cache, no source.
Name stages (AS builder) so COPY --from=builder stays readable.
