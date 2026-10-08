---
id: kb-0072
title: "Herdr plugins for productivity 2026-08-24: section map"
type: reference
domain: tooling
tags: [herdr, plugins, deep-research, agent-orchestration]
aliases: [herdr plugin recommendations, herdr-mobile-relay, herdr-lantern, herdr-worktrunk]
triggers:
  - "which herdr plugins to install / herdr plugin recommendations for headless server"
  - "herdr 0.7.5 vs 0.8.x upgrade / headless session resume bug"
  - "herdr notification / approval routing from phone / token monitoring plugin"
status: active
confidence: high
created: 2026-08-24
updated: 2026-08-24
supersedes: []
related: [kb-0008, kb-0032, kb-0031]
source: "refs/2026-08-24-herdr-plugins-productivity.md (deep-research workflow 2026-08-24; 21 sources, 25 claims adversarially verified)"
summary: "Section map for the herdr-plugins productivity report in refs/. Upgrade herdr 0.7.5→0.8.2 first (headless session-resume fix); best plugins: herdr-mobile-relay/collie, herdr-worktrunk, telemetry-bridge, herdr-lantern."
---
Deep-research report on herdr plugins, 2026-08-24. Full doc is
**`refs/2026-08-24-herdr-plugins-productivity.md`**. Grep the section map below,
then read that section by offset. ⚠ herdr is fast-moving 0.x + the marketplace
refreshes every 30 min — re-verify version compat against the installed binary
(`herdr --version`) and the live topic `herdr-plugin` before installing. The
herdr guide map [[kb-0008]] §13 is the older ~20-plugin baseline this supersedes
on the question of *which plugins to pick*.

## Section map of refs/2026-08-24-herdr-plugins-productivity.md

- **1. Headline: upgrade first** — the highest-value action is `brew upgrade
  herdr` 0.7.5→0.8.2, NOT a plugin. 0.8.0 fixes headless session-resume bug #2064
  (restored agents didn't resume until a TUI client attached — exactly the
  systemd-user-service scenario). 0.7.5 also made plugin enable-state
  global-per-user (#1174, breaking for named-session setups). 0.8.2 = 2026-08-19
  Homebrew stable.
- **2. Ecosystem ~40× the snapshot** — topic `herdr-plugin` = 813 repos;
  marketplace indexes 762 plugins/749 repos. Discovery = topic search +
  herdr.dev/plugins, NOT awesome-herdr. Noisy (mis-tagged + dead repos).
- **3. Ranked recommendations** — the core. Per-category tables with verdict,
  install cmd, maintenance evidence, confidence:
  - Notifications/approval: **herdr-mobile-relay** (best fit, herdr ≥0.7.5) or
    **collie** (504★, strongest maintenance). Runners-up herdr-ntfy-notify
    (0.7.x unconfirmed), herdr-push.
  - Worktree/PR lifecycle: **herdr-worktrunk** + **qdentity/herdr-worktree-lifecycle**
    (setup/teardown hooks). Skip mattarau/wt-herdr (low maturity).
  - Token/cost: **herdr-telemetry-bridge** (experimental, 2 commits) vs
    herdr-token-dashboard (baseline, 0.7.5-ok).
  - Supervisor: **herdr-lantern** (v0.9.10, Claude Code supported, herdr ≥0.7.5).
    Do NOT use its removed Telegram "Bridge".
  - 0.8.0-only (install after upgrade): **herdr-pane-id**.
  - New high-star unverified: herdr-reviewr (509★), herdr-browser (340★), +6 more.
- **4. Other verified findings** — built-in session restore recreates layout
  but NOT processes; herdr-restart-always fills the process-death gap;
  `herdr integration install claude` is first-party (rewrites settings.json #2066).
- **5. Security caveats** — marketplace unreviewed, plugins unsandboxed, run as
  user. Precedent: herdr-lantern Bridge removed v0.8.0 after shell-access. Prefer
  zero-dep shell-free plugins; read on_event scripts; avoid chat→shell bridges.
- **6. Refuted claims** — 2 of 25 killed: ntfy-notify "v1 compat question"
  (just unconfirmed, not a red flag); herdr-push "single Cloudflare relay"
  (relay is flexible).
- **7. Open questions** — upgrade breaking changes for systemd; reviewr/browser
  functionality+security; ntfy-notify v1 vs 0.7.x; mobile-relay relay choice on
  NAT'd laptop.

## Related
- [[kb-0008]] — herdr guide map: §13 plugins & ecosystem (the older baseline this
  report extends/supersedes on *which plugins to pick*); §11 worktree/PR workflows
- [[kb-0032]] — multi-persona herdr setups: worktree per task not per persona
  (the PR-factory lifecycle the worktree plugins automate)
- [[kb-0031]] — multi-agent orchestrator landscape; herdr's real inter-agent comms
