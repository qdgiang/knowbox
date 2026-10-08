---
id: kb-0036
title: "GitHub push auth is per-URL gh credential helpers — empty global credential.helper is by design"
type: gotcha
domain: fedora
tags: [git,gh,credentials,cron]
aliases: [gh auth git-credential, credential.helper empty, headless git push auth]
triggers:
  - "git push to github fails with auth errors"
  - "diagnosing credential.helper on this box"
  - "unattended pushes under cron"
status: verified
confidence: high
created: 2026-08-11
updated: 2026-08-11
supersedes: []
related: [kb-0028]
source: "raw/session-distill-2026-08-09.md, re-proposed 2026-08-11 (3x total)"
summary: "Pushes to github.com authenticate via credential.https://github.com.helper=!/usr/bin/gh auth git-credential in ~/.gitconfig; global credential.helper empty by design. gh auth logout silently breaks cron pushes."
---
GitHub push auth on fedora-server runs through per-URL git credential helpers, not a global `credential.helper` — so `git config --global --get credential.helper` returning empty is BY DESIGN, not a misconfiguration.

`gh auth login` / `gh auth setup-git` wrote per-URL pairs into `~/.gitconfig`:

```
credential.https://github.com.helper=
credential.https://github.com.helper=!/usr/bin/gh auth git-credential
```

(same pair for gist.github.com; the empty-value line clears inherited helpers). `gh auth git-credential` reads `~/.config/gh/hosts.yml` and emits a token with no tty, so unattended pushes work headlessly — this is what the hourly dotfiles capture cron relies on ([[kb-0028]]).

**Diagnostic rule:** when a push fails with auth errors, run `git config --list --show-origin | grep -i credential` — the per-URL lines are the real auth path. Do not "fix" the empty global helper.

**Failure mode:** `gh auth logout` silently breaks unattended pushes — they fail non-fatally into their logs (e.g. `push FAILED` in `~/.local/share/dotfiles/capture.log`) while local capture keeps working. Re-auth with `gh auth login` to restore. Verified live 2026-08-09 during the qdgiang/dotfiles mirror setup.

## Related
- [[kb-0028]] — the dotfiles mirror whose hourly push depends on this
