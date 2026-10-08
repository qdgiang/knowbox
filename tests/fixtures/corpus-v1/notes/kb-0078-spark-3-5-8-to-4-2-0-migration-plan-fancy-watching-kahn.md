---
id: kb-0078
title: "Spark 3.5.8 → 4.1.3 lakehouse migration (plan fancy-watching-kahn; pivoted from 4.2.0)"
type: project
domain: projects
tags: [spark, lakehouse, migration, e2e-data-ai-project]
aliases: [fancy-watching-kahn]
status: active
confidence: high
created: 2026-08-26
updated: 2026-08-26
supersedes: []
related: []
source: ""
summary: "Pointer to the Spark lakehouse migration plan (fancy-watching-kahn): 4.2.0 attempted, blocked upstream, pivoted to 4.1.3 and verified live 2026-08-26; carries the 4.2 blockers + follow-up trigger."
---
**State: done 2026-08-26 — pivoted 4.2.0 → 4.1.3, migrated + verified live + final gates green; committed (`c9b02bf`, DataFlint restoration `f0facf5`).**

- Plan file: `~/.claude/plans/fancy-watching-kahn.md` (project: `~/projects/e2e-data-ai-project`).
- Outcome of the 4.2.0 attempt (both blockers reproduced live): NO Iceberg runtime for 4.2 until 1.12 (the 4.1 runtime hard-fails at catalog LOAD — Spark 4.2 changed `connector.catalog.View` interface→class, IncompatibleClassChangeError), and DataFlint 0.9.9 crashes at SparkContext init on Spark 4.2's UI (it dropped the shaded Jetty that 4.1 ships in spark-core). User approved the plan's fallback Option A: **Spark 4.1.3**.
- Done + live-verified on 4.1.3: image `apache/spark:4.1.3-scala2.13-java17-python3-ubuntu` (tag `spark:4.1-iceberg`), Iceberg runtime 4.1 channel NATIVE, `hadoop-aws:3.4.2` + `software.amazon.awssdk:bundle:2.29.52` (SDK v2), `SparkSubmitResource.spark_version=4.1.3`, ANSI on, add-opens kept. DataFlint artifact `dataflint-spark4_2.13` baked in and ENABLED (kill-switched for one day on a wrong Jetty assumption, then restored after bytecode + live verification — only Spark 4.2 breaks DataFlint 0.9.9; switches stay wired as `DATAFLINT_ENABLED` / `dataflint_enabled`). Gates: smoke-lakehouse PASS (Nessie + Polaris), materialize SUCCESS, parity 131/60/58, DataFlint tab HTTP 200 on both submission paths. Sail re-parity skipped (user: not needed now).
- Follow-up trigger: Iceberg 1.12 ships `iceberg-spark-runtime-4.2_2.13` AND DataFlint publishes a Spark-4.2-compatible release (4.2's UI breaks 0.9.9) → FROM bump + `ICEBERG_RUNTIME_CHANNEL=4.2` + Hadoop/SDK re-derive + smoke/materialize gate.
- Final gates: ruff check/mypy/compose-config green, 414 unit tests passed, code review no blockers (two LOW findings fixed in-place); 3 pre-existing unformatted files left untouched (out of scope). Docs (ARCHITECTURE.md, IMPLEMENTATION_PLAN.md addendum) + KB refs updated. Committed: `c9b02bf` (migration) + `f0facf5` (DataFlint restoration).

## Related
- `~/kb/refs/e2e-data-ai-project.md` — canonical project KB (load-bearing lakehouse facts); gets updated by plan step 6.
- [[kb-0076]] — Sail engine experiment facts.
