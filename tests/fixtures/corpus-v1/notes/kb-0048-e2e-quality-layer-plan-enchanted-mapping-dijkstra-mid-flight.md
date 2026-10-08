---
id: kb-0048
title: "e2e quality-layer plan (enchanted-mapping-dijkstra): mid-flight, checkpoint committed"
type: project
domain: projects
tags: [e2e-data-ai-project,plan,quality]
aliases: [enchanted-mapping-dijkstra, e2e quality layer plan, e2e stabilization plan]
triggers:
  - "continuing work on e2e-data-ai-project quality/stabilization"
  - "asked what is pending or in flight in the e2e repo"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0007, kb-0049]
source: "session: plan mode 2026-08-15"
summary: "Pointer: e2e quality plan ~/.claude/plans/enchanted-mapping-dijkstra.md (A1-A8 + B1-B5). Mid-flight: A1 done; 86-file WIP checkpoint-committed as 76f87e7; per-item commits pending."
---
**State: mid-flight (as of 2026-08-19)** — approved 2026-08-15; the whole 86-file working tree was checkpoint-committed as `76f87e7` on 2026-08-19 (one-off identity per [[kb-0007]]). Per-item plan-order commits (A1→…→B4) were NOT made — audit the tree against plan items before continuing.

- Plan file: `~/.claude/plans/enchanted-mapping-dijkstra.md` — full per-item designs + ground rules.
- Scope: Phase A (A1–A8, mechanical stabilization: pin tooling, config validation `extra="forbid"`, NULL-cast DQ hole, inverted mae gate, API auth/tenant/path-traversal fixes, ingest schema contract) + Phase B (B1–B5: retraining trigger, eval harness + LLM observability, lineage-to-appdb, features as Iceberg table + registry-lite manifest). Commit order in plan: A1→…→A8→B5→B1→B2→B3→B4.
- Confirmed landed (now inside `76f87e7`): **A1** — `ruff==0.15.20` pinned in pyproject (the red `ruff format --check` gate); **CLAUDE.md KB pointer** fixed to `~/kb/refs/e2e-data-ai-project.md` (live break #2).
- **Checkpoint commit `76f87e7` (2026-08-19)** swept all 86 files (data_pipeline, ai_agents compose/app/graph/guardrails/rag, model_training, spark, Taskfile, .env.example, …) — consistent with A3/A6/A7/A8 + B-series work + DataFlint ([[kb-0049]]); verify which plan items are actually done before continuing.
- Ground rules (from the plan): suite green after every commit (`ruff format --check`, `ruff check`, `mypy`, pytest non-integration); Settings/YAML changes require the local config-validation probe; no cross-stack imports; Spark is the only Iceberg writer.
- Deferred by the plan: vector-DB widening, MLflow, Feast, full Phase 9 (README/demo/nightly CI), dagster infra-stack extraction.
- Next: audit the checkpoint against plan items, get the suite green; history is one big checkpoint rather than per-item commits.

## Related
- [[kb-0007]] — e2e project map + one-off git identity rule
- [[kb-0049]] — DataFlint work committed in the same checkpoint
