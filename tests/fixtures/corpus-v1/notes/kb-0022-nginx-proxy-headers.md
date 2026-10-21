---
id: kb-0022
title: "Reverse proxies must forward Host and X-Forwarded-* headers"
type: fact
domain: nginx
tags: []
aliases: []
triggers:
  - "when working on nginx topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-03
updated: 2026-08-03
supersedes: []
related: []
source: "synthetic fixture"
summary: "Reverse proxies must forward Host and X-Forwarded-* headers"
---
proxy_set_header Host $host keeps virtual hosting working behind nginx.

X-Forwarded-For/Proto let the app see the real client and scheme.
Without X-Forwarded-Proto, apps behind TLS terminators generate http:// redirects.
