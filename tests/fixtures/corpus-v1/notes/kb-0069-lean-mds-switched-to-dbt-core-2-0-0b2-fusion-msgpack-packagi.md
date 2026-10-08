---
id: kb-0069
title: "lean-mds switched to dbt-core 2.0.0b2 (Fusion): msgpack packaging bug, arguments: test YAML, storage stays v64-compatible"
type: gotcha
domain: projects
tags: [lean-mds, dbt, fusion, duckdb]
aliases: []
status: active
confidence: high
created: 2026-08-24
updated: 2026-08-24
supersedes: []
related: [kb-0065, kb-0047]
source: ""
summary: "How the lean-mds stack moved from dbt-core 1.12.2 + dbt-duckdb to dbt-core 2.0.0b2 (Fusion engine): what broke, what was verified, and how to roll back."
---
~/projects/lean-mds switched dbt-core 1.12.2 + dbt-duckdb 1.11.0 →
**dbt-core 2.0.0b2** (the Fusion Rust engine) on 2026-08-24; DuckDB is a
built-in first-party adapter in v2, so dbt-duckdb was dropped entirely.
In-process `dbtRunner` still works (Rust/PyO3 bridge). v2 is beta (GA TBD).

## What broke / needed changing
1. **Packaging bug:** the 2.0.0b2 wheel declares `mashumaro[msgpack]` but
   uv/pip skip it → `import msgpack` crash. Fix: pin `mashumaro[msgpack]>=3.14`
   explicitly in pipeline/pyproject.toml (comment marks it removable).
2. **v2 parser rejects top-level generic-test args:** `accepted_values: {values:
   [...]}` must become `accepted_values: {arguments: {values: [...]}}`
   (_staging__models.yml). dbt 1.x accepted both and had been deprecation-
   warning about the old form.
3. **COPY TO parquet post-hook** fails if the target dir doesn't exist —
   `transform.run()` already mkdirs `parquet_dir`; keep it.
4. Config is otherwise unchanged: profiles.yml `type: duckdb` + `path` works;
   no `-m`/`--models` flags were in use (removed in v2 → `--select`).

## What was verified before switching
- Pre-flight `dbt parse --use-v2-parser` on 1.12 surfaced the one YAML break.
- Scratch trial: dbt v2 `build` against a COPY of warehouse.duckdb → 16/16,
  then duckdb 1.5.5 CLI opened the v2-written file READ-ONLY **and** rw
  (CREATE/DROP schema) → **no storage-version bump**; the "duckdb 1.5.5
  everywhere" pin discipline holds (bundled Fusion driver writes v64 files).
- Post-switch live: Dagster `elt` run container banner shows dbt-core
  2.0.0-beta.2, 16/16 pass, parquet re-exported, Superset READ_ONLY still
  healthy; `task test` 5/5 on host workspace.

## Rollback path
pipeline/pyproject.toml: dbt-core==1.12.3 (current stable) + dbt-duckdb==1.11.0,
drop the mashumaro pin, `uv sync --all-packages`, rebuild pipeline/prefect/dagster
images. The `arguments:` test YAML form is accepted by 1.x too — no other
reverts needed.

## Related
- [[kb-0065]] — the build this modifies (State line carries the review history)
- [[kb-0047]] — research that flagged dbt-core's maintenance mode + Fusion
