---
id: kb-0003
title: "fedora-server: headless Fedora 44 laptop, runs lid-closed (logind config)"
type: project
domain: fedora
tags: [fedora-server, logind, lid, headless]
aliases: [fedora laptop server, headless laptop]
triggers:
  - "the laptop suspends when the lid closes"
  - "configuring or checking this home server"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0004, kb-0006, kb-0005]
source: "raw/headless-laptop-server.md + memory:fedora-laptop-server.md"
summary: "Home server is a headless Fedora 44 laptop (i5-9300H) running lid-closed. Lid-suspend disabled via logind drop-in — all 3 HandleLidSwitch keys required."
---
`fedora-server` — Fedora 44, headless on an old laptop (Intel i5-9300H, 4C/8T, 45W). Boots to `multi-user.target`, no GNOME. Primary working dir `/home/giang` (not a git repo). Configured 2026-07-24 for always-on lid-closed operation.

## Lid closed must not suspend

`/etc/systemd/logind.conf.d/99-lid.conf`:

```ini
[Login]
HandleLidSwitch=ignore
HandleLidSwitchExternalPower=ignore
HandleLidSwitchDocked=ignore
```

Apply: `sudo systemctl restart systemd-logind` (immediate; may drop SSH sessions). **All three keys are required** — `HandleLidSwitchExternalPower` governs an always-plugged-in server and is the one most commonly missed; setting only `HandleLidSwitch` leaves a plugged-in machine still suspending.

Verify (these are logind D-Bus properties, NOT unit properties — `systemctl show systemd-logind` shows nothing, and empty output there is not evidence of failure):

```sh
busctl get-property org.freedesktop.login1 /org/freedesktop.login1 \
  org.freedesktop.login1.Manager HandleLidSwitch   # expect s "ignore"
```

## Quick health check

```sh
busctl get-property org.freedesktop.login1 /org/freedesktop.login1 \
  org.freedesktop.login1.Manager HandleLidSwitch   # s "ignore"
cat /sys/module/kernel/parameters/consoleblank      # 30 (see [[kb-0004]])
systemctl is-active console-off-after-boot.service  # active (exited); display off 30 s after boot (see [[kb-0004]])
sensors | grep Package                              # vs 83 °C baseline (see [[kb-0006]])
journalctl -k | grep -ci "thermal throttl"          # 0
```

## Related
- [[kb-0004]] — consoleblank console blanking
- [[kb-0006]] — thermal baseline / closed-lid delta
- [[kb-0005]] — sudo+glob gotcha on /boot
