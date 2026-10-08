---
id: kb-0001
title: "KB design: index-first, atomic notes, lexical search"
type: decision
domain: kb
tags: [knowledge-base,design]
aliases: [second brain, agent memory, llm wiki]
triggers:
  - "setting up or changing the knowledge base"
  - "why is the KB structured this way"
  - "should we add a vector DB"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0009, kb-0011]
source: "research synthesis 2026-08-02"
summary: "Evidence base for ~/kb: index-first markdown + FTS5 beats RAG/vector DBs at this scale (context rot; lexical suffices). Vectors only worth revisiting past ~200 notes. Tool comparison: kb-0011."
---
The KB is a small always-readable index plus atomic notes, queried lexically — not RAG, not a vector DB. This follows from four converging evidence streams (researched 2026-08-02).

## The evidence

1. **Context rot is real and gradient.** Chroma tested 18 frontier models: reliability degrades with input length even on trivial tasks, and *a single distractor measurably hurts*. Contradictory content is worse than volume: one stale contradicting note damages more than ten missing ones. Goal = smallest high-signal token set (Anthropic's context-engineering post).
2. **Lexical search is enough at this scale.** Amazon (AAAI 2026): an agent with only keyword tools hits ~90% of vector-RAG quality. "Direct Corpus Interaction" (arXiv 2605.05242): agents with grep/read beat dense retrievers on BEIR/BRIGHT. FTS5/BM25 is ideal for keyword-dense queries (error codes, tool names) where embeddings blur exactly what matters. Vectors only pay off >200 notes with synonym-heavy conceptual queries — revisit then.
3. **The index is the product.** Every working system reduces to a one-line-per-note catalog read first (Claude's MEMORY.md, llms.txt, Karpathy's index.md). Karpathy's LLM-wiki gist (2026-04): raw/ immutable + LLM-written wiki + schema file; ops = ingest/query/lint. His weak point: hand-maintained index → drift. Our fix: index.md is *generated* from frontmatter `summary:` fields; curation goes into the summary.
4. **Rot starts at write time.** mem0's ADD/UPDATE/DELETE gate, ChatGPT Dreaming, Claude's 200-line MEMORY.md cap all decide durability at write. Observed verbosity drift: entries grow 4-8× in two weeks without a pruning force. Our forces: 400-word budget (lint-enforced), search-before-create gate, supersede-don't-delete, git as undo.

## Rules that follow

- Max 2 reads to an answer: index → note. Or one `kb search`.
- Claim-style titles; load-bearing fact in sentence one (lost-in-the-middle).
- Two shapes: atomic notes (≤400 words) and whole refs/ docs fronted by section-map notes (chunking destroys semantic units).
- Deterministic hygiene in the CLI (lint, auto-dates, auto-commit, log), not prose.
- Tool-agnostic: plain markdown + AGENTS.md convention block + `kb` on PATH; works for Claude Code, OpenCode, Copilot alike.

## Related
- [[kb-0009]] — which agent tools are on this box and how they load context
- SCHEMA.md — the operational contract
