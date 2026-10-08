---
id: kb-0008
title: "Herdr guide: section map — read refs/herdr-guide.md by section, never whole"
type: reference
domain: tooling
tags: [herdr, tmux, agent-orchestration, systemd]
aliases: [agent multiplexer, tmux for AI agents]
triggers:
  - "using herdr / running agents in panes / supervisor+worker pattern"
  - "herdr as a systemd user service / headless herdr"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0009, kb-0003, kb-0002]
source: "refs/herdr-guide.md (researched 2026-08-02, verified against v0.7.5)"
summary: "Section map for the 36KB Herdr guide in refs/. Grep the map, then read the exact section. ⚠ 0.x CLI changes between minors — installed binary --help is authoritative."
---
The full guide is **`refs/herdr-guide.md`** (36KB — do not read whole). Find the section below, then read it by offset, e.g. `rg -n '^## 10\.' refs/herdr-guide.md` and Read with that offset. ⚠ Herdr is fast-moving 0.x: commands/flags change between minors — run `herdr <command> --help`; the installed binary is authoritative over the guide and blog posts.

## Section map of refs/herdr-guide.md

- **1. What Herdr is** — agent multiplexer ("tmux for AI agents"), real PTYs, state tracking working/blocked/done/idle, socket API; ~23k stars, Apache-2.0 (not AGPL — stale copy exists); name collision with an older unrelated "herdr"
- **2. Installation** — installer curl script (but see [[kb-0002]]: on this box prefer `dnf`/package-manager routes), brew/mise/nix, `herdr update`
- **3. Core concepts** — server vs client, workspaces/tabs/panes, persistence
- **4. Your first session** — mouse-first UX, named sessions
- **5. Keybindings** — panes, tabs, workspaces, copy mode, custom bindings
- **6. Configuration** — config file, default_shell, toast delivery "terminal" for SSH
- **7. Detach, persistence & restarts** — ctrl+b q, survives SSH drops/lid events
- **8. The CLI** — workspaces & tabs, panes, **agents facade (v0.7.5 moved wait/send under `herdr agent`)**
- **9. Agent detection & integrations**
- **10. Socket API: agents driving Herdr** — protocol + the canonical orchestration recipe (guard → spawn sibling pane → start agent → submit+wait → harvest → drive UI if blocked) + MCP servers
- **11. Workflows** — A: supervisor + parallel specialists (flagship); B: git worktree per task; C: review pipelines; D: PR factories; parallelism hygiene
- **12. Remote & headless usage ⭐** — plain SSH vs thin client; **systemd user service unit + memory-cap bug; known headless bugs as of v0.7.5** (relevant: fedora-server)
- **13. Plugins & ecosystem** — plugin system, awesome-herdr highlights, which agents work well inside
- **14. Gotchas & limitations** — from issues and real users; vs alternatives
- **15. Quick-start cheat sheet**

## Related
- [[kb-0009]] — agent tools installed on this machine
- [[kb-0003]] — the headless server this runs on
