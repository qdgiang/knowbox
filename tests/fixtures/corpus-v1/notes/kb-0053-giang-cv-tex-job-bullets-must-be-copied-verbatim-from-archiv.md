---
id: kb-0053
title: "Giang_CV.tex: job bullets must be copied verbatim from archive/PlushCV.tex (never paraphrased), and the VPBank bullet's link to the hackathon presentation PDF is intentional"
type: fact
domain: projects
tags: [resume,cv,personal-resume,verbatim,drive-links,latex]
aliases: [resume bullets verbatim, VPBank presentation link, PlushCV source of truth]
triggers:
  - "editing Giang_CV.tex"
  - "paraphrasing resume bullets"
  - "VPBank bullet link wrong"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0016]
source: "raw/session-distill-2026-08-08.md"
summary: "Resume rules: bullets copied verbatim from archive/PlushCV.tex, no AI paraphrasing; VPBank runner-up bullet intentionally links the hackathon presentation Drive PDF — don't fix it."
---
Two load-bearing rules for editing Giang_CV.tex, both from explicit user decisions: bullets are verbatim-only, and one achievement link that looks "wrong" is correct.

**Rule 1 — bullets verbatim, never paraphrased.** The user rejected AI rewriting of their own resume language: "I do not want AI paraphrasing my own written stuffs. Revert the description of the job back to the original one on my old resume." The old resume, archived at `archive/PlushCV.tex`, is the verbatim source of truth for experience/project bullets — including `\textbf` emphasis, phrasings like "PIC 1 of", "etc. on bare-metal and K8s", "2.6 times", "code base", quotes, and merged (not split) bullets. The offending rewrite had changed "PIC 1 of"→"Owned", "K8s"→"Kubernetes", "2.6 times"→"2.6×", and split the single FPT Palantir+Innovation-sprints bullet into two. Verify reverts by grepping the `.tex` source for exact phrases — never rely on PDF text extraction.

**Rule 2 — the VPBank bullet's Drive link is correct; don't "fix" it.** The "Runner-up, VPBank" bullet intentionally points to Drive file `1P8yM1BTY-EkHb6h_LGHTZV4KSgaZqU0G`, which resolves to "VPBANK Hackathon Presentation.pdf" (user-confirmed when flagged as wrong). The HCMUT CSE Technical Day 2023 bullet points to `1be6nY74-…` = "HK223_90_LVTN_DATN_Finish_QuachDangGiang_1952044.pdf", the graduation thesis. Both IDs are shared identically between Giang_CV.tex and PlushCV.tex; what each file actually is (presentation vs. thesis) is not derivable from the repo — it was determined by fetching the share URLs and reading page titles.

## Related
- [[kb-0016]] — another silent gotcha in this LaTeX CV file family; keep in mind when hand-editing the same sources
