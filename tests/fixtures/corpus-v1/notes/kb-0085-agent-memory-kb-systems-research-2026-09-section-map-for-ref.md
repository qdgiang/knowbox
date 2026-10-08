---
id: kb-0085
title: "Agent-memory KB systems research (2026-09): section map for refs/agent-memory-kb-systems-research-2026-09-05.md"
type: reference
domain: kb
tags: [kb, agent-memory, research, conflict-resolution, supersession, staleness]
aliases: [basic-memory, mem0, zep, graphiti, ai-memory, sqlite-memory, contradiction lint, SemDeDup, MADR, dead man's switch]
triggers:
  - "designing or fixing conflict/supersession/staleness mechanisms in ~/kb"
  - "evaluating agent-memory tools or patterns"
  - "reviving the KB maintenance cron or adding contradiction detection"
status: active
confidence: high
created: 2026-09-05
updated: 2026-09-05
supersedes: []
related: [kb-0011, kb-0038, kb-0014]
source: "refs/agent-memory-kb-systems-research-2026-09-05.md"
summary: "How comparable agent-memory systems handle conflicts, supersession, synthesis, stale indexes, silent cron failure - verified mechanisms mapped to ~/kb's audited weak points."
---
Adversarially verified survey (26 sources, 127 claims, 25 triple-voted: 22 confirmed,
3 refuted) of open-source CLI-first markdown+git agent-memory systems, run after the
2026-09-05 three-agent audit of ~/kb. Read via the section map below + offset reads of
`refs/agent-memory-kb-systems-research-2026-09-05.md` — never the whole file.

## Section map

## 1. Executive summary — weak-point→solution-pattern table; field consensus: markdown-as-truth + derived rebuildable index (validates kb-0011 deeper).
## 2. Architecture consensus — cross-agent-memory, basic-memory, ai-memory, sqlite-memory, memweave/memsearch quotes; embeddings deferred until measured need. (high, 3-0)
## 3. Stale derived indexes — watchers, content-hash change detection, SAVEPOINT-atomic replacement, `doctor` file↔DB check; maps to audit H1 fix (mtime+sha1 in .kb.db). (high)
## 4. Mem0 write-time conflict resolution — LLM tool-call ADD/UPDATE/DELETE/NOOP over top-10 embedding-similar notes; cost-control for contradiction lint; OSS-vs-paper divergence caveat. (high, 3-0)
## 5. Supersession without deletion — Zep/Graphiti bi-temporal valid_at/invalid_at, Mem0g soft-obsolete, ai-memory is_latest=false+supersedes, MADR frontmatter lifecycle; ~/kb schema already matches, gap is automation. (high, 3-0)
## 6. Cross-note synthesis — basic-memory typed-relation grammar (`- relation_type [[Target]]`) + build_context multi-hop; SemDeDup cluster-then-cosine for cheap overlap reports. (high, 3-0)
## 7. memory_lint (ai-memory) — the contradiction lint kb-0038 deferred: declared `contradicts:` edges (zero-LLM) + optional LLM pass, output as git-committable markdown, rule-promotion suggestions. (medium, single-vendor)
## 8. Alternative precedence — provenance-gated promotion (cross-agent-memory, weak sourcing), tool-level overwrite guards (basic-memory), block-level LWW CRDT (sqlite-memory). (medium, split votes)
## 9. Silent maintenance failure — healthchecks.io dead man's switch; local equivalent: heartbeat file + lint ERROR when older than schedule. Fixes the kb-0014 cron class.
## 10. Caveats + refuted claims — 3 refutations bounding the findings; claude-mem has NO invalidation (same hole, most popular tool).
## 11. Open questions — write-time pass cost, tombstone graveyard effects, PR-review+auto-detection hybrid unbuilt.
## 12. Sources — primary repos/papers/docs + lower-weight blogs/forums (one reddit angle failed to fetch).

⚠ Vendor papers: mechanisms cited, self-reported efficacy numbers NOT relied on.
⚠ Mem0 OSS v3 diverged from paper (graph memory removed, ADD-only allegation) — cite arXiv:2504.19413.

## Related
- [[kb-0011]] — the 2026-08-02 tool-landscape survey this report deepens and partially updates (same convergence verdict, plus per-weakness mechanisms).
- [[kb-0038]] — KB self-review that deferred "contradiction lint"; §4/§7 of this report are exactly that deferred item, with shipped implementations to copy.
- [[kb-0014]] — weekly janitor cron (silently dead per 2026-09-05 audit); §9 dead man's switch is the external fix pattern for its failure class.
