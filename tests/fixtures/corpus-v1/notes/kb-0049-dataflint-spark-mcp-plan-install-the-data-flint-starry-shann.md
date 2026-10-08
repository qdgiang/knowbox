---
id: kb-0049
title: "DataFlint + spark-mcp plan: DataFlint committed; MCP bridge REJECTED by user"
type: project
domain: projects
tags: [e2e-data-ai-project,plan,spark,dataflint,mcp,rejected]
aliases: [install-the-data-flint-starry-shannon, dataflint plan, spark-mcp plan]
triggers:
  - "asked about MCP integration for Spark/DataFlint observability in the e2e stack"
  - "asked about DataFlint or Spark observability in the e2e stack"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-25
supersedes: []
related: [kb-0048, kb-0007, kb-0015]
source: "session: plan mode 2026-08-17"
summary: "Pointer: DataFlint install done (checkpoint 76f87e7); the local spark-mcp bridge was planned but REJECTED by the user 2026-08-25 — do not resurface unprompted. IcebergCommitEvent now fires (08-24 logs)."
---
**State: rejected 2026-08-25: user dropped the MCP bridge ("I don't want the MCP Flint anymore").** DataFlint install itself is done and committed. Do NOT resurface the spark-mcp plan without an explicit new request.

- **DataFlint done:** `io.dataflint:spark_2.12:0.9.9` wired into the Spark stack — spark/Dockerfile, entrypoint.sh, `data_pipeline/.../spark_submit.py`, tests, ARCHITECTURE + IMPLEMENTATION_PLAN docs; UI tab landed. Committed in e2e checkpoint `76f87e7` (2026-08-19; see [[kb-0048]]).
- **spark-mcp rejected:** planned (never started, no code existed), then rejected at plan approval 2026-08-25. The design remains on record in `~/.claude/plans/install-the-data-flint-starry-shannon.md` + subagent draft `…-agent-a997a30f8949ade7a.md` (uv-workspace `spark_mcp/` package, `.mcp.json`, HS REST + MinIO event-log parsing) in case the user explicitly asks again. Official `@dataflint/mcp-server` remains unsuitable regardless: SaaS-only OAuth + npx (banned on this box, [[kb-0015]]).
- **IcebergCommitEvent resolved (2026-08-25 probe):** zero events in all pre-08-24 logs despite reporter config, but present (2/app) in the 2026-08-24 `iceberg-exp01*` experiment logs — Polaris, Hadoop AND Hive catalog writes — with only the existing `spark.dataflint.iceberg.autoCatalogDiscovery=true` flag. No config diff explains the gap; suspected remaining variable is REST-submission vs spark-submit-CLI path (unverified). The 08-24 apps came from a separate session's HMS comparison-catalog experiments (`hive/` stack, `iceberg-hive-metastore-1.8.1` jar — left uncommitted in the e2e tree).
- Next: nothing pending here.

## Related
- [[kb-0048]] — same e2e tree; DataFlint rode its checkpoint commit; quality-layer plan
- [[kb-0007]] — e2e project map + one-off git identity
