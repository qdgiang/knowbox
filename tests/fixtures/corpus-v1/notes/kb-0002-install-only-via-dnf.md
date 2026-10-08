---
id: kb-0002
title: "Install software only via dnf (COPR if needed), never static binaries"
type: decision
domain: preferences
tags: [dnf, fedora, installation]
aliases: [package manager only, no manual binaries]
triggers:
  - "installing any new software or CLI tool on this machine"
  - "tempted to curl a static binary into ~/.local/bin"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0003, kb-0010]
source: "memory:package-manager-only.md (explicit user instruction 2026-08-02)"
summary: "Standing user rule: all installs go through dnf — official repos first, COPR for missing packages. Never download static/prebuilt binaries."
---
All software installation on this machine must go through the system package manager (`dnf`). Do not download static/prebuilt binaries into `~/.local/bin` or anywhere else, even to avoid a sudo prompt. This was an explicit user instruction during zellij setup (2026-08-02): a static-binary install was rejected in favor of the COPR route.

**How to apply:**
1. Official Fedora repos first: `sudo dnf install <pkg>`.
2. Package missing → find the documented COPR repo (e.g. zellij: `sudo dnf copr enable varlad/zellij`).
3. If sudo is needed and unavailable → **ask the user to run the command**, never work around it.

Note: personal scripts written *by us* (like `~/kb/bin/kb`) are not "installed software" — this rule governs third-party tools.

## Current exceptions (recorded by janitor 2026-08-03)

- **herdr, zellij** — Homebrew-managed under `/home/linuxbrew/.linuxbrew` (tree created 2026-08-02 10:19, *before* this rule was set later that day). No Fedora/COPR package exists for herdr, so brew is the only viable route; manage with `brew upgrade`, never `dnf`. zellij *could* be migrated to COPR — see [[kb-0010]].
- **claude** (`~/.local/bin/claude`) — official Claude Code installer; no dnf package exists. Sanctioned.
- **uv** (`~/.local/bin/uv`, v0.12.1) — standalone install, not from rpm. **User decision 2026-08-03: keep as-is** (Fedora's uv may lag 0.12.1); accepted exception, not switchable-on-next-opportunity anymore.

## Related
- [[kb-0003]] — the machine this applies to
- [[kb-0010]] — zellij's actual (brew) provenance + migration route
