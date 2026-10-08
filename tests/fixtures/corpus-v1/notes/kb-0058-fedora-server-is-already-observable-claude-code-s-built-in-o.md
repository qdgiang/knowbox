---
id: kb-0058
title: "fedora-server is already observable — Claude Code's built-in OTel serves metrics on :9464, Cockpit runs on :9090, systemd-cgtop watches agent sessions for free"
type: fact
domain: fedora
tags: [observability,opentelemetry,cockpit,systemd,monitoring,claude-code]
aliases: [otel metrics, port 9464, cockpit 9090, systemd-cgtop, agent monitoring]
triggers:
  - "observing Claude Code token/cost usage on this box"
  - "choosing a host or agent monitor for fedora-server"
  - "checking whether Cockpit or OTel is already installed"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0003, kb-0002, kb-0030]
source: "raw/session-distill-2026-08-16.md"
summary: "fedora-server observability in place: Claude Code's built-in OTel serves :9464 (v2.1.222-verified + caveats), Cockpit on :9090, systemd-cgtop/session scopes watch agents free."
---
fedora-server already has three observation surfaces (all verified 2026-08-16) — no new host monitor needs installing.

**Claude Code built-in OpenTelemetry**: native OTel instrumentation, off until `CLAUDE_CODE_ENABLE_TELEMETRY=1` plus an exporter (`OTEL_{METRICS,LOGS,TRACES}_EXPORTER` = otlp|prometheus|console|none). With the prometheus exporter it serves `http://localhost:9464/metrics` — token/cost/session counters plus prompt/API-error/tool-result events — confirmed inside the locally installed v2.1.222 binary. Caveats: traces are beta (`CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`); OTEL_* vars do NOT propagate into Bash-tool/MCP subprocesses; version-specific bugs exist (OTLP no-op in 2.1.113), so re-verify after CLI upgrades; self-hosted runners at capacity >1 re-expose metrics on the runner's own /metrics instead of a per-session :9464.

**Cockpit**: cockpit-365 + cockpit-podman-129 already listen on :9090 (`cockpit.socket`; dnf packages cockpit + cockpit-podman) — host/services/containers visibility with no install; use an SSH tunnel or add a firewalld rule for remote access. Beszel containers are the optional lightweight add for history/alerts (no PromQL, no logs).

**Agent processes**: the zero-install way is systemd — every SSH-launched session (incl. Claude Code) becomes a `session-N.scope` cgroup, and `systemd-cgtop` shows live per-cgroup CPU/memory/IO. process-exporter and systemd_exporter have NO Fedora packages (dnf-checked) → run them as containers. ctop installs only via brew/npm — a dnf-rule violation unless containerized. claude-code-otel is a ready-made Collector→Prometheus/Loki→Grafana stack. Full cited report: `~/projects/server-monitor/research-monitoring-tools.md`.

## Related
- [[kb-0003]] — fedora-server, the machine these surfaces run on
- [[kb-0002]] — dnf-only rule: vetoes ctop, pushes exporters into containers
- [[kb-0030]] — Claude Code version gates: telemetry behavior is version-specific, re-verify after upgrades
