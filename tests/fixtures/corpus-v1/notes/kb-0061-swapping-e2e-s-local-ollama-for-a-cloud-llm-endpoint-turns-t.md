---
id: kb-0061
title: "Swapping e2e's local Ollama for a cloud LLM endpoint turns the self-correction loop into a tenant-data exfiltration path — sanitize echoed DB errors first"
type: gotcha
domain: projects
tags: [security,llm,ollama,exfiltration,rls,text-to-sql]
aliases: [llm provider swap, ollama to cloud, exfil path, provider swap constraint]
triggers:
  - "swapping e2e's LLM provider off local Ollama"
  - "reviewing e2e security before any provider or vector-store change"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0007, kb-0048]
source: "raw/session-distill-2026-08-16.md"
summary: "Security constraint: swapping e2e's local Ollama for a cloud LLM turns the self-correction loop's echoed DB errors into tenant-data exfiltration — sanitize before any swap."
---
The text-to-SQL agent's three-layer defense (guardrail denylist, SELECT-only role, fail-closed RLS) is only as strong as what leaves the machine. The LangGraph self-correction loop keeps the FULL PostgreSQL error text — column values, SQL fragments — in graph state to feed the next LLM attempt; by design that error text never reaches the API caller. This containment holds only while the LLM call stays local (Ollama on 127.0.0.1).

The swappability review rated the LLM provider port (a one-method `LLMPort` protocol, fake-LLM tested) a clean swap — but flagged the consequence: the moment the provider moves to any cloud endpoint, the echoed DB rows/SQL leave the host, turning self-correction into a tenant-data exfiltration channel in exactly the area (RLS binding) the whole security story depends on.

Required before any provider swap: sanitize/abbreviate exceptions before they are embedded in subsequent prompts (the error-append points in `ai_agents/src/ai_agents/graph.py`), independent of prompt-history trimming and observability logging.

The same reasoning applies to the vector-store port (Qdrant→pgvector/LanceDB): per-tenant payload filters (`tenant_id`) must accompany the swap, because the current `search()` takes no filter.

## Related
- [[kb-0007]] — e2e-data-ai-project map: where the agent graph, LLMPort, and RLS layers live
- [[kb-0048]] — e2e quality-layer plan: this constraint gates any provider/infra swaps the plan schedules
