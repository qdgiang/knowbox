# Herdr plugins for productivity — deep-research report

**Research date:** 2026-08-24 · **Question:** Which herdr plugins should I install to
improve productivity on a headless Fedora 44 server running herdr v0.7.5 as a
systemd user service, with Claude Code multi-agent orchestration (supervisor +
parallel workers, one worktree per task PR-factory, multi-persona setups)?

**Method:** deep-research workflow — 5 search angles → 21 sources fetched →
104 claims → top 25 adversarially verified (3 voters each, 2/3 refutes kills) →
23 confirmed, 2 refuted, 12 findings after synthesis. 103 agent calls.

> ⚠ **Staleness:** herdr is a fast-moving 0.x — commands/flags and the plugin
> marketplace change weekly. This snapshot is 2026-08-24. Re-verify version
> compatibility against the installed binary (`herdr --version`) and the live
> GitHub topic `herdr-plugin` before installing. See [[kb-0008]] §13.

---

## 1. Headline: the highest-value action is a herdr upgrade, not a plugin

Upgrade herdr **0.7.5 → 0.8.2** before adding plugins. 0.8.0 (2026-08-03) fixed a
headless-specific bug that directly affects this setup — restored agent
sessions no longer wait for a TUI client to attach before resuming — and 0.8.2
(2026-08-19) is the current Homebrew stable (local 0.7.5 is marked outdated).
This is a **prerequisite, not optional**.

- Changelog (verbatim): `[0.8.0] Fixed: "Headless servers now resume restored
  agent sessions without waiting for a TUI client to attach. (#2064)"`.
  `#2064` reproduces the exact bug at 0.7.5 (restored agents unspawned until a
  client attaches, then respawn with `claude --resume`).
- Local verification: `herdr --version` = 0.7.5; `brew info herdr` shows
  0.7.5 → stable 0.8.2 (outdated). Upgrade: `brew upgrade herdr`.
