---
id: kb-0038
title: "2026-08-11 KB review: precision-first search, CLI hardening, distiller write-gate; porter+contradiction-lint deferred"
type: decision
domain: kb
tags: [knowledge-base,]
aliases: [kb review 2026-08, search precision changes]
triggers:
  - "why does kb search behave AND-first"
  - "what did the 2026-08-11 review change"
  - "proposing search or pipeline changes to the KB"
status: verified
confidence: high
created: 2026-08-11
updated: 2026-08-11
supersedes: []
related: [kb-0001, kb-0011, kb-0026]
source: "session 2026-08-11: comprehensive review (3 audit agents + web research sweep)"
summary: "2026-08-11 review: recall excellent (HIT@1 94%); weakness was absent-topic precision. Adopted AND-first search + 0.40 rescue gate, CLI hardening, distiller write-gate. Deferred: porter, contradiction lint."
---
A full review (retrieval battery, CLI code review, content audit, internet research sweep) found the architecture validated — markdown-canonical + derived index is where the whole industry converged (AGENTS.md in 60k+ repos; Mem0/Zep/Letta all chasing graph/temporal layers we deliberately avoid per [[kb-0011]]) — but surfaced one systemic weakness and several defects, now fixed.

## Verdict

Recall is excellent: 38-query adversarial battery scored HIT@1 94%, HIT@3 100%, every present query resolved in one iteration. **The weakness was precision in the absent case**: pure-OR BM25 returned full-confidence wrong notes for topics not in the KB (2/3 absent queries false-positived), and semantic rescue fired below its own calibration band.

## Adopted (2026-08-11)

- **Search**: AND-first with OR-fallback (announced as "partial match"), rescue threshold 0.30→0.40 capped at 3 results (kb-0024 calibration: real hits ≥0.45, noise ≤0.21), no prefix expansion for <3-char tokens (diacritics-stripped `co*` explosions), stopword list extended, scores surfaced via labels.
- **CLI hardening** (code review, empirically tested): atomic DB/index rebuilds (temp file + `os.replace`), rebuild-on-corrupt retry in search, `flock` around id allocation (parallel `kb new` race), path-traversal guard in `kb show`, ambiguous-slug refusal, git add/commit/push failures now printed instead of swallowed, `$EDITOR` with args, scoped `git add` (no more `-A`).
- **Content**: 17→1 lint warnings; 5 oversized summaries rewritten; triggers/aliases added to 6 keywordless notes; related-link and provenance gaps closed; gh-credential fact extracted after being proposed 3× by the distiller (kb-0036); multi-agent hub (kb-0037); kb-0017 de-volatilized (pointers instead of copied model lists); embeddings backfilled 27→35.
- **Pipeline**: distiller now reads index.md and skips already-covered facts (write-gate at capture time — the distiller had been re-proposing the same facts 2-3×); weekly `kb embed` cron beside the lint cron; SCHEMA documents capture→promotion→consolidation stages.

## Deferred (with triggers)

- **FTS5 porter stemmer**: prefix expansion already covers most morphology (query "chown" hits "chowns"); porter risks token-alignment bugs on error strings. Trigger: measured paraphrase misses in .search-misses.log that stemming would fix.
- **Automated contradiction lint**: needs an LLM pass (structural lint can't see semantic conflict); run during manual janitor passes for now (ask a fresh agent, different model). Trigger: corpus >100 notes or a wrong-answer incident.
- **KB-MCP server, vector graduation**: unchanged from [[kb-0026]] deferrals.

## Related
- [[kb-0001]] — the design this review re-validated
- [[kb-0011]] — landscape validation + graduation triggers
- [[kb-0026]] — harness roadmap whose exclusions were honored
