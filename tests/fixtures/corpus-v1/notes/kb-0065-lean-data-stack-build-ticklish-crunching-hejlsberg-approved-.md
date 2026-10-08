---
id: kb-0065
title: "Lean data stack build (ticklish-crunching-hejlsberg): approved, starting P0"
type: project
domain: projects
tags: [lean-mds, duckdb, dlt, dbt, prefect, dagster, docker]
aliases: [ticklish-crunching-hejlsberg, lean-mds-build]
status: active
confidence: high
created: 2026-08-21
updated: 2026-08-24
supersedes: []
related: [kb-0047, kb-0007, kb-0059]
source: "plan ~/.claude/plans/ticklish-crunching-hejlsberg.md; research kb-0047"
summary: "Pointer note for the approved lean data stack build plan in ~/projects/lean-mds — phases P0–P5, both Prefect and Dagster profile-gated for comparison."
---
**State: DONE 2026-08-21 — stack built and e2e-verified from a clean slate.
Commits: P0 68aaad6, P1 fbf21a8, P2 cfc0eb0, P3 c7d9581, P4 6e24a51, P5 ff40a86.
Research handoff marked done. Repo docs: docs/ARCHITECTURE.md, CLAUDE.md.**

P2 gotchas (Prefect): server binds 127.0.0.1 by default → other containers
ConnectError until `--host 0.0.0.0` (+PREFECT_SERVER_API_HOST); `prefect-version`
in prefect.yaml must be a STRING; deployment with `pull: []` records path=None →
flow crashes loading code — use `storage: {type: local, basepath: /app}`;
pip-installed mds_pipeline breaks the parents[3] dbt_project derivation →
`MDS_DBT_PROJECT_DIR` env override (set in prefect Dockerfile, reuse for dagster);
an idle readonly duckdb session holds a shared lock and blocks writers →
duckdb-shell now profile-gated (`tools`).

P3 gotchas (Dagster): postgres:18 mounts at /var/lib/postgresql (not .../data);
CLI is `dagster-daemon run -w workspace.yaml` (no `dagster daemon`, no pyproject
fallback); DockerRunLauncher needs `image:` in dagster.yaml (DAGSTER_CURRENT_IMAGE
ignored); default multiprocess executor runs independent assets in parallel →
DuckDB lock clash — use in-process executor via job run-config (no max_concurrent
key there).

P4 gotchas (Superset): 6.x image runs from a pip-LESS uv venv /app/.venv →
install drivers with /app/docker/pip-install.sh (plain pip lands in a python
Superset never imports); duckdb-engine ignores URI query params → read-only via
`extra` = {"engine_params":{"connect_args":{"read_only":true}}}; mutating REST
calls need an X-CSRFToken from /api/v1/security/csrf_token/.

Deviation: countries source is **World Bank API**, not REST Countries (v3.1
deprecated in 2026; v5 requires an API key). Gotchas absorbed: uv workspace needs
`--all-packages` (bare root sync skips members) + venv on PATH in images; dbt flags
go AFTER the command; dbt-duckdb needs explicit `_ENV.close()` release
(mds_pipeline.runner.transform); REST Countries → World Bank; dlt state dir via
DLT_DATA_DIR for test isolation.

## Plan
- File: `~/.claude/plans/ticklish-crunching-hejlsberg.md` (canonical; subagent draft
  `ticklish-crunching-hejlsberg-agent-a8b267832ca9fb8bd.md` alongside).
- Repo: `~/projects/lean-mds` (was empty 2026-08-20).

## Scope
Replace the heavyweight stack ([[kb-0007]] e2e-data-ai-project, still running — do not
touch, avoid its ports) with a lean Docker-compose stack per [[kb-0047]] research:
dlt 1.30.0 + DuckDB 1.5.5 (pin everywhere) + dbt-core 1.12.2/dbt-duckdb 1.11.0.
User decisions: BOTH Prefect 3.8.3 and Dagster 1.13.18, each behind its own compose
profile (compare in practice); Superset 6.1.0 profile-gated; demo pipeline = REST
Countries + Frankfurter FX via dlt, synthetic fallback mode; structure mirrors the old
repo (root compose `include:` of per-component fragments, root .env contract,
Taskfile, uv-only). Hard rules: DuckDB single-writer (serialize runs; never both
orchestrator profiles up), dbt invoked via dbtRunner not dlt's runner (dlt #2226),
kb-0059 compose hygiene (no `${VAR:?}`, down with all profiles + --remove-orphans).

Phases: P0 scaffold → P1 core pipeline → P2 Prefect → P3 Dagster → P4 Superset →
P5 docs/final e2e.

## Next step
None — build complete. **Review 2026-08-22** (dual audit, findings verified):
applied `task up` prefect⊕dagster guard, dbt telemetry off, `.env` 600 +
COMPOSE_PROFILES=prefect,superset. Accepted risks: UIs stay on 0.0.0.0
unauthenticated (trusted network, user decision); docker.sock. Open gaps:
**no volume backup** (highest), real sources, Late-run alerting. Gotcha found
while fixing: [[kb-0068]] (go-task built-in shell).
**2026-08-23/24:** Dagster verified operationally (found + fixed: run-container
sweep label was baked into a STALE image — rebuilt, re-verified labeled);
dbt switched to **2.0.0b2 (Fusion)**, dbt-duckdb dropped — see [[kb-0069]];
comparison now informed: Prefect lean/schedule-proven, Dagster heavier but
queue-native.

## Related
- [[kb-0047]] — research findings + ref report driving this build
- [[kb-0007]] — old heavyweight stack being replaced (structure template + port map)
- [[kb-0059]] — compose teardown traps encoded in the plan's Taskfile rules
