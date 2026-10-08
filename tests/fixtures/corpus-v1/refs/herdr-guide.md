# Herdr: A Practical Guide

*Researched and written 2026-08-02 · Verified against Herdr v0.7.5 (released 2026-07-21)*

---

## 1. What Herdr is

**Herdr is an agent multiplexer that lives in your terminal** — think "tmux for AI coding agents." It's a single ~10 MB Rust binary (no Electron, no account, no hosted control plane) that gives you persistent workspaces, tabs, and panes, and — the thing that makes it more than tmux-with-extra-steps — it **recognizes the AI agents running inside your panes and tracks their state**: `working`, `blocked` (needs your approval/answer), `done` (finished, you haven't looked yet), and `idle`.

Key properties:

- **Real PTYs.** Agents run as ordinary terminal processes; Herdr renders their output and shows you the real terminal, "not a wrapped interpretation."
- **Persistence like tmux.** Detach and everything keeps running; reattach from any terminal, over SSH, even from a phone. A background *server* owns the panes — the TUI you see is just a *client*.
- **Agent-aware sidebar.** A blocked agent makes its pane, tab, and workspace visibly blocked, so you can run six agents and instantly see which one needs you.
- **A socket API.** Agents (and scripts) can drive Herdr: spawn panes, start other agents, send them prompts, wait on their status, read their output. This is the basis for agents-orchestrating-agents workflows.
- **Mouse-first, keyboard-optional.** Click panes, drag borders, right-click for menus. tmux-style prefix keys also work.

**State of the project (important context):**

