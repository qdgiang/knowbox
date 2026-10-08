---
id: kb-0077
title: "zellij 0.45.0 freezes herdr-in-zellij: floating-pane layout regression; downgraded to 0.44.3 via local brew tap (bottle purged)"
type: gotcha
domain: tools
tags: [zellij, herdr, brew, downgrade]
aliases: [herdr sluggish, zellij downgrade]
status: active
confidence: high
created: 2026-08-26
updated: 2026-08-26
supersedes: []
related: []
source: ""
summary: "If herdr-in-zellij freezes/lags, suspect zellij >= 0.45.0: floating-pane layouts fail to apply and spam resize/host-query errors. Fix = zellij 0.44.3 via giang/local tap binary formula (old bottle purged). Both zellij formulae pinned; unpin-test-relink procedure included for future versions."
---
herdr+zellij became unusable (frozen panes, laggy TUI) because **zellij 0.45.0
cannot apply layouts containing a floating pane** — it fails at session start
with `Failed to apply layout: failed to find position for floating pane`, the
pane's terminal is never created, and the server then loops forever on
`failed to set terminal id 0 to size (…): failed to find terminal fd for id 0`
plus plugin host-query bursts (~100 queries/10ms observed) and cached-resize
errors on every attach. Over a ~130ms Tailscale DERP relay this renders the
session frozen.

## Timeline (2026-08-26 incident)
- 08-25 16:51: `brew upgrade` pulled herdr 0.8.2 **and** zellij 0.45.0
  (0.45.0 released 08-20; previous was 0.44.3 from May 13 — proven-good for months).
- 08-26 08:38 reboot = first activation of the combo (a running zellij/herdr
  server keeps the old binary until restarted).
- Boot-time zellij session applied a "1 tiled + 1 floating panes" layout →
  failure loop from the first second.
- Misdirections ruled out: Tailscale (plain SSH fine), herdr alone (herdr
  without zellij fine — herdr's panels are internal; the herdr binary contains
  **zero** zellij references; zellij is only the outer multiplexer).
- Also on 0.45.0: `zellij -s <name> --layout <file>` dies with
  `There is no active session!` even with an empty socket dir.

## Fix applied
Downgraded to **zellij 0.44.3** (static musl binary from the official GitHub
release), installed brew-managed via the local tap:

- Tap: `giang/local`; formula:
  `$(brew --repository)/Library/Taps/giang/homebrew-local/Formula/zellij@0.44.3.rb`
  — **note:** on this box `brew --repository` is
  `/home/linuxbrew/.linuxbrew/Homebrew` (non-standard; NOT
  `/home/linuxbrew/.linuxbrew`), so the tap lives under `.../Homebrew/Library/Taps/`.
- The formula installs the release tarball
  (`zellij-x86_64-unknown-linux-musl.tar.gz`, sha256-pinned) — no compile.
- Both formulae pinned (`zellij` and `zellij@0.44.3`) so `brew upgrade`
  changes nothing. Core `zellij` 0.45.0 remains installed but **unlinked** in
  the Cellar as rollback (`brew unlink zellij@0.44.3 && brew link zellij` to go back).

## Why it wasn't a simple brew downgrade
- `brew extract` needs the homebrew/core git tap (API mode here — heavy clone).
- The 0.44.3 bottle is **purged**: 404 on ghcr and on USTC / Tsinghua / Aliyun
  bottle mirrors (they only retain the current version). Lesson: old brew
  bottles are not retrievable months later; pin immediately if a version matters.
- `--build-from-source` failed twice: no system compiler on this box (no gcc,
  no passwordless sudo), and brew's own `gcc` bottle does not satisfy brew's
  DevelopmentTools check (only versioned `gcc-16` etc. are linked; unversioned
  `cc`/`gcc` symlinks in the brew bin dir make it pass — removed afterwards).
  A failed cargo build leaves `/var/tmp/zellijA0.44.3-*` (~GB) to rm manually.

## Verify / re-test
- `zellij --version` → 0.44.3.
- Pass signal after starting a session: no `Failed to apply layout` and no
  `failed to find terminal fd` lines in `/tmp/zellij-1000/zellij-log/zellij.log`.
- **Outcome 2026-08-26: downgrade verified — herdr+zellij runs smoothly again.**

## Trying a NEWER zellij one day (both formulae are pinned)
`brew list --pinned` → `zellij` and `zellij@0.44.3`: plain `brew upgrade`
touches neither, so the working 0.44.3 survives any upgrade run. When upstream
eventually fixes the floating-pane layout bug and a new version is worth testing:

1. `brew unpin zellij && brew upgrade zellij`
2. Test with a layout containing a **floating pane** (the original trigger)
   and with bare session creation (`zellij -s test` — 0.45.0 broke that too,
   "There is no active session!"); watch
   `/tmp/zellij-1000/zellij-log/zellij.log` for `Failed to apply layout` /
   `failed to find terminal fd`.
3. If good: `brew unlink zellij@0.44.3 && brew link zellij`; then optionally
   `brew uninstall zellij@0.44.3` and delete the `giang/local` tap formula.
4. If bad: `brew pin zellij` again and keep using 0.44.3.

## Side gotcha from the same incident (separate topic)
A herdr server started from inside a Claude Code session inherits
`CLAUDE_CODE_CHILD_SESSION` etc., which every claude agent it spawns then
inherits → "Transcript saving is off" warning in agent panes. Fix: restart the
server from a clean shell; check with
`tr '\0' '\n' < /proc/<server-pid>/environ | grep -c CLAUDE` (expect 0).
(A herdr.service systemd unit created during the incident was removed again
afterward — the zellij downgrade fixed the real problem; a clean-shell restart
is all this gotcha needs.)
