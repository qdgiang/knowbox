# Iceberg deep dive: hands-on experiment curriculum (iceberg-learning)

Living plan for the Iceberg mastery project: run every Iceberg mechanism live in
`~/projects/iceberg-learning` against the e2e lakehouse stack, capture verbatim
output per experiment, and publish each as a blog post (series "Iceberg Deep
Dive") in `~/projects/personal-website`. Source research doc:
`iceberg-interview-prep.md` in the project dir. Posts: English, `draft: true`
until the user flips them. Curriculum will iterate — edit this doc + `kb touch kb-0043`
(the section-map note that routes here).

## Post style guidelines (updated 2026-09-02)

- **Voice and prose: kb-0083 is the standing requirement** — written for readers
  unfamiliar with the topic; plain, smooth, to the point; no humor, no snappy or
  dramatic statements, no big words, no AI-style constructions.
- Posts are **published and self-contained**: readers on the website use them as a
  study guide and have no access to the local machine. So: NO forward references
  to later parts of the series (e.g. "content=0 means data file; 1 and 2 will mean
  position/equality deletes later" → explain fully at the point of mention), and NO
  references to machine-local files ("full output in the experiment's output.log" →
  put the relevant output inline).
- Post 1 was refreshed 2026-09-01 (Spark 4 rerun + style pass); post 2 stays as-is.

## Per-experiment layout

One self-contained folder per blog post under
`~/projects/iceberg-learning/experiments/`:

```
experiments/NN-slug/
  run_experiment.py     # spark-submit job (idempotent, labeled steps)
  inspect_files.py      # host-side uv-venv MinIO/Avro/Parquet inspection (if any)
  output.log            # verbatim captured run output (the blog's evidence)
```

Posts: `~/projects/personal-website/src/content/writing/iceberg-deep-dive-*.md`,
shared tag `iceberg-deep-dive` + series-nav blockquote (site schema has no series
field; required frontmatter: title/date/description; tags, draft optional).
**Gotcha:** frontmatter `date` must be QUOTED (`"2026-08-17"`) — unquoted, YAML
coerces to a date type and the collection schema (string) rejects it. Validate via
`docker compose run --rm dev npm run check` / `... npm run build` (never host node).

## Environment facts (updated 2026-09-01 for the Spark 4 migration, kb-0078)

- Stack: Spark 4.1.3 + iceberg-spark-runtime-4.1_2.13 (Scala 2.13, Spark-4
  channel NATIVE) **1.11.0**, Docker compose
  profile `lakehouse` in `~/projects/e2e-data-ai-project` (Nessie 0.108.2 REST
  `http://nessie:19120/iceberg` → `s3://warehouse/nessie`; Polaris 1.6.0 REST
  `http://polaris:8181/api/catalog` → `s3://warehouse/polaris`;
  hive-metastore HMS 3.1.3 thrift://hive-metastore:9083 → `s3://warehouse/hive`;
  MinIO 9000).
  Bring up: `docker compose --profile lakehouse up -d postgres minio minio-bootstrap
  nessie polaris-bootstrap polaris polaris-setup spark-master spark-worker hive-metastore`
  (Taskfile `smoke-lakehouse` at Taskfile.yml:148-186 has the full round-trip).
- Spark-4 behavior deltas observed live 2026-09-01 (exp 01 + 01c reruns):
  (a) snapshot summaries stamp `iceberg-version -> Apache Iceberg 1.8.1` under
  BOTH catalogs now — the pinned iceberg-hive-metastore 1.8.1 jar wins the
  version lookup on Spark 4's classpath (on Spark 3.5 only Hive commits stamped
  1.8.1; Nessie stamped 1.11.0). Runtime really is 1.11.0.
  (b) `DESCRIBE EXTENDED` on a V2 table no longer exposes `_row_id` /
  `_last_updated_sequence_number` (Spark 3.5 exposed them on V2) — the base five
  metadata columns only.
- Run an experiment: `docker compose cp experiments/NN-slug/run_experiment.py
  spark-master:/tmp/` then `docker compose exec -e LAKEHOUSE_CATALOG=<nessie|polaris>
  -T spark-master /opt/spark/bin/spark-submit --master spark://spark-master:7077
  /tmp/run_experiment.py`. Scripts: `sys.path.insert(0,'/opt/spark/jobs');
  from lakehouse_catalog import build_spark`.
- MinIO objects are `<name>/xl.meta` on the volume — read via S3 API (uv venv:
  boto3/fastavro/duckdb/pyiceberg), never off the raw volume. Creds in e2e `.env`.
- Experiment namespace: `lakehouse.iceberg_lab` (throwaway tables, DROP-first scripts).

## Engine capability fact-check (primary sources, 2026-08-16)

1. **DV writes work on Spark 3.5 + Iceberg 1.11** (PR #11561) — but need BOTH
   `format-version=3` (default is 2) AND `write.{delete,update,merge}.mode=merge-on-read`.
2. **Nessie REST deliberately hides table-level Iceberg refs** → `rollback_to_snapshot`,
   `fast_forward`, `cherrypick_snapshot`, `CREATE BRANCH/TAG`, WAP **must run on Polaris**.
   **Verified live (exp 1, 2026-08-17):** Nessie keeps only the CURRENT snapshot in
   metadata.json (`snapshots` array + `snapshot-log` hold just the latest; `.snapshots`
   SQL table shows 1 row after 2 commits) — history lives in Nessie's commit log, and
   every metadata.json is named `00000-<uuid>` (no version counter; only the catalog
   pointer identifies the current one). DROP TABLE orphans the dir (`gc.enabled=false`).
   **Verified live (exp 2, 2026-08-17):** with only the current snapshot in metadata,
   `VERSION AS OF <old-id>` → "Cannot find snapshot with ID", `TIMESTAMP AS OF <old ts>`
   → "Cannot find a snapshot older than", `CALL rollback_to_snapshot` →
   ValidationException "Cannot roll back to unknown snapshot id"; the visible snapshot's
   `parent_id` shows NULL (chain link severed). Each commit writes a NEW metadata.json
   holding exactly its own snapshot (3 files after create + 2 commits, incl. the
   CREATE-time file with `current-snapshot-id: -1`). ALL files survive on storage —
   history is stranded, not deleted. Nessie's commit log
   (`GET /api/v2/trees/main/history`) keeps every commit by hash, append-only, even for
   dropped tables. **User directive (2026-08-17): stay on Nessie for the whole series** —
   Polaris-only features become find-the-limit experiments instead.
   **Verified live (exp 3, 2026-08-19):** branches/tags/WAP are commit-protocol features
   Nessie rejects at the door: `ALTER TABLE … CREATE BRANCH` and branch-targeted writes →
   `CommitFailedException: Requirement failed: ref must be 'main', but is '<ref>'` (4 client
   retries); served metadata `refs` = {main: current snapshot} only; `VERSION AS OF '<branch>'`
   → ValidationException. **Gotchas found:** (a) `spark.wap.branch` is silently IGNORED unless
   the TABLE has `write.wap.enabled=true` (verified in SparkWriteConf bytecode) — run-1 "WAP"
   write went straight to main; (b) branch-WAP failure leaves orphan Parquet: write-abort cleanup
   also fails (Nessie S3 signer refuses bulk-delete signing, ForbiddenException); (c) staged WAP
   (`spark.wap.id`) can't stage — commit goes current on main immediately, wap.id survives only
   as snapshot-summary label, and publish_changes/cherrypick_snapshot answer
   `DuplicateWAPCommitException: …published already`; no stranded snapshot in any metadata.json
   (staging negated, not stranded — unlike exp-2's history); (d) Iceberg 1.11 registry has 20
   procedures, NO create_branch/create_tag (their CALL dies at PARSE time — core Spark has no
   CALL grammar; the Iceberg extension parser only claims recognized commands) and renames
   fast_forward_branch → fast_forward (old name parses via substring match, then
   "Procedure … not found"); (e) Nessie v2 create-ref API shape (needed for the catalog-level
   branch demo): `POST /api/v2/trees?name=<new>&type=BRANCH` with JSON body = the SOURCE
   reference incl. its hash (`{"type":"BRANCH","name":"main","hash":…}`); delete =
   `DELETE /api/v2/trees/<name>%40<hash>?type=BRANCH` (wrapper-object payloads → Jackson 400).
3. Nothing in this stack writes **equality deletes** (Flink-only; Spark = position
   deletes/DVs; PyIceberg upsert is CoW) → cover conceptually, don't fake it.
4. Row lineage (`_row_id`) queryable in Spark 3.5 on V3 tables (since Iceberg 1.9.0; undocumented).
5. DuckDB 1.5.x: iceberg bundled core extension; reads V2 position deletes + V3 DVs;
   time travel `AT (VERSION => id)`; `iceberg_metadata()`/`iceberg_snapshots()`;
   REST ATTACH documented for Polaris (Nessie undocumented — smoke-test).
6. PyIceberg 0.11.1 reads V3/DVs but cannot create/write V3 (format pinned to {1,2});
   Nessie gotcha: never set `prefix`.

## Curriculum (experiments → posts)

| # | Experiment (catalog) | Blog post | Status |
|---|---|---|---|
| 1 | Anatomy: create table + insert; walk metadata.json → snap-*.avro manifest list → manifest avro → Parquet; 2nd insert → snapshot chain | Delivered as TWO posts: Part 1 "What a Table Actually Is on Disk" (catalog→metadata.json→manifest list) + Part 2 "Manifests and the Parquet Underneath" (manifest entries, field-ID stats, DuckDB parquet) — both draft:true, dated 2026-08-17 | done |
| 2 | Time travel (Nessie, per user directive): batch A → capture snapshot id while visible; batch B → select; `.snapshots`/`.history`/`.refs` show only current; VERSION/TIMESTAMP AS OF old snapshot → fail; AS OF current → works; rollback → fails; host-side: 3 metadata.json + all files intact on S3; Nessie commit log has every commit | Part 3 "Time Travel, and Who Actually Keeps the History" — draft:true, dated 2026-08-17, written under the self-contained/informal style guidelines | done |
| 3 | Branches/tags/WAP on Nessie (find-the-limit per user directive): CREATE BRANCH → CommitFailedException "ref must be 'main'"; branch-WAP (needs table prop write.wap.enabled=true or spark.wap.branch is silently ignored) → same wall + 2 orphaned Parquet files (write-abort cleanup failed: Nessie signer refuses bulk delete); staged WAP (spark.wap.id) → silently publishes straight to main, wap.id only a summary label, publish_changes/cherrypick → DuplicateWAPCommitException; no stranded staged snapshot in any metadata.json; Nessie v2 catalog-level branch create/drop works (POST /api/v2/trees?name=…&type=BRANCH, body = source ref WITH hash) | Part 4 "Branches, Tags, WAP, and Who Gets to Have Them" — draft:false, dated 2026-08-19 | done |
| 4 | Schema evolution: add/rename/reorder/widen int→long/drop/drop+re-add gotcha; field-ID diffs in metadata.json; files untouched | Part N: Schema evolution & field IDs | pending |
| 5 | Hidden partitioning & partition evolution: days(ts); partition tuples; ADD PARTITION FIELD bucket(4,id); mixed spec_id | Part N: Hidden partitioning & partition evolution | pending |
| 6 | CoW vs MoR (V2, Nessie): file-replace diff; position delete file dumped; read merge; compaction applies deletes; equality-delete theory | Part N: Row-level updates — CoW vs MoR | pending |
| 7 | V3 & DVs: format-version=3 + MoR modes; .puffin DV in MinIO; `_row_id` lineage; DuckDB cross-reads V3 | Part N: Format V3 — deletion vectors & row lineage | pending |
| 8 | Pruning funnel: known ranges; manifests/files bounds; metrics modes none vs truncate(16) vs full | Part N: How Iceberg skips files | pending |
| 9 | Maintenance (Nessie): small files → rewrite_data_files; expire_snapshots; remove_orphan_files dry_run+real; rewrite_manifests | Part N: Table maintenance | pending |
| 10 | Catalogs & multi-engine (both): same table Nessie vs Polaris roots; DuckDB ATTACH both; Nessie-no-refs gotcha live; opt PyIceberg read | Part N: Catalogs — Nessie vs Polaris + DuckDB | pending |

## Execution loop

1. Write the folder's scripts (labeled steps `=== STEP n ===`, idempotent).
2. Copy in + spark-submit with the right `LAKEHOUSE_CATALOG`; tee to `output.log`.
3. Never fake output — investigate + re-run; surprises become findings.
4. Post from the log: verbatim output + commentary; update series nav in prior posts.
5. Curriculum change → edit this doc + `kb touch kb-0043`.

End of project: site build check (personal-website Docker flow; CI: astro check),
durable gotchas into KB notes, stack teardown decision with user.
