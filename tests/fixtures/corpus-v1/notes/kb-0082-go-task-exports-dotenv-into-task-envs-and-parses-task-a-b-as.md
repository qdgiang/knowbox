---
id: kb-0082
title: "go-task exports dotenv into task envs and parses 'task a b' as two tasks; compose CLI --profile overrides COMPOSE_PROFILES instead of merging"
type: gotcha
domain: tooling
tags: [go-task, docker-compose, env, lean-mds]
aliases: [task-dotenv-export, compose-profile-override]
status: active
confidence: high
created: 2026-08-27
updated: 2026-08-27
supersedes: []
related: [kb-0080, kb-0068, kb-0059]
source: "lean-mds quack pilot wiring, 2026-08-27"
summary: "Three silent env traps: go-task dotenv EXPORTS .env into every task's environment; 'task a b' runs two tasks (args need --); compose CLI --profile overrides COMPOSE_PROFILES instead of merging — merge into the env var."
---
Three env/precedence traps found while wiring the lean-mds quack pilot
(2026-08-27); all silent, all reproducible:

1. go-task `dotenv: [".env"]` EXPORTS the .env values into the ENVIRONMENT of
   every task command. Anything keyed in .env reaches subprocesses even when
   the Taskfile never references it (lean-mds: `task test` suddenly ran pytest
   in quack mode because WAREHOUSE_QUACK leaked in; fixed with an autouse
   conftest delenv, not by changing the Taskfile).
2. `task dbt build` parses as TWO tasks — `dbt`, then `build` (go-task treats
   multiple args as multiple tasks). To pass args to one task use `--`:
   `task dbt -- build`. A command with sensible defaults can mask this for
   months.
3. docker compose CLI `--profile` flags OVERRIDE `COMPOSE_PROFILES`; they do
   NOT merge. `docker compose --profile quack up -d` with
   COMPOSE_PROFILES=dagster,superset starts ONLY quack-profiled + ungated
   services. To combine, merge into the env var:
   `COMPOSE_PROFILES="dagster,superset,quack" docker compose up -d`.
   (kb-0059 covers the related teardown traps.)
