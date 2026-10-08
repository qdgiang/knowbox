---
id: kb-0059
title: "Docker Compose teardown traps — a bare down leaves profile-gated services running, and an unset ${VAR:?} blocks every command including down"
type: gotcha
domain: tooling
tags: [docker,compose,profiles,interpolation,env-vars]
aliases: [compose down leaves containers, required variable missing a value, VAR:? blocks down]
triggers:
  - "docker compose down leaves containers running or a network undeletable"
  - "compose fails with "required variable X is missing a value""
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0007, kb-0017]
source: "raw/session-distill-2026-08-16.md"
summary: "Compose teardown traps: bare down leaves profile-gated services running (e2e profiles listed), and unset ${VAR:?} blocks every command incl. down — grep recipe included."
---
Two compose teardown traps hit on fedora-server's e2e-data-ai-project on 2026-08-16.

**Profiles**: `docker compose down` only manages services whose `profiles:` list is currently enabled, and the enabled set is not remembered from `up` — profile-gated containers keep running after a bare `down`, and the shared network then refuses deletion ("Resource is still in use"). This stack's profiles (root docker-compose.yml with `include:` fragments): default = postgres/minio/dagster-daemon/dagster-webserver/minio-bootstrap; `governance` = opensearch/openmetadata/marquez; `lakehouse` = nessie/polaris/spark; `ai` = ollama/qdrant/ai_agents. Seen live: bare `down` removed the 5 core containers but left spark-master/worker, nessie, polaris plus 2 exited bootstrap containers; `--profile governance down` did nothing (the fragments carry `profiles: ["lakehouse"]`); `--profile lakehouse down` removed them, after which the `platform` network finally deleted. Rule: bring down with the same profiles you brought up with, or sweep with all profiles; add `-v` to purge volumes.

**Required interpolation**: compose validates `${VAR:?msg}` for *every* subcommand, so a variable missing from `.env` makes even `down` fail before any container is touched — a running stack can appear impossible to tear down. Incident: `AGENT_API_KEYS` required at `ai_agents/compose.yaml:44` but added after `.env` was last touched (compose Aug 15 vs .env Aug 7); running containers predated it. Fixes: append the dev default from `.env.example:40` (`AGENT_API_KEYS={"tenant_a":"dev-insecure-agent-key"}`), or pass inline: `AGENT_API_KEYS='...' docker compose down`. Find every required var: `grep -rn ':\${.*:?}' --include=compose.y*ml .`.

## Related
- [[kb-0007]] — e2e-data-ai-project map: the stack, profiles, and .env layout these traps live in
- [[kb-0017]] — Hermes also runs in Docker on this box; the same compose mechanics apply
