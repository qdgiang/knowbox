---
id: kb-0043
title: "Iceberg deep dive curriculum: section map for refs/iceberg-learning-curriculum.md"
type: reference
domain: projects
tags: [iceberg, blog-series, lakehouse, spark, duckdb]
aliases: [iceberg experiments, iceberg blog plan, iceberg-learning, iceberg deep dive]
triggers:
  - "working on the Iceberg deep-dive experiments or blog series"
  - "running something against the e2e lakehouse stack (Nessie/Polaris/Spark)"
  - "asked what Iceberg features this stack supports (DVs, time travel, branches, WAP)"
status: active
confidence: high
created: 2026-08-16
updated: 2026-09-04
supersedes: []
related: [kb-0007, kb-0047, kb-0074, kb-0075, kb-0083]
source: "/home/giang/projects/iceberg-learning/iceberg-interview-prep.md"
summary: "Section map: Iceberg deep-dive curriculum — live experiments on the e2e lakehouse (Spark 4.1.3 / Iceberg 1.11 / Nessie) published as blog posts; environment recipe, verified engine capabilities, experiment status."
---
Living curriculum for the Iceberg mastery project — full plan in `refs/iceberg-learning-curriculum.md`; consult BY SECTION via the map below, never whole. ⚠ The status table (§6) and live-verified findings (§5) move as the series progresses; treat specifics below as of 2026-08-24. Original execution plans: `~/.claude/plans/wise-drifting-panda*.md` — subsumed into this curriculum (the `-agent-` variant holds the engine capability fact-check that became §5).

## Section map

