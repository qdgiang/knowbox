---
id: kb-0006
title: "Closed-lid thermals on fedora-server are safe: 83 °C baseline, exhaust not via keyboard"
type: fact
domain: fedora
tags: [thermals, i5-9300h, lid]
aliases: [laptop overheating lid closed, thermal baseline]
triggers:
  - "worried about running the laptop lid-closed"
  - "the server runs hot / fans loud"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-03
supersedes: []
related: [kb-0003]
source: "raw/headless-laptop-server.md (measured 2026-07-24)"
summary: "Lid-closed is a designed mode here: heatpipe→fins→blower→side vent, not keyboard deck. Baseline 83 °C @ load 3.36, 0 throttle events. Real risks: soft surfaces, sustained load."
---
Closing the lid is not a significant thermal risk on this machine. Baseline measured 2026-07-24 (lid open, load avg 3.36 on 8 threads): package **83 °C**, hottest core 83 °C, crit 100 °C, **zero** kernel throttle events over 1.5 days uptime. Expect a few degrees with the lid closed, not a category change — the heat path is heatpipe → fin stack → blower → side/rear vent with intake through the bottom panel; the keyboard deck is a minor passive radiator, not an exhaust. Clamshell is a designed-for mode (every docking station does it). 83 °C on a 9300H is normal-warm (45 W gaming chip; idles in the 50s, 90s under sustained load).

**What actually matters more:**
1. **What it sits on** — intake is the bottom panel. Carpet/bed/couch seals the vents and costs far more cooling than the lid ever will. Hard flat surface, or prop for an air gap.
2. **Sustained workload** — long-running CPU consumers seen here: `opencode`, `java`, `dagster-daemon`, `python`.

**Measure the closed-lid delta:** lid closed 15 min under normal load → `sensors | grep Package`. <5 °C above baseline = noise; >90 °C = act. Fixes in order: prop lid an inch open (logind keeps display off), raise chassis for bottom clearance, `sudo dnf install thermald` to cap turbo.

> `ps -eo pcpu --sort=-pcpu` reports **lifetime average** CPU, so a fresh shell reads near 100%. Use `top`/`htop` for instantaneous load.

## Related
- [[kb-0003]] — server overview + health check
