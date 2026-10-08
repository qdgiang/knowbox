---
id: kb-0084
title: "AI-writing tells catalog + de-AI-ing checklist: section map for refs/2026-09-03-ai-writing-tells-research.md"
type: reference
domain: preferences
tags: [writing, blog, ai-detection, style]
aliases: [AI slop, AI-isms, humanize AI text, AI detection tells, AI vocabulary, ChatGPT writing signs, em dash tell]
triggers:
  - "editing an AI-drafted blog post to sound human"
  - "checking a draft for recognizable AI-writing patterns"
  - "wondering which words or phrases mark text as AI-generated"
status: active
confidence: high
created: 2026-09-03
updated: 2026-09-03
supersedes: []
related: [kb-0083, kb-0043]
source: "deep-research workflow 2026-09-03: 20 sources, 93 claims extracted, 25 adversarially verified (3-vote panels), 18 confirmed, 7 refuted"
summary: "Verified catalog of AI-writing tells (lexical/syntactic/rhetorical/structural) + 8-step checklist to de-AI blog drafts; the tell is density, not single occurrences."
---
Adversarially verified catalog of how readers recognize AI-generated text — deep-research output (18 confirmed claims, 7 refuted), kept whole in `refs/2026-09-03-ai-writing-tells-research.md`. The core principle across all findings: every tell is probabilistic; **density and repetition are the tell, never a single occurrence** (em dashes, triads, hedges, and "not A, but B" are all legitimate human devices).

## Section map (refs/2026-09-03-ai-writing-tells-research.md)

- **Summary** — the four levels of tells (lexical / syntactic / rhetorical / structural) + why a checklist beats gut feel (human detection accuracy tops out at ~57%, and judges key off exactly these surface cues).
- **Findings (verified)** — F1–F7 with confidence, evidence, votes, sources:
  - F2 Lexical: high-density AI vocabulary (delve, tapestry, crucial, pivotal, robust, testament, landscape, meticulous…) + formulaic phrases ("Dive into", "Navigating the landscape", "a stark reminder", "left an indelible mark", "Certainly, here are"). Density is the tell — empirically backed by arXiv corpus-spike studies.
  - F3 Negative parallelisms: "not X, but Y" / "it's not about X, it's about Y" / "no…, no…, just…" — the exact "not A, not B — it's C" pattern from kb-0083.
  - F4 Rule-of-three triads, em-dash overuse (1–2 fine, 5/section is the tell), copula avoidance ("serves as/functions as" instead of "is").
  - F5 Rhetorical: excessive hedging ("typically", "often", "may") + servile positivity (sanding specific facts into generic upbeat generalities). Most actionable pair for technical blogs.
  - F6 Structural: uniform paragraph lengths with tidy wrap-up lines, stacked "Moreover/Furthermore/Consequently", filler frames ("it is important to note that", "in today's fast-paced world"). Medium confidence.
  - F7 Non-prose residue: pasted chatbot replies, Markdown bleed-through, emoji formatting, broken citations — cheapest wins, fix before prose.
- **Refuted claims — do NOT cite** — 7 claims that failed adversarial review, incl. the "hundreds of times more likely" multipliers, the "27-word average sentence", the "metronome rhythm" claim, and the causal "training data skews formal" explanation (contradicted by COLING 2025).
- **Caveats** — all tells probabilistic; tells migrate across model generations (lexical layer most volatile — "delve" already fading by 2025; structural/rhetorical layers durable); over-applying the checklist has its own tells (forced fragments, fake irregularity read as artificially humanized).
- **Open questions** — model-version-tagged inventories, tells in technical writing specifically, whether humanization creates its own uncanny-valley style, density thresholds.
- **Practical checklist: de-AI-ing a blog draft** — 8 steps ordered cheapest-first: strip residue → kill filler frames → hunt AI vocabulary (3–4 flagged words per paragraph → rewrite) → break negative parallelisms → defuse triads and em dashes → commit (delete hedges, add concrete numbers/named tools) → vary rhythm naturally → keep legitimate structure.
- **Sources** — the ~15 primary sources (Wikipedia "Signs of AI writing" guide is the richest single catalog; PLOS ONE detection study; COLING 2025 delve paper; editor/writer blogs; Reddit/HN threads).

## Related
- [[kb-0083]] — the standing blog-style decision (no AI-isms, plain prose for beginners); this catalog is the enforcement reference for its "no recognizable AI-writing patterns" rule
- [[kb-0043]] — Iceberg deep-dive blog curriculum, where this checklist gets applied
