---
id: kb-0073
title: "Node/JS tooling runs in the project's docker container, never on the host"
type: fact
domain: tooling
tags: [docker, node, astro, personal-website]
aliases: [always use docker tools, no host node]
status: active
confidence: high
created: 2026-08-24
updated: 2026-08-31
supersedes: []
related: [kb-0043, kb-0015]
source: "user correction, 2026-08-24"
summary: "How to run node/npm/astro on this box: inside the project's compose dev container, never a host node binary."
---
Node/JS tooling on this machine always runs inside the project's docker container (the compose `dev` service), never on the host — the host has no node, and AGENTS.md bars dev tooling from the machine's main env. User instruction 2026-08-24: "Always use docker tools."

Working invocation for `~/projects/personal-website` (Astro): `docker compose run --rm dev npm run build` (also `npm run check`, `npm run dev`). Image is `node:22-alpine` with `npm ci` baked in; compose mounts `.:/app` plus an anonymous `/app/node_modules` volume, so host `node_modules/` is irrelevant to container runs. `dist/` is untracked build output.

Preview is the same container rule but needs two extra flags to be browsable from another LAN machine (this box is a headless server): `docker compose run --rm --service-ports dev npm run preview -- --host 0.0.0.0` — `--service-ports` publishes the compose port on the host, and Astro binds loopback-only inside the container by default. If browsing via the machine's hostname then 403s ("Blocked request. This host is not allowed"), that's Astro's host allowlist — see [[kb-0054]] gotcha 4: `astro preview` reads only the TOP-LEVEL `server.allowedHosts` in `astro.config.mjs`, never `vite.preview.allowedHosts`.

## Related
- [[kb-0043]] — same rule recorded for iceberg-learning ("via docker `npm run check`/`build`, never host node")
- [[kb-0015]] — the Python equivalent: always `uv`, never system Python
