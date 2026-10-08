---
id: kb-0060
title: "e2e decisions 2026-08-16 — Feast deferred, features persist as an Iceberg table with OpenMetadata as registry-lite, and Qdrant stays the vector DB"
type: decision
domain: projects
tags: [feature-store,feast,iceberg,qdrant,vector-db,architecture]
aliases: [feast decision, feature store decision, qdrant decision, six-lens review]
triggers:
  - "deciding whether e2e needs Feast or a feature store"
  - "choosing or revisiting the vector DB for e2e"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0007, kb-0048]
source: "raw/session-distill-2026-08-16.md"
summary: "2026-08-16 e2e decisions: defer Feast (features-as-Iceberg-table + OpenMetadata registry-lite; IcebergSource makes it drop-in later); keep Qdrant over pgvector/LanceDB."
---
In the 2026-08-15 six-lens review of e2e-data-ai-project (Iceberg lakehouse; 2 tenants / 30 days / 1 sklearn model; single laptop), three independent reviewers converged: do NOT add a dedicated feature store now. Instead persist features as an Iceberg table, with OpenMetadata as registry-lite — `metadata_sync` already publishes Iceberg schemas there, so the text-to-SQL agent grounds on the feature table for free. Feast is deferred until a second model or a low-latency online consumer appears: Feast now ships a native zero-copy Iceberg offline-store source (`IcebergSource`), so its offline store would simply be the same Iceberg/MinIO tables the platform already has — it becomes a drop-in later.

Vector-DB verdict: keep Qdrant — the repo already runs it, and 2026 consensus still rates Qdrant the best self-hosted price/performance default; pgvector is the alternative when consolidating onto Postgres, LanceDB the embedded up-and-comer.

Concrete pattern implemented as B2: a PySpark job writes `lakehouse.orders.feature_orders_daily` (Spark stays the sole Iceberg writer), orchestrated by a thin Dagster asset; training reads it via the existing `LakehouseReadResource`; an appdb sync of the feature frame provides the "online" layer behind the same RLS boundary. Add Feast/MLflow later only on the triggers above.

## Related
- [[kb-0007]] — e2e-data-ai-project map: the lakehouse/Dagster/agent components this decision shapes
- [[kb-0048]] — e2e quality-layer plan: this deferral keeps the platform surface small before layering more tooling on