- Repo: [github.com/herdrdev/herdr](https://github.com/herdrdev/herdr) (formerly `ogulcancelik/herdr`, which redirects). ~23k stars, created March 2026, solo maintainer working full-time, sponsored.
- **Very fast-moving 0.x releases.** Commands and flags have changed between minor versions (e.g. v0.7.5 moved `wait`/`send` under a new `herdr agent` facade). Treat blog posts as potentially stale; **the installed binary is authoritative** — run `herdr <command> --help`.
- **License:** the repo's LICENSE file is **Apache-2.0** (verified; changed from AGPL-3.0 mid-2026 via [issue #1340](https://github.com/herdrdev/herdr/issues/1340)). The Homebrew formula metadata and some website copy still say AGPL — that's stale.
- ⚠️ **Name collision:** there is an unrelated older tool also called "herdr" (a multi-LLM prompt comparison CLI). Some reviews/articles describe that tool. If something you read mentions `~/.herdr/config.yaml` or fanning one prompt out to GPT-4/Claude/Ollama, it's not this Herdr.

---

## 2. Installation

### Linux / macOS (official installer — recommended)

```sh
curl -fsSL https://herdr.dev/install.sh | sh
```

Installs to your user path (no root needed). If `herdr` isn't found afterward, add `~/.local/bin` to your `PATH`.

Update later with:

```sh
herdr update          # only for installer-managed installs
```

### Package managers

| Manager | Command | Notes |
|---|---|---|
| Homebrew | `brew install herdr` | Bottles for macOS + Linux x86_64/aarch64 |
| mise | `mise use -g herdr` | If not found, update mise; fallback: `mise use -g github:herdrdev/herdr` |
| Nix | `nix run github:herdrdev/herdr/v0.7.5` | Omit tag = track `master` (not recommended) |

Windows exists only as a preview beta (`powershell -ExecutionPolicy Bypass -c "irm https://herdr.dev/install.ps1 | iex"`). WSL2 has a cluster of open bugs — genuinely beta.

### Manual binaries

GitHub Releases publish `herdr-linux-x86_64`, `herdr-linux-aarch64`, `herdr-macos-x86_64`, `herdr-macos-aarch64`:

```sh
chmod +x herdr-linux-x86_64
mv herdr-linux-x86_64 ~/.local/bin/herdr
```

### Verify

```sh
herdr --version
herdr                 # launches/attaches to your default session
```

---

## 3. Core concepts

The hierarchy is **Session → Workspace → Tab → Pane**, and agents run inside panes.

| Concept | What it is |
|---|---|
| **Session** | A persistent Herdr server namespace. `herdr` attaches to the *default* session. Named sessions (`herdr --session work`) are fully separate runtimes — separate panes, sockets, state. Use workspaces first; named sessions only when you need full isolation. |
| **Workspace** | A project container — typically one per repo or investigation. Owns tabs and panes. Its sidebar state rolls up from the agents inside it. |
| **Tab** | A layout inside a workspace (e.g. `agents`, `logs`, `server`, `review`). |
| **Pane** | A real terminal. Splittable, renamable, readable/writable from the CLI. Survives client detach. Public IDs look like `w1:p1`. |
| **Agent** | A process Herdr *recognizes* inside a pane — via lifecycle hooks, foreground-process detection, or "screen manifests" (TOML rules matched against the pane's bottom buffer). |

**Agent states:**

| State | Meaning |
|---|---|
| `blocked` | Needs input, approval, or a decision (a permission/question UI is visible) |
| `working` | Actively running |
| `done` | Finished background work **and you haven't looked at it yet** |
| `idle` | Finished/waiting, and seen |

States roll upward: a blocked agent makes its pane, tab, *and* workspace look blocked. CLI reads do **not** mark an agent seen; focusing it does — "a done agent stays visible until you view it."

**Architecture:** a background *server* owns panes and processes; the TUI is a *client*. Closing your terminal just detaches the client. `herdr server stop` kills everything.

---

## 4. Your first session

```sh
cd ~/project      # start where the work lives
herdr             # launches or attaches to the default background session
```

- Your first workspace is auto-created.
- Run an agent like you normally would — `claude`, `codex`, `pi`, `opencode`, `gemini` — and Herdr detects it and starts showing its state in the sidebar.
- **Detach:** `ctrl+b q` — or just close the terminal window. Server and agents keep running.
- **Reattach:** run `herdr` again, from any terminal.
- **Actually end everything:** `herdr server stop`.

### Mouse (first-class, on by default)

- Click panes/tabs/workspaces/agents to focus
- Drag split borders to resize
- Right-click for context menus (split pane, new tab, …)
- Drag-select copies; double-click copies a token
- Ctrl-click opens links (on macOS you may need Shift-Cmd-click)

### Named sessions

```sh
herdr --session work
herdr session list
herdr session attach work
herdr session stop work
```

---

## 5. Keybindings

Default prefix is `ctrl+b`: press it, release, then press the action key (`prefix+c` = `ctrl+b`, release, `c`). `prefix+?` shows every active binding in-app. `ctrl+b ctrl+b` sends a literal `ctrl+b` to the pane.

### Panes

| Action | Key |
|---|---|
| Split right (side by side) | `prefix+v` |
| Split down (stacked) | `prefix+minus` |
| Focus left / down / up / right | `prefix+h` / `j` / `k` / `l` |
| Swap pane left/down/up/right | `prefix+shift+h/j/k/l` |
| Next / previous pane | `prefix+tab` / `prefix+shift+tab` |
| Close pane | `prefix+x` |
| Zoom (toggle fullscreen within tab) | `prefix+z` |
| Resize mode | `prefix+r` |
| Rename pane | `prefix+shift+p` |
| Copy mode | `prefix+[` |

### Tabs

| Action | Key |
|---|---|
| New tab | `prefix+c` |
| Next / previous tab | `prefix+n` / `prefix+p` |
| Jump to tab 1–9 | `prefix+1` … `prefix+9` |
| Rename tab | `prefix+shift+t` |
| Close tab | `prefix+shift+x` |

### Workspaces & session

| Action | Key |
|---|---|
| Workspace picker / navigation | `prefix+w` |
| New workspace | `prefix+shift+n` |
| Rename workspace | `prefix+shift+w` |
| Close workspace | `prefix+shift+d` |
| Goto / session navigator | `prefix+g` |
| Toggle sidebar | `prefix+b` |
| **Detach** | **`prefix+q`** |
| New worktree-backed workspace | `prefix+shift+g` |
| Reload config | `prefix+shift+r` |
| Settings | `prefix+s` |
| Help | `prefix+?` |

### Copy mode (`prefix+[`)

Output stays live; the process is not paused.

- Move: `h/j/k/l`, tmux-style `w/b/e`, `{`/`}`, `PageUp`/`PageDown`, `ctrl+u`/`ctrl+d`
- Search: `/` forward, `?` backward, `n`/`N` repeat
- Select: `v` or Space → copy: `y` or Enter → exit: `q`/Esc

### Custom bindings

In `config.toml`, bindings accept arrays and the syntax `ctrl+a`, `shift+n`, `alt+1`, `cmd+k`, plus specials (`enter`, `tab`, `esc`, arrows, `minus`, `comma`, …):

```toml
[keys]
prefix = "ctrl+a"                     # common change if you SSH a lot
next_tab = ["prefix+n", "ctrl+alt+]"]
```

You can also bind custom commands (`[[keys.command]]`) that open popups (e.g. `lazygit`), temporary panes, or background shells. Reset bindings with `herdr config reset-keys`. Avoid OS-owned chords like `ctrl+alt+arrows` or `ctrl+alt+t`.

---

## 6. Configuration

File: `~/.config/herdr/config.toml` (override with `HERDR_CONFIG_PATH`). Invalid values fall back to safe defaults with a warning. Reload live with `herdr server reload-config` or `prefix+shift+r`. Print the fully commented default (148 keys) with:

```sh
herdr --default-config
```

A reasonable starting config:

```toml
[keys]
prefix = "ctrl+b"          # consider "ctrl+a" on a machine you SSH into

[terminal]
# default_shell = "/usr/bin/zsh"     # empty = $SHELL
new_cwd = "follow"                   # new panes open in focused pane's cwd

[theme]
name = "catppuccin"                  # or "terminal" to inherit your palette

[ui]
mouse_capture = true                 # click-focus, drag-resize, wheel scroll
copy_on_select = true
confirm_close = true
# toast delivery: "terminal" works over SSH
[ui.toast]
delivery = "terminal"

[ui.sound]
enabled = true                       # chime when an agent blocks; set false to mute

[session]
resume_agents_on_restore = true      # restart agent conversations after server restart

[worktrees]
directory = "~/.herdr/worktrees"

[advanced]
scrollback_limit_bytes = 10000000
```

Logs live at `~/.config/herdr/herdr.log` (plus `-client` / `-server` variants).

---

## 7. Detach, persistence & restarts

| Scenario | Processes survive? | Layout returns? | Screen content returns? | Agent conversations resume? |
|---|---|---|---|---|
| Detach + reattach | ✅ | ✅ | ✅ (live terminal) | ✅ (never stopped) |
| Server restart | ❌ | ✅ | only with `pane_history` | ✅ for supported agents (native restore) |
| `herdr update` | mixed | ✅ | only with `pane_history` | only native restore |
| `herdr update --handoff` | best effort | ✅ | if handoff succeeds | if handoff succeeds |

- **Detach/reattach is the strongest path** — original processes never stop.
- After a **server restart**, workspaces/tabs/panes/cwd/layout/focus are restored, but dead shells come back as *new shells in the saved directories*.
- **Native agent session restore** (on by default): Herdr records agent session IDs via integrations and restarts conversations after a server restart — e.g. `claude --resume <id>`, `codex resume <id>`, `pi --session <id>`. Supported for ~14 agents. Disable with `resume_agents_on_restore = false`.
- **Pane history replay** is opt-in: `[experimental] pane_history = true` replays recent terminal contents after restart. ⚠️ Off by default for a reason — terminal output can contain secrets/tokens.
- **Live handoff** (`herdr update --handoff`) transfers live PTYs to a replacement server so processes survive updates. Experimental; installer-managed installs only.

---

## 8. The CLI (the parts you'll actually use)

Most commands print JSON (great for scripting); exit 1 = runtime error (JSON on stderr), exit 2 = usage error. **Never probe mutating commands by omitting args** — `herdr workspace create` with no args just *creates one with defaults*.

### Workspaces & tabs

```sh
herdr workspace list
herdr workspace create --cwd ~/project --label api
herdr workspace focus w2
herdr workspace rename w1 "checkout"
herdr tab create --label logs
```

### Panes

```sh
herdr pane split --current --direction right --cwd "$PWD" --no-focus
herdr pane run w1:p2 "npm test"                    # atomic text+Enter; prefer over send-text
herdr pane read w1:p2 --source recent --lines 50
herdr pane wait-output w1:p2 --match "test result" --timeout 120000   # or --regex
herdr pane send-keys w1:p3 ctrl+c                  # interrupt a stuck pane
herdr pane list --workspace "$HERDR_WORKSPACE_ID"
```

`pane read` sources: `visible` (viewport), `recent` (soft-wrapped), `recent-unwrapped` (best for logs/transcripts), `detection` (what the state detector sees).

### Agents (the v0.7.5 facade)

```sh
herdr agent list
herdr agent start reviewer --kind codex --pane w1:p2       # blocks until detected & ready
herdr agent prompt reviewer "Review the diff" --wait --timeout 120000
herdr agent wait reviewer --until blocked --timeout 120000
herdr agent read reviewer --source recent-unwrapped --lines 120
herdr agent send-keys reviewer esc
herdr agent attach reviewer                                # ctrl+b q to detach; --takeover to steal input
herdr agent explain w1:p1                                  # why does this pane have its current state?
```

Targets are a **unique live agent name** or the **pane id** hosting it. `agent start --kind` supports: `claude codex pi gemini cursor devin cline opencode copilot kimi droid amp grok hermes kilo qodercli maki` and more.

Every process Herdr launches gets these env vars (useful in scripts): `HERDR_ENV=1`, `HERDR_PANE_ID`, `HERDR_TAB_ID`, `HERDR_WORKSPACE_ID`, `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`.

---

## 9. Agent detection & integrations

Herdr knows an agent's state one of three ways:

1. **Lifecycle hooks** (authoritative): the agent's own hooks report `idle`/`working`/`blocked`. Best-instrumented: **Pi**, OpenCode, Kilo Code CLI, Hermes, Kimi Code CLI, MastraCode, OMP.
2. **Screen manifests**: Herdr watches the pane's bottom buffer and matches TOML rules. Used for **Claude Code, Codex**, Copilot CLI, Devin, Droid, Cursor Agent, Amp, Grok, and others. Manifests auto-update from herdr.dev.
3. **Custom reporting**: any process can call `herdr pane report-agent w1:p1 --source custom:mybot --agent docs-bot --state working`.

Install the official integration for your agent:

```sh
herdr integration install claude
herdr integration status
```

For Claude Code this adds hooks that give richer state *and* enable native session restore.

⚠️ **Gotcha ([issue #2066](https://github.com/herdrdev/herdr/issues/2066)):** `herdr integration install claude` currently **rewrites your entire `~/.claude/settings.json`** (reorders keys, drops trailing newline) just to add one hook. **Back up the file first.**

Detection caveats:

- **`blocked` is deliberately strict** for screen-manifest agents: only known approval/question UIs trigger it; an unrecognized prompt shape falls back to `idle`. Treat blocked-status as a heuristic, not a contract. `herdr agent explain <target>` shows exactly which rule matched.
- Agents hidden behind VM/sandbox wrappers: hint with `HERDR_AGENT=claude fence -- claude` (scoped per-process, don't export globally).
- Local detection overrides at `~/.config/herdr/agent-detection/<agent>.toml` always win over remote manifests.
- If your shell framework auto-enters tmux inside Herdr, Herdr sees tmux, not the agent. Don't nest them — pick one session model.

---

## 10. The socket API: agents driving Herdr

This is the feature that makes Herdr "an infrastructure for agents orchestrating agents." The TUI itself uses this API.

### Protocol

- **Newline-delimited JSON** over a Unix socket:
  - Default session: `~/.config/herdr/herdr.sock`
  - Named session: `~/.config/herdr/sessions/<name>/herdr.sock`
- Request/response example:
  ```json
  {"id":"req_1","method":"pane.split","params":{"direction":"right"}}
  {"id":"req_1","result":{"pane":{"pane_id":"w1:p2"}}}
  ```
- `session.snapshot` returns a full one-shot state dump; `events.subscribe` keeps the connection open and pushes events (`pane.agent_status_changed`, `pane.output_matched`, `worktree.created`, …).
- Machine-readable contract: `herdr api schema --json` (full JSON Schema of every method and event); `herdr api snapshot` prints live state.
- **Auth: none.** Access control is filesystem permissions on the socket (restricted to your user). Treat the API as shell-level access — anyone who can reach the socket can type into any pane.

### The canonical orchestration recipe

The official skill ([SKILL.md](https://github.com/herdrdev/herdr/blob/master/skills/herdr/SKILL.md), installable into agents via `npx skills add herdrdev/herdr --skill herdr -g`) teaches any agent in a pane to:

```bash
# 0. Guard: only run inside Herdr
test "${HERDR_ENV:-}" = 1 || exit 1

# 1. Create a sibling worker pane (no focus steal)
split=$(herdr pane split --current --direction right --cwd "$PWD" --no-focus)
pane=$(printf '%s\n' "$split" | jq -r '.result.pane.pane_id')

# 2. Start an agent in it (blocks until ready)
herdr agent start reviewer --kind codex --pane "$pane"

# 3. Submit work and wait in one atomic call
herdr agent prompt reviewer "Review the current diff; report only actionable findings." \
  --wait --timeout 120000

# 4. Harvest the result
herdr agent read reviewer --source recent-unwrapped --lines 120

# 5. If it blocked on a prompt, drive the UI
herdr agent send-keys reviewer esc
```

Or run plain commands in a worker pane:

```bash
herdr pane run w1:p2 "just test"
herdr pane wait-output w1:p2 --regex "passed|failed" --timeout 120000
herdr pane read w1:p2 --source recent-unwrapped --lines 120
```

Skill safety rules worth adopting even for hand-written scripts: use `--no-focus` for background work; parse IDs from JSON responses (never sidebar order); don't close things you didn't create; never `herdr server stop` from an active session; use named throwaway sessions for experiments.

**Wait on state transitions, don't poll.** `agent wait` is server-owned and event-driven; `agent prompt --wait` makes submit-and-wait one race-free request. One subtlety: for non-hook agents (Claude, Codex), Herdr reliably catches `idle→working` but can miss `working→idle` — robust orchestrators either self-report via `pane report-agent` or race a periodic `agent get` poll against the wait.

### MCP servers (third-party)

There's no first-party MCP server; the ecosystem has three:

| Server | Approach | Notes |
|---|---|---|
| [eugeneb50/herdr-mcp](https://github.com/eugeneb50/herdr-mcp) | Shells out to the `herdr` CLI + socket event subscriber | ~51 tools incl. agent-to-agent primitives, recipes, cron scheduler, HTTP playground on :7676 |
| [runchr-works/herdr-mesh](https://github.com/runchr-works/herdr-mesh) | Node, wraps CLI | `herdr_relay` (message+submit) and `herdr_handoff` (send→wait→read in one call) are the A2A workhorses; auto-registers with Claude Code/Codex/opencode |
| [54rt1n/herdr-simple-mcp](https://github.com/54rt1n/herdr-simple-mcp) | Speaks the socket directly, stateless | 75 tools, one per socket method; **role profiles** (`coordinator`/`client`/`observer`) via `HERDR_MCP_PROFILE` — nice for giving supervised agents fewer privileges |

Native (non-MCP) socket clients also exist: [herdr-python-client](https://github.com/54rt1n/herdr-python-client) and [herdr-sock-go](https://github.com/lib-x/herdr-sock-go).

---

## 11. Workflows people actually run

### A. Supervisor + parallel specialists (the flagship pattern)

One orchestrator (you, or a lead agent) splits worker panes — Claude on frontend, Codex on backend, a third on tests. Your job moves up one level: review diffs, answer blocked agents, reject bad approaches. The sidebar is your dashboard; the `blocked` roll-up tells you where to look.

### B. Git worktree per task

HN users consistently call worktree management the killer feature — "so that different agents don't clash with each other." Herdr has native worktree support:

```sh
herdr worktree create --branch feat/search --base origin/main
herdr worktree open --branch feat/search
herdr worktree remove --workspace w3      # runs git worktree remove; never deletes the branch
```

`prefix+shift+g` creates a worktree-backed workspace interactively. Worktrees land in `~/.herdr/worktrees` by default. (Caveat from real users: if you find worktrees fighting git's "branch already checked out" rule, [jj](https://jj-vcs.github.io/jj/) dissolves the problem; a community `herdr-plugin-jj-workspace` exists.)

Ecosystem: `git-wt-herdr` (worktrees→tabs), `herdr-plugin-gh-workflow` (issue→branch→worktree→workspace), `herdr-worktree-lifecycle` (repo-owned setup/teardown), `herd-manager` (npm, worktree-first fleet manager).

### C. Review pipelines

- **Adversarial cross-model review:** [overflowy/herdr-claude-gpt-adversarial-review-skill](https://github.com/overflowy/herdr-claude-gpt-adversarial-review-skill) — Claude spawns a GPT reviewer in a split pane to attack the diff, then interrogates each finding → Confirmed/Disputed/Unverified.
- **Dual review + auto-merge:** [Tudor0404/dual-author](https://github.com/Tudor0404/dual-author) — paired Codex + Claude reviewers run fix-and-review rounds until clean.
- **Human-in-loop:** [persiyanov/herdr-reviewr](https://github.com/persiyanov/herdr-reviewr) — review the agent's diff in a sidebar; comments route back into its prompt.

### D. PR factories and fleets

`herdr-pr-loop` (durable multi-agent PR loop), `razajamil/herdr-factory` (Jira/markdown → PRs), `herdr-symphony` (GitHub Projects board → panes), `sean1588/herdr-orchestrator` (YAML state-graph issue→PR). Real users report running 10+ agents on multi-hour workstreams with hard stage gates (context → design → implement → critic → CI fixes) — the common rule: **no code committed without human review.**

### Parallelism hygiene (consensus from heavy users)

- Parallel agents = parallel blast radius. Small prompts, isolated worktrees, diff review.
- Pick **non-overlapping tasks**; agents still botch major merge conflicts ("often ends up dropping a lot of code").
- Be extremely careful with `--dangerously-skip-permissions` multiplied across a fleet.
- Many agents = CPU/RAM/LLM-cost multiplier. `herdr-token-dashboard`, `herdr-codex-usage-kit` give cost visibility.

---

## 12. Remote & headless usage ⭐ (your Fedora laptop server)

Herdr's server/client split makes headless-first setups first-class. Two modes:

### Mode 1: plain SSH

```sh
ssh you@laptop
herdr                 # or herdr session attach work
# detach: ctrl+b q — survives SSH drops, laptop lid events, etc.
```

### Mode 2: thin client (recommended on slow links)

From your main machine:

```sh
herdr --remote laptop                        # uses ~/.ssh/config aliases
herdr --remote ssh://you@laptop:2222
```

This **auto-installs Herdr on the remote host** if missing, streams *rendering data* rather than raw bytes (noticeably better over Wi-Fi), bridges your local clipboard (including image paste via `ctrl+v`), and keeps your local keybindings (`--remote-keybindings server` to use the server's instead).

Mobile access works too: at widths ≤ 64 columns Herdr switches to a touch-friendly layout — people attach from iPhone SSH clients. Community options for phone approval flows: `collie` (PWA over Tailscale with push), `herdr-ios` (native), `herdr-ntfy-notify`, `ccgram`/`tinysend-herdr` (Telegram / reply-by-email to unblock agents).

### Running the server as a systemd user service

Since your laptop is a headless lid-closed server, run the server at boot rather than from an SSH session:

```ini
# ~/.config/systemd/user/herdr.service
[Unit]
Description=Herdr agent multiplexer server
After=network.target

[Service]
Type=simple
ExecStart=%h/.local/bin/herdr server
Restart=on-failure
# Memory cap — see the bug notes below
MemoryMax=2G

[Install]
WantedBy=default.target
```

```sh
systemctl --user daemon-reload
systemctl --user enable --now herdr
sudo loginctl enable-linger $USER        # survive logout / never-logged-in boots
```

Then attach from anywhere with `herdr --remote laptop`.

### ⚠️ Known headless bugs to plan around (live as of v0.7.5)

The headless path is the project's liveliest bug area right now:

- **[#1762](https://github.com/herdrdev/herdr/issues/1762):** on GNOME Linux, **restarting GNOME Shell/GDM kills the herdr server** (tmux survives). If your laptop runs any GDM session, the systemd-user-service + linger setup above is the fix. On a truly headless box this doesn't apply.
- **[#2178](https://github.com/herdrdev/herdr/issues/2178)** (2026-08-01): headless server cgroup accumulated **9.9 GB of orphaned shared memory in 80 min**, freed only by restart. **[#1273](https://github.com/herdrdev/herdr/issues/1273):** similar ~3.3 GB-in-14-min growth report. → Keep the `MemoryMax=` cap; keep the binary updated (fixes are landing in real time).
- **[#2064](https://github.com/herdrdev/herdr/issues/2064)** (fixed 2026-07-30, *after* the v0.7.5 tag): after a headless restart, restored agents didn't respawn until a client attached. Fresh `agent start` works fully headless. → Run a build newer than v0.7.5 (or attach once after restart).
- **[#2063](https://github.com/herdrdev/herdr/issues/2063):** plain `agent prompt` (no `--wait`) can return `ok` for prompts never actually submitted to an out-of-view pane. → Always use `--wait`, or verify with `agent read`.
- **[#276](https://github.com/herdrdev/herdr/issues/276):** deterministic crash after ~35–41 h uptime on a long-running server (12 workspaces/45 panes). → `Restart=on-failure` in the unit file matters.

Practical headless recipe: installer-managed binary → systemd user service with linger + `MemoryMax` → attach with `herdr --remote laptop` → update weekly for now.

---

## 13. Plugins & ecosystem

### Plugin system

A plugin is a directory with a `herdr-plugin.toml` manifest plus executables in any language. **There is no SDK — "the entire Herdr CLI is the plugin API."** Plugins get `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`, per-plugin config/state dirs, and can hook events (`worktree.created`), add actions, keybindings, custom panes, and link handlers. The marketplace is an auto-index of GitHub repos tagged `herdr-plugin`, refreshed every 30 min.

```sh
herdr plugin install persiyanov/herdr-reviewr        # shows a source preview first; --yes to skip
herdr plugin install owner/repo --ref v1.2.3         # pin untrusted plugins to a rev
herdr plugin list
herdr plugin uninstall herdr-reviewr
```

Notes: installs are **global per-user** (since v0.7.5); there's **no `plugin update` in v1** — reinstall to refresh; Herdr validates manifests but **does not sandbox plugin code** — skim scripts before installing.

### Highlights from [awesome-herdr](https://github.com/yigitkonur/awesome-herdr)

| Category | Picks |
|---|---|
| **Agent integration** | Official [SKILL.md](https://github.com/herdrdev/herdr/blob/master/skills/herdr/SKILL.md) (start here) · [herdr-pm](https://github.com/yigitkonur/herdr-pm) (per-tab conductor) · [msadig/herdr-peer-agents-skill](https://github.com/msadig/herdr-peer-agents-skill) (agent-to-agent delegation) · [yigitkonur/claude-code-herdr-plugin](https://github.com/yigitkonur/claude-code-herdr-plugin) (drive a Codex sub-agent, get one JSON verdict) |
| **Connectivity** | [herdr-mcp](https://github.com/eugeneb50/herdr-mcp) / [herdr-mesh](https://github.com/runchr-works/herdr-mesh) / [herdr-simple-mcp](https://github.com/54rt1n/herdr-simple-mcp) · [herdr-python-client](https://github.com/54rt1n/herdr-python-client) · [herdr-sock-go](https://github.com/lib-x/herdr-sock-go) |
| **Notifications** | [herdr-remote](https://github.com/dcolinmorgan/herdr-remote) (approve from phone/watch/Telegram) · [collie](https://github.com/AltanS/collie) (phone PWA + push) · [herdr-ntfy-notify](https://github.com/zom-2018/herdr-ntfy-notify) · [ccgram](https://github.com/alexei-led/ccgram) (Telegram) |
| **Editor** | [vim-herdr-navigation](https://github.com/paulbkim-dev/vim-herdr-navigation) (seamless ctrl+hjkl across vim splits & panes) · [herdr.nvim](https://github.com/devxplay/herdr.nvim) · [shepherd.nvim](https://codeberg.org/SOM38/shepherd.nvim) (send text/files/selections to agents) |
| **Sessions** | [switchr](https://github.com/ridho9/switchr) (TUI session picker) · [herdrctx](https://github.com/j0urneyk/herdrctx) · [herdr-session-restore](https://github.com/nickmaglowsch/herdr-session-restore) (layouts + Claude sessions across reboots) · [shirei](https://lib.rs/crates/shirei) (fzf switcher across tmux + Herdr) |
| **Worktrees** | [git-wt-herdr](https://github.com/noamsiegel/git-wt-herdr) · [herdr-worktree-lifecycle](https://github.com/qdentity/herdr-worktree-lifecycle) · [herdr-worktreeinclude](https://socket.dev/npm/package/herdr-worktreeinclude) (in-project worktrees + gitignored-file restore) · [herdr-branch-cleanup](https://github.com/dutifuldev/herdr-branch-cleanup) |
| **UI extras** | [herdr-webui](https://github.com/alecuba16/herdr-webui) (full browser UI: terminals + Git + files + agent states) · [herdr-file-viewer](https://github.com/smarzban/herdr-file-viewer) (git-aware side pane) · [herdr-token-dashboard](https://github.com/Davidcreador/herdr-token-dashboard) (live cost dashboard) · [herdr-ask](https://github.com/TaylorFinklea/herdr-ask) (popup command generator; never auto-executes) |

### Which agents work well inside Herdr

- **Best instrumented (lifecycle hooks):** Pi, OpenCode, Kilo Code CLI, Hermes, Kimi Code CLI, MastraCode, OMP.
- **Most popular pairings (screen detection + hooks for restore):** **Claude Code** and **Codex** — by far the largest ecosystem surface.
- Also detected: Copilot CLI, Devin, Droid, Cursor Agent, Gemini CLI, Cline (partial), Amp, Grok, and more. Anything unsupported still runs as a normal process; you can report its state via the socket API.

---

## 14. Gotchas & limitations (from issues and real users)

**Sharp edges:**

- `herdr integration install claude` rewrites `~/.claude/settings.json` wholesale ([#2066](https://github.com/herdrdev/herdr/issues/2066)) — back it up.
- Running Herdr **inside tmux breaks both** its detach persistence and OSC-52 copy. Pick one.
- `blocked` detection is a conservative heuristic; novel prompt UIs read as `idle`. Agent status can lag a few seconds.
- Mouse capture (`mouse_capture = true`, default) breaks plain middle-click paste (use Shift+middle-click); `false` restores paste but selections span the whole grid.
- Pasting multiline strings can auto-submit each line ([#670](https://github.com/herdrdev/herdr/issues/670)).
- Arrow keys don't auto-repeat (kitty keyboard protocol) — use `hjkl` in copy mode.
- No tmux-style automatic tab renaming; madflex replicates it with zsh `preexec` hooks guarded on `$HERDR_TAB_ID` calling `herdr tab rename`.
- `zoom` only maximizes within a tab — with one pane per tab it does nothing; there's no true zen mode and the tab bar is always visible.
- Idle CPU: the client redraws the focused pane; Claude Code's spinner animation pins a few % CPU even "idle."

**Structural:**

- **No sandboxing.** Agents run with your user permissions; Herdr won't stop destructive actions. Add your own (containers, VMs, `fence`, `safecodex`).
- The persistent server holds whatever's in your panes — API keys in env, tokens in output. Restrict who can reach the machine and the socket. `pane_history` persists screen contents to disk — leave it off on shared boxes.
- Bus factor of 1 (solo maintainer, albeit sponsored and full-time).
- Young ecosystem vs tmux's 18 years; and LLMs already "know" tmux from training, whereas Herdr needs its skill installed.

### vs the alternatives

| Tool | Honest tradeoff |
|---|---|
| **tmux + hooks** | Maturity, ecosystem (resurrect/continuum/tmuxinator), deep customization, and LLMs know it cold. No native agent state — though HN users have replicated it with Claude Code hooks wiring window names. "An afternoon of vibing" gets you ~80% there. Herdr wins on zero-config state detection, mouse UX, the socket API, and remote attach. |
| **[claude-squad](https://github.com/smtg-ai/claude-squad)** (`cs`) | Built on tmux + git worktrees: each session gets its own worktree/branch with diff review, commit/push, pause/resume. More opinionated task-queue UX; less general (no socket API, no agents-driving-agents, tied to tmux). Pick it for "a queue of isolated branch tasks"; pick Herdr for supervision and programmability. |
| **cmux / Conductor** | Desktop GUIs, worktree-based, macOS-centric — can't match SSH-from-anywhere. |
| **agentgrid** | `npx agentgrid 2x3 claude` — dead-simple grid with status colors; no persistence or orchestration. |

Reviewer consensus: **overkill for single-agent use; compelling once you run ≥3 agents or long remote jobs.**

---

## 15. Quick-start cheat sheet

```sh
# Install
curl -fsSL https://herdr.dev/install.sh | sh

# Daily driver
cd ~/project && herdr                 # start/attach
claude                                # run an agent in a pane
# prefix+v  split right · prefix+c new tab · prefix+w workspaces
# ctrl+b q  detach · herdr  reattach

# Orchestration (from a script or an agent pane)
pane=$(herdr pane split --current --direction right --no-focus | jq -r '.result.pane.pane_id')
herdr agent start worker --kind codex --pane "$pane"
herdr agent prompt worker "Add tests for auth.ts" --wait --timeout 120000
herdr agent read worker --source recent-unwrapped --lines 120

# Remote (from another machine)
herdr --remote laptop

# When something's wrong
herdr agent explain w1:p2             # why this state?
herdr status                          # server/socket/protocol health
herdr integration status              # agent hooks installed?
```

---

## Sources

Official: [herdr.dev/docs](https://herdr.dev/docs) · [GitHub: herdrdev/herdr](https://github.com/herdrdev/herdr) · [SKILL.md](https://github.com/herdrdev/herdr/blob/master/skills/herdr/SKILL.md) · [Homebrew formula](https://formulae.brew.sh/formula/herdr) · [marketplace](https://herdr.dev/marketplace)

Ecosystem: [awesome-herdr](https://github.com/yigitkonur/awesome-herdr) · [herdr-mcp](https://github.com/eugeneb50/herdr-mcp) · [herdr-mesh](https://github.com/runchr-works/herdr-mesh) · [herdr-simple-mcp](https://github.com/54rt1n/herdr-simple-mcp) · [herdr-webui](https://github.com/alecuba16/herdr-webui) · [pi-bellwether](https://github.com/joelhooks/pi-bellwether) · [adversarial-review skill](https://github.com/overflowy/herdr-claude-gpt-adversarial-review-skill)

Reviews & write-ups: [Dotzlaw: parallel agent sessions](https://dotzlaw.com/insights/claude-code-13-herdr-parallel-agent-sessions/) · [pvgomes: the throughput case for parallel agents](https://dev.to/pvgomes/herdr-and-the-throughput-case-for-parallel-coding-agents-4dk0) · [madflex: trying herdr instead of tmux](https://madflex.de/trying-herdr-instead-of-tmux/) · [andrew-ooo review](https://dev.to/andrew-ooo/herdr-review-the-agent-multiplexer-your-terminal-needed-li1) · [bitdoze review](https://www.bitdoze.com/herdr-agent-multiplexer/) · [oflight deep dive](https://www.oflight.co.jp/en/columns/herdr-terminal-agent-multiplexer-2026-07) · [tecmint Linux guide](https://www.tecmint.com/herdr-run-ai-coding-agents-in-linux-terminal/) · [HN discussion](https://hn.nuxt.dev/item/48756578) · [classmethod: tmux→herdr migration](https://dev.classmethod.jp/en/articles/herdr-tmux-replacement/)

Issues cited: [#1340 (license)](https://github.com/herdrdev/herdr/issues/1340) · [#2066 (settings.json rewrite)](https://github.com/herdrdev/herdr/issues/2066) · [#1762 (GDM kills server)](https://github.com/herdrdev/herdr/issues/1762) · [#2178 (shmem leak)](https://github.com/herdrdev/herdr/issues/2178) · [#1273 (memory growth)](https://github.com/herdrdev/herdr/issues/1273) · [#2064](https://github.com/herdrdev/herdr/issues/2064) · [#2063](https://github.com/herdrdev/herdr/issues/2063) · [#276 (long-uptime crash)](https://github.com/herdrdev/herdr/issues/276) · [#670 (multiline paste)](https://github.com/herdrdev/herdr/issues/670)
