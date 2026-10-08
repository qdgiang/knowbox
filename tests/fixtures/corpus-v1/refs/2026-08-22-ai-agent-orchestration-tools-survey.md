# Survey: Multi-Session AI Agent Orchestration Tools (August 2026)

**Research date:** 2026-08-22 · **Method:** deep-research workflow (5 search angles → 21 sources fetched → 100 claims extracted → 25 top claims adversarially verified with 3-vote panels → 22 confirmed, 3 refuted). 103 agents, ~1.4M tokens.

**Question:** Survey of multi-session AI agent orchestration tools — tools for running, coordinating, and supervising multiple concurrent AI coding-agent sessions (herdr, orca, and similar fleet managers, TUI multiplexers, headless orchestration frameworks). Notable players, comparisons (architecture, features, maturity, adoption), and productivity setups. Framed for a heavy Claude Code user on a headless Linux server with native subagents/tasks/workflows already in place.

---

## Executive summary

As of August 2026, the space has split into **four categories**:

1. **Native orchestration inside Claude Code** — experimental "agent teams" (lead session + independent teammates, shared dependency-aware task list with file-locked claiming, JSON-file mailboxes) plus native `--worktree` isolation since v2.1.49. This commoditizes the bare worktree-fleet value proposition of external tools.
2. **Agent-aware terminal multiplexers/daemons** — Herdr (~22k stars, persistent background server, 20 detected agent CLIs, agent-to-agent socket API) and Claude Squad (~8k stars, tmux + git worktrees, 6 backends, AGPL-3.0).
3. **Desktop/web "agent development environments"** — Orca (cross-platform, 25+ backends, per-task worktrees, SSH remote execution) and Conductor (Mac-only, Melty Labs).
4. **Heavier persistent frameworks** — Gas Town (daemon, mailboxes, Beads-backed state, watchdogs, merge queue) and Composio's Agent Orchestrator (per-agent worktree/branch/PR with autonomous CI-repair loops).

**The decisive gap for a headless-server user:** agent teams cannot spawn teammates in headless mode (`claude -p` / Agent SDK), so external orchestrators retain real value for headless, persistent, and multi-backend fleet workflows even as native features close in.

**Mature, verified productivity patterns:** detached tmux+worktree fleets, fan-out-one-prompt-to-N-agents-then-pick-the-winner, GitHub Actions `claude -p --output-format json` PR-review loops. Notifications and cost/token tracking produced **no claims that survived verification** — treat as uncovered, not confirmed-absent.

---

## Verified findings (all survived 3-vote adversarial verification)

### 1. Claude Code native "agent teams" (research preview, Feb 2026)

Enabled via `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` (settings.json or env). Architecture: one lead session + independent teammate Claude Code instances coordinated through a **shared task list** (dependencies, file-locked claiming) and **file-based peer-to-peer mailboxes** at `~/.claude/teams/{team-name}/inboxes/{agent-name}.json`. Teammates self-claim unassigned/unblocked tasks; messaging is direct between agents; Ctrl+T task-list overlay; tmux/iTerm2 split panes are opt-in (default in-process since v2.1.179). Launched alongside Opus 4.6 in Feb 2026. Claude-only.

Key difference vs native subagents: teammates share a task list and communicate peer-to-peer; subagents only report results to the main agent.

- Confidence: high (3-0 across merged claims; docs verbatim + multiple independent sources, zero contradictions)
- Sources: https://code.claude.com/docs/en/agent-teams · https://htdocs.dev/posts/from-conductor-to-orchestrator-a-practical-guide-to-multi-agent-coding-in-2026/

### 2. Agent teams is interactive-only — the key headless finding

Agent teams **cannot spawn teammates in headless/non-interactive mode** — with `claude -p`, including Agent SDK sessions, no teammates are spawned (named subagents run as ordinary subagents). Docs verbatim: "Spawning teammates also requires an interactive session. In non-interactive mode with the -p flag, including Agent SDK sessions, Claude doesn't spawn teammates." Changelog through v2.1.234 (Aug 17 2026) has no headless enablement; GitHub issue anthropics/claude-code#72343 (June 2026, unfixed) confirms the unsupported path crashes. Workarounds (interactive session under tmux with a PTY) are alternatives, not native agent teams.

→ **Single most important finding for headless-server workflows:** native multi-session orchestration is interactive-only, preserving a concrete role for external orchestrators on a headless box.

- Confidence: high (3-0)
- Sources: https://code.claude.com/docs/en/agent-teams · https://github.com/anthropics/claude-code/issues/72343

### 3. Native `--worktree` isolation since v2.1.49

`claude --worktree <name>` / `-w` creates a git worktree for the session (also WorktreeCreate/WorktreeRemove hooks); actively developed (GitLab MR support in v2.1.233). Verified on this machine (v2.1.222 `claude --help`). Undercuts external orchestrators whose primary value is worktree-fleet isolation — they must differentiate on persistence, multi-backend support, supervision UI, or CI/PR loops.

