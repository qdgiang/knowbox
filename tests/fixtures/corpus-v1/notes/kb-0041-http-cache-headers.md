---
id: kb-0041
title: "Cache-Control beats Expires when both are present"
type: fact
domain: http
tags: []
aliases: []
triggers:
  - "when working on http topics in the fixture corpus"
status: active
confidence: medium
created: 2026-08-02
updated: 2026-08-02
supersedes: []
related: []
source: "synthetic fixture"
summary: "Cache-Control beats Expires when both are present"
---
Cache-Control: max-age overrides Expires; send both only for ancient clients.

ETag enables cheap revalidation (304) even when content is not cacheable long.
immutable marks fingerprinted assets so browsers never revalidate them.
