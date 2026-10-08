# e2e-data-ai-project — knowledge base

Local end-to-end data + AI platform monorepo at `~/projects/e2e-data-ai-project`.
Docker Compose + Dagster + DuckDB + Iceberg lakehouse (Nessie + Polaris + Spark) +
OpenMetadata/Marquez lineage + (planned) Ollama/Qdrant/LangGraph agents. Shared kernel:
`shared_libs/platform_core`. The living phase tracker is in-repo at
`docs/IMPLEMENTATION_PLAN.md` (companion: `docs/ARCHITECTURE.md`).

This note is the session-to-session knowledge base on the Fedora server. It replaces the
old Mac-Mini Obsidian vault (`60-ai-logs/e2e-data-ai-project/`), which is no longer
reachable from here.

---

## Current state

**Phases done (on `main`, CI green):** 0, 1, 2, 3a/3b/3c, 3L-a/3L-b/3L-c, 4, 5, 6,
7, 8. Remediation P0+P1 committed as `266e644` (2026-08-07).
**Stabilization + quality layer (six-lens review follow-up, 2026-08-15; committed
as checkpoint `76f87e7` 2026-08-19):** Phase A1–A8 + B5/B1/B2/B3/B4 done, offline
gates green
throughout (ruff format/check, mypy strict, unit pytest). See
`docs/IMPLEMENTATION_PLAN.md` "Phase 8.5" and `## Quality-layer gotchas` below.
Headline: pinned dev tooling + format gate green; `extra="forbid"` config;
direction-aware eval gate; staging NULL gate; RAG path containment; ingest schema
contract; fail-closed Bearer auth + tenant binding on the agent API; appdb lineage
bridge in Marquez; retrain-on-curated sensor (RUNNING); `feature_orders_daily`
Iceberg FEATURE TABLE as the feature store (engine-selectable since the 2026-08-25
Sail experiment — Spark was the sole writer; ISO dow, calendar
lags); registry-lite (manifest.json + `promoted/current.json`, gate scores the
LOADED artifact); LLM eval harness (`task eval`, ratchets) + per-call structured
JSON logs (no prompt/response text).
**Next:** Phase 9 (CI/CD hardening & E2E demo — the last phase, all items open).
**2026-09-01: Iceberg v3 enrollment ATTEMPTED + REVERTED — blocked on Nessie.** Educational
pass: added `TBLPROPERTIES ('format-version' = '3')` to all four Spark job CREATE/CTAS
sites (via a shared `V3_TBLPROPERTIES` constant in `lakehouse_catalog.py`) + a v3
assertion in the smoke. Live result: **Polaris 1.6.0 creates v3 fine** (smoke PASS,
format-version=3 round-tripped); **Nessie 0.108.2 silently downgrades to format-version 2**
(confirmed in the table's metadata JSON on MinIO) — Nessie's official stance is still
"spec v3 not supported" (release notes, through at least 0.108.4). User decision: WAIT
for Nessie — everything stays format-version 2, enrollment fully reverted (tree clean at
`f0facf5`). Revisit trigger: a Nessie release whose notes say spec v3 is supported.
Related readiness facts (all verified 2026-09-01): Spark 4.1.3 + Iceberg runtime 1.11.0
writer is v3-ready; DuckDB 1.5.4 + `INSTALL iceberg` reader is v3-ready (duckdb-iceberg
>= 1.5.3); catalog side is the ONLY blocker (Nessie; Polaris fine). Format bump is
one-way (no v3→v2 downgrade) and changes nothing by itself — deletion vectors, VARIANT,
nanosecond timestamps are per-feature opt-ins.
Working tree CLEAN (2026-08-26): Spark 3.5.8 → 4.1.3 migration committed as
`c9b02bf`, DataFlint restoration as `f0facf5`. Commits before them: Sail
engine experiment `b2a8985` (after HMS comparison catalog `2b612b7` and the RAG
citation fix `885a116`).
**2026-08-26: Spark lakehouse engine migrated 3.5.8 → 4.1.3 — verified live.**
Original target was 4.2.0; evaluated live and stepped back — two upstream
blockers: (1) NO Iceberg runtime for 4.2 until 1.12 (no date; the 4.1 runtime
hard-fails at catalog LOAD on 4.2: Spark 4.2 changed
`connector.catalog.View` interface→class → IncompatibleClassChangeError,
unavoidable); (2) DataFlint 0.9.9 crashes at SparkContext init on Spark **4.2**
(its UI dropped shaded Jetty) — 4.1.x is UNAFFECTED and runs DataFlint fine.
Changes: image `apache/spark:4.1.3-scala2.13-java17-python3-ubuntu` + compose tag
`spark:4.1-iceberg`; Iceberg runtime 4.1 channel now NATIVE; S3A jars swapped to
`hadoop-aws:3.4.2` + `software.amazon.awssdk:bundle:2.29.52` (SDK v2); DataFlint
artifact `dataflint-spark4_2.13` baked in + ENABLED (see gotchas correction);
`SparkSubmitResource.spark_version` → 4.1.3; ANSI stays ON (Spark 4 default);
`--add-opens` block kept. Verified live: `task smoke-lakehouse` PASS (Nessie +
Polaris), `task materialize` SUCCESS with exact parity (staging 131 / curated 60
/ feature 58), event logs landing on S3A, DataFlint tab HTTP 200 on both
submission paths. FOLLOW-UP: move to 4.2.x once Iceberg
1.12 ships + DataFlint supports Spark 4.2's UI (FROM bump +
`ICEBERG_RUNTIME_CHANNEL=4.2` + Hadoop/SDK pin re-derive; smoke + materialize
as the gate). Full record: `docs/IMPLEMENTATION_PLAN.md` "Spark migration:
3.5.8 → 4.1.3 (2026-08-26)".
**2026-08-25: Sail engine experiment DONE + VERIFIED — stack REVERTED to
`LAKEHOUSE_ENGINE=spark`.** `LAKEHOUSE_ENGINE=spark|sail` (root `.env`, back
on `spark`) selects which engine the pipeline drives; Spark stays intact and
both write the SAME tables through the SAME catalog — swap = flip the var +
`docker compose --profile lakehouse up -d sail-server` + recreate the dagster
containers. Verified live: `task smoke-lakehouse-sail` PASS (Nessie +
Polaris), `task materialize` SUCCESS under sail with row-count parity
(staging 131 / curated 60 / feature 58), swap back to spark re-verified
(smoke + materialize SUCCESS), offline gates green (`task test` 413 passed).
Verdict: an OPTION + footprint play, not a capability upgrade at this
stack's data volumes (observability regression, 0.7.1 maturity gaps).
Spark is NO LONGER the sole Iceberg writer. Load-bearing Sail facts +
live-run gotchas (Nessie bare warehouse name, prefix double-encode pin
`prefix="main"`, `AWS_ALLOW_HTTP=true` + explicit `AWS_REGION` for MinIO,
CTAS `USING iceberg` + DROP/CTAS in place of CREATE OR REPLACE): [[kb-0076]]
+ `docs/IMPLEMENTATION_PLAN.md` "Experiment — Sail as an alternative
lakehouse engine (2026-08-25)".
**2026-08-25: Phase 8.5 CLOSED — full live pass green.** `task build && task up-all`
(all profiles healthy; marquez-web moved to host port 3002 in local `.env` because
another project's container holds 3001), `task smoke-agent` SMOKE PASS (text2sql +
RAG ingest + query, Bearer auth), `task eval` green after fixing two first-run issues
(up-test/up-ai ordering; RAG citation prompt — see Quality-layer gotchas, B4 bullet).
Offline gates green throughout (ruff, mypy strict, 404 unit tests).
**2026-08-17: Spark History Server added + materialize verified live.** New
`spark-history-server` (lakehouse profile, UI :18080 on HOST_BIND) reads event
logs from a new `spark-logs` MinIO bucket (`history/` prefix — pre-created by
minio-bootstrap because the History Server EXITS on a missing log dir). Event
logging is per-app and reaches the app two ways (see `## Lakehouse gotchas`).
`task materialize` is green live (first full pass of the Phase 8.5 working
tree), which surfaced + fixed TWO pre-existing first-live-run bugs in
`spark/jobs/feature_orders_daily.py` (see gotchas below).
**2026-08-18: DataFlint observability installed on the Spark stack (verified
live).** `io.dataflint:spark_2.12:0.9.9` baked into the spark image; enabled via
`spark.plugins` on BOTH submission paths + Iceberg write-metrics auto-discovery.
"DataFlint" tab renders on live apps and on the History Server (HTTP 200).
Details + kill switches in `## Lakehouse gotchas`.
**2026-08-25: local MCP bridge over Spark/DataFlint observability — planned, then
REJECTED by the user; nothing was built.** Do not resurface unprompted; design on
record in [[kb-0049]]. Same probe: `IcebergCommitEvent` now present (2/app) in the
2026-08-24 `iceberg-exp01*` event logs (Polaris/Hadoop/Hive writes) with only
`autoCatalogDiscovery` — absent from all pre-08-24 logs; suspected REST-vs-CLI
submission-path difference, unverified. (Those apps came from HMS
comparison-catalog experiments — `hive/` stack + `iceberg-hive-metastore-1.8.1`
jar, uncommitted in the tree.)
**2026-08-16: all published ports flipped loopback → `0.0.0.0`** via a new
`HOST_BIND` var (`.env.example`, default `0.0.0.0`; every compose fragment uses
`${HOST_BIND:-0.0.0.0}:<port>`). Rationale: headless server, UIs browsed from
tailnet devices; exploration-only data, user accepted the exposure. Set
`HOST_BIND=127.0.0.1` in `.env` to restore loopback-only. Uncommitted in the
working tree along with the Phase 8.5 quality layer.
**Phase 8 — model_training/** committed 2026-08-08 as `846c8bf`: `feature_orders` →
`trained_model` → `model_eval` as its OWN second Dagster code location
(ARCHITECTURE §9 dec. 14); StandardScaler+Ridge pipeline; eval gate raises
`Failure` below threshold; artifacts per run at
`models/trained_model/<run_id>/{model.joblib,metrics.json}`. Verified live:
both code locations load, pipeline green (r2=0.11 > 0.0), and
`TRAINING_EVAL_THRESHOLD=0.999` fails the run loudly. Gotchas in
`## Phase 8 gotchas` below.
**2026-08-07: six-persona review completed** (data-eng / AI-ML / devops / security /
QA / architect, high findings skeptic-verified). Findings kept for history in
`## Review findings` below; P0/P1 are all fixed + verified. P2 status after the
2026-08-15 quality layer: CLOSED — lineage-to-appdb (→B5 appdb bridge dataset),
ingest path containment (→A6). PARTIALLY closed — config hardening (→A3
`extra="forbid"`; full secrets ceremony still open). Still open: container memory
limits, config single-homing, DuckDB extension pinning, wiring tests, secrets
ceremony, nightly integration CI.

RAG agent (Phase 7) is complete and verified LIVE: `task up-ai` then
`curl localhost:8000/rag/ingest -d '{"path":"corpus"}'` (4 docs → 8 chunks,
idempotent) + `curl localhost:8000/rag/query`. Chunk → embed (`mxbai-embed-large`,
1024-dim, Ollama `/api/embed` batch) → Qdrant (deterministic UUID5 IDs) → retrieve
→ grounded answer with `[1]`/`[2]` citations. A Dagster `rag_ingest` asset (in
`data_pipeline`) triggers the agent's ingest over HTTP (no `ai_agents` import into
the pipeline — mirrors `metadata_sync`→OM; skips gracefully when the `ai` profile
is down).

Text-to-SQL agent (Phase 6) is complete and verified LIVE: `task up-ai` then
`curl localhost:8000/text2sql/query`. The LLM (qwen2.5-coder:3b on CPU) generates SQL
grounded from appdb's `information_schema`, self-corrects on DB errors (LangGraph loop,
bounded by `max_attempts`), and is secured at three layers: guardrail denylist →
SELECT-only `agent_ro` role → RLS tenant scoping. tenant_a vs tenant_b return different
(tenant-isolated) rows; `delete all orders` is rejected before execution.

AI serving (Phase 5) is complete: `task smoke-ai` round-trips Ollama (chat generation +
a 1024-dim embedding) and Qdrant (`/readyz`). Models: `qwen2.5-coder:3b` (chat) +
`mxbai-embed-large` (embedding, **1024-dim**). First `up` pulls ~2.6 GB, cached on the
`ollama_models` volume thereafter.

Lakehouse (3L) is complete: Spark writes Iceberg to MinIO via either the Nessie or
Polaris REST catalog (`LAKEHOUSE_CATALOG=nessie|polaris`, a config-only switch); DuckDB
reads the curated Iceberg table for serving. `task smoke-lakehouse` round-trips both
catalogs (Spark write → DuckDB read).

Recent commits (newest first): `b2a8985` (Sail engine experiment — swap option,
stack stays on spark) → `885a116` (Phase 8.5 live-pass close, RAG citations) →
`2b612b7` (HMS comparison catalog) → `76f87e7` (Phase 8.5 checkpoint) →
`846c8bf` (Phase 8 — model_training) → `266e644` (remediation P0+P1) →
`ae2a0ec` (Phase 7 RAG). All on `main`, CI green.

---

## Review findings (2026-08-07 multi-persona review)

**STATUS 2026-08-07: everything below in P0/P1 is FIXED + verified** (subagent
fleet: guardrail hardening, MinIO loopback + credential rotation, Spark DQ
validate-before-publish + DuckDB transform tests, RAG source-qualified IDs +
eviction + batched embeds + num_ctx, scoped smoke teardowns + `task build` +
offline-safe model pull + loud polaris bootstrap, buckets raw+warehouse+models,
raw_orders in-memory, docs reconciled, Phase 8 code-location decision recorded).
Kept for history; P2 items remain open. Original findings:

- **MinIO published on 0.0.0.0** (`minio/compose.yaml` — the ONLY compose fragment
  without a `127.0.0.1:` prefix) with default root creds → anyone on the LAN gets
  full S3 access incl. the entire Iceberg `warehouse` bucket. One-line fix.
- **Text-to-SQL tenant-isolation bypass (empirically reproduced):** guardrail
  denylist misses session functions — `set_config('app.current_user_id', …)` inside
  a MATERIALIZED CTE passes `validate_sql()` and flips the RLS GUC mid-transaction
  → cross-tenant reads. `pg_sleep`/timeout-lifting also pass. Writes still blocked
  by the SELECT-only role.
- **Spark layer fell out of all static/test surfaces:** `spark/` is excluded from
  ruff, mypy, AND pytest; dedup/typing/aggregation logic has zero automated
  coverage; `test_transforms.py` docstring claiming `smoke-lakehouse` covers it is
  FALSE (smoke submits only the trivial `iceberg_smoke.py`).
- **DQ gates run AFTER `CREATE OR REPLACE`** (`spark/jobs/staging_orders.py`), and
  `curated_orders_daily` has none → an empty/bad mart can be TRUNCATE-loaded into
  the agent-facing table all-green.
- **RAG gaps:** point IDs = `uuid5(chunk_text)` WITHOUT source → cross-doc chunk
  collisions overwrite citations; no stale-chunk eviction on edits; unbounded
  single-batch embed under the 120 s timeout; `num_ctx` never set → Ollama silently
  drops context from the front while citations stay confident.
- **Doc drift (post-lakehouse):** ARCHITECTURE.md §2 + Phase 1 checkboxes describe
  `platform_core` observability/, Repository protocols, IngestSourceUseCase — none
  exist (ports actually live in `ai_agents/src/ai_agents/ports.py`; 3 adapter
  docstrings still cite the phantom kernel ports).
- **Ops traps:** every `task smoke-*` teardown also stops the RUNNING core stack
  (`compose down` kills profile-less services); no `build` task + fixed image tags
  → stale code silently served; `ollama-pull` hard-gates the API on every `up`
  → offline re-boots break the ai profile even with models cached;
  `polaris-bootstrap || echo` masks genuine failures (refines the gotcha below).
- **Config sprawl & residue:** `OLLAMA_CHAT_MODEL` in 4 places / OM URL in 5, tied
  by "keep in sync" comments; `ICEBERG_WAREHOUSE_BUCKET` read by Nessie but defined
  nowhere (rest of the repo uses `ICEBERG_WAREHOUSE`); vestigial `warehouse` volume
  + `staging`/`curated` buckets; no container memory limits (Nessie/Polaris JVMs
  have no heap cap); OpenLineage stops at the Iceberg mart (agent-facing
  `app.curated_orders_daily` has zero lineage); integration suite (incl. the RLS
  guarantee) never runs in CI; Phase 8 code-location design conflict unresolved.

---

## Load-bearing facts (don't rediscover these)

### Toolchain
- `task` (go-task, 3.52) and `uv` (0.12.1) are on PATH — call them directly (`task --list`,
  `uv run …`), no `~/.local/bin/` prefix needed. See `Taskfile.yml`.
- **Local venv MUST be Python 3.12** (`uv venv --python 3.12 --clear`). System Python is
  3.14, which breaks dagster/pydantic (`Field 'base_dir' requires a type annotation`).
  CI uses 3.12.
- `.env` is gitignored (never commit it); `.env.example` is the committed topology
  template — `${VAR:?}` for secrets, public dev defaults otherwise.

### Git
- Git identity is NOT configured in the repo. Commit with the one-off identity:
  `git -c user.name="A random DE" -c user.email="55189671+qdgiang@users.noreply.github.com" commit ...`
  Do NOT modify git config. Commit/push only when explicitly asked.

### Lakehouse gotchas
- **Iceberg v3 enrollment (2026-09-01): blocked ONLY by Nessie.** Polaris 1.6.0 accepts
  `format-version=3` (verified live); Nessie 0.108.x SILENTLY writes v2 metadata when v3
  is requested (no error — check `format-version` in the table's metadata JSON on MinIO
  to see it). Writer (Spark 4.1.3 + Iceberg 1.11.0) and reader (DuckDB 1.5.4 + iceberg
  ext >= 1.5.3) are both v3-ready. Enrollment recipe when Nessie catches up: shared
  `TBLPROPERTIES ('format-version' = '3')` at the 4 CREATE/CTAS sites + smoke assertion
  (nothing was committed — the reverted code is simple enough to rewrite from the
  description in `## Current state`).
  Iceberg itself is already at the latest release (1.11.0, 2026-05-19); 1.12.0 was still
  unreleased (RC stage) as of 2026-09-01 and is the gate for BOTH the v3-on-Nessie
  revisit and the Spark 4.2 follow-up (first version with a `iceberg-spark-runtime-4.2`
  artifact — 404 on Maven Central as of that date).
- **DataFlint (2026-08-18):** plugin config rides the SAME two-path split as event
  logging — rendered `spark-defaults.conf` (entrypoint.sh) for spark-submit runs,
  `sparkProperties` injection in `SparkSubmitResource` for REST submissions. The jar
  MUST be baked into `/opt/spark/jars` (History Server cannot ivy-load; baking also
  keeps it in the same classloader as the Iceberg jars, dodging Iceberg's known
  cross-classloader metrics-reporter bug). Kill switches: drop `spark.plugins`
  (jar alone is inert); drop `spark.dataflint.iceberg.autoCatalogDiscovery` if the
  Iceberg reporter misbehaves (DataFlint 0.9.9 compiled vs Iceberg 1.5.0, we run
  1.11.0 — reporter is the only drift surface). HS tab URL is
  `/history/<appId>/dataflint/` (NO attempt segment). Fully local — no egress.
  Fallback if auto-discovery ever misses the catalog: set
  `spark.sql.catalog.lakehouse.metrics-reporter-impl` in `lakehouse_catalog.py`.
  **2026-08-26 CORRECTION — plugin WORKS on Spark 4.1.x, enabled again:** the
  migration-day kill switch was based on a faulty scan (the image has no `unzip`,
  so the jar sweeps returned nothing) + extrapolation from Spark 4.2.0. Corrected
  facts: Spark 4.1 bundles SHADED Jetty inside spark-core (`org.sparkproject.jetty`,
  1469 classes); DataFlint's Spark-4 artifact references NO Jetty classes itself
  (bytecode-verified; only jakarta/servlet); live run on 4.1.3 initializes the
  plugin cleanly and the tab renders (HTTP 200) for apps from BOTH submission
  paths. Only Spark **4.2** breaks DataFlint 0.9.9 (its UI dropped shaded Jetty →
  CNFE org.eclipse.jetty.servlet.ServletContextHandler at SparkContext init).
  Kill switches stay wired for the eventual 4.2 upgrade: `DATAFLINT_ENABLED`
  (entrypoint.sh + compose env, default true) and
  `SparkSubmitResource.dataflint_enabled` (default True).
- **Spark History Server (2026-08-17):** event logging is a PER-APP SparkConf,
  and there are TWO submission paths that must both carry it: (1) spark-submit
  CLI runs (smoke-lakehouse) get it from `spark-defaults.conf`, which the spark
  image's `/opt/entrypoint.sh` RENDERS at container start from env (the file has
  no env-var substitution; `/opt/spark/conf` doesn't exist in the base image —
  the Dockerfile creates + chowns it for uid 185); (2) the pipeline's raw REST
  submissions (`SparkSubmitResource`) never read that file — they inject the
  same `spark.eventLog.*` + `spark.hadoop.fs.s3a.*` properties into the REST
  payload's `sparkProperties` (settings field `spark_event_log_dir`). The
  History Server itself reads `spark.history.fs.logDirectory` from the rendered
  defaults. Log dir = `s3a://spark-logs/history` (`SPARK_EVENT_LOG_DIR`).
- **`spark/jobs/*.py` are parsed by the IMAGE's Python (3.10), not the repo's
  3.12:** backslashes inside f-string EXPRESSIONS are a SyntaxError there
  (legal from 3.12 — the offline gates can't catch it; hoist the join out of
  the f-string). Same class of trap: `date_format(order_date, 'u')` passes
  every static check but is REJECTED at run time by Spark 3.0+'s default
  datetime parser policy (`INCONSISTENT_BEHAVIOR_CROSS_VERSION`) — the ISO dow
  expression is now `CAST(EXTRACT(DAYOFWEEK_ISO FROM base.order_date) AS INT)`
  (supersedes the B2 spelling below). Driver tracebacks live in the worker's
  `/opt/spark/work/driver-*/stdout`, NOT container logs.
- **Polaris**: `POLARIS_BOOTSTRAP_CREDENTIALS` does NOT create the schema for
  `relational-jdbc` — bootstrap must use the admin-tool. CORRECTED 2026-08-07
  (was: `... || echo` wrap): exit 3 is the GENERIC bootstrap-failure code (wrong
  password / unreachable JDBC also exit 3 — proved against polaris-admin-tool
  1.6.0 source). The realm-already-exists case is exit 3 WITH 'already been
  bootstrapped' in the output. `polaris/compose.yaml` now captures output and
  tolerates only rc=0 or rc=3+'already been bootstrapped'; anything else fails
  loudly. The management iface :8182 is loopback-only, so `polaris-setup` waits
  on the :8181 token endpoint (`get_token()` retries), not :8182.
- **Spark** cluster-mode REST submission needs the Java 17 `--add-opens` flags
  (`SparkSubmitResource` injects them); reading raw Parquet via S3A needs hadoop-aws.
  Image: `apache/spark:4.1.3-scala2.13-java17-python3-ubuntu` (compose tag
  `spark:4.1-iceberg`; Spark 4 is Scala 2.13-only) + Iceberg JARs
  (`iceberg-spark-runtime-4.1_2.13:1.11.0` — native channel for 4.1,
  `iceberg-aws-bundle:1.11.0`, `iceberg-hive-metastore:1.8.1`, `hadoop-aws:3.4.2`,
  `software.amazon.awssdk:bundle:2.29.52` — hadoop-aws 3.4.2 links AWS SDK **v2**,
  the old v1 `aws-java-sdk-bundle` is gone). Why not 4.2: no Iceberg runtime for 4.2
  until 1.12 (4.1 runtime hard-fails at catalog load — View interface→class) +
  DataFlint 0.9.9 breaks on the Jetty-less UI; full record in
  `docs/IMPLEMENTATION_PLAN.md` "Spark migration: 3.5.8 → 4.1.3 (2026-08-26)".
  JARs ADDed from URL land mode 0600 — `chmod 644` them in the Dockerfile.
- **DuckDB ATTACH Iceberg**: Nessie uses `AUTHORIZATION_TYPE 'none'`; Polaris uses OAuth2
  (`CLIENT_ID/CLIENT_SECRET/OAUTH2_SERVER_URI`); both need `ACCESS_DELEGATION_MODE 'none'`
  + static S3 config.
- Catalog abstraction: `spark/jobs/lakehouse_catalog.py` (Spark side) +
  `data_pipeline/.../resources/iceberg_read.py` (DuckDB side). Framework-free table
  identifiers shared via `data_pipeline/.../lakehouse.py`.

### AI tier gotchas (Phase 5)
- **Ollama model pull** is a one-shot `ollama-pull` container (`restart: "no"`, mirrors
  `minio-bootstrap`): it reads `ollama/config/models.txt` and pulls each line.
  Offline-safe since 2026-08-07: `pull.sh` checks `ollama list` first and skips
  cached models (untagged entries normalize to `:latest`), so an offline re-`up`
  no longer breaks the ai profile. First boot ~2.6 GB, cached on the
  `ollama_models` volume. Keep `models.txt` in sync with `OLLAMA_CHAT_MODEL` /
  `OLLAMA_EMBED_MODEL` in `.env.example`.
- **Qdrant image ships no curl/wget/nc** — only bash. Container healthcheck is a bash
  `/dev/tcp` TCP probe; the real HTTP `/readyz` check runs from the host in `smoke-ai`.
  Config is env-driven (`QDRANT__SECTION__KEY`), no config-file mount (OpenSearch precedent).
- **`docker compose wait SVC` prints** `container "<id>" exited with status code N`, NOT
  just `N` — parse the exit code with `awk '{print $NF}'` (a bare `tr -d '[:space:]'`
  smashes it and breaks the `[ "$code" = 0 ]` check).
- **mxbai-embed-large = 1024-dim** vectors → Phase 7 Qdrant collections must be size 1024.
- Images pinned `ollama/ollama:0.32.3` + `qdrant/qdrant:v1.18.3` (verified 2026-07-24).
  CPU-only (no GPU reservation). Both host-published on `HOST_BIND` (default
  0.0.0.0 since 2026-08-16; was loopback-only).

### Text-to-SQL agent gotchas (Phase 6)
- The agent queries **Postgres `appdb.orders` (RLS)**, NOT DuckDB — the plan's
  `DuckDBReadOnlyRepository` was retired post-lakehouse (curated mart is Iceberg, no
  tenant isolation). The "read_only" security layer is the `agent_ro` SELECT-only grant.
- Schema grounding: `InformationSchemaProvider` (appdb) is the workhorse; the OM provider
  is OPT-IN (`openmetadata_table_fqns`, default empty) + graceful — OM tracks the
  lakehouse, not appdb, so for appdb queries the fallback serves.
- Guardrails scan the FULL LLM response for forbidden keywords BEFORE SQL extraction —
  extraction can strip a destructive prefix (e.g. `GRANT SELECT …`). Exception is named
  `GuardrailError` (ruff N818 wants an Error suffix). Since 2026-08-07 the denylist also
  blocks session/privilege callables (`set_config`, `current_setting`, `pg_*`, `lo_*`,
  `dblink*` prefixes) — closes the tenant-GUC-flip bypass — and the graph's failure
  answer is generic ('Query failed after N attempt(s).'); full DB errors stay in state
  for self-correction but never reach the API caller.
- DI container keys are CONCRETE adapter classes (platform_core Container convention), not
  Protocols — avoids mypy `type-abstract`; the port abstraction lives in
  `TextToSQLGraph`'s signature (fakes satisfy the ports structurally in tests).
- mypy: added `platform_core/py.typed` (PEP 561) so isolated `mypy ai_agents` resolves
  `PlatformBaseSettings`; excluded `conftest.py` from mypy (each stack's `tests/` has one
  with no `__init__.py` → mypy collapses them to a duplicate top-level `conftest` module).
- pytest stays on default `prepend` import mode — do NOT switch to `importlib` (the
  top-level stack folders like `data_pipeline/` shadow the installed src-layout packages).
  Same-named test files clash across stacks → the settings test is `test_agent_settings.py`.
- uvicorn runs via `--factory ai_agents.api.app:create_app_from_settings`; the agent
  container `depends_on: ollama-pull: service_completed_successfully` (models present).

### RAG agent gotchas (Phase 7)
- **qdrant-client 1.18 removed `QdrantClient.search`** — use `query_points(collection_name,
  query=vector, limit=k)` and read `response.points` (`.id`/`.score`/`.payload`). The
  `timeout` arg is `int | None` (pass `int(timeout)`).
- **Deterministic IDs = UUID5 of source + chunk index + chunk text** (changed
  2026-08-07 from chunk-text-only, which collided across docs sharing identical
  chunks) → re-ingest of the same source stays idempotent, and upsert EVICTS the
  source's existing points first (Qdrant filter key is the payload-relative
  `source`, NOT `payload.source`), so edited docs leave no stale vectors.
  `embedding_dim` (1024 for mxbai-embed-large) MUST match the model; the Qdrant
  collection is auto-created at that size (cosine). Embeddings are batched
  (default 32 chunks per Ollama call); `llm_num_ctx` (default 8192) is always
  sent, and settings validate `rag_top_k × chunk_size` against a context budget.
- The `rag_ingest` Dagster asset (in `data_pipeline`) triggers the agent's `/rag/ingest`
  over **HTTP** (no `ai_agents` import → no langgraph/fastapi/qdrant-client in the
  pipeline closure). Mirrors `metadata_sync`→OM; skips gracefully (`synced=false`) when
  the `ai` profile is down. `rag_ingest` rides `AssetSelection.all()` (so `task
  materialize` includes it).
- The demo corpus lives at `ai_agents/corpus/` (baked into the agent image); ingest
  `{"path":"corpus"}` resolves under `corpus_base_dir` (the ai_agents stack root).
- `create_app(graph, rag=None)` — the RAG routes return 503 when no RAG runner is wired.

### Phase 8 gotchas (model_training)
- **model_training is its own Dagster code location** (`model_training/src/model_training/definitions.py`,
  `MODEL_TRAINING_JOB_NAME = "model_training_pipeline"`), registered in
  `data_pipeline/config/workspace.yaml` alongside `data_pipeline.definitions`. It
  must NOT import data_pipeline — the lakehouse read goes through
  `platform_core.lakehouse` kernel identifiers (`CatalogTopology`); cross-location
  lineage is `deps=[AssetKey("curated_orders_daily")]` only.
- **Dagster 1.11.16 API drift** (cost real time): asset modules must NOT use
  `from __future__ import annotations` (Dagster inspects annotations live);
  asset returns need parameterized `Output[pd.DataFrame]`; use
  `context.run.run_id` (not `context.run_id`); `joblib.dump(model, BytesIO)` —
  there is no `joblib.dumps`; `Definitions.resolve_all_asset_specs()` /
  `resolve_job_def()` (not `get_asset_graph`/`get_job_def`); materialization
  metadata is `event.materialization.metadata`.
- **GraphQL run launch** (Dagster has no CLI launch against a live webserver here):
  mutation `launchPipelineExecution(executionParams: {selector:
  {repositoryLocationName, repositoryName: "__repository__", pipelineName}})`;
  poll `runOrError(runId){... on Run{status}}`; terminal = SUCCESS/FAILURE/CANCELED.
  The `logs` field does not exist (capturedLogs needs fileKey) — use daemon logs.
- **Postgres init scripts run ONLY on a fresh volume.** When the Phase 3c table
  `app.curated_orders_daily` was missing from the live appdb (volume predates the
  init script), the fix was manual patch DDL (table + OWNER appuser + RLS policy
  `curated_tenant_isolation` using `current_setting('app.current_user_id', true)`
  + `GRANT SELECT` to agent_ro; RLS enabled but not FORCED so the sync writes as
  owner). Any future init-script change needs the same manual migration on
  existing volumes.
- **Seed data is now a 30-day trending history** (`data_pipeline/src/data_pipeline/seed.py`:
  `_DAYS = 30`, per-tenant base + slope/day + bounded noise). The old 7-day
  uniform-noise seed gave the Ridge model r2 = −1.5 and the eval gate correctly
  FAILED the run — the gate works; the seed needed a learnable signal.
  `test_seed.py` pins the 30-day shape.
- **Dagster image uv pin moved**: `ghcr.io/astral-sh/uv:0.12.1-python3.12-trixie-slim`
  (uv ≥ 0.10 images are trixie, not bookworm; multi-package
  `uv sync --package data-pipeline --package model-training` needs uv ≥ ~0.10 —
  0.9.6 rejects repeated `--package`). Build-time assertion must import
  `model_training.settings`, NOT `model_training.definitions` (the latter builds
  `defs` at import, which needs runtime MinIO creds).
- Eval gate: `model_eval` persists `metrics.json` to MinIO EVEN when it raises
  `Failure` below threshold (artifact first, then raise) — the failure message
  carries the metrics URI. Live demo of a red run:
  `docker compose run --rm -e TRAINING_EVAL_THRESHOLD=0.999 dagster-webserver python -c …`.
- Stale `staging`/`curated` buckets linger in the live MinIO volume (bootstrap
  only creates buckets, never deletes) — harmless residue.

### Quality-layer gotchas (2026-08-15 stabilization + quality layer)
- **Dagster 1.11.16, again:** `DagsterInstance.get_materialization_records` does NOT
  exist — use `get_latest_materialization_event(AssetKey) -> Optional[EventLogEntry]`
  (read `.run_id`). Run-status sensors: test with `build_run_status_sensor_context`
  over a real ephemeral instance; `defs.sensors` is nullable.
- **B1 retrain sensor** (`model_training/sensors.py`): `monitor_all_code_locations=True`
  is REQUIRED (curated is materialized by the data_pipeline location); `request_job=`
  makes `run_key=f"retrain-{run.run_id}"` dedup legal; own-job skip prevents the
  self-loop; never-raise (any error → `SkipReason`). Default status RUNNING — flip
  to STOPPED for opt-in retraining.
- **B2 engine parity decisions (pinned by DuckDB tests):** ISO day-of-week Mon=1..Sun=7
  — Spark `CAST(EXTRACT(DAYOFWEEK_ISO FROM order_date) AS INT)` (changed 2026-08-17
  from `date_format(order_date,'u')`, which Spark 3.0+'s parser policy rejects at
  RUN time), DuckDB/pandas reference
  `dt.dayofweek + 1` (NOT `date_part('dow')`/`dayofweek()` — engine-divergent).
  Calendar lags via LEFT JOIN on `order_date - INTERVAL '{i}' DAY` (a date gap ⇒ NULL
  ⇒ warm-up drop, never the previous row's value). One-hots over the SORTED tenant
  vocab (`^[A-Za-z0-9_.-]+$` or the job refuses). Spark job is the source of truth;
  `model_training/features.py` is the offline reference ("if they disagree, the
  Spark job wins").
- **B3 registry-lite:** the eval gate scores the LOADED joblib bytes
  (`store().get()` + `joblib.load(BytesIO)`) — gated model == shipped model.
  Manifest + `promoted/current.json` pointer written ONLY on gate pass; metrics.json
  persists either way. `provenance.code_sha()` = `git rev-parse HEAD` with a 2 s
  timeout → env `CODE_SHA` → `"unknown"` (images exclude `.git`); provenance must
  never fail the gate.
- **B4 eval:** live tests carry BOTH `eval` and `integration` markers (default unit
  selector never runs them); run via `task eval` after `task up-test` + `task up-ai`.
  Compose host ports mirror in-network port numbers (ollama 11434, qdrant 6333),
  so host-side runs override only hosts. Scoring is execution accuracy — compare
  RETURNED rows via the same RLS repository, never SQL strings. Ratchets are lenient
  for the local 3B model (text2sql ≥0.6, rag recall ≥0.5, citation coverage ≥1.0).
  Observability records (`llm_generate`/`llm_embed`) never contain prompt/response text.
  **Live-run lessons (2026-08-25, first green pass):** (1) ORDERING — `task up-ai`'s
  bare `up -d` RE-CREATES postgres without the host-port overlay, silently undoing a
  prior `task up-test`; apply up-test LAST or re-run it before host-side suites.
  Symptom when hit: text2sql accuracy collapses (schema provider "connection refused"
  → model gets no schema → hallucinated tables). (2) CITATIONS — the 3B model skips
  `[n]` markers under soft instructions; `_ANSWER_PROMPT` (rag/query.py) now mandates
  them ("an answer without at least one [n] marker is invalid") → coverage 1.0.
  Ratchets were NOT lowered to get there.
- **A8 auth:** fail-closed by design — unauthenticated callers now get 401. Keys =
  `AGENT_API_KEYS` (JSON tenant→key map; dev default in settings, override in `.env`);
  pipeline caller sends `AI_AGENTS_API_KEY`; `task smoke-agent` carries the header.
  Known follow-up: `graph.py` reflects only the LAST error (`history[-1]`).

### Shared backing services (ARCHITECTURE dec. 4)
- One shared `postgres` (PG18) hosts 6 DBs: dagster, marquez, appdb, openmetadata,
  nessie, polaris (`postgres/init/00-databases.sql`). OpenMetadata uses the shared
  `opensearch/` stack for search — no component bundles its own backing store.
- **MinIO** is bound to `HOST_BIND` on both ports (default 0.0.0.0 since
  2026-08-16 — before that, loopback; the ORIGINAL 0.0.0.0-default-creds P0 was
  fixed 2026-08-07); root password rotated in the untracked `.env` (regenerate:
  `openssl rand -base64 24`; takes effect on next `docker compose up -d`).
  Buckets: `raw` + `warehouse` + `models` + `spark-logs` (event logs; staging/curated
  retired with the lakehouse). `spark/jobs/lakehouse_catalog.py` fails fast on missing
  MINIO_ACCESS_KEY/MINIO_SECRET_KEY — no default-credential fallbacks anywhere.

---

## Verify cheatsheet

```sh
task --list
task lint typecheck test                       # ruff + mypy + 295 unit tests
task build                                     # rebuild the four built images (fixed tags don't auto-rebuild)
task smoke-lakehouse                           # Spark → {Nessie,Polaris} → DuckDB round-trip
task smoke-ai                                  # Ollama chat + 1024-dim embedding + Qdrant /readyz
task materialize                               # raw→staging→curated→feature_orders_daily→metadata_sync→rag_ingest (+auto-retrain sensor)
task eval                                      # B4 LLM/RAG goldens — needs `task up-test` + `task up-ai`
docker compose --profile lakehouse config -q   # CI runs this (all profiles + .env.example)

