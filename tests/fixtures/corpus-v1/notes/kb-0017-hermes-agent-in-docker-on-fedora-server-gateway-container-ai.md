---
id: kb-0017
title: "Hermes Agent in Docker on fedora-server: gateway container + ai-box.vn proxy quirks"
type: project
domain: tooling
tags: [hermes, docker, llm-proxy, ai-box]
aliases: [hermes docker]
status: active
confidence: high
created: 2026-08-03
updated: 2026-08-27
supersedes: []
related: [kb-0009, kb-0018, kb-0019, kb-0020, kb-0021, kb-0022, kb-0013, kb-0025, kb-0040]
source: "https://hermes-agent.nousresearch.com/docs/user-guide/docker"
summary: "Hermes Agent gateway in Docker on fedora-server: compose in ~/containers/hermes, state ~/.hermes, ai-box.vn provider 'aibox', file-only config rule. Gotchas: kb-0019..kb-0022; memory: kb-0025."
---
Hermes Agent (Nous Research) runs as a single s6-supervised gateway container `hermes` from `nousresearch/hermes-agent:latest` (docker compose in `~/containers/hermes/`), all state bind-mounted at `~/.hermes` → `/opt/data`, inferring through the ai-box.vn proxy — current default model `qwen3.8-flash` (glm-5.2 → deepseek-v4-flash → deepseek-v4-flash-0731 (2026-08-22) → qwen3.8-flash (2026-08-27)) via the named provider `aibox` (OpenAI wire).

## Deployment shape (set up 2026-08-03)

Files in `~/containers/hermes/`: `docker-compose.yml` (source of truth), `Dockerfile` (optional derived image), `README.md`, backup secrets copy. Lifecycle: `docker compose up -d` / `logs -f` / `pull && up -d` (upgrade; migrations/backups run at boot) / `down`.

Key compose settings: `command: gateway run`, `restart: unless-stopped`, `-p 127.0.0.1:8642:8642` (loopback only; the API server is additionally gated on `API_SERVER_ENABLED=true`, unset), `-v ~/.hermes:/opt/data`, `-e PUID=1000 -e PGID=1000` ([[kb-0019]] — omit and you lose ~/.hermes to UID 10000).

- `~/.hermes/.env` (mode 600) holds `ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL` and `OPENAI_API_KEY`/`OPENAI_BASE_URL` — all the same ai-box.vn token used by Claude Code's `ANTHROPIC_AUTH_TOKEN`. Backup copy at `~/containers/hermes/.env`; on token rotation update both (only `~/.hermes` is read at runtime).
- `config.yaml`: `model: {default: qwen3.8-flash, provider: aibox}` (history: glm-5.2 → deepseek-v4-flash → 0731 (2026-08-22) → qwen3.8-flash (2026-08-27)); provider `providers: aibox: {base_url: https://api.ai-box.vn/v1, key_env: OPENAI_API_KEY}` (→ [[kb-0020]] for why named provider). All proxy chat models are registered as `model_aliases:` under their exact endpoint names — the list is volatile, **truth lives in `~/.hermes/config.yaml`** (don't copy it here); the same name works in `/model <name>` and `--model <name>`. User preference: keep endpoint names verbatim, no shortened aliases; chat models only (rerank/embed/image endpoints not wired). `tool_loop_guardrails.hard_stop_enabled: true` (recommended for unattended gateways).
- Dashboard (enabled 2026-08-04): `HERMES_DASHBOARD=1` + basic auth (user `giang`, password in compose), port 9119 bound to the Tailscale IP only (tailnet-only; needs tailscale0 up at container start; check compose for the current IP).

## Operating notes

- **RULE (user directive, 2026-08-04): ALL Hermes configuration and changes via files — never one-off `docker exec -it` commands.** Edit `~/.hermes/config.yaml` / `.env` (or the compose file), then `docker restart hermes` / `docker compose up -d`. Read-only `docker exec hermes …` for inspection/smoke tests is fine. NOTE: the gateway REWRITES `config.yaml` on write-backs (normalizes formatting, strips comments, appends `onboarding.*`) — keep functional keys minimal, don't rely on comments surviving.
- One-shot test: `docker exec hermes hermes chat -q "..."` (exec shim drops to the hermes user). Logs: `docker logs hermes`; file logs under `~/.hermes/logs/`.
- Proxy channels flap: intermittent 503s on glm-5.2/kimi-k2.7-code, self-heal in minutes.
- Telegram platform: auth semantics + security fix in [[kb-0021]]; silence triage in [[kb-0022]].

## Related
- [[kb-0009]] — other agent tools on this box
- [[kb-0013]] — docker group is root-equivalent here (container-based fixes without sudo)
- [[kb-0018]] — Claude Code tier routing over this same proxy
- [[kb-0019]] / [[kb-0020]] — docker ownership + model-name mangling gotchas
- [[kb-0021]] / [[kb-0022]] — Telegram auth + silence diagnostics
- [[kb-0040]] — community workflows, cron patterns, gotchas
