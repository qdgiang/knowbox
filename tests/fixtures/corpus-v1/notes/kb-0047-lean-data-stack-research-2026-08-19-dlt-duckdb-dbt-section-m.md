---
id: kb-0047
title: "Lean data stack research 2026-08-19 (dlt + DuckDB + dbt): section map for refs/lean-data-stack-research-2026-08-19.md"
type: reference
domain: projects
tags: [lean-mds,data-stack,duckdb,dlt,dbt,orchestration]
aliases: [lean data stack, replacing spark airflow minio, duckdb stack, prefect dagster choice]
triggers:
  - "working on ~/projects/lean-mds or the lean data stack"
  - "choosing between Prefect, Dagster, dlt, dbt, DuckDB, Airbyte, SQLMesh"
  - "asked about DuckDB concurrency or single-writer constraints"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0007, kb-0043]
source: "session: deep-research workflow wf_44619fe5-f8d 2026-08-19 (105 agents, 23 sources, 22 confirmed / 3 refuted)"
summary: "Section map: lean data stack to replace Spark/Airflow/MinIO on fedora-server — dlt+DuckDB+dbt-core core, Prefect-vs-Dagster-vs-cron orchestration, DuckDB single-writer constraint, Iceberg exit path."
---
Deep-research design for a lean Docker data stack replacing the heavyweight Spark/Airflow/MinIO stack (target: fedora-server, project `~/projects/lean-mds`) — consult `refs/lean-data-stack-research-2026-08-19.md` BY SECTION via the map below. ⚠ The live working copy is the handoff `~/reports/handoffs/2026-08-19-lean-data-stack-research.md` (status active until the stack is built); the refs/ copy is a 2026-08-19 snapshot — if they disagree, the handoff is newer.

## Section map

1. Goal + TL;DR — core = dlt (ingest) + DuckDB (warehouse) + dbt-core/dbt-duckdb (transform), proven down to Raspberry Pi scale; orchestration = Prefect server+worker (leanest) or Dagster compose (featured) or plain cron; local disk + Parquet replaces MinIO; DuckDB→Iceberg exit path keeps options open.
2. Verified findings 1–9 (3-vote adversarial) — (1) dlt+DuckDB+dbt proven at laptop scale, 5 independent implementations; (2) DuckDB concurrency: exactly one read-write process, others READ_ONLY; multi-writer only via beta Quack (~v2.0, fall 2026); (3) Prefect acquired Dagster Labs 2026-07-13, no-change commitments temporally qualified; (4) Dagster+ usage-based pricing since 2026-05-01; (5) Dagster Docker footprint ≥4 long-running containers + one per run; (6) Prefect has NO daemon — dead worker silently kills schedules, alert on "Late" runs; (7) dlt vs Airbyte: pip library vs ≥4CPU/8GB platform, dlt 2.8–6× faster (vendor benchmark); (8) SQLMesh effectively dormant post-Fivetran acquisition, LF-donated, dbt still lower-risk — plus dbt Fusion rewrite leaves dbt Core bug-fix-only; (9) sizing rule: DuckDB over Spark under ~1 TB on one machine.
3. Refuted claims — DO NOT CITE: cnstlungu repo layout, steleios model counts, pmunhoz Dagster-vs-Airflow footprint comparison.
4. Implementation gotchas — dlt's dbt runner breaks on dbt-core ≥1.9 (pin or invoke dbt directly); all demos used toy datasets; no measured footprint numbers for this hardware class.
5. Caveats — vendor-sourced evidence flagged; acquisition risk; Quack beta; two findings passed 2–1.
6. Open questions — footprint benchmark on this server; Quack readiness; post-acquisition Dagster OSS parity; SQLMesh viability.
7. Proposed compose shapes — Option A leanest (Prefect + 1 Python env container, recommended start), Option B Dagster official, Option C cron-only (Engelbrecht pattern).
8. Next steps — choose orchestrator option, scaffold compose in ~/projects/lean-mds with single-writer serialization, pin dbt-core if using dlt runner, mark handoff done when built.

## Related
- [[kb-0007]] — e2e-data-ai-project runs the heavyweight stack (Dagster/Spark/Iceberg) this research aims to replace at smaller scale
- [[kb-0043]] — Iceberg deep-dive curriculum; relevant to the DuckDB→Iceberg exit path
