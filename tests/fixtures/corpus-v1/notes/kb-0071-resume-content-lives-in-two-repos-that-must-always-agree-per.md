---
id: kb-0071
title: "Resume content lives in two repos that must always agree: personal-resume (ATS PDF) and personal-website /resume"
type: fact
domain: projects
tags: [resume, personal-resume, personal-website, content-sync]
aliases: [resume dual-source rule, resume sync rule]
status: active
confidence: high
created: 2026-08-24
updated: 2026-08-24
supersedes: []
related: [kb-0053, kb-0050]
source: "user directive, 2026-08-24"
summary: "Which files hold resume content, why there are two copies, and the rule that any resume edit must touch both."
---
The resume has TWO content sources that must always agree: `~/projects/personal-resume/Giang_CV.tex` (LaTeX → ATS-friendly PDF sent to recruiters) and `~/projects/personal-website/src/data/resume.ts` (rendered by `src/pages/resume.astro` as the site's /resume page). "Edit my resume" always means edit BOTH.

## The two sources

- **`~/projects/personal-resume/`** — XeLaTeX CV: `Giang_CV.tex` + `qdgiang-cv.cls`, built with `latexmk -xelatex`. Output PDF is the recruiter/ATS artifact (gitignored, regenerable). Sections: masthead + contact, summary, 01 Experience, 02 Projects, 03 Education & Awards, 04 Skills, 05 References (full named references with email/phone).
- **`~/projects/personal-website/src/data/resume.ts`** — typed data arrays (`EXPERIENCE`, `PROJECTS`, `EDUCATION_AWARDS`, `SKILLS`) consumed by `src/pages/resume.astro`. The page also injects a Certifications section from `getCerts()` and a "print / save as pdf" button.

The ONLY reason for the split is distribution format: the LaTeX PDF satisfies ATS/recruiter requirements; the website page is the public copy. Content should be identical — there is no intent to maintain different content per surface.

## The rule

Any request to change resume content (new role, reworded bullet, skill change, etc.) applies to BOTH repos in the same change. Check the other copy before finishing.

## Intentional carve-outs (do NOT reconcile)

User-declared 2026-08-24. Future sessions must not "fix" these:

- **References** — the LaTeX CV carries full named references (contact details for recruiters); the website says "Available on request." (privacy). Both correct as-is.
- **Certifications** — website-only section (dynamic, from content certs); the LaTeX CV intentionally has none.
- Bold emphasis (`\textbf`) is CV-only formatting, not a content difference (the website page can't render bold in experience bullets).

## Divergence status as of 2026-08-24

- **Education** — RECONCILED: "Tran Phu Gifted High School — diploma 9.2/10. 2016 – 2019." added to the LaTeX CV.
- **Bullet/summary/heading wording** — RECONCILED 2026-08-24: user chose the CV wording for every drifted item; website `resume.ts` + `resume.astro` lead updated to match, including date short forms (Jun/Sep/Jul) and the CV's Tools chip order. `\textbf` emphasis remains CV-only formatting.
- Layout note: the CV is tuned to exactly 2 pages; a one-line addition overflows to a 3rd page. Vertical glue was trimmed to absorb the Tran Phu line (labrow vspace 2mm→1.5mm; refs gaps 2.5→2mm and 1→0.5mm). Future content additions will need similar squeeze.

## Related
- [[kb-0053]] — Giang_CV.tex job bullets must be copied verbatim from archive/PlushCV.tex (tension: the website copy paraphrases them)
- [[kb-0050]] — prior personal-resume vs personal-website repo confusion
