---
name: knowbox
description: Maintain a Markdown knowledge base with Knowbox. Use for ingestion, lint and health passes, supersession, conflict review, and structured search fallback.
---

# Knowbox maintenance

Read `SCHEMA.md` at `KNOWBOX_ROOT` (or `~/knowbox`) before changing notes. The
installed `knowbox` binary is separate from the KB. Use `knowbox --help` for
the native CLI's command and flag reference.

## Ingest or update

1. Search first. If a matching note's first-sentence claim remains true,
   update it and run `knowbox touch ID`; don't create a duplicate.
2. Create atomic notes with `knowbox new --type … --domain … --title …`.
   Use `--body-stdin` for piped Markdown. Set a traceable `source`, a short
   routing `summary`, and triggers that explain when to read the note.
3. Keep long documents whole in `refs/`, with a `type: reference` note mapping
   the sections. `knowbox refs` flags unmapped documents.
4. When a claim changes, create or identify a successor and run
   `knowbox supersede OLD --with NEW`. The retired note stays addressable by ID.
5. After manual edits, run `knowbox reindex` and `knowbox lint`. Mark ingested
   staging entries with `knowbox raw-mark FILE N --note ID`; use `--rejected`
   with a reason for rejected entries. Never delete staging history.

## Search and verification

Read the routing index first, then search for distinctive terms. Partial,
typo-corrected, hybrid, and semantic-only hits are lower-confidence pointers.
Use `knowbox expand ID --depth 2` for authored links and backlinks, or
`knowbox search QUERY --expand` for links near the top lexical results.
`knowbox show ID` warns about edits made after verification. Recheck a claim
before recording `knowbox verify ID --evidence "…"`.

Use `knowbox stale`, `knowbox churn`, and `knowbox raw-backlog` during a health
pass. Investigate both lint errors and warnings. Only user-approved historical
`created-vs-log` findings are waivable. `touch` changes are not committed until
`sweep` or another event commit; scheduled maintenance is separate from the CLI.

## Review similar notes

Refresh embeddings with `knowbox embed` if the configured provider is approved
for this KB. Run `knowbox conflicts --json`, read both notes in each candidate
pair, and skip pairs already recorded under `raw/conflicts-*.md`. The command
returns similarity, not a verdict. Record an append-only `## pair: ID ID`
block with `> verdict:` (`duplicate`, `contradiction`, `complementary`, or
`false-positive`) and a brief rationale. Apply any merge or supersession
through the CLI, not by erasing history.
