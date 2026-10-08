---
id: kb-0033
title: "SQL CTE-shape visualization & dbt column-lineage tool landscape"
type: fact
domain: tooling
tags: [dbt, sql, lineage, visualization]
aliases: [sql lineage, CTE visualization, dbt lineage]
triggers:
  - "looking for a SQL query visualization or dbt lineage tool"
status: active
confidence: high
created: 2026-08-09
updated: 2026-08-11
supersedes: []
related: []
source: "web research 2026-08-09"
summary: "No tool renders per-CTE table shapes + join conditions. Closest: sql-x-ray, SQL Crack, flowscope, Gudu SQLFlow; dbt lineage: dbt-colibri, SQLMesh. sqlglot qualify/lineage = build primitives."
---
No dominant tool does "render each CTE of a long query as a mini table shape with its columns + join conditions"; the space splits into (a) per-query CTE flow visualizers and (b) dbt artifact-based column-lineage tools, with sqlglot as the shared engine underneath.

## Per-query CTE/intermediate-shape visualizers (closest to the idea)
- **SQLucent / sql-x-ray** (github.com/thehwang/sql-x-ray) — CLI ("see through any SQL without running it"): data-flow graph (Mermaid/HTML), per-CTE responsibilities, column lineage, risk lint. Deterministic sqlglot core; optional LLM only for prose. MIT, brand new (~0 stars).
- **SQL Crack** (github.com/buva7687/sql-crack) — VS Code extension: interactive execution-flow diagram, CTE/subquery expansion panels, click-a-column lineage tracing, export PNG/SVG/Mermaid. node-sql-parser + dagre. MIT, ~170 stars.
- **flowscope** (github.com/pondpilot/flowscope) — Rust/WASM in-browser lineage engine: tables/CTEs/columns flow graph, **schema-aware wildcard expansion** (resolves `SELECT *`), **dbt/Jinja preprocessing**, CLI + linter. Apache-2.0, ~90 stars.
- **Gudu SQLFlow** (gudusoft.com) — commercial (free cloud tier): resolves CTEs/subqueries/star-expansion; models JOIN/WHERE/GROUP BY columns as toggleable "indirect (impact)" lineage — best existing join-condition visualization.
- **sqllineage** (github.com/reata/sqllineage) — Python, column-level lineage within one statement, web UI.
- **LineageX** (arXiv 2505.23133) — academic column-lineage extraction from static SQL, no DB connection.
- **spark-sql-flow-plugin** (github.com/maropu) — same idea for Spark SQL.

## dbt-specific (manifest.json + catalog.json)
- **dbt-colibri** (github.com/b-ned/dbt-colibri) — CLI → static HTML dashboard from manifest.json + catalog.json; sqlglot-based; MIT, ~290 stars; builds on Canva's extractor. Cross-model column lineage, not intra-model CTE shapes.
- **Canva dbt-column-lineage-extractor** (github.com/canva-public) — lightweight sqlglot-based extractor.
- **SQLMesh** (Tobiko) — runs directly on existing dbt projects; `pip install "sqlmesh[web]"` gives browser UI with column-level lineage (tobikodata.com/blog/column-level-lineage-for-dbt).
- **dbt Cloud Explorer / Catalog** — official column-level lineage + column evolution (transformed vs reused), Discovery API; Cloud-only.
- Others: Fszta/dbt-column-lineage, tomoki-takahashi-oisix/dbt-column-lineage, dbt-lineage Rust crate; Recce blog compares the options.

## Engine primitives if building one
- **sqlglot**: `optimizer.qualify`/`optimizer.scope` propagate schemas through CTEs/subqueries (supply a MappingSchema — dbt catalog.json maps onto it directly); `sqlglot.lineage(column, sql, schema)` walks CTE/DERIVED_TABLE scopes into a node graph. DataHub's column-lineage parser is built on this.
- Gap identified: nothing consumes the dbt **semantic layer** (semantic models/metrics/descriptions) to annotate per-CTE column shapes; that combination (catalog types + manifest compiled SQL + semantic descriptions + per-CTE shape render + join-condition edges) is whitespace.

## Related
