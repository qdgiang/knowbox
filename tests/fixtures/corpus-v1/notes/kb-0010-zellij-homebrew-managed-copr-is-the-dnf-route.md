---
id: kb-0010
title: "zellij is Homebrew-managed; COPR varlad/zellij is the dnf-compliant route (not yet used)"
type: fact
domain: tooling
tags: [zellij, homebrew, dnf]
aliases: [zellij install origin, brew zellij, why rpm does not know zellij]
triggers:
  - "updating, reinstalling, or debugging zellij"
  - "wondering why dnf/rpm does not know about zellij"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-03
supersedes: []
related: [kb-0002]
source: "janitor 2026-08-03: command -v zellij; rpm -q zellij; brew list"
summary: "zellij on PATH is Homebrew-managed (/home/linuxbrew/.linuxbrew, tree predates the dnf-only rule). rpm knows nothing of it. DNF-compliant migration if wanted: COPR varlad/zellij."
---
The zellij on PATH (`/home/linuxbrew/.linuxbrew/bin/zellij`) is **Homebrew-managed**, not dnf/rpm — `rpm -q zellij` = not installed. The brew tree was created 2026-08-02 10:19, *before* the dnf-only rule ([[kb-0002]]) was established later that same day; the earlier note version claiming a COPR install was wrong (corrected by janitor pass 2026-08-03, see git history).

To manage it today: `brew upgrade zellij` / `brew uninstall zellij` — dnf cannot see it.

**Migration to a dnf-compliant install, if ever wanted:**

```sh
sudo dnf copr enable varlad/zellij && sudo dnf install zellij
brew uninstall zellij            # remove the brew copy so PATH resolves the rpm one
```

The same brew tree also owns **herdr** — which has no dnf/COPR route at all, so it stays brew-managed by necessity (see exceptions in [[kb-0002]]).

## Related
- [[kb-0002]] — the install rule + current exceptions list
