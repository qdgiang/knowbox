---
id: kb-0070
title: "ai-box.vn provider: check https://api.ai-box.vn/docs before configuring anything"
type: howto
domain: tooling
tags: [ai-box, llm-proxy, configuration]
aliases: [ai box, aibox, ai-box docs]
triggers:
  - "configuring anything that talks to the ai-box.vn LLM proxy"
  - "choosing or wiring up models/endpoints served by ai-box"
  - "debugging model names, endpoints, or auth against api.ai-box.vn"
status: active
confidence: high
created: 2026-08-24
updated: 2026-08-24
supersedes: []
related: [kb-0017, kb-0018, kb-0020]
source: "session"
summary: "User directive: before configuring anything for the ai-box.vn AI provider, consult https://api.ai-box.vn/docs for models, endpoints, and API details."
---
Before configuring anything related to the ai-box.vn AI provider (models, endpoints, keys, base URLs, client wiring), check its documentation at https://api.ai-box.vn/docs.

User directive (2026-08-24): this site is the authoritative source for what the proxy serves — available models, endpoint names, and API behavior. Consult it first instead of guessing model names or assuming OpenAI-compatible defaults; the endpoint/model list is volatile, and docs beat stale notes or memory.

Applies to every consumer of the proxy: Hermes (`~/.hermes/config.yaml`), Claude Code's `ANTHROPIC_BASE_URL`/`OPENAI_BASE_URL` routing, and any future tool pointed at `https://api.ai-box.vn/v1`.

## Related
- [[kb-0017]] — Hermes gateway runs through this proxy; model aliases must match exact endpoint names
- [[kb-0018]] — Claude Code tier routing over the same proxy
- [[kb-0020]] — model-name mangling gotcha on this proxy
