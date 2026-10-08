---
id: kb-0062
title: "CI format gates need exact-pinned formatters (ruff==x, never >=) — a floor-only pin broke ruff format --check the day ruff 0.15.20 shipped"
type: gotcha
domain: python
tags: [ci,ruff,formatting,pinning,reproducibility]
aliases: [ruff pin, formatter pin, format gate red, exact pin formatters]
triggers:
  - "ruff format --check fails in CI with no code changes"
  - "pinning formatters or linters in pyproject"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0048, kb-0015]
source: "raw/session-distill-2026-08-16.md"
summary: "2026-08-15 incident: floor-pinned ruff>=0.11 resolved to 0.15.20 in CI, reformatted 8 files, broke format --check. Fix: exact-pin formatters/linters, never >=."
---
On 2026-08-15 e2e-data-ai-project's `ruff format --check` CI gate failed on 8 files with zero code changes: `pyproject.toml` pinned `ruff>=0.11`, and a fresh CI resolve pulled ruff 0.15.20, which formats differently than the version that originally wrote the files. Formatters have no compatibility contract across minor releases, so a floor-only pin gives every install a potentially different formatter.

Fix pattern applied (dev-tooling only; runtime deps left floor-pinned): change `ruff>=0.11`, `mypy>=1.10`, `pytest>=8.0`, `pytest-cov` to exact versions (ruff 0.15.20, mypy 2.1.0, pytest 9.1.1, pytest-cov 7.1.0 at the time), then run `ruff format .` once to bring the tree in line with the pinned version before merging.

General rule: pin formatters and linters precisely — `>=` floors remain acceptable for runtime libraries whose APIs you test against, but a formatting gate enforces byte-level determinism, and any newer formatter breaks it.

## Related
- [[kb-0048]] — e2e quality-layer plan: the CI format/lint gates this pinning rule keeps green
- [[kb-0015]] — uv rules: dev deps live in the uv-managed env; exact pins belong in the project's dev dependency set