- Confidence: high (3-0)
- Sources: https://code.claude.com/docs/en/changelog · https://github.com/anthropics/claude-code/issues/27044

### 4. Herdr (herdr.dev, github.com/herdrdev/herdr, ~22k stars)

Background **daemon/runtime** for coding agents — "not an app you keep open" — with an attachable TUI sidebar showing per-pane working/blocked/idle status; session/layout persistence across machine sleep and restart; **agent-to-agent coordination**: agents drive Herdr via CLI and socket API to split panes, launch each other, prompt each other, waiting for a genuinely blocked agent rather than blindly sending keystrokes. Detects **20 agent CLIs** out of the box (Claude Code, Codex, opencode, Cursor, Grok, Copilot, Hermes, Amp, Kimi, Qwen Code, …; Gemini CLI/Cline less thoroughly). Catalogued as "agent-aware multiplexer with persistent workspaces" (awesome-agent-orchestrators). **Closest architectural match to a headless laptop-server workflow in this survey.**

Caveats (Flavio Coes deep dive, 2026-08-10): after restart, session resume depends on each agent's native resume capability (Claude Code has it); status detection "is not perfect."

- Confidence: high (2-1 / 3-0 / 3-0 / 3-0 across merged claims)
- Sources: https://herdr.dev/ · https://herdr.dev/docs/agents/ · https://github.com/herdrdev/herdr · https://flaviocopes.com/herdr

### 5. Orca (onorca.dev, github.com/stablyai/orca, YC-backed, MIT)

Cross-platform (macOS/Windows/Linux) desktop app; brands itself an **"Agent Development Environment" (ADE)**. Every task in its own **isolated git worktree**; ships **25+ built-in agent backends** (README lists 29: Claude Code, Codex, Grok, Gemini, Cursor, Copilot, OpenCode, Amp, Goose, Cline, Droid, Kimi, Qwen Code…) plus arbitrary CLI agents. Signature feature: **fan one prompt out across N agents in parallel, compare, merge the winner**. **Remote execution over SSH** with full file editing, git and terminals, auto-reconnect, port forwarding, passphrase caching — agents run on a remote box while the local app stays in control. Mechanism-level SSH docs (remote node-pty relay, known_hosts verification) inconsistent with vaporware.

Relevant: Orca's SSH worktrees could move fleets off the laptop onto another machine, or vice versa.

- Confidence: high (3-0 all claims)
- Sources: https://www.onorca.dev/ · https://www.onorca.dev/docs/ssh · https://www.onorca.dev/docs/recipes/parallel-agents · https://github.com/stablyai/orca

### 6. Claude Squad (github.com/smtg-ai/claude-squad, ~8.4k stars, AGPL-3.0, binary `cs`)

Canonical open-source **tmux-plus-worktree TUI**: each agent in its own tmux session + git worktree; sessions detach (ctrl-q) and keep working after the pane closes. Six backends — Claude Code (default), Codex, Gemini CLI, Aider, OpenCode, Amp (last two via the generic `-p/--program` flag). Widely recommended as the starting point for solo devs running parallel agents; treated as the reference implementation of the "tmux agent manager" category. Detach-and-persist model maps directly onto headless/cron-adjacent workflows.

- Confidence: high (3-0 all merged claims)
- Sources: https://github.com/smtg-ai/claude-squad · https://www.augmentcode.com/tools/open-source-agent-orchestrators · https://fleetctl.ai/best/run-multiple-claude-code-agents/

### 7. Gas Town (github.com/gastownhall/gastown, ~17.6k stars, Go, MIT, Steve Yegge, announced Jan 2026)

Heaviest persistent-orchestration framework in the survey: actual **daemon** (heartbeat every 3 min, `gt up` boots Deacon/Mayor/Witnesses/Refineries roles) with built-in **mailboxes, worker identities, handoffs**; **Beads-backed** (git-backed issue tracker) persistent work state; **three-tier watchdog** system; capacity-governor scheduling; **Refinery merge queue** (Bors-style bisecting). Explicitly targets **20–50+ long-lived agents** — the tier above Claude Code's task system or a TUI fleet, and the most direct competitor to hand-rolled handoff-file + KB persistence conventions.

Caveat: secondary coverage self-describes as an "architecture read, not a production review" — feature inventory verified, production efficacy not.

- Confidence: high (3-0)
- Sources: https://github.com/gastownhall/gastown · https://parallelcode.app/blog/multi-agent-coding-tools-2026/

### 8. Composio Agent Orchestrator (open-sourced ~Feb 2026)

Most complete verified **PR-review-loop** product: each git-backed agent gets its own **worktree, branch, and PR**; autonomously handles **CI failures and review comments** — routed back to the owning agent with retry/escalation config (e.g. `auto: true`, send-to-agent, `retries: 2`, escalate to human). Agent-agnostic (Claude Code, Codex, Aider, custom — 26 agents listed), runtime-agnostic (tmux, Docker, Kubernetes, cloud sandboxes), tracker-agnostic (GitHub, Linear, Jira). Caveats: scratch workers get branchless dirs; CI loop escalates to human after 2 retries rather than guaranteeing a fix.

