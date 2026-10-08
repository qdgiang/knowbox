---
id: kb-0028
title: "dotfiles + chezmoi centralizes all agentic-tool configs"
type: decision
domain: tooling
tags: [chezmoi, dotfiles, claude-code, hermes]
aliases: [dotfiles centralization, dotfiles repo, server-ops former name]
triggers:
  - "editing any agent tool config (claude, opencode, hermes, zellij)"
  - "wondering where a tool's config is managed or backed up"
status: active
confidence: high
created: 2026-08-08
updated: 2026-08-19
supersedes: []
related: [kb-0002, kb-0007, kb-0013, kb-0029, kb-0036]
source: "session 2026-08-08: config-scattering problem; market survey (yadm dormant, rcm frozen, stow loses on symlink/atomic-write) then implementation"
summary: "All agentic-tool configs under chezmoi in ~/dotfiles (git repo = source state); secrets in ~/.config/dotfiles/secrets.env; hourly :17 cron auto-commits drift + pushes to private mirror qdgiang/dotfiles."
---
Configs for all agentic tools live in `~/dotfiles`, which is simultaneously a git repo and chezmoi's source state (`~/.config/chezmoi/chezmoi.toml` → `sourceDir = "/home/giang/dotfiles"`). `chezmoi apply` renders **real files** (no symlinks): agent/editor atomic writes (temp+rename) replace symlinks with regular files, and Claude Code ≥2.1.69 ignores user skills under a symlinked `~/.claude/skills`.

## Key mechanics

- **Drift capture**: hourly cron runs `scripts/capture-chezmoi.sh` = `chezmoi re-add` + auto-commit + push; the repo's pre-commit secret hook guards every auto-commit. Edit configs in place; versioned within the hour. **Gotcha**: it's `git add -A` on the whole repo, so half-finished edits to repo infra (`scripts/`, `README.md`, `.githooks/`) get swept in as `auto-capture: drift` — commit promptly. Empty `~/.local/share/dotfiles/capture.log` is normal idle, not a broken cron (verify via `journalctl | grep capture-chezmoi`). **The crontab is itself chezmoi-managed**: the `17 * * * *` capture line's source of truth is in `~/dotfiles`, so commenting it out via `crontab -e` is undone the next time chezmoi re-installs the dotfiles. To hold a push before it propagates, disable the line in the chezmoi source, not just in the live crontab.
- **GitHub mirror** (2026-08-09): private repo `qdgiang/dotfiles`, https remote authed via `gh auth git-credential` helper ([[kb-0036]]). Push failure is non-fatal (logged, retried next run, local commits kept). History was scanned for secret values before first push.
- **Secrets**: never in git, deliberately OUTSIDE sourceDir at `~/.config/dotfiles/secrets.env` (0600) — in-tree would mean hourly `git add -A` auto-staging + `chezmoi apply` rendering into $HOME. Template: `secrets.env.example`. `~/.claude/use-profile.sh` injects `${AIBOX_TOKEN_CLAUDE}`/`@HOME@` placeholders at compose time. **Claude and hermes use different aibox keys**; hermes dashboard creds use `${VAR:?}` substitution from `~/containers/hermes/.env` (fail-hard).
- **Layout**: managed dotfiles = `dot_*`/`private_*`/`executable_*` entries; repo infra (`scripts/`, `system/`, `seeds/`, `docs/`, README) excluded via `.chezmoiignore`. Full manifest: `~/dotfiles/README.md`.
- **Permissions**: chezmoi stores only coarse modes; sensitive files normalized to 600/700 (`~/.config/gh`, gh config.yml, `~/.hermes/config.yaml`).
- **Repo identity**: `dotfiles/dotfiles@localhost` repo-local (global identity deliberately unset, per [[kb-0007]]); `core.hooksPath = .githooks`.

## Decisions worth remembering

- chezmoi over stow/yadm/rcm/dotbot (2026-08 survey): real-file rendering + apply-prompt drift safety + `re-add` capture; yadm/rcm dormant.
- No age/templates (single machine for now; secrets externalized instead).
- 2026-08-08 rename for universal recognition: `~/server-ops` → `~/dotfiles`; plain `mv` preserved history; chezmoi.toml + crontab + all refs repointed.
- Unmanaged by design: runtime state, `~/.hermes/skills`, tirith binary (dnf-only rule [[kb-0002]]), `gh hosts.yml`, `.env` files, `node_modules`, claude `plugins/` state (reinstall via `scripts/install-claude-plugins.sh`).
- Root-owned system files (`system/`) install via staged-script pattern per [[kb-0013]].
- Secret rotation pending: original aibox token + hermes dashboard password were plaintext in configs (and transcripts) for months. Note: hermes `ANTHROPIC_API_KEY` still holds the old shared claude token — replace with a hermes-specific key at rotation.

Start Claude Code sessions from `~/dotfiles`. The retired `server-ops` name survives only in git history/transcripts — live refs must say `~/dotfiles`. Fresh-machine bootstrap: `~/dotfiles/README.md` § Fresh-machine bootstrap.

## Related
- [[kb-0002]] — chezmoi installed via dnf
- [[kb-0007]] — repo-local git identity pattern reused here
- [[kb-0013]] — staged-script pattern for the root-owned system glue
- [[kb-0036]] — the gh credential helper the mirror push uses
