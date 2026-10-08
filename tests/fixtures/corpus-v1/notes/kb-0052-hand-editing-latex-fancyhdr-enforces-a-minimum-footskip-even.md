---
id: kb-0052
title: "Hand-editing LaTeX: fancyhdr enforces a minimum footskip even with an empty footer; newlines render as spaces so a trailing period at end-of-line splits the sentence"
type: gotcha
domain: latex
tags: [latex,fancyhdr,footskip,page-geometry,hand-edit,xelatex]
aliases: [fancyhdr footskip too small, newline-as-space period bug, CV page compaction]
triggers:
  - "removing header footer does not reclaim space"
  - "sentence split by period in PDF"
  - "footskip too small warning"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0016]
source: "raw/session-distill-2026-08-08.md, raw/session-distill-2026-08-12.md"
summary: "Two hand-edit LaTeX traps: fancyhdr's enforced minimum footskip reserves page space after footer removal (fix \pagestyle{empty}); newlines render as spaces, so trailing periods split sentences."
---
Two silent layout bugs strike when hand-editing LaTeX: fancyhdr reserves footer space even when nothing is drawn, and a source newline renders as a space — so a trailing period at end of line splits a sentence.

**fancyhdr footskip.** Removing header/footer *content* from a fancyhdr-based class does not reclaim their space: fancyhdr warns "footskip too small" and enforces a minimum footskip, so text still ends ~24pt above the real margin with nothing drawn. What actually frees the space is `\pagestyle{empty}` plus trimming `headheight`/`headsep`/`footskip`. Evidence from compacting `qdgiang-cv.cls` 3→2 pages: after deleting the running header and footer tagline, content still ended at y=767pt instead of the ~791pt geometry allowed until `\pagestyle{empty}` was applied. A class still carrying headheight=14pt + headsep=4mm + footskip=10mm reserves ~19mm/page of dead space — reclaiming it shrinks a CV without touching wording.

**Newline-as-space.** LaTeX collapses a source newline to a space, so `…airline customers at FPT.` newline `Software.\par` in Giang_CV.tex's standfirst rendered "…at FPT. Software." — splitting the company name and dangling "Software.". The source looked fine; only the PDF revealed the break. Fix: drop the period.

After any hand-edit of a .tex file: check every line ending in a period, and verify claims about page space against the rendered PDF (xelatex rebuild), not the source.

## Related
- [[kb-0016]] — same class of silent LaTeX gotcha: source looks correct, output is wrong (end-of-doc \label drop)
