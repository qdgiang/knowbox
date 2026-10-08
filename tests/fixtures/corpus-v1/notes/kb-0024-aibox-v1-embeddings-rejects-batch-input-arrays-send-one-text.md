---
id: kb-0024
title: "aibox /v1/embeddings rejects batch input arrays: send one text per request"
type: gotcha
domain: tooling
tags: [aibox, embeddings, kb-embed]
aliases: [text-embedding-v4, embedding batch 400, api.ai-box.vn embeddings]
triggers:
  - "calling api.ai-box.vn/v1/embeddings or building anything embedding-based against the aibox proxy"
status: active
confidence: high
created: 2026-08-06
updated: 2026-08-06
supersedes: []
related: [kb-0001, kb-0017]
source: "session"
summary: "aibox proxy embeddings endpoint (text-embedding-v4) returns HTTP 400 on batch input arrays - one text per request. kb embed does batch-first with per-item fallback."
---
`https://api.ai-box.vn/v1/embeddings` (auth: the same `ANTHROPIC_AUTH_TOKEN`
as the chat endpoints) returns HTTP 400 when `input` is an array of multiple
texts, even though the OpenAI-compatible spec allows batch arrays. Send one
text per request.

Verified 2026-08-06 with model `text-embedding-v4`: a single-string request
→ 200; a 16-item batch array → 400.

## How the KB handles this

`kb embed` sends a batch first and falls back to per-item requests on error,
so it keeps working (and gets faster automatically) if the proxy ever starts
accepting batches. Embed text = title + summary + tags + aliases + triggers +
body[:2000]; incremental via sha1 of that text; vectors stored in
`.kb-embed.json` (gitignored, rebuild on model change).

Score calibration on the 22-note corpus: real hits cluster at cos 0.45–0.51
(e.g. "screen blanks by itself" → kb-0004 @0.508), noise ≤0.21 (gibberish
queries). The BM25-miss rescue in `kb search` therefore thresholds at 0.30;
below that the query is logged to `.search-misses.log` instead of surfacing
junk. Explicit semantic lookup: `kb vsearch`.

Related: [[kb-0001]] — the KB design this embedding layer plugs into;
[[kb-0017]] — other aibox proxy quirks (lists text-embedding-v4 as not wired
for the Hermes gateway).
