---
id: kb-0075
title: "iceberg-hive-metastore ≥1.9 hits NoSuchMethodError under iceberg-spark-runtime's shaded Jackson — pin 1.8.1 + table-property-max-size=0"
type: gotcha
domain: projects
tags: [iceberg, spark, hive-metastore, hms, jackson, shading, lakehouse]
aliases: [iceberg hive catalog NoSuchMethodError, iceberg-hive-metastore 1.8.1, hive table-property-max-size]
status: active
confidence: high
created: 2026-08-25
updated: 2026-08-25
supersedes: []
related: [kb-0043, kb-0074]
source: "/home/giang/projects/e2e-data-ai-project/spark/Dockerfile"
summary: "Mixing iceberg-hive-metastore ≥1.9 with iceberg-spark-runtime throws NoSuchMethodError at commit — the hive module calls unshaded JsonUtil.mapper() but the runtime shades Jackson. Pin hive-metastore jar to 1.8.1 and set spark.hadoop.iceberg.hive.table-property-max-size=0. Needed for the e2e HiveCatalog (HMS) comparison catalog."
---
`iceberg-hive-metastore` ≥ 1.9 is incompatible with `iceberg-spark-runtime` in the same classpath: on every commit the hive module serializes HMS table properties via `JsonUtil.mapper()`, but that returns the *shaded* `org.apache.iceberg.shaded…ObjectMapper` from the runtime jar → `NoSuchMethodError`. The only such serialization path in 1.8.1 is gated behind `iceberg.hive.table-property-max-size` (default 32672), so pinning 1.8.1 and setting that property to 0 sidesteps it.

Details for the e2e lakehouse stack (the HMS comparison catalog, `hive/` compose fragment):
- `spark/Dockerfile` pins `org.apache.iceberg:iceberg-hive-metastore:1.8.1` alongside `iceberg-spark-runtime` 1.11.0 — version skew is deliberate, do NOT bump to match.
- `spark/config/entrypoint.sh` renders `spark.hadoop.iceberg.hive.table-property-max-size 0` into spark-defaults.conf — do NOT raise.
- Cost of the workaround: HMS table parameters don't carry the Iceberg schema/snapshot JSON. Irrelevant to Iceberg-API experiments (the metadata still lives in `metadata.json` on MinIO, HMS only holds the `metadata_location` pointer).
- Related HMS-on-this-stack facts (devops agent, 2026-08-25): HMS is `apache/hive:3.1.3` run metastore-only, backed by the SHARED postgres (`hive` db); `DROP TABLE` does not purge files (tables are EXTERNAL) — use `DROP TABLE <t> PURGE` then sweep MinIO; warehouse prefix `s3://warehouse/hive`.

## Related
- [[kb-0043]] — Iceberg deep-dive curriculum; HMS is the Nessie comparison catalog
- [[kb-0074]] — companion catalog-wiring gotcha: use client.region, not s3.region
