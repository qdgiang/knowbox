---
id: kb-0016
title: "LaTeX: \\label after the last typeset content silently drops its aux write when the final page is already full"
type: gotcha
domain: latex
tags: [latex, xelatex, aux, label, fancyhdr, makeuppercase]
aliases: [label missing from aux, pageref shows ??, lastpage folio broken]
triggers:
  - "a \\pageref folio or last-page reference shows ?? in a xelatex document"
  - "newlabel missing from .aux even though the \\label line is reached"
  - "debugging LaTeX aux writes with \\typeout probes"
status: verified
confidence: high
created: 2026-08-03
updated: 2026-08-11
supersedes: []
related: [kb-0002]
source: "session 2026-08-03: personal-resume XeLaTeX CV folio debugging"
summary: "Bare \\label at end of doc is a deferred whatsit — if the last page is full, the aux write vanishes; attach labels inside the last typeset content. \\MakeUppercase uppercases \\pageref keys; probes need \\makeatletter."
---

A bare `\label{...}` (or `\write\@auxout{...}`) placed after the last typeset
content is a **deferred** whatsit: it reaches the `.aux` only when the page it
sat on ships. If the final page is already full, the output routine ships it
*before* TeX reads the label token; the whatsit then sits on a never-shipped
next page and `\end{document}` discards it. No error — `newlabel` just never
appears and `\pageref` shows `??` on every run. **Fix:** put the label inside
the last typeset argument (e.g. at the end of the final line's text), or use a
lastpage-style shipout hook. Compile twice afterwards.

Two more traps from the same session:

- `\MakeUppercase{page \thepage\ of \pageref{k}}` uppercases the label **key**
  (`k`→`K`), so the lookup never matches. Keep `\pageref` outside the
  case-changer: `\MakeUppercase{page}\ \thepage\ \MakeUppercase{of}\ \pageref{k}`.
- Probing with `\typeout{...\if@filesw T\else F\fi}` in document body is
  bogus: outside `\makeatletter`, `@` is not a letter, so the tokens are
  `\if`+`@`+`f` (a char-code compare that is always false). Wrap probes in
  `\makeatletter ... \makeatother` or you will chase a phantom `\nofiles`.

Machine facts (fedora-server, distro texlive, XeLaTeX projects):

- `lastpage.sty` is **not installed**; a `\label`+`\pageref` folio avoids the
  dependency entirely (install via `dnf install texlive-lastpage` only if a
  project truly needs it).
- `poppler-utils` is absent (`pdftoppm`/`pdftocairo` missing); rasterize PDFs
  with `gs -q -sDEVICE=png16m -r200 -o out-%d.pdf input.pdf` (ghostscript is
  installed). Per [[kb-0002]] install anything missing only via dnf.