- **0.7.5 breaking change (already in effect):** installed/linked plugins and
  their enabled state became **global per-user** instead of session-scoped
  (#1174). Any plugin installed only inside a named session on 0.7.3 must be
  reinstalled/relinked — relevant if the systemd service used named sessions.
- Upgrading unlocks 0.8.0-only plugins (e.g. herdr-pane-id).

> **Open question (untested here):** does the 0.7.5→0.8.2 upgrade introduce
> further breaking changes for a headless/systemd-user-service deployment, and
> does plugin global-state migration need manual cleanup? Back up the systemd
> unit + session/plugin config first.

---

## 2. Ecosystem is ~40× the awesome-herdr snapshot

- The GitHub topic `herdr-plugin` aggregates **813 public repos**; the official
  marketplace auto-indexes **762 plugins across 749 repos** from that topic
  (refreshed every 30 min). The early-August awesome-herdr snapshot had ~20.
- **Discovery method going forward:** topic search + `herdr.dev/plugins`
  marketplace, not the awesome-herdr list.
- ⚠ Raw topic search is noisy: the top-starred list includes mis-tagged
  non-plugins (oh-my-opencode-slim, terminal-browser, pi-workflows) and many
  abandoned experiments. 813 repos ≠ 813 maintained plugins.

---

## 3. Ranked recommendations

All install commands: `herdr plugin install owner/repo` (preview shown first;
`--yes` to skip; `--ref vX` to pin untrusted plugins). ⚠ Plugins are
unsandboxed — read scripts before installing (see §5).

### Notifications / approval routing from a headless server

| # | Plugin | Verdict | Evidence | Confidence |
|---|---|---|---|---|
| 1 | `herdr-mobile-relay` (`0cv/herdr-mobile-relay`, ~123★, pushed 2026-08-24) | **Best fit.** Phone approvals + push for blocked agents over E2E-encrypted WebRTC/Cloudflare-tunnel/self-hosted relay; declares herdr ≥0.7.5 (exact); 215 commits, v0.18.1. Structured plan questions for Claude Code/Codex/OpenCode. | Repo description verified verbatim; topic page updated today. | high |
| 2 | `collie` (`AltanS/collie`, 504★, updated 2026-08-24) | **Mature PWA.** Tailnet-accessible, Web Push, quick actions; herdr ≥0.7.0; 424 commits ~v0.15.0 — strongest maintenance evidence of any plugin here. Already in baseline; keep it. | Topic page + repo verified. | high |
| — | `herdr-ntfy-notify` (`zom-2018`) | Zero-dep ntfy push on done/blocked — but **1 commit** and README says "Herdr v1"; 0.7.x compat **unconfirmed**. | README verified; refuted the "raises a compat question" framing as too strong — verdict is simply unknown. | low/medium |
| — | `herdr-push` (`dcolinmorgan`, ~9★/13 commits) | Zero-dep push to `herdr-remote` relay, one-tap approval. Needs the `herdr-remote` relay running separately. | About verified verbatim. | medium |

> `herdr-mobile-relay` vs `collie`: mobile-relay is the closer match (exact
> 0.7.5 pin, push for blocked agents, structured Claude Code approvals); collie
> has the strongest maintenance track record. **Open question:** for
> mobile-relay, which relay deployment (Cloudflare tunnel vs self-hosted VPS)
> is best on a lid-closed laptop behind NAT, and what are each's auth
> properties?

### Worktree + PR lifecycle

| # | Plugin | Verdict | Confidence |
|---|---|---|---|
| 1 | `herdr-worktrunk` (`devashish2203/herdr-worktrunk`, 103★, updated 2026-08-21) | Wraps worktrunk's `wt` CLI (create/switch/remove worktrees with lifecycle hooks) into herdr; herdr ≥0.7.0. ⚠ Single author, ~13 commits. | high |
| 2 | `herdr-worktree-lifecycle` (`qdentity/herdr-worktree-lifecycle`, ~7 commits) | Dispatches `worktree.created`/`opened` → `scripts/worktree-setup` and `worktree.removed` → `scripts/worktree-teardown` (ABI `<branch> <worktree> <primary>`). Perfect for per-worktree venv/dep setup in a PR-factory. | high |
| skip | `wt-herdr` (`mattarau/wt-herdr`, 2★, 4 commits, all 2026-06-14, no releases) | Mirrors `wt` worktrees as herdr workspaces; very low maturity; declares only "herdr v0.6+" generically. Wait — herdr-worktrunk covers the same need with more activity. | high |

### Token / cost monitoring

| Plugin | Verdict | Confidence |
|---|---|---|
| `herdr-telemetry-bridge` (`CodyBontecou`, 2★, 2 commits 2026-06-26) | Streams workspace/repo/agent telemetry incl. best-effort token counters + trace summaries (message counts, tool calls) to NDJSON file / HTTP webhook / command stdin — right shape for headless (terminal-independent sinks). Herdr ≥0.7.0, Node ≥18. ⚠ **Experiment only:** 2 commits, no releases; token counts explicitly best-effort not billing-accurate; untested on 0.7.5. | high |
| `herdr-token-dashboard` (`Davidcreador`, from baseline) | Declares min herdr 0.7.0, Linux-supported → compatible with 0.7.5. **Open question:** whether it's a safer token-monitoring option than telemetry-bridge. | medium |

### Agent-to-agent orchestration / supervisor visibility

| Plugin | Verdict | Confidence |
|---|---|---|
| `herdr-lantern` (`aigorahub/herdr-lantern`, id `aigora.lantern`, v0.9.10) | Supervisor dashboard for multi-agent patterns: shows who needs attention, what agents are working on, jump-to-pane, seat new agents with Claude Code explicitly supported (`HELPER_AGENT="claude"` from PATH, alongside Cursor/Devin/Codex/Grok). Requires exactly herdr ≥0.7.5. **Do NOT** seek its old Telegram/WhatsApp/Slack "Bridge" — removed in v0.8.0 for cause (shell access, §5). | high |

### Compatibility-blocked (install after upgrading to 0.8.2)

- `herdr-pane-id` (`imtim/herdr-pane-id`, created 2026-08-18, pushed today):
  labels panes/tabs/workspaces with IDs and detected agent names — useful for
  multi-agent setups. Manifest requires herdr ≥0.8.0; herdr refuses to install
  plugins whose min version exceeds the running binary. Install after upgrade:
  `herdr plugin install imtim/herdr-pane-id`. (Alternative that works on 0.7.5
  now: `4Born/herdr-pane-id-labeler`, requires ≥0.7.5.)

### New high-star plugins the baseline missed (topic top-20, 2026-08-24)

Not verified for functionality/security — flagged for a follow-up deep-dive:

- `herdr-reviewr` (persiyanov, 509★, pushed 2026-08-23) — review sidebar;
  select changed lines, send comments back to the agent as
  `path:start-end — comment` for the next turn.
- `herdr-browser` (ogulcancelik, 340★, pushed 2026-08-22)
- `herdr-plus` (cloudmanic, 255★) · `herdr-mirror` (178★) · `herdr-sidebar`
  (177★) · `memex` (158★) · `herdr-navigator` (106★)

---

## 4. Other verified findings

- **Session restore (built-in):** after a herdr server restart (systemd restart
  or reboot), herdr's snapshot restore recreates workspaces/tabs/panes/cwd/layout
  — but **not running processes**; panes come back as new shells in saved dirs.
- **Process-level restore:** `herdr-restart-always` (`terafin`) — per-pane
  detached monitor that polls `herdr pane get`, detects dead agents, re-runs the
  resume command. Fills the gap that herdr's native restore only triggers on
  server restart, not on a mid-running-agent process death.
- **Claude Code state reporting is first-party:** `herdr integration install
  claude` writes `~/.claude/hooks/herdr-agent-state.sh` + modifies
  `~/.claude/settings.json`, reporting semantic states (working/blocked/idle)
  over the socket API — not a community plugin. ⚠ It rewrites
  `~/.claude/settings.json` wholesale (#2066) — back it up first.

---

## 5. Security caveats (load-bearing for an always-on headless server)

1. **Marketplace is unreviewed:** "Listings aren't reviewed by Herdr, so install
   at your own discretion." Docs add herdr does **not sandbox plugin code** —
   plugins run as the user, inherit the environment, can call the full herdr CLI.
2. **Documented precedent:** `herdr-lantern`'s Telegram/WhatsApp/Slack Bridge
   was **removed in v0.8.0** (2026-08-21) because "an allowlisted sender got a
   shell on the machine running it… It is not a sandbox."
3. **No** review process, scanning, trust tier, or signing exists in any
   official material.

**Rule:** treat every plugin as arbitrary code running as your user. Prefer
zero-dependency, shell-free plugins; read `on_event` scripts before install;
avoid chat-bridge plugins that pipe messages to a shell. Keep installs minimal
and vetted.

---

## 6. Refuted claims (adversarial verification killed these)

- *"herdr-ntfy-notify explicitly targets Herdr v1, raising a compatibility
  question for a user running 0.7.5"* — **0-3 refuted.** The "v1" framing was
  too strong; the accurate verdict is simply that 0.7.x compat is **unconfirmed**.
- *"herdr-push relies on a Cloudflare quick tunnel with no documented auth or
  encryption, a headless security caveat"* — **1-2 refuted.** Relay choice is
  flexible (self-hosted / Cloudflare); the single-relay characterization was
  inaccurate.

---

## 7. Open questions

1. Does `brew upgrade herdr` to 0.8.2 introduce breaking changes beyond the
   headless-resume fix that affect a systemd-user-service deployment, and does
   plugin global-state migration need manual cleanup?
2. What do `herdr-reviewr` (509★) and `herdr-browser` (340★) actually do, and
   are they safe/useful for a headless Claude Code PR-factory workflow?
3. Does `herdr-ntfy-notify`'s "Herdr v1" targeting mean incompatible with
   0.7.x/0.8.x, or forward-looking? Is `herdr-token-dashboard` a safer
   token-monitor than `herdr-telemetry-bridge`?
4. For `herdr-mobile-relay`, which relay deployment (Cloudflare tunnel vs
   self-hosted VPS) is best on a lid-closed laptop behind NAT, and what are the
   auth properties of each?

---

## Sources (21 fetched, 2026-08-24)

Primary: `github.com/topics/herdr-plugin` · `github.com/herdrdev/herdr/blob/master/CHANGELOG.md`
· `github.com/herdrdev/herdr/releases` · `herdr.dev/plugins/` ·
`herdr.dev/docs/plugins/` · `herdr.dev/docs/session-state/` · repo READMEs for
0cv/herdr-mobile-relay, AltanS/collie, zom-2018/herdr-ntfy-notify,
dcolinmorgan/herdr-push, devashish2203/herdr-worktrunk,
qdentity/herdr-worktree-lifecycle, mattarau/wt-herdr,
CodyBontecou/herdr-telemetry-bridge, aigorahub/herdr-lantern, imtim/herdr-pane-id,
Davidcreador/herdr-token-dashboard, terafin/herdr-restart-always, persiyanov/herdr-reviewr.
Secondary/blog: `yigitkonur/awesome-herdr` (README + integrations.md) ·
`coles.codes/posts/herding-agents-with-herdr/` · `flaviocopes.com/herdr-plugins/`
· `r/ClaudeCode` approval-routing thread.
