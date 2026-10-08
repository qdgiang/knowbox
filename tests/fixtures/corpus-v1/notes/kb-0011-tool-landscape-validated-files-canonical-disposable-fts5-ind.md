---
id: kb-0011
title: "Tool landscape validated: files canonical + disposable FTS5 index is the converged architecture"
type: decision
domain: kb
tags: [knowledge-base,architecture,validation]
aliases: [why not mem0, why not vector db, why not graphiti]
triggers:
  - "considering adding vectors/embeddings or a memory product to the KB"
  - "comparing mem0/letta/zep/graphiti"
  - "is grep really enough"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0001]
source: "OSS landscape research 2026-08-02 (4-agent sweep)"
summary: "What the winners shipped: markdown canonical + derived index (Basic Memory, zk, nb, ReMe, Claude Code itself). When to graduate: measured recall failure, temporal queries, multi-agent shared memory."
---
A 2026-08-02 survey of every notable agent-memory project independently validated ~/kb's architecture — and shows exactly when to revisit it.

## What the winners shipped

- **Anthropic removed RAG from Claude Code on purpose**: early versions had a local vector DB; Boris Cherny: "agentic search generally works better — simpler and doesn't have the staleness issues. Plain glob and grep, driven by the model, beat everything."
- **Letta's own benchmark**: a filesystem+grep agent scored 74.0% on LoCoMo vs mem0's 68.5%. mem0's full-context baseline (~73%) beat mem0 in its own paper.
- **The credible OSS tools converged on markdown-as-truth + disposable index**: Basic Memory (files canonical, SQLite derived, `basic-memory reset` rebuilds), zk (FTS5 as disposable cache), nb (markdown in git), Alibaba's ReMe (rebranded to markdown+wikilinks+BM25), obsidian-second-brain (measured keyword recall@10 = **1.0** on 2,350 notes; paraphrase 77%).
- **Vendor benchmarks are unusable**: locomo-audit found judges accepted 62.8% of deliberately wrong answers. Zep claims 94.7%, measures 77–80% internally, independent ENGRAM measured 42%.

## Cautionary tales (why no hooks-daemon capture, no dual source of truth)

- **claude-mem** (~89k★): broken releases, 296 orphaned processes/12GB leaks, one user burned $17.72 in 3h on hidden observer subagents, unpinned `uvx` supply-chain hole.
- **Ruflo/claude-flow** (~67k★): audit found "99% fake" — ~85% stub tools, fabricated benchmarks. Stars ≠ substance.
- **mem0 v2.x** quietly dropped ADD/UPDATE/DELETE for ADD-only → contradictory memories stored side-by-side (issue #5867); 23% of stored memories had fabricated timestamps in one audit.
- **Graphiti**: the one uncontested differentiator (bi-temporal point-in-time queries), but ingestion cost is brutal — one user measured 24,000 API calls/41M tokens in 2 hours.
- **Zep CE deprecated** (issue tracker deleted); **Letta classic server now "legacy"**; **Cognee** had a CVSS 9.8 RCE.

## Our lightweight temporal model

`supersedes:` + `created/updated` + `status: superseded` + git history = bi-temporal validity at personal scale, zero LLM extraction cost.

## Graduate to a heavier tier ONLY on measured failure

1. **Paraphrase/vocabulary recall** fails empirically (query rephrased 2-3× still misses) → add a *derived* embedding index, rebuildable, never canonical.
2. **Multi-agent shared memory** across machines/users → consider Graphiti-style server.
3. **Corpus >100K files** → grep degrades; not foreseeable here.
Also considered, rejected: `autoMemoryDirectory` (points Claude auto-memory at ~/kb) — mixes their frontmatter format into our schema; the pointer-note division of labor is cleaner for multi-tool use.

## Related
- [[kb-0001]] — the design this validates
