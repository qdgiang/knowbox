---
id: kb-0081
title: "duckdb-quack beta: server always plain HTTP, client forces HTTPS outside a strict IsLocal() whitelist, and dbt's built-in adapter has no disable_ssl — make the server appear local"
type: gotcha
domain: tooling
tags: [duckdb, quack, ssl, dbt, lean-mds]
aliases: [quack-ssl, quack-islocal]
status: active
confidence: high
created: 2026-08-27
updated: 2026-08-27
supersedes: []
related: [kb-0080, kb-0069]
source: "github.com/duckdb/duckdb-quack source + live lean-mds pilot 2026-08-27"
summary: "Why cross-host duckdb quack attaches fail (server always plain HTTP; client HTTPS outside the IsLocal whitelist localhost/127.0.0.1/::1; dbt adapter lacks disable_ssl) and the proven fix: localhost-form URI + per-container loopback forwarder or netns sharing."
---
DuckDB's quack client-server protocol (beta, duckdb 1.5.5; GA targeted 2.0,
fall 2026) has an asymmetric SSL scheme decision that breaks every cross-host
attach by default — source-verified on github.com/duckdb/duckdb-quack:

- The SERVER ALWAYS listens WITHOUT SSL (quack_start_stop.cpp: "the server
  will always listen without SSL"; quack_serve's disable_ssl parameter is
  registered but never inspected at bind).
- The CLIENT picks the scheme from the attach URI's host alone:
  `enable_ssl = !initial_uri.IsLocal()`, and IsLocal() is a STRICT string
  whitelist — exactly `localhost`, `127.0.0.1`, `::1` (case-insensitive; no
  DNS suffixes, no LAN names). Any other hostname ⇒ HTTPS ⇒ fails against the
  plaintext server.
- The only override is the ATTACH option `disable_ssl` — available to the
  Python API and CLI, but NOT to dbt-core 2.0.0b2's built-in DuckDB adapter
  (strings on dbt/_core.abi3.so: zero hits for disable_ssl/quack; attachment
  fields are exactly path/alias/type/read_only/ducklake). So cross-host
  dbt-over-quack is impossible in the beta.

Working pattern (lean-mds pilot, proven live): make the server appear local.
Set the URI to `quack:127.0.0.1:9494` (prefer 127.0.0.1 over localhost — the
server binds 0.0.0.0/IPv4 only) and run a loopback TCP forwarder inside each
client container proxying 127.0.0.1:9494 → server-host:9494 (in lean-mds:
mds_pipeline.quack_forward, started by docker-entrypoint.sh when
WAREHOUSE_QUACK is set; DockerRunLauncher run containers keep the image
ENTRYPOINT). Shell-less CLI images with no python (duckdb/duckdb) instead
share the server's network namespace: `docker run --entrypoint duckdb
--network container:<server>` (the duckdb/duckdb image has NULL entrypoint —
raw docker run must pass --entrypoint).

Other beta walls (1.5.5): qualified remote refs broken client-side (all
remote SQL via `CALL <alias>.query('<sql>')`); cross-catalog CTAS from remote
scans unsupported (replicate server-side); client-side introspection blind
for quack catalogs (server-side own-catalog introspection works).
