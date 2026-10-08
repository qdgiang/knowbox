---
id: kb-0005
title: "grubby syncs /etc/kernel/cmdline on Fedora 44 — don't double-edit; quote /boot globs under sudo"
type: gotcha
domain: fedora
tags: [grubby, kernel-cmdline, sudo, zsh]
aliases: [boot args duplication, no matches found /boot]
triggers:
  - "editing kernel cmdline or boot args on fedora-server"
  - "sudo grep/glob under /boot fails with 'no matches found'"
  - "an arg appears twice in /etc/kernel/cmdline"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-03
supersedes: []
related: [kb-0004]
source: "memory:fedora-kernel-cmdline-grubby.md + raw/headless-laptop-server.md"
summary: "grubby --update-kernel=ALL updates BLS entries AND /etc/kernel/cmdline — old 'edit both' guidance duplicates args. Also: zsh expands globs before sudo runs."
---
On this box (Fedora 44, grubby + BLS under `/boot/loader/entries/`), `grubby --update-kernel=ALL --args="…"` updates **both** the per-kernel BLS entries **and** `/etc/kernel/cmdline`. Older guidance says grubby only touches BLS entries and `/etc/kernel/cmdline` must be edited separately so future kernel installs inherit the arg. **That is no longer true here** — following it appends the argument a second time. Harmless (kernel takes the last value) but accumulates on every re-run.

Use grubby alone:

```sh
sudo grubby --update-kernel=ALL --args="consoleblank=60"
sudo grubby --update-kernel=ALL --remove-args="rhgb quiet"
```

Repair duplicates: `sudo sed -i 's/ ARG//g; s/$/ ARG/' /etc/kernel/cmdline`.

## Gotcha: globs under /boot with sudo

`/boot` is not world-readable, and zsh expands globs **before** `sudo` runs — as the unprivileged user. So this fails with `no matches found` without ever invoking sudo:

```sh
sudo grep consoleblank /boot/loader/entries/*.conf     # BROKEN
sudo sh -c 'grep -h consoleblank /boot/loader/entries/*.conf'   # RIGHT
```

Expect exactly one `consoleblank=60` per entry.

## Related
- [[kb-0004]] — consoleblank howto
