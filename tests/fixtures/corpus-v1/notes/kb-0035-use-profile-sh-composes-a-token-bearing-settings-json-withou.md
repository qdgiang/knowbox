---
id: kb-0035
title: "use-profile.sh composes a token-bearing settings.json — without umask 077 it lands world-readable (644)"
type: gotcha
domain: tooling
tags: [claude-code, security, secrets, permissions, use-profile]
aliases: [umask, world-readable token, settings.json permissions]
triggers:
  - "a token lands in a world-readable file"
  - "reviewing use-profile.sh or settings.json generation"
status: active
confidence: high
created: 2026-08-10
updated: 2026-08-11
supersedes: []
related: [kb-0018, kb-0028]
source: "session 2026-08-10: use-profile.sh security review"
summary: "Composed ~/.claude/settings.json carries the aibox ANTHROPIC_AUTH_TOKEN but is created 644 world-readable by default — use-profile.sh must umask 077 (and chmod 600 existing files/backups) or the token leaks."
---
`~/.claude/settings.json` is not just preferences: under the **aibox** profile it
holds `ANTHROPIC_AUTH_TOKEN` in plaintext, and with a normal umask it is created
**644 (world-readable)** — so the proxy token is readable by any local user unless
`use-profile.sh` sets `umask 077` and the already-existing files are `chmod 600`.

## Why it happens
- `use-profile.sh` composes `settings.json` = `settings.base.json` × `settings.<profile>.json`,
  injecting `AIBOX_TOKEN_CLAUDE` at compose time. Under aibox the token is in the output file.
- No `umask` → `composed … > settings.json.tmp` creates it at the default **644**; `mv` keeps
  that mode. Snapshots inherit it too (`cp -p` preserves the source mode).
- Claude Code itself does not fix this: on fedora-server the live `settings.json` was 644.
  (Contrast: Claude protects its own OAuth `.credentials.json` at 600, but not `settings.json`.)

## The fix
- Add near the top of `use-profile.sh` (after `set -euo pipefail`): `umask 077`
  → new `settings.json` 600, `backups/` dir 700.
- Tighten what already exists:
  `chmod 600 ~/.claude/settings.json`
  `chmod 700 ~/.claude/backups && find ~/.claude/backups -name 'settings.json.*.bak' -exec chmod 600 {} +`
- Note: `settings.local.json` (permissions allowlist only, no secret) can stay 644.

## Companion fixes (same review, same file)
Two non-security bugs fixed alongside, so a fresh machine behaves sanely:
- **silent exit:** on first run (no `settings.json`) the retention `ls` glob matches nothing,
  `ls` exits 2, and `set -euo pipefail` aborts before the confirmation echo. Wrap it:
  `{ ls -1t … || true; } | …`.
- **spurious stderr:** the hand-edit guard and `status` compose *both* profiles, so a
  token-less aibox compose leaks a scary "needs ${AIBOX_TOKEN_CLAUDE}" error. Silence only
  those comparison calls: `composed "$p" 2>/dev/null`; keep the real write `composed "$1"` loud.
- Verified NOT a bug: on the write path `composed "$1" > tmp`, `composed` runs in the main
  shell (redirection doesn't fork a function), so its internal `exit 1` correctly aborts the
  whole script. The `&&`-exemption from `set -e` only bites inside the `<(...)` subshells.

## Related
- [[kb-0018]] — the aibox tier routing that puts this token in settings.json
- [[kb-0028]] — dotfiles/chezmoi manages use-profile.sh + the sanitized profile files
