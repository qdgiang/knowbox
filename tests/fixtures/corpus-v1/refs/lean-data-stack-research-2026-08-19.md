---
status: active
date: 2026-08-19
slug: lean-data-stack-research
project: ~/projects/lean-mds
origin: deep-research workflow wf_44619fe5-f8d (105 agents, 23 sources fetched, 103 claims extracted, 25 verified, 22 confirmed / 3 refuted)
---

# Lean data stack research — replacing Spark/Airflow/MinIO

## Goal
Design a modern lean data stack (Docker/docker-compose, single server) to replace the
heavyweight big-data stack (Spark, Airflow, MinIO). Target: fedora-server, headless
Fedora 44 laptop, i5-9300H (4C/8T). Candidates: Prefect, Dagster, dlt, dbt, DuckDB.

## TL;DR recommendation
- **Core (strongly supported by practitioner evidence):** dlt (ingestion) + DuckDB
  (warehouse) + dbt-core with dbt-duckdb adapter (transformation). This exact pattern
  is documented running on laptops and a Raspberry Pi in 3 containers.
- **Orchestration:** leanest verified option = Prefect server + always-on worker
  (note: Prefect has no daemon — a dead worker silently kills schedules; alert on
  "Late" runs). Most featured = Dagster official single-host compose, at the cost of
  4–5 long-running containers + one fresh container per run. Given "lean", lean
  toward Prefect or even plain cron/`serve()` initially.
- **Storage/serving:** local disk + Parquet replaces MinIO. DuckDB is single-writer:
  serialize dlt loads and dbt runs (orchestrator must not overlap them); BI readers
  use `access_mode='READ_ONLY'`.
- **Exit path:** DuckDB can write Iceberg tables → swap in Spark later without
  changing the rest of the stack. Consensus rule: DuckDB over Spark for anything
  under ~1 TB that fits on one machine.

## Verified findings (adversarially verified, 3-vote)

1. **dlt + DuckDB + dbt pattern is proven at this scale** (high confidence, 5
   independent implementations):
   - Rasmus Engelbrecht (Sep 2025): dlt + DuckDB + dbt + Evidence.dev, one `./run.sh`,
     no orchestrator, no Docker — https://rasmusengelbrecht.substack.com/p/my-local-data-stack-duckdb-dlt-dbt
     (repo: github.com/rasmusengelbrecht/localdatastack)
   - dltHub demo: dlt + dbt-duckdb + local DuckDB dev / MotherDuck prod —
     https://dlthub.com/blog/dlt-motherduck-demo
   - The Data Institute (Oct 2025): Dagster + dlt + dbt + DuckDB + Superset +
     Parquet-on-S3 in **3 Docker containers**, demoed on a Raspberry Pi —
     https://www.datainstitute.io/en/blog/data-stack-in-a-drawer
   - cnstlungu/portable-data-stack-dagster: generate→stage→model→analyze
     (Python→Parquet, Dagster+DuckDB staging, dbt, Superset) —
     https://github.com/cnstlungu/portable-data-stack-dagster
   - steleios (Jul 2026): Airflow + Parquet + dbt Core + DuckDB + Superset on a
     laptop — https://steleios.com/blog/anatomy-of-a-bi-pipeline-airflow-dbt-duckdb-superset

2. **DuckDB concurrency model is THE design constraint** (high confidence):
   in-process read-write = exactly one process; multiple processes only in
   `READ_ONLY` mode. Multi-process writes only via **Quack** (client-server protocol),
   still beta at v1.5.2, maturing ~DuckDB v2.0 (fall 2026).
   Sources: https://duckdb.org/docs/current/connect/concurrency.html, https://duckdb.org/quack/

3. **Prefect acquired Dagster Labs** (announced 2026-07-13, high confidence):
   ~40 Dagster staff joining; commitments: no product/licensing/operational changes
   for Dagster and Dagster+, OSS license unchanged, "no plans to merge Dagster into
   Prefect". Commitments are temporally qualified (advance notice for future changes).
   Both major Python orchestrators now under one roof → multi-year corporate
   concentration risk either way.
   Sources: https://www.prefect.io/prefect-acquires-dagster, https://dagster.io/blog/prefect-is-acquiring-dagster

4. **Dagster+ pricing** (high confidence): Solo/Starter moved to usage-based pricing
   2026-05-01 ($10/mo + $0.04/credit Solo; $100/mo + $0.035/credit Starter).
   Irrelevant for pure OSS self-hosting, but relevant if Dagster+ was ever considered.

5. **Dagster Docker footprint** (high confidence): minimal compose = webserver +
   daemon + Postgres + one gRPC user-code server per code location (≥4 long-running
   containers) + a fresh container per run (docker.sock run launcher). Official
   example targets single-host (e.g. one EC2):
   https://docs.dagster.io/deployment/oss/deployment-options/docker,
   https://github.com/dagster-io/dagster/blob/master/examples/deploy_docker

6. **Prefect has no daemon** (high confidence): the server only creates Scheduled
   runs; an always-on worker or long-lived `serve()` process must exist or scheduled
   runs silently never execute (marked "Late", no error, no retry). Exception: Cloud
   push work pools (not applicable self-hosted). Design implication: compose needs
   server + worker + monitoring on Late runs.
   Sources: https://docs.prefect.io/v3/concepts/schedules, https://blog.pmunhoz.com/blog/dbt/dbt-orchestration/dbt-orchestration-tools-comparison/

