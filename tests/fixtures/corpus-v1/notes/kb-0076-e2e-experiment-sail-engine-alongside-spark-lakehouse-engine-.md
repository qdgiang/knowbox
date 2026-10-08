---
id: kb-0076
title: "e2e experiment: Sail engine alongside Spark (LAKEHOUSE_ENGINE switch)"
type: project
domain: projects
tags: [e2e-data-ai-project, sail, lakehouse, experiment]
aliases: [sail-experiment, LAKEHOUSE_ENGINE]
status: active
confidence: high
created: 2026-08-25
updated: 2026-08-25
supersedes: []
related: [kb-0007]
source: "https://docs.lakesail.com/sail/latest/"
summary: "Pointer note: the COMPLETED experiment adding Sail (pysail 0.7.1) as a drop-in alternative lakehouse write engine next to Spark in e2e-data-ai-project, behind LAKEHOUSE_ENGINE=spark|sail — verified live both ways 2026-08-25, verdict recorded, stack reverted to spark as the active engine."
---
**State: done 2026-08-25 — experiment VERIFIED end to end; stack REVERTED to
`LAKEHOUSE_ENGINE=spark` (the verified default), sail kept as the
flip-a-var alternative (sail-server service defined, stopped by default).**
Live proof: `task smoke-lakehouse-sail` PASS on BOTH Nessie +
Polaris; `task materialize` SUCCESS under sail with DuckDB row-count parity
vs the Spark baseline (staging 131 / curated 60 / feature 58); swapped back
(`LAKEHOUSE_ENGINE=spark`): `task smoke-lakehouse` + `task materialize` both
SUCCESS; offline gates green (ruff/mypy, `task test` = 413 passed).
To re-activate sail: `LAKEHOUSE_ENGINE=sail` in root `.env` +
`docker compose --profile lakehouse up -d sail-server` + recreate the dagster
containers.
Plan/spec: `docs/IMPLEMENTATION_PLAN.md` "Experiment — Sail as an alternative
lakehouse engine (2026-08-25)".

**Design (locked):** `LAKEHOUSE_ENGINE=spark|sail` (root `.env`, default `spark`) —
same config-only pattern as `LAKEHOUSE_CATALOG=nessie|polaris`, orthogonal to it: both
engines write the SAME tables through the SAME active catalog; DuckDB read path
untouched. Sail (pysail 0.7.1) runs a Spark Connect server; the Sail jobs are CLIENTS
(`pyspark-client==4.2.0`, a new `sail` extra on data-pipeline). Catalogs are
server-STARTUP config in Sail (`SAIL_CATALOG__LIST` rendered by `sail/entrypoint.sh`
from the same catalog env the spark stack uses; Polaris needs an OAuth token fetched
at server start — Sail's iceberg-rest only accepts static tokens, ~1 h TTL caveat).

**Load-bearing Sail facts (researched, don't rediscover):**
- No official Docker image — build from `python:3.12-slim` + pip (pysail ships
  cp38-abi3 manylinux wheels; works on py3.10–3.14).
- Server: `sail spark server --ip 0.0.0.0 --port 50051`; config via `SAIL_*` env
  (TOML syntax, `.` → `__`), catalogs ONLY at startup.
- Iceberg: append/overwrite/CTAS supported (copy-on-write), V1/V2; NO row-level
  DELETE/MERGE/UPDATE (this stack uses none). `CREATE DATABASE` is the documented
  namespace-DDL spelling (Spark's `CREATE NAMESPACE` equivalent).
- S3/MinIO data plane via server env: `AWS_ACCESS_KEY_ID/SECRET`,
  `AWS_ENDPOINT`, `AWS_VIRTUAL_HOSTED_STYLE_REQUEST=false`, PLUS
  `AWS_ALLOW_HTTP=true` (LOAD-BEARING: Sail leaves allow_http off for
  `s3://` locations; object_store 0.13 defaults https-only → reqwest
  "URL scheme is not allowed") and an EXPLICIT `AWS_REGION` (empty region
  triggers bucket-region inference against REAL AWS S3).
- iceberg-rest catalog options: uri/warehouse/prefix/oauth_access_token/
  bearer_access_token(_file) — NO client-credentials flow → token fetched at
  startup by `sail/render_catalog.py`. Config merge: server defaults < Sail
  client config < server overrides.

**Live-run findings (don't rediscover; baked into `sail/`):**
- Nessie `/v1/config?warehouse=` 500s on anything but the configured
  warehouse NAME — pass the bare `ICEBERG_WAREHOUSE` value, like Spark.
- Nessie's `/v1/config` prefix (`main%7Cwarehouse`) gets DOUBLE-encoded by
  Sail's REST client → 400 on every namespaced call; pin `prefix="main"`
  (ref-only form) in the catalog entry.
- Sail CTAS needs `USING iceberg` (provider-less CTAS defaults to parquet →
  rejected by iceberg-rest); `CREATE OR REPLACE TABLE` unsupported → sail
  jobs emulate with DROP + CTAS.

**Files (all on main working tree, UNCOMMITTED):** `sail/{Dockerfile,entrypoint.sh,
render_catalog.py,compose.yaml}`, `sail/jobs/{sail_catalog,iceberg_smoke,
staging_orders,curated_orders_daily,feature_orders_daily}.py`;
`data_pipeline/.../resources/sail_run.py` + `LakehouseEngineResource` base in
`spark_submit.py` (Dagster rejects UNION resource annotations — a shared
ConfigurableResource base is the fix); assets take `lakehouse_engine` resource key;
settings `lakehouse_engine/sail_remote_url/sail_jobs_dir`; Taskfile
`smoke-lakehouse-sail`; root pyproject excludes `sail/` from ruff/mypy like `spark/`.

**Verdict — why (not) use Sail (evaluated 2026-08-25):** Sail WINS on
footprint + operational simplicity (one Rust process, no JVM heaps/tuning/
add-opens/event-log plumbing; submission collapses to `python job.py`) and on
optionality (jobs are plain pyspark-client code — the swap is a config var,
not a rewrite; the stack can ride an Arrow/DataFusion-native engine as it
matures). It does NOT win on capability yet: no measurable speed difference
at this stack's data volumes (131-row tables — both engines blink; Sail's
perf claims live at real scale, untested here), an observability regression
(no job UI/plans/history — server log only, `SAIL_RUST_LOG=debug` knob), and
0.7.1 maturity gaps surfaced live (no CREATE OR REPLACE, no row-level ops,
Nessie prefix double-encode, AWS_ALLOW_HTTP trap, static-token-only OAuth).
Conclusion: an OPTION + footprint play, not a capability upgrade — the stack
was therefore reverted to `LAKEHOUSE_ENGINE=spark` (the verified default)
with sail kept as the flip-a-var alternative. An honest A/B decision would
need a RAM benchmark (sail-server vs spark master+worker) + timed
materialize on a larger dataset — not yet run.

**Deferred leftovers (triggers if Sail work resumes):** `CREATE OR REPLACE
TABLE` upstream support (Sail issue — jobs use DROP+CTAS until then); Polaris
token TTL (~1 h) vs long-running sail-server; Sail-side observability
(Spark-History-Server/DataFlint are Spark-only).

## Related
- [[kb-0007]] — project map (this experiment touches its Lakehouse facts on success)
