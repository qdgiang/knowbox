# Claude Code Harness Audit & Improvement Report

**Machine:** `fedora-server` — headless Fedora 44 laptop (i5-9300H, 4C/8T, 24 GB RAM, ~633 GB free disk), user `giang`, always-on lid-closed
**Date:** 2026-08-05 · **Claude Code:** 2.1.222 (native install) · **Profile:** aibox proxy (`api.ai-box.vn`), model `fable`, effort `xhigh`
**Method:** 9-agent fleet — 2 local auditors (harness config, `~/kb` system) + 6 web researchers (harness best practices, subagents/teams, memory/KB, code graphification, MCP/plugins, model routing) + 1 completeness critic that cross-checked all findings against disk. ~197k tokens of research, 300 tool calls, all sources fetched 2026-08-05.

---

## 1. Executive summary

**The setup is unusually well-designed and unusually under-wired.** You have built three things most people never do: a research-backed personal knowledge base (`~/kb`, index-first, lifecycle-gated, git-backed), a clean cross-tool convention layer (`AGENTS.md` shared by four agent tools), and a disciplined memory division of labor (auto-memory = pointers + session prefs; KB = canonical facts). Anthropic's own 2026 memory docs describe almost exactly the architecture you already run.

What's missing is not design — it's **automation, enforcement, and observability**:

- **Zero hooks.** Every "always should happen" behavior (KB capture, safety gates, notifications) currently runs on the honor system of prose instructions.
- **Zero custom subagents** despite 36 UI nudges and OpenCode on the same box already running a cost-routing worker pattern.
- **Zero MCP servers, zero installed plugins** (54 available in the already-registered official marketplace).
- **Zero permission allowlists** despite 132 queued prompts — maximum prompt friction.
- **No notification path** for a headless box — when a background session needs you, nothing happens.
- **No backup** for ~1 GB of agent state (`~/.hermes` alone is 861 MB) — `~/kb` is the *only* thing with an off-box copy.
- **A plaintext API token** sitting in three files any agent session can read, and a **profile system that has already drifted** out of sync once.

And one headline finding for your three named focus areas:

| Focus area | Verdict |
|---|---|
| **Persistent knowledge base** | Excellent foundation. Next steps: fix 6 small CLI bugs, run the linter (9 of 22 notes have never been linted), add a SessionEnd distillation hook + thin KB-MCP server. Do **not** add embeddings yet — your own revisit rule (>200 notes) is right; mechanize the trigger instead. |
| **Subagents / agent teams** | Build a 5-agent native fleet first (free, 30 min). Background sessions (`claude --bg`) are the killer headless feature. Agent teams = experimental, 7× tokens — trial only on research tasks later. |
| **Graphify codebase** | **Not yet.** Your largest codebase is 104 Python files; graphs pay off at >50k–150k LOC. The honest ROI is per-project `CLAUDE.md` files (3 projects have none) + a `.venv` guard. Revisit with a measurement gate. |

---

## 2. Current state — what's on the box

### 2.1 Configuration
- Three settings files + `use-profile.sh` switcher: `official` (real Anthropic API, `opus[1m]`) and `aibox` (proxy with tier mapping: fable→`qwen3.8-max`, opus→`glm-5.2`, sonnet→`kimi-k2.7-code`, haiku→`deepseek-v4-flash`).
- ⚠️ **Active `settings.json` matches neither profile** ("custom"). Hand-edits (`model: fable`, `skipWorkflowUsageWarning`, fable tier change) were made after the last profile copy. Re-running `use-profile.sh aibox` would silently revert them.
- ⚠️ **Plaintext aibox API token** in `settings.json`, `settings.aibox.json`, and commented in `~/.config/opencode/opencode.jsonc`. Hermes docker-compose also carries a plaintext dashboard password.
- `effortLevel: xhigh`, `agentPushNotifEnabled: true`, `autoUpdates: false` (yet an auto-update 2.1.221→2.1.222 ran on 2026-08-04 — update policy is accidental).