# Phase 8 model_training — GraphQL against the webserver (localhost:3000/graphql):
# mutation launchPipelineExecution(executionParams:{selector:{repositoryLocationName:
#   "model_training.definitions", repositoryName:"__repository__",
#   pipelineName:"model_training_pipeline"}})
# gate-failure demo: docker compose run --rm -e TRAINING_EVAL_THRESHOLD=0.999 \
#   dagster-webserver python -c '...materialize([feature_orders,trained_model,model_eval])...'

# Text-to-SQL agent (Phase 6) — needs `task up-ai` (waits for the model pull);
# since A8 every route except /healthz needs the tenant Bearer key:
curl -s localhost:8000/text2sql/query -H 'Authorization: Bearer dev-insecure-agent-key' \
  -H 'Content-Type: application/json' \
  -d '{"question":"total order amount per customer","tenant_id":"tenant_a"}'
# RAG agent (Phase 7) — same stack (any valid key):
curl -s localhost:8000/rag/ingest -H 'Authorization: Bearer dev-insecure-agent-key' \
  -H 'Content-Type: application/json' -d '{"path":"corpus"}'
curl -s localhost:8000/rag/query  -H 'Content-Type: application/json' -d '{"question":"..."}'
uv run pytest -m integration ai_agents/tests/test_agent_rls_integration.py   # needs `task up-test`
```
