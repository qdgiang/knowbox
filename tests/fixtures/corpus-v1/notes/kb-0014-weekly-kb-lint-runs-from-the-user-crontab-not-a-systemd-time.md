---
id: kb-0014
title: "Weekly KB janitor runs from the user crontab, not a systemd timer"
type: decision
domain: kb
tags: [cron,lint,janitor,scheduling]
aliases: [when does kb lint run, who runs the janitor, weekly embed cron]
triggers:
  - "changing the lint schedule"
  - "debugging why the weekly lint did or did not run"
  - "adding more scheduled KB jobs"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0001, kb-0012, kb-0028, kb-0038]
source: "user decision 2026-08-03; crond is-active/enabled verified same day; embed line added 2026-08-11 (kb-0038)"
summary: "Mondays via user crontab: 10:13 kb lint+stale+churn (→~/kb/.lint-cron.log), 10:37 kb embed. crond is a system service — headless, no linger. Source of truth: dotfiles crontab.txt."
---
The weekly KB janitor runs from the **user crontab**, Mondays, in two lines:

```cron
13 10 * * 1  kb lint + kb stale --days 90 + kb churn --days 90 → ~/kb/.lint-cron.log
37 10 * * 1  kb embed → ~/kb/.embed-cron.log
```

(`kb embed` — incremental, keeps the semantic-rescue vectors fresh, added 2026-08-11 per [[kb-0038]].)

## Why crontab instead of a systemd user timer

A systemd *user* timer only fires while the user manager is alive — on a headless box that means while an SSH session (or a detached zellij session, incidentally) keeps it up; otherwise it needs `sudo loginctl enable-linger giang`. **crond is a system service** — it runs regardless of logins, so jobs fire headless with zero extra setup. User decision 2026-08-03 after exactly this trade-off discussion. The original `kb-lint.timer`/`kb-lint.service` user units were removed the same day (no double runs).

## Details that matter when touching this

- **Source of truth is `~/dotfiles/scripts/crontab.txt`**, installed via `scripts/install-crontab.sh` — edit the file, then re-run the installer; never `crontab -e` directly ([[kb-0028]]). `\%` escapes are required for `%` in crontab (the lint line uses `date +\%FT\%TZ`).
- Cron runs with a minimal PATH/env — hence absolute paths (`uv`, `kb`). Keep them absolute if editing.
- Logs live inside `~/kb/` but are git-ignored (derived output, not knowledge).
- Structural-only by design (dead links, enums, sizes, index drift). Semantic verification — re-running notes' verify commands, external spot-checks — stays human-triggered; see `kb verify` and the 2026-08-03 janitor pass (git history).
- If another scheduled job appears: still crontab, same absolute-path pattern.

## Related
- [[kb-0001]] — KB design (lint as machine-enforced hygiene)
- [[kb-0012]] — why the cron lines invoke uv, not system python
- [[kb-0028]] — the dotfiles repo that owns crontab.txt
- [[kb-0038]] — the 2026-08-11 review that added the embed line