- Confidence: high (3-0)
- Sources: https://composiohq-agent-orchestrator.mintlify.app/introduction · https://github.com/ComposioHQ/agent-orchestrator

### 9. Conductor (conductor.build)

**Mac-only** desktop app from **Melty Labs** (YC S24, Charlie Holtz/Jackson de Campos, $22M Series A, Spark/Matrix) — NOT Anthropic. Runs multiple Claude Code, Codex, Cursor, OpenCode agents in parallel, each isolated in its own git worktree with branch, files, terminal, diff, and review view. (Some secondary sources misspell "Melky Labs".)

- Confidence: high (3-0)
- Sources: https://conductor.build · https://fleetctl.ai/best/run-multiple-claude-code-agents/

### 10. Headless PR-review loop needs no orchestrator

Proven pattern: GitHub Actions workflow triggers on `pull_request` (opened/synchronize) → runs `claude -p "Code review for this PR…" --output-format json` → posts review as PR comment via `actions/github-script`. Combined with cron/scheduled `claude -p` runs, this is the verified recipe space for headless-server automation, independent of any fleet manager. Caveat: fork PRs cannot access `ANTHROPIC_API_KEY` under `pull_request` events.

- Confidence: high (3-0)
- Sources: https://institute.sfeir.com/en/claude-code/claude-code-headless-mode-and-ci-cd/examples/ · https://github.com/FlorianBruniaux/claude-code-ultimate-guide

---

## Refuted claims (do NOT repeat as fact)

| Claim (killed) | Vote | Source |
|---|---|---|
| Parallel `claude -p` instances scale pipeline speed linearly by agent count | 0-3 | institute.sfeir.com |
| Git worktrees are the universal standard isolation across open-source orchestrators | 0-3 | augmentcode.com |
| Inter-agent coordination is categorically the weakest area across open-source orchestrators | 0-3 | augmentcode.com |

## Caveats

1. Herdr/Orca feature claims verified against vendor sites/docs — feature inventory solid, real-world efficacy unproven by independent benchmarks.
2. Time sensitivity high: agent teams experimental (display-mode defaults changed as recently as v2.1.179); Herdr/Orca/Gas Town coverage is June–Aug 2026, star counts are snapshots. Re-verify gating flags before relying.
3. Notifications and cost/token tracking asked but produced zero verified claims — uncovered, not confirmed-absent.
4. All GitHub Actions review-loop cost/speed statistics in underlying sources are marketing; excluded.

## Open questions

- Will agent teams gain headless/`-p`/Agent SDK support? Is the PTY-under-tmux workaround (interactive session driven headlessly) viable on a server? No verified source covers it.
- What do these tools actually provide for notifications and cost/token tracking?
- Gas Town's Beads-backed state vs hand-rolled handoff-file + KB conventions for cross-session continuity at 20+ agents — no head-to-head evidence.
- Beyond star counts, what evidence exists for production maturity/retention of Herdr, Orca, Gas Town, Composio orchestrator?

## Additional landscape signals (from search, not fully verified)

- April 2026 landscape tiers: in-process agents (Claude Code subagents/Agent Teams, oh-my-claudecode, Pi), local orchestrators with worktree isolation (Conductor, Claude Squad, Parallel Code, Vibe Kanban, Composio, Forge), cloud-async agents (Claude Code Web, Copilot Coding Agent, Codex Web, Jules).
- Other named tools surfaced: Agent-Manager, Pane (Electron, AGPL-3.0), Golutra, Baton, Bernstein, Crystal/Nimbalyst, Vibe Kanban, Agent Kanban, Emdash, Scott Chacon's Grit.
- Practitioner signals: top-voted r/ClaudeCode thread (Aug 2026) describes a "foreman" agent session driving herdr as session multiplexer; HN practitioners report cycling from elaborate multi-agent setups back to a single base session due to drift, change rot, and merge conflicts. Practical ceiling on a laptop: ~4-5 concurrent agents before moving fleets to a remote box.
- Curated list: https://github.com/andyrewlee/awesome-agent-orchestrators

## Source inventory

Primary: code.claude.com/docs/en/agent-teams, code.claude.com/docs/en/changelog, herdr.dev (+docs), onorca.dev (+docs), github.com/smtg-ai/claude-squad, github.com/gastownhall/gastown, github.com/ComposioHQ/agent-orchestrator, conductor.build, institute.sfeir.com.
Secondary/forum: htdocs.dev, parallelcode.app, augmentcode.com, fleetctl.ai, developersdigest.tech (×2), alexlavaee.me, aakashx.com, heeki.medium.com, backgrind.com, buildthisnow.com, blog.yeyupiaoling.cn, flaviocopes.com, reddit.com/r/ClaudeCode, news.ycombinator.com, github.com/andyrewlee/awesome-agent-orchestrators. (amux.io rated unreliable, excluded.)
