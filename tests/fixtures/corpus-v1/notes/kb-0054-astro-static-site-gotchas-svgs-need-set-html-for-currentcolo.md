---
id: kb-0054
title: "Astro static-site gotchas: SVGs need set:html for currentColor, route params can't contain /, sort keys byte-wise not with localeCompare, preview reads top-level server.allowedHosts"
type: gotcha
domain: projects
tags: [astro,svg,slug,sorting,preview,allowedHosts,personal-website]
aliases: [set:html currentColor, tagSlug, localeCompare nondeterministic, getStaticPaths slash, astro preview allowedHosts]
triggers:
  - "adding SVG icons to an Astro page that should inherit currentColor"
  - "deriving route params or tag slugs from free-form labels"
  - "sorting collections for static-site pages"
  - "browsing astro preview via a LAN hostname and getting a 403 Blocked request"
status: active
confidence: high
created: 2026-08-19
updated: 2026-08-31
supersedes: []
related: [kb-0050]
source: "raw/session-distill-2026-08-09.md, raw/session-distill-2026-08-18.md"
summary: "Four static-site gotchas: <img> blocks currentColor (use set:html); getStaticPaths params can't contain / (Unicode-safe tagSlug regex); localeCompare reorders across builds (sort byte-wise); astro preview reads top-level server.allowedHosts, ignoring vite.preview.allowedHosts."
---
Four independent gotchas hit on the personal-website Astro site; each fails the build or the render silently.

**1. Monochrome SVGs must be inlined via `set:html`, never `<img>`.** An SVG loaded through `<img src="…">` is a separate document whose `currentColor` resolves against the SVG's own root, not the host page — a `currentColor` fill renders black and disappears on a dark palette. Read the file at build time (`readFile(new URL('../../public/cert-logos/' + slug + '.svg', import.meta.url))` from `node:fs/promises`, cached in a `Map`) and inject raw markup: `<span aria-hidden="true" set:html={svg} />`. With no baked-in `fill`, CSS `svg { fill: currentColor }` themes icons exactly like text.

**2. Route params can't contain `/` — sanitize free-form labels at the source.** `getStaticPaths` params must be a single URL-safe segment: a `/` breaks the build or mints unexpected nested paths, and `?`/`%`/`&` ship dead hrefs. Working Unicode-safe slug (`tagSlug` in `src/lib/posts.ts`):

```js
label.trim().toLowerCase().replace(/[^\p{L}\p{N}\s-]/gu, ' ').trim().replace(/\s+/g, '-')
```

It keeps letters/digits of every script (Vietnamese `tiếng việt` → `tiếng-việt`, diacritics intact, no transliteration) while punctuation becomes word gaps (`A/B testing` → `a-b-testing`, not broken `a/b-testing`). ASCII-first stripping would have destroyed Vietnamese slugs. Guard the schema so slugs are never empty or punctuation-only: `z.array(z.string().trim().min(1).regex(/\p{L}|\p{N}/u)).default([])`.

**3. Sort static-site keys byte-wise, never with `localeCompare`.** `localeCompare` output depends on the ICU locale data bundled with the runtime/OS, so the same sort can reorder tag indexes/listings between machines or Node versions. Use plain code-unit comparison: `(a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0)` — deterministic everywhere. Fixed-width ASCII dates (`YYYY-MM-DD`) are the only safe `localeCompare` exception; arbitrary UTF-8 (Vietnamese tag slugs) is not.

**4. `astro preview` host-allowing comes from the TOP-LEVEL `server.allowedHosts`, never `vite.preview`.** Browsing preview via a LAN hostname 403s with Vite's "Blocked request. This host is not allowed… add it to `preview.allowedHosts` in vite.config.js" — that message misleads for Astro: `astro preview` starts Vite with `configFile: false` and feeds it `preview.allowedHosts: settings.config.server.allowedHosts` itself (verified in `astro/dist/core/preview/static-preview-server.js`), so a `vite: { preview: { allowedHosts } }` block in `astro.config.mjs` is silently ignored while the error persists. Fix: top-level `server: { allowedHosts: ['mac-mini', 'fedora-server'] }`. The DEV server is a different path — `vite: { server: { allowedHosts } }` IS read there, which is why dev worked while preview 403'd with the identical-looking host error. On this box preview also needs the container flags from [[kb-0073]].

## Related
- [[kb-0050]] — the blog-tags plan whose tagSlug/tag-page implementation surfaced gotchas 2 and 3
- [[kb-0073]] — the container invocation preview runs under on this box