### 2.2 Knowledge & memory
- `~/kb`: 22 atomic notes, 2 refs, custom 554-line stdlib Python CLI (BM25 FTS5 + trigram fallback, index-first, git auto-commit/push, weekly lint cron). 4 days old, 54 commits — actively used.
- Auto-memory in 3 project dirs: KB-pointer notes + genuine feedback notes. Working as designed; three pointer notes are byte-duplicated across two dirs (minor).
- `~/AGENTS.md` + `~/.claude/CLAUDE.md` (with `@~/AGENTS.md` import): shared conventions for Claude Code, OpenCode (symlinked), Copilot, OpenClaw. Clean single-source-of-truth pattern.

### 2.3 Projects
| Project | Claude integration | Reality |
|---|---|---|
| `~/projects/e2e-data-ai-project` | CLAUDE.md ✅ (with a **dead reference** to `~/ServerStuff/…` — file actually lives in `~/kb/refs/`) | 104 real .py files; 458 MB is mostly `.venv` (11,941 apparent files — agent trap) |
| `~/projects/personal-website` | **none — Claude has never run here** | Astro site, maintained, 36 KB ARCHITECTURE.md sitting unused |
| `~/projects/personal-resume` | none | LaTeX; **no git repo**; build artifacts committed |
| `~/containers/hermes` | none (rules live in auto-memory + kb-0017…0022) | 4 config files; Hermes Agent state in `~/.hermes` (861 MB, unbacked-up) |

### 2.4 The zeros
No hooks · no `~/.claude/agents/` · no MCP servers · no installed plugins · no statusline · no keybindings · no output styles · no `settings.local.json` anywhere · no backup tooling (`borg`/`restic`/`rclone` absent) · one crontab line total.

---

## 3. Findings by area

### 3.1 Persistent knowledge base — strong core, weak lifecycle machinery

