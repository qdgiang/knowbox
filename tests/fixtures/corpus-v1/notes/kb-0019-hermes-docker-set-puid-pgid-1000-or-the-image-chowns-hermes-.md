---
id: kb-0019
title: "Hermes docker: set PUID/PGID=1000 or the image chowns ~/.hermes to UID 10000"
type: gotcha
domain: tooling
tags: [hermes, docker, permissions]
aliases: [hermes UID 10000, hermes volume ownership, PUID PGID hermes]
triggers:
  - "~/.hermes is unreadable or owned by uid 10000"
  - "deploying or re-creating the hermes container"
status: verified
confidence: high
created: 2026-08-05
updated: 2026-08-05
supersedes: []
related: [kb-0017, kb-0013]
source: "session 2026-08-03: hermes deployment on fedora-server"
summary: "The hermes-agent image's /init chowns its data volume to internal UID 10000; without -e PUID=1000 -e PGID=1000 the host user loses ~/.hermes. Includes the root docker-run recovery command."
---
The `nousresearch/hermes-agent` image's `/init` runs as root and chowns the bind-mounted data volume to its internal `hermes` user (UID 10000) — without `-e PUID=1000 -e PGID=1000` on the container, the host user (uid 1000) is locked out of `~/.hermes` (dir becomes mode 700, owner 10000).

The compose file in `~/containers/hermes/` sets both vars ([[kb-0017]]); keep them on any re-creation or `docker run` equivalent.

Recovery if ownership was already lost (needs root inside docker — fine here, docker group is root-equivalent, [[kb-0013]]):

```
docker run --rm --user root -v ~/.hermes:/opt/data \
  --entrypoint /usr/bin/chown nousresearch/hermes-agent:latest -R 1000:1000 /opt/data
```

## Related
- [[kb-0017]] — the hermes deployment this belongs to
- [[kb-0013]] — why `--user root` docker runs work here without sudo
