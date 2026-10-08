---
id: kb-0007
title: "e2e-data-ai-project: monorepo map — read refs/e2e-data-ai-project.md before working on it"
type: reference
domain: projects
tags: [e2e-data-ai-project, dagster, duckdb, iceberg, ollama, qdrant]
aliases: [data platform project, lakehouse project]
triggers:
  - "any work in ~/projects/e2e-data-ai-project"
  - "dagster / duckdb / iceberg / ollama / qdrant on this machine"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-19
supersedes: []
related: [kb-0002]
source: "refs/e2e-data-ai-project.md (living session-to-session KB for the project)"
summary: "Section map for the e2e data+AI platform doc (Dagster/DuckDB/Iceberg lakehouse monorepo). Three always-needed facts: venv MUST be Python 3.12; git commits need the one-off -c identity; Postgres superuser is platform."
---
The full living knowledge doc is **`refs/e2e-data-ai-project.md`** (repo: `~/projects/e2e-data-ai-project`). Read it before working on the project — do not re-derive its "load-bearing facts". Three facts you need even for a quick drive-by:

- **Local venv MUST be Python 3.12** (`uv venv --python 3.12 --clear`). System Python is 3.14, which breaks dagster/pydantic.
- **Git identity is NOT configured in the repo** — commit with the one-off identity from the doc; do NOT modify git config; commit/push only when explicitly asked.
- **Postgres superuser role is `platform`, not `postgres`** (`POSTGRES_USER=platform` in `.env.example`) — `psql -U postgres` fails with `FATAL: role "postgres" does not exist`. Container-local check, no password needed: `docker compose exec -T postgres psql -U platform -Atc "select datname from pg_database order by 1"`; interactive equivalent: `task psql`.

## Section map of refs/e2e-data-ai-project.md

- **Header** — stack overview: Docker Compose + Dagster + DuckDB + Iceberg (Nessie + Polaris + Spark) + OpenMetadata/Marquez + Ollama/Qdrant/LangGraph; shared kernel `shared_libs/platform_core`; in-repo tracker `docs/IMPLEMENTATION_PLAN.md`
- **Current state** — phases done 0–7 (CI green); remediation P0+P1 DONE 2026-08-07 (uncommitted); next = Phase 8 model_training/ as its own Dagster code location (dec. 14), artifacts to `models` bucket; recent commit SHAs
- **Review findings (2026-08-07)** — six-persona review: confirmed issues, P0/P1 fixed + verified, P2 still open
- **Load-bearing facts › Toolchain** — `task` + `uv` on PATH; Python 3.12 venv rule; .env gitignored, .env.example is the template
- **Load-bearing facts › Git** — one-off commit identity, no config changes
- **Load-bearing facts › Lakehouse gotchas** — Polaris bootstrap admin-tool quirk; Spark cluster-mode --add-opens + JAR chmod 644; DuckDB ATTACH auth per catalog; catalog abstraction files
- **Load-bearing facts › AI tier gotchas** — ollama-pull one-shot container; Qdrant has no curl (bash /dev/tcp probe); `docker compose wait` output parsing; mxbai-embed-large = 1024-dim; pinned image versions
- **Load-bearing facts › Text-to-SQL gotchas** — queries Postgres appdb (RLS) not DuckDB; guardrails scan full response; DI keys are concrete classes; mypy py.typed; pytest prepend mode
- **Load-bearing facts › RAG gotchas** — qdrant-client 1.18 `query_points` (search removed); UUID5 source+index chunk IDs + evict-then-upsert; batched embeds + num_ctx; rag_ingest asset triggers over HTTP
- **Shared backing services** — one PG18 hosts 6 DBs; OpenMetadata shares opensearch stack; MinIO loopback-bound, rotated root password, buckets raw+warehouse+models
- **Verify cheatsheet** — task lint/typecheck/test, smoke-lakehouse, smoke-ai, materialize, agent curl commands

## Related
- [[kb-0002]] — installs via dnf only (applies to this box)