**What the deep audit confirmed is genuinely good:** the SCHEMA.md contract (performance budgets, atomic notes, supersede-don't-delete), index-first routing (validated independently by the "index-guided retrieval beats RAG at personal scale" finding in the community), BM25 weights tuned with aliases above body, the `/kb` skill's clean procedure-vs-convention split, and the write gate (search-before-create). The memory-framework research (mem0 v3, Zep/Graphiti, Letta, supermemory, cognee, Basic Memory) converges on *your* architecture: every framework is append→dedupe/contradict→hybrid-retrieve→inject, just with more daemons and an LLM in the write path. At 22 notes, your manual gate is a feature.

**Concrete problems found (with evidence):**
1. **`kb lint` hasn't run since the corpus had 13 notes** — 9 notes (incl. kb-0016's 355-char summary, over the 220 limit) have never been checked. Cron exists but log lines have no timestamps and failures alert nowhere.
2. **CLI bugs:** `kb new` commits *before* `$EDITOR` opens and never after (the template lands in git, your edits ride uncommitted); `kb touch --supersede`'s insert-if-missing branch is a no-op `re.sub`; search swallows `OperationalError` as "no matches"; trigram fallback has no `ORDER BY`.
3. **Lifecycle never used:** 0/22 `supersedes:` entries; both real fact-changes were in-place edits against SCHEMA's letter. `kb verify` leaves no trace in the note itself (evidence only in log.md). Friction, not discipline, is the cause.
4. **Link drift:** 8/22 notes have body `[[kb-…]]` links missing from frontmatter `related:` — the graph is understated and unqueryable.
5. **Churn invisible:** kb-0017 was touched 12× in 3 days (the skill's own "make it a live pointer" warning) with no tooling to surface it.
6. **Capture is honor-system:** nothing prompts KB writes at session end; `/kb` skill usage = 1 recorded invocation.

**State of the art you could draw from (researched, with fit verdicts):**
- **SessionEnd distillation hooks** are now a mainstream pattern (claude-mem ~90k★; claude-memory-compiler — structurally your pipeline with hooks bolted on).
- **KB-as-MCP-server:** Basic Memory is the reference design (markdown-first + SQLite index + MCP tools with behavior hints). For you: a ~80-line FastMCP wrapper over the existing `kb` CLI.
- **Embeddings:** sqlite-vec + sqlite-rembed + Ollama (`nomic-embed-text` or `qwen3-embedding:0.6b`) fit your stack if ever needed. BM25 *beats* embeddings for your rare-term-heavy corpus (grubby, consoleblank, kb-ids) until paraphrase queries dominate — i.e., not yet.

### 3.2 Subagents & agent teams — the highest-leverage empty room

**Native capabilities (2026):** custom agents in `~/.claude/agents/*.md` (hot-reloaded; frontmatter controls tools/model/effort/permissionMode/memory/`isolation: worktree`); subagents run **background by default** since v2.1.198; nesting depth 3, 20 concurrent / 200 per session; `SendMessage` resumes finished subagents with full history; forks (`/subtask`) reuse the parent prompt cache.

**Four orchestration layers exist now:**
1. **Subagents** (stable) — workers inside a session.
2. **Agent view / background sessions** (research preview): `claude --bg`, `claude agents` TUI, a **supervisor daemon** that survives SSH disconnects, auto worktree isolation, auto-branch/push. *The most headless-friendly feature Claude Code has* — built for exactly your machine.
3. **Agent teams** (experimental, `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`): lead + teammates, shared task list, inter-agent mailboxes, tmux split panes. **≈7× token cost**, no resume, one team/session. Docs say start with research/review, not code.
4. **Dynamic workflows** (what produced this report): JS-orchestrated fan-outs, `/deep-research` bundled, capped 16 concurrent — right primitive for scheduled whole-repo audits.

**Token economics (Anthropic engineering + cost docs):** subagents ≈4× chat tokens, teams ≈7–15×; multi-agent beat single-agent by 90% on research evals but *hurt* most coding tasks. Sweet spot: 3–5 spawned at once, cheap model for grunt work, outputs written to files not returned through context.

**Community frameworks assessed:** Ruflo/claude-flow (67k★, maximalist — would fight your KB for "brain" role), oh-my-claudecode (38k★, interactive-first), Gas Town (17.5k★, the most serious always-on agent factory — tmux+Dolt daemon fleet, real ops weight), claude-squad (cockpit, not daemon), wshobson/agents (203 ready-made agents — cherry-pick 2–3, don't bulk install), Claude-Code-Router (36k★ — a gateway, not orchestration).

### 3.3 Graphifying the codebase — honest answer: not yet

Claude Code has **no native indexing/embeddings** (verified against docs; discovery = bundled ripgrep + Read), so the gap is real — but graphs pay off at **>~50k–150k LOC**, polyglot monorepos, or refactor/impact-heavy work. Your largest repo is **104 source files**. Counter-evidence matters too: a forced 9k-token full function list performed *worse* than a focused 500-token map; MCP tool schemas consume context; and on your aibox backend tool-search is disabled so every schema loads upfront (see 3.5).

**Ranked for later (when a repo crosses ~50k LOC or grep demonstrably fails):**
1. **Graphify** (102k★, Apache-2.0, `uv tool install graphifyy`) — category leader, ships a Claude Code *skill* + hooks, pure-local AST, inspectable markdown output. Young (Apr 2026) — pin and verify.
2. **codebase-memory-mcp** (37.6k★, static C binary + SQLite) — lowest footprint; `trace_path`/`detect_changes` are the grep-impossible queries.
3. **Serena** (27.6k★, mature, LSP-based) — if *editing* (symbol replace/rename) hurts more than finding; 100–400 MB RAM per language server; "eats context" per field reports.
4. Skip: GitNexus (PolyForm Noncommercial license + 2 GB buffer pool default), code-graph-rag (Docker+Memgraph), Greptile hosted (code leaves box).

**The free equivalent that covers your actual size:** per-project `CLAUDE.md` (3 of 4 projects have none), a `.venv`-guard note in e2e's CLAUDE.md, and the existing `~/kb` refs maps. Aider's repo-map lineage confirms: *map first, then targeted reads* — your KB already does this for knowledge; a 20-line CLAUDE.md does it per repo.

### 3.4 Hooks & automation — the biggest structural gap

2026 Claude Code has ~30 hook events (SessionStart/UserPromptSubmit/Pre+PostToolUse/PermissionRequest/Notification/SubagentStop/TaskCreated/Stop/PreCompact/SessionEnd/ConfigChange/FileChanged…), four handler types (`command`, `http`, `mcp_tool`, `prompt`, `agent`), `if:` filters, async hooks, and — critically — **hooks deny even in `bypassPermissions`**: the only enforcement layer a mode-switch can't bypass. Recipes that map directly to this box:

- **Notification → Telegram** (you already run the Hermes bot; headless boxes have no notification daemon — this is load-bearing for everything fire-and-forget).
- **SessionEnd KB capture** (`transcript_path` is in every hook input) → distill → `~/kb/raw/` proposals.
- **SessionStart `compact` matcher** → re-inject KB index routing reminders after compaction.
- **PreToolUse safety gates** (protected files, `.env`, destructive git) — the official `security-guidance` plugin ships exactly this.
- **Stop-hook verification gates** for unattended runs.
- **ConfigChange audit** for settings edits.

### 3.5 MCP servers & plugins — few, high-value, security-aware

**Proxy caveat first:** with `ANTHROPIC_BASE_URL` on aibox, **tool search is disabled** → every MCP tool schema loads upfront into backend-model context. Keep total servers ≤4–5, tools in the low dozens, prefer servers with few well-named tools.

**Shortlist (ranked, install-method checked against your dnf/uv rules):**
1. **Context7** — remote HTTP (`https://mcp.context7.com/mcp`), zero local software, 2 tools, kills hallucinated library APIs.
2. **Playwright MCP** via **podman** (`mcr.microsoft.com/playwright/mcp`, headless-Chromium image) — browser automation on a headless box; a11y-tree driven.
3. **GitHub MCP** (remote, PAT) — only if `gh` CLI via Bash proves insufficient (it covers ~80%).
4. **Fetch** (`uvx mcp-server-fetch`) — marginal over built-in WebFetch; mainly for proxy-friendly fetching.
5. ~~mcp-memory-service~~ — **dropped**: duplicates `~/kb` (contradiction resolved in KB-research's favor).
- Skip: npm-only servers (filesystem/memory/sequential-thinking — dnf-rule conflict), bundles/aggregators (security anti-pattern + context tax), Puppeteer (archived).

**Plugins (marketplace already registered as `claude-plugins-official` — use that suffix; a research draft wrongly used `@claude-code-plugins` and its commands would fail):** `security-guidance` (PreToolUse vulnerability flags), `hookify` (author hooks conversationally), `code-review` (5 parallel reviewers), `commit-commands`, `claude-md-management`, `skill-creator` (eval loop for your skills), `superpowers` (confirmed present in the official manifest despite one report claiming otherwise). Community: `anthropics/skills` (docx/pdf/pptx/xlsx), `obra/superpowers-marketplace`, `wshobson/agents` à la carte.

**Security notes (spec + Invariant Labs tool-poisoning research):** MCP servers run with your privileges; tool *descriptions* are untrusted input (poisoning/shadowing/rug-pulls are demonstrated attacks). Mitigations: first-party sources only, review tool descriptions on first connect via `/mcp`, pin versions, least-privilege/read-only tokens, per-server `mcp__server__tool` permission rules.

### 3.6 Model routing & the aibox proxy

**Verdict: keep aibox + profile switching; don't add a router yet.** Your tier mapping matches the mid-2026 cost/perf leaders (Kimi K2.7 Code, GLM-5.2, DeepSeek V4 Flash are each tier leaders; Qwen3.8-Max is 2 days old — vendor-only numbers). Get per-task routing from native knobs: `model: "opusplan"` (Opus tier for planning, Sonnet for execution), `ANTHROPIC_DEFAULT_*_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL`, `fallbackModel` (max 3), and a hardening env block:

```json
"CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS": "1",
"API_TIMEOUT_MS": "600000",
"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
"CLAUDE_CODE_AUTO_COMPACT_WINDOW": "<match tier, e.g. 256000 for kimi>"
```

Known proxy failure modes (all documented, all with mitigations): beta-header 400s (`output_config` carries effort level — relevant at xhigh; adaptive-thinking 400s on unrecognized model names), cache semantics depending entirely on the gateway, tool-use SSE fidelity, context-window mismatch. Kimi-specific: verify thinking mode is actually on — without it requests silently route to K2.6.

**Decide FABLE = `qwen3.8-max`** (released ~Aug 3) and re-baseline the profile file — the `-preview` mapping is stale.

**Observability:** `ccusage` reads `~/.claude/projects/**/*.jsonl` locally — works for both profiles, per-model cost breakdown, cache read/write tracking. Needs Node (`dnf install nodejs` + ephemeral `npx` is a defensible middle path) or a ~50-line uv stdlib parser to stay purist.

**Revisit Claude-Code-Router** (36k★: per-task modes, fallback chains, cost logs) only if you later want cross-provider fallback or subagent-pinned models. ⚠️ **LiteLLM caveat: PyPI versions 1.82.7/1.82.8 (2026-03-24) were credential-stealing supply-chain attacks** — never install unpinned; rotate anything exposed if they ever ran here.

**Data residency:** GLM/Kimi/DeepSeek/Qwen endpoints serve from China — fine for personal projects; make a conscious one-line policy for e2e-data-ai-project if it's work-sensitive (use the official profile for that data).

### 3.7 Things every report missed (critic pass, all disk-verified)

1. **Backup/DR is the single biggest real gap.** `~/.hermes` 861 MB, `~/.openclaw` 77 MB, `~/.claude` 51 MB (incl. OAuth credentials), personal-resume has **no git at all** — everything dies with the disk except `~/kb`. Fix: `dnf install restic`, one off-box repo, one cron, one tested restore.
2. **Secret deny-rules.** `permissions.deny` is the only harness-enforced guardrail; today any agent or prompt-injected WebFetch result can `Read` your settings (with the token), `.credentials.json`, opencode config, `.env`s. One user-scope deny block fixes it; it holds even in `bypassPermissions`.
3. **Official fallback profile never smoke-tested** — and `hasAvailableSubscription: false`, so it bills pay-as-you-go. 2-minute test converts an assumption into a fallback. Also `use-profile.sh` does a bare `cp` with no snapshot — one mistyped switch destroys hand-edits (already happened once).
4. **Update policy accidental** (`autoUpdates: false` yet updates ran) — half the features above are version-gated; silent drift matters.
5. **No measurement anywhere** — without cost/search-miss counters, in 3 months nobody will know which change helped.

---

## 4. Proposed roadmap

### Tier 0 — hygiene & safety (this week, ~3 h total)
| # | Action | Why | Effort |
|---|---|---|---|
| 0.1 | **Rotate the aibox token**; add `permissions.deny` for `~/.claude/settings*.json`, `.credentials.json`, `~/.config/opencode/**`, `~/containers/hermes/.env`, project `.env`s (user scope; verify anchoring syntax `Read(//home/giang/…)` vs `~/…`) | Token sat plaintext in 3+ files; deny rules are the only enforced guardrail | 20 min |
| 0.2 | **Fix the profile system:** decide fable=`qwen3.8-max`, re-baseline `settings.aibox.json` from live file, make `use-profile.sh` snapshot before `cp` and refuse ambiguous switches, add the §3.6 hardening env block | Drift already happened once; substrate for everything else | 15 min |
| 0.3 | **Smoke-test the official profile** (switch, one trivial prompt, switch back) | Converts assumed fallback into real | 2 min |
| 0.4 | **Run `kb lint` now**; fix kb-0016's 355-char summary | 9/22 notes never linted | 10 min |
| 0.5 | **Fix e2e CLAUDE.md** dead `~/ServerStuff/…` ref → `~/kb/refs/e2e-data-ai-project.md`; add "ignore `.venv`" guard | Dead path + 11,941-file agent trap | 5 min |
| 0.6 | **Backup:** `dnf install restic` → off-box repo → cron covering `~/.hermes`, `~/.openclaw`, `~/.claude/{settings*,.credentials.json,skills}`, `~/.config/opencode`, `~/kb`, `~/projects/personal-resume`; test one restore | Only `~/kb` survives a disk death today | 1 h |
| 0.7 | **Decide update policy** (autoUpdatesChannel stable + monthly re-verify, or calendar manual) + one AGENTS.md sentence on proxy data sensitivity | Version-gated features; conscious privacy choice | 10 min |

### Tier 1 — wiring the harness (next 2 weeks, ~1–2 evenings)
1. **Notification → Hermes Telegram hook** (matchers `permission_prompt`, `agent_needs_input`, `agent_completed`). The enabling infrastructure for every fire-and-forget feature below. (30–60 min)
2. **Subagent fleet in `~/.claude/agents/`** (zero dependencies, hot-reloaded):
   - `Explore` override with `model: haiku` — exploration currently inherits the fable tier; single cheapest cost cut available.
   - `researcher` — `tools: Read, Grep, Glob, WebFetch` + `kb search` via Bash. ⚠️ **No WebSearch**: it is observably broken through the aibox proxy (echoed markup instead of results) — server-side tools are exactly what proxy translation layers break.
   - `code-reviewer` — read-only, "use proactively after code changes".
   - `test-runner` — returns only failures.
   - `kb-researcher` — scoped to `~/kb`, mirrors the AGENTS.md 2-read protocol; ports the pattern OpenCode's `aibox-worker` already runs.
   (30 min)
3. **Permission allowlists:** run the `fewer-permission-prompts` skill per project — it reads existing transcripts; 132 queued prompts and zero allowlists today. (15 min)
4. **Plugins:** `/plugin install security-guidance@claude-plugins-official` (hooks on day one) + `hookify@claude-plugins-official` (author the rest conversationally). Then stop — more plugins = context tax with tool-search disabled. (10 min)
5. **Statusline** (`/statusline <description>`): model, context %, cost, git branch — zero-token at-a-glance state for a headless box; add cache-hit segment later. (15 min)
6. **kb CLI bug batch:** ranked trigram fallback (`ORDER BY bm25(kb_tri)`); `kb verify` writes `verified:` into frontmatter + flips status; atomic `kb supersede OLD --with NEW` (0/22 usage is friction, not discipline); fix commit-before-editor; lint checks for body-vs-frontmatter link parity (8/22 drifted) + empty `source:`; timestamped lint log + non-zero-exit alert piped to the Telegram hook. (1–2 h)

### Tier 2 — knowledge-loop automation (month 2)
1. **SessionEnd distillation hook → `~/kb/raw/session-distill-<date>.md`.** Headless `claude -p` over `transcript_path` with a *self-contained* prompt (embed the SCHEMA rules — `--bare` becomes the `-p` default and skips auto-discovery). Proposals only; the weekly lint/ingest pass and `kb new` human gate stay untouched. This is hooks→distill→markdown, now validated by two independent community projects. (1–2 h)
2. **Thin KB-MCP server** (~80 lines FastMCP over the existing `kb` CLI: `kb_search`, `kb_read`, `kb_propose`) registered in Claude Code + OpenCode; copy Basic Memory's tool-hint and progressive-disclosure conventions. Makes the 2-read protocol mechanical for *every* MCP-capable tool instead of prose-dependent. (half day)
3. **Instrumentation:** weekly cost report per project/tier (ccusage or uv parser) + `kb search` zero-hit counter. The miss counter mechanizes your embeddings-revisit rule and gives ground truth for every other change. (1 h)
4. **Churn + staleness tooling:** `kb churn` (touches/note from log.md — kb-0017 pattern), `kb stale --days 90`, as the second cron job next to lint. (1–2 h)
5. **Project onboarding:** `CLAUDE.md` for personal-website (point at the existing 36 KB ARCHITECTURE.md), hermes (anchor the compose/restart recipe from memory/kb-0017), and personal-resume; `git init` + `.gitignore` for the resume. (30 min each)
6. **`~/.claude/loop.md`** standing maintenance routine (check hermes logs, tend PRs, kb health-check) for `/loop`; consider `claude --bg` + supervisor daemon as the standard way to launch long jobs (survives SSH disconnects; auto-worktree isolation). (1 h)

### Tier 3 — go wild (exploratory, each with a kill criterion)
| Bet | What | Kill criterion / gate |
|---|---|---|
| **Agent teams trial** (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) on a *research* task (parallel hypotheses, security/perf/test review) — tmux split panes | 7× tokens, no resume, experimental | One task; judge vs. a dynamic workflow (which produced this report and handles research fan-outs with less overhead) |
| **Graphify on e2e-data-ai-project** (`uv tool install graphifyy`, skill + hooks, gitignore `graphify-out/`) | Young (Apr 2026); context tax on aibox | Before/after `/cost` on one representative task; if no measurable win at 104 files, defer to the >50k-LOC trigger |
| **Embedding recall net** (`kb embed`: sqlite-vec + sqlite-rembed + Ollama `nomic-embed-text`, RRF-fused after BM25-miss) | Pre-1.0 deps; corpus is BM25-friendly | Only if Tier-2 miss counter shows real synonym failures, or corpus passes ~200 notes |
| **Hermes memory tool:** back the API memory tool (`memory_20250818`) with a mounted directory so gateway-routed agents get persistent memory (~50-line handler per SDK examples) | Hermes-specific | Try on one workflow |
| **skill-creator eval loop** on the `kb` skill (test cases, isolated runs, grading, description tuning) | Time | Worth it once 2+ skills exist |
| **Superpowers methodology trial** (confirmed in official manifest: `superpowers@claude-plugins-official`) — opinionated brainstorm→spec→plan→subagent-TDD workflow | Changes workflow shape | One week trial after the fleet settles |
| **Gas Town** as a standing agent factory (Docker compose, daemons, merge queue) | Heavyest ops footprint on a 4-core laptop | Only if you decide you want a permanent multi-agent workforce |

---

## 5. What NOT to do (resolved contradictions — don't let these recirculate)

- **No code-graph tooling at current repo sizes** (104 files vs. the 50k–150k LOC payoff threshold). Free equivalents first (3.3).
- **No extra router (CCR/LiteLLM/new-api) yet** — native knobs cover per-task routing for one proxy + one fallback; revisit with real fallback-chain needs. Never install LiteLLM unpinned (supply-chain incident).
- **No parallel memory store** (mcp-memory-service, mem0, Zep/Neo4j, Letta, supermemory as stores) — duplicates `~/kb`, adds daemons and per-ingest LLM cost. Steal ideas (bi-temporal invalidation ≈ your supersede; progressive disclosure), skip stacks.
- **No WebSearch in subagent tool lists** until aibox proves server-side-tool fidelity — use WebFetch + `kb search`.
- **No bulk plugin/marketplace installs** — with tool search disabled on aibox, every schema is paid for in backend context. Cherry-pick.
- **No agent teams for coding** while experimental — start research-only if at all.
- **Plugin suffix note:** the registered marketplace is `claude-plugins-official`; install commands found in the wild with `@claude-code-plugins` will fail here.

## 6. Measuring success (so this isn't unfalsifiable)

1. **Weekly cost per project/tier** (ccusage or uv parser) — baseline: $87.16 metered on the root project to date (≈50/50 split fable/haiku tiers).
2. **`kb search` miss rate** — baseline 0 observed failures at 22 notes; this number gates embeddings.
3. **Permission-prompt count before/after** allowlists.
4. **Before/after `/cost` + outcome** on one representative e2e task for any graph tooling.
5. **Lint cadence:** "notes linted in last 7 days" should be 22/22 after Tier-1.6, forever.

---

## Appendix A — source highlights

Official docs (all now under `code.claude.com/docs/en/`): settings · permissions · hooks reference + guide · memory · sub-agents · agent-teams · agent-view · workflows · worktrees · costs · mcp · plugins · statusline · headless · best-practices · llm-gateway + llm-gateway-protocol · prompt-caching · changelog.
Anthropic engineering: multi-agent research system (token economics) · effective context engineering · effective harnesses for long-running agents · auto mode.
Community/tooling: Ruflo (claude-flow) · oh-my-claudecode · Claude-Code-Router · Gas Town · claude-squad · wshobson/agents · awesome-claude-code(+skills) · Serena · Graphify · GitNexus · codebase-memory-mcp · aider repo-map · Greptile · Sourcegraph context docs · mem0 · Zep/Graphiti · Letta · supermemory · Memobase · cognee · Basic Memory · claude-mem · claude-memory-compiler · sqlite-vec/rembed · Context7 · Playwright MCP · GitHub MCP · mcp-server-fetch · MCP security spec + Invariant Labs tool-poisoning · ccusage · LiteLLM security incident (BerriAI #24512/#24518, The Hacker News, 2026-03) · third-party-models-in-cc field guide · cc-compatible-models matrix · model benchmarks via airank/llm-stats/benchmarklist (vendor-reported figures flagged as such).

## Appendix B — raw fleet reports

Full agent transcripts: `~/.claude/projects/-home-giang/b1ada415-0a1f-4fce-9111-ea11914611a7/subagents/workflows/wf_b40e9f56-fae/` (journal.jsonl + per-agent JSONL). Extracted reports cached in `/tmp/harness-audit/`. Consider ingesting this report into `~/kb` as a `type: reference` note with a section map if you want it queryable.

*Report compiled 2026-08-05 by a 9-agent audit/research fleet; all version-specific claims re-checked against live docs the same day.*