1. Purpose & ground rules — run every Iceberg mechanism live in `~/projects/iceberg-learning` against the e2e lakehouse, capture verbatim output, publish as English `draft: true` posts ("Iceberg Deep Dive") on personal-website.
2. Post style guidelines — voice/prose follows [[kb-0083]] (written for beginners, plain + smooth, no humor/drama/AI-isms); series rules: published + self-contained, no forward references, no machine-local pointers; output-display rules (user directives 2026-09-04): every listed output shows ALL columns and rows — never elide/hide, at most shorten over-long cell content; every displayed result must show the command that produced it, and that command must be READER-RUNNABLE — SQL, curl, standard CLI tools only; NEVER the experiment's private scripts (readers don't have them; e.g. replaced `uv run inspect_files.py` with curl SigV4 + xmllint/jq and a `uvx --with fastavro` one-liner). Post 1 refreshed 2026-09-01, output blocks restored verbatim from the logs and script blocks swapped for reader-runnable commands 2026-09-04; posts 2–4 not yet audited against the output-display rules.
3. Per-experiment layout — `experiments/NN-slug/` (run_experiment.py, inspect_files.py, output.log); post paths + required frontmatter; gotcha: frontmatter `date` must be QUOTED; validate via docker `npm run check`/`build`, never host node.
4. Environment facts — Spark 4.1.3 + Iceberg 1.11.0 (Spark-4 runtime channel) in compose profile `lakehouse` (Nessie/Polaris/MinIO; hive-metastore HMS 3.1.3 at thrift://hive-metastore:9083, warehouse s3://warehouse/hive — wiring gotchas [[kb-0074]] [[kb-0075]]); updated 2026-09-01 after the migration ([[kb-0078]]) incl. two Spark-4 deltas (snapshot summaries stamp iceberg-version 1.8.1 under BOTH catalogs; DESCRIBE on V2 shows base-five metadata columns, no `_row_id`); bring-up command list; spark-submit recipe with `LAKEHOUSE_CATALOG`; MinIO read only via S3 API in a uv venv; namespace `lakehouse.iceberg_lab`.
5. Engine capability fact-check — (1) DV writes need format-version=3 + merge-on-read modes; (2) Nessie REST hides table-level refs — rollback/branches/WAP fail on Nessie, live-verified exp 1–3 incl. the `spark.wap.branch` silently-ignored gotcha, orphaned-Parquet cleanup failure, and the Nessie v2 create-ref API shape; user directive (updated 2026-08-25): Nessie stays the PRIMARY catalog; Hive Metastore is the only comparison catalog — Polaris/Hadoop are out of experiment/post focus (they served only as one-off confirmation in 01b); ADDENDUM 2026-08-24 (exp 01b, `experiments/01b-catalog-snapshot-comparison/`): Nessie's one-snapshot `.snapshots` view is Nessie-specific — Polaris and HadoopCatalog both keep the full 2-snapshot chain + 2-entry snapshot-log after the same two INSERTs, and HadoopCatalog physically keeps v1–v3.metadata.json (blog claim CONFIRMED); ADDENDUM 2026-08-25 (exp 01c, `experiments/01c-hive-anatomy/`): HiveCatalog (HMS 3.1.3 + pinned iceberg-hive-metastore 1.8.1) behaves like Polaris/Hadoop — `.snapshots` keeps both snapshots with parent_id chain intact + 2-entry snapshot-log after the same two INSERTs; three physical `<NNNNN>-<uuid>.metadata.json` versions (00000–00002, under `<table>/metadata/`) accumulate with growing snapshot-log/metadata-log; HMS itself stores only the `metadata_location` pointer in TABLE_PARAMS and marks the table EXTERNAL_TABLE (though Spark DESCRIBE EXTENDED prints Type MANAGED); snapshot summaries stamp `iceberg-version: Apache Iceberg 1.8.1` (the pinned hive jar) vs Nessie's 1.11.0 — see [[kb-0075]]; HMS identifiers in postgres are quoted-UPPERCASE (DBS/TBLS/TABLE_PARAMS); docs check 2026-08-24: single-snapshot serving is DELIBERATE Nessie design (projectnessie.org/guides/iceberg-rest), code-verified on 0.108.2 — `00000-<uuid>` naming hardcoded, `metadata-log` deliberately emptied, `gc.enabled=false` stamped at create (nessie-gc owns cleanup); Nessie-native time travel DOES work via backtick refs `` `t@branch` ``, `` `t#commit-id` ``, `` `t#timestamp` `` — Part 3's "nobody is allowed to visit" needs that nuance; also the `client.region` gotcha → [[kb-0074]]; (3) equality deletes are Flink-only; (4) `_row_id` lineage works on V3; (5) DuckDB 1.5 reads V2/V3 + time travel; (6) PyIceberg reads but can't write V3, never set `prefix` for Nessie.
6. Curriculum table — 10 experiments → posts with live status (as of 2026-08-25: exp 1–3 done → posts 1–4; 01b = Nessie-vs-Polaris/Hadoop snapshot-retention comparison, done 2026-08-24, evidence for the post-1 claim; 01c = Hive Metastore (HMS) anatomy mirror, done 2026-08-25, evidence for the Nessie-vs-HMS catalog comparison section (added to the Part 1 post 2026-08-25, replacing the old Hadoop/Polaris mentions); 4–10 pending). 2026-09-01: exps 01 + 01c RERUN on Spark 4.1.3 (post-migration, [[kb-0078]]) — Nessie single-snapshot behavior re-confirmed; Part 1 post rewritten from the fresh logs.
7. Execution loop — idempotent labeled scripts, spark-submit + tee output.log, never fake output, post from the log, curriculum change → edit refs doc + `kb touch kb-0043`.

## Related
- [[kb-0007]] — e2e-data-ai-project map (the lakehouse stack lives there; venv Python 3.12 rule)
- [[kb-0047]] — lean data stack research; its DuckDB→Iceberg exit path builds on the V3/DV capabilities fact-checked here
- [[kb-0074]] — s3.region is ignored; client.region is the real Iceberg key (bit exp 01b's Hadoop-catalog run)
- [[kb-0075]] — iceberg-hive-metastore pinned 1.8.1 + table-property-max-size=0 (exp 01c's HMS catalog wiring)
- [[kb-0083]] — standing voice/prose requirement for all educational posts (written for beginners)
