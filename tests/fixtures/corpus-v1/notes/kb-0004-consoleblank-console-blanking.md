---
id: kb-0004
title: "Laptop display off 30 s after boot: consoleblank=30 + console-off-after-boot.service"
type: howto
domain: fedora
tags: [consoleblank, grubby, kernel-cmdline]
aliases: [console blanking, turn off console display]
triggers:
  - "the laptop screen stays on / console won't go dark"
  - "adding or removing kernel boot args on fedora-server"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-03
supersedes: []
related: [kb-0003, kb-0005]
source: "raw/headless-laptop-server.md"
summary: "consoleblank=30 staged via grubby (running kernel still 60 until reboot); console-off-after-boot.service force-blanks tty1 + zeroes intel_backlight 30s after boot. See kb-0005 first."
---
Goal on fedora-server: see boot messages and the login prompt on power-on, keep the display on for the first 30 s after boot (open-the-lid diagnostics window), then go fully dark — signal **and** backlight.

## consoleblank=30 (idle-based signal blank)

```sh
sudo grubby --update-kernel=ALL --args="consoleblank=30"   # reboot required
cat /sys/module/kernel/parameters/consoleblank             # verify: 30 (default 0)
```

**Live state 2026-08-03 (janitor pass):** boot args are staged to 30 (`/etc/kernel/cmdline` has exactly one `consoleblank=30`) but the *running* kernel still reports 60 — takes effect on next reboot. The service + backlight below are live regardless.

The timer measures **idle time on the console**, not time since boot — anything that prints to tty1 restarts the 30s countdown; a physical keypress wakes it. Blanks the display signal only; **the backlight stays on** — that is handled by the unit below.

## console-off-after-boot.service (hard boot+30s bound, kills backlight)

`/etc/systemd/system/console-off-after-boot.service` (canonical source copy: `/home/giang/console-off-after-boot.service`): oneshot, `After=multi-user.target`; sleeps 30 s, then force-blanks the console (`setterm --blank force > /dev/tty1`) and writes `0` to `/sys/class/backlight/intel_backlight/brightness`. This is the strict "off after the first 30 s after boot" bound — it fires even if something keeps printing to the console, and it powers off the backlight, which consoleblank cannot do. Install/repair: `sudo bash /home/giang/setup-clamshell-display.sh` (idempotent; also (re)applies the consoleblank arg). Verified 2026-08-02: live start took brightness 120000 → 0.

`rhgb quiet` is still on the cmdline (splash instead of boot log). To watch the real boot log: `sudo grubby --update-kernel=ALL --remove-args="rhgb quiet"`.

**On Fedora 44, grubby already syncs `/etc/kernel/cmdline`** — see [[kb-0005]] before editing anything under `/boot` or `/etc/kernel/cmdline`.

## Related
- [[kb-0005]] — grubby syncs /etc/kernel/cmdline (don't double-edit) + /boot glob gotcha
- [[kb-0003]] — server overview + health check
