---
id: kb-0020
title: "Hermes anthropic provider mangles dotted model names; ai-box.vn needs them verbatim"
type: gotcha
domain: tooling
tags: [hermes, ai-box, llm-proxy]
aliases: [dot hyphen model mangling, 503 Không có kênh khả dụng, hermes named provider]
triggers:
  - "hermes returns 503 or rejects a dotted model name like glm-5.2"
  - "adding a new model alias to the hermes aibox provider"
status: verified
confidence: high
created: 2026-08-05
updated: 2026-08-05
supersedes: []
related: [kb-0017, kb-0018]
source: "session 2026-08-03: code read of hermes_cli/model_normalize.py + runtime_provider.py (issue #28660)"
summary: "Hermes' anthropic provider rewrites dots to hyphens in model names (glm-5.2 → glm-5-2); ai-box.vn rejects that with HTTP 503. Fix: named custom provider with key_env; dot-free models unaffected."
---
`hermes_cli/model_normalize.py::normalize_model_for_provider` converts dots to hyphens for the `anthropic` provider (built for OpenRouter `claude-opus-4.6` → `claude-opus-4-6`). ai-box.vn's distributor rejects the mangled name with HTTP 503 "Không có kênh khả dụng cho model glm-5-2" — it needs `glm-5.2` exactly. Dot-free proxy models (`deepseek-v4-pro`, `deepseek-v4-flash`) work under any provider.

The `custom` provider passes names through untouched, and the proxy speaks OpenAI wire at `/v1/chat/completions`, so `provider: custom` is the working route — with one sub-gotcha:

**Sub-gotcha (key resolution):** bare `provider: custom` only falls back to the `OPENAI_API_KEY` env var for openai.com/openrouter hosts (`runtime_provider.py`, gated per upstream issue #28660); for other hosts it sends a `no-key-required` sentinel and gets 401. Workarounds: `model.api_key: ${OPENAI_API_KEY}` substitution, or better a **named provider entry with `key_env:`** — the current setup (`providers: aibox: {base_url: https://api.ai-box.vn/v1, key_env: OPENAI_API_KEY}`), which resolves cleanly and makes the endpoint reusable by all `model_aliases`.

## Related
- [[kb-0017]] — deployment + the full model_aliases list on this provider
- [[kb-0018]] — Claude Code's tier routing over the same ai-box.vn proxy
