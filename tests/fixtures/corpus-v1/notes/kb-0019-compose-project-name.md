---
id: kb-0019
title: "Compose prefixes every resource with the project name"
type: fact
domain: docker
tags: []
aliases: []
triggers:
  - "when working on docker topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-20
updated: 2026-08-20
supersedes: []
related: []
source: "synthetic fixture"
summary: "Compose prefixes every resource with the project name"
---
docker compose namespaces containers, networks, and volumes as <project>_<name>.

The project name defaults to the directory name; -p or COMPOSE_PROJECT_NAME overrides.
Two checkouts of one repo collide unless the project name differs.
