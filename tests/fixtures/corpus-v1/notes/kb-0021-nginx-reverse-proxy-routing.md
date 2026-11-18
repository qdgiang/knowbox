---
id: kb-0021
title: "nginx location blocks route by longest prefix match"
type: howto
domain: nginx
tags: []
aliases: []
triggers:
  - "when working on nginx topics in the fixture corpus"
status: verified
confidence: medium
created: 2026-08-02
updated: 2026-08-02
verified: 2026-09-01
supersedes: []
related: []
source: "synthetic fixture"
summary: "nginx location blocks route by longest prefix match"
---
nginx picks the longest matching location prefix for reverse proxy routing.

proxy_pass with a trailing slash rewrites the URI prefix; without, it passes unchanged.
Exact match (= /path) beats prefix; regex locations beat prefixes when declared with ~.
