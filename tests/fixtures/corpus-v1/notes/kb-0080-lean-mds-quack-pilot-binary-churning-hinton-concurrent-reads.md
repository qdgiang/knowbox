---
id: kb-0080
title: "lean-mds Quack pilot (binary-churning-hinton): concurrent reads during writes via DuckDB client-server protocol"
type: project
domain: projects
tags: [lean-mds, duckdb, quack, concurrency, pilot]
aliases: [binary-churning-hinton, lean-mds-quack-pilot]
status: active
confidence: medium
created: 2026-08-27
updated: 2026-08-27
supersedes: []
related: [kb-0065, kb-0069]
source: ""
summary: "Pointer note for the approved lean-mds Quack pilot plan (concurrent reads during writes)."
---
**State: done 2026-08-27 — pilot LIVE and proven in both directions.**

Plan: `~/.claude/plans/binary-churning-hinton.md` (detailed validation:
`binary-churning-hinton-agent-aa52418769ec23337.md` in the same dir).

Scope: pilot DuckDB's Quack client-server protocol (beta, built into the pinned
duckdb 1.5.5) so Superset/duckdb-shell can read **during** dlt/dbt writes.
Env-flagged by `WAREHOUSE_QUACK`; local-file mode stays default + rollback.

Outcome: all steps S0–S9 done. Quack mode is the OPERATIONAL state of
`~/projects/lean-mds` (`WAREHOUSE_QUACK=quack:127.0.0.1:9494` in .env).
Proven live: dbt-over-quack green (after fixing the beta SSL wall — see
[[kb-0081]]); reads during writes via CLI (netns-shared shell) and Superset
(replica file); dagster run-container e2e; rollback to local mode and back.
Docs: repo `docs/ARCHITECTURE.md` "Warehouse modes" (mode table + failure
matrix) and `CLAUDE.md` gotchas carry the durable details; spike working doc
was /tmp/quack-spike/findings.md (ephemeral). Env/tool traps found on the way:
[[kb-0082]].

Deferred leftovers (triggers to revisit):
- **DuckDB 2.0 GA (fall 2026)**: re-check the quack beta walls — if the
  probe-strip + client-side introspection are fixed, Superset could attach
  quack directly (drop the replica); if the server gains TLS or the dbt
  adapter exposes disable_ssl, the loopback forwarder can go.
- Serialization guards (staging single-runner, one orchestrator) stay until
  post-GA concurrency is proven; relaxing them is a later phase.
- `mds_pipeline.quack_forward` has an in-process restart loop but no external
  supervisor; if quack flaps under load, revisit.

## Related
- [[kb-0065]] — original lean-mds build plan (lineage)
- [[kb-0069]] — dbt-core 2.x Fusion gotchas (same adapter under test here)
- [[kb-0081]] — quack beta SSL semantics + the loopback-forwarder fix pattern
- [[kb-0082]] — go-task dotenv/arg-parse + compose --profile override traps