7. **dlt vs Airbyte** (high confidence): dlt = pip-installable Python library, no
   always-on services, runs serverless; self-hosted Airbyte = full platform
   (abctl/K8s-style, documented minimums ~4 CPU / 8 GB RAM, plain docker-compose
   deprecated Aug 2024). dltHub's own TPC-H benchmark (~6.42 GB / 24.6M rows): dlt
   w/ ConnectorX+PyArrow beat Sling/Airbyte 2.8–6x (vendor-published, Nov 2024, dated).
   Sources: https://www.datafold.com/blog/the-modern-data-stack-open-source-edition/, https://dlthub.com/blog/self-hosted-tools-benchmarking

8. **SQLMesh status** (medium confidence): Fivetran acquired Tobiko Data
   (2025-09-03); development dropped ~87% (22 → 2.5 commits/wk), effectively dormant
   by early 2026; donated to Linux Foundation 2026-03-25 under open community
   governance; activity partially recovered since May 2026 (~5–10 commits/wk).
   Fivetran simultaneously owns dbt Labs (merged Oct 2025) → conflict-of-interest
   question. **dbt remains the lower-risk transformation choice today.**
   Also: dbt Core rewrite "dbt Fusion" (Rust, Elastic License 2.0 — source-available,
   not OSI-open-source) announced 2025-05-28; dbt Core receives only bug fixes /
   security / compatibility updates going forward (tobikodata.com/blog/dbt-fusion-death-of-dbt-core).

9. **Sizing rule + exit path** (high confidence): "DuckDB over Spark for anything
   under a terabyte" (Datafold OSS MDS guide, Apr 2026); in-process engines run
   inside an orchestrator task with no DevOps, unlike Spark/Trino. Exit: DuckDB
   writes Iceberg → swap compute engine, keep the stack (The Data Institute).

## Refuted claims — DO NOT CITE
- That cnstlungu's repo is a single docker-compose.yml with each tool in its own
  container (0-3 — actual repo structure differs).
- steleios's "33 dbt models / 35 tests medallion warehouse" detail (1-2).
- pmunhoz blog's "Dagster footprint smaller than Airflow" comparison (1-2).

## Implementation gotchas
- **dlt's dbt runner is incompatible with dbt-core ≥ 1.9** (dlt issue #2226, breaks
  on removed dbt.logger). dlt docs recommend pinning dbt-core in a separate venv —
  or just invoke dbt directly from the orchestrator instead of the dlt runner.
- All "runs on a laptop/Raspberry Pi" demos used small datasets (Jaffle Shop etc.) —
  proven for small workloads only.
- No measured RAM/CPU footprint numbers exist for these stacks on i5-9300H-class
  hardware; only container counts. Airbyte 4-CPU/8GB and Airflow ~4GB minimums are
  docs figures, not measurements.

## Caveats
- Vendor-sourced evidence: dlt-vs-Airbyte benchmark (dltHub), MotherDuck demo
  (dltHub blog), Data Institute post (first-party + dltHub amplification), Datafold
  recommendations (vendor blog, though promoting no Datafold product).
- Prefect–Dagster acquisition is ~5 weeks old; "no changes" commitment carries
  unquantifiable corporate risk over 2–3 years.
- Quack is explicitly beta; not production for concurrent writers yet.
- Two findings passed 2-1 rather than 3-0 (Prefect no-change commitment; SQLMesh
  commit drop — though the verifier independently reproduced the GitHub stats).

## Open questions
1. Measured RAM/CPU footprints of candidate orchestrator stacks on this hardware —
   needs a local benchmark (none found on the web).
2. Will Quack be production-ready by DuckDB v2.0 (fall 2026), and should serving use
   it or stay on READ_ONLY connections / Parquet exports?
3. Post-acquisition: will Prefect keep Dagster OSS licensing/feature parity intact
   beyond announced commitments?
4. Does SQLMesh's partial recovery under LF governance make it viable again, or does
   Fivetran owning both dbt Labs and (de facto) SQLMesh make it structurally risky?

## Proposed compose shape (synthesized from findings — not yet benchmarked)
- Option A (leanest, recommended starting point): 1 container = Python env
  (dlt + dbt-core + dbt-duckdb via uv), Prefect server + worker containers, DuckDB
  file + Parquet on a shared volume, optional Superset container for BI (read-only
  mode). Serialize dlt/dbt writes in the DAG.
- Option B (more featured): official Dagster single-host compose (webserver, daemon,
  Postgres, user-code gRPC) + dlt/dbt as run containers. ~5+ idle containers.
- Option C (most minimal): no orchestrator — cron + one container with dlt+dbt
  (Engelbrecht pattern); add orchestration when schedules/retries/UI are needed.

## Next steps
1. Decide orchestrator option (A/B/C) — footprint benchmark on this server if needed.
2. Scaffold docker-compose in ~/projects/lean-mds with DuckDB single-writer
   serialization and Parquet storage layout.
3. Pin dbt-core < 1.9 if using dlt's dbt runner; otherwise invoke dbt directly.
4. Mark this file `done` once the stack is built.

## Paths
- Workflow output: /tmp/claude-1000/-home-giang-projects-lean-mds/58d13fdd-8e61-41a6-a34a-6a224d4a6cbe/tasks/wwqbb2kqw.output
- Per-agent journal: /home/giang/.claude/projects/-home-giang-projects-lean-mds/58d13fdd-8e61-41a6-a34a-6a224d4a6cbe/subagents/workflows/wf_44619fe5-f8d/journal.jsonl
