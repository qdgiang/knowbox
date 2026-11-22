---
name: knowbox
description: Maintain a Markdown knowledge base with Knowbox. Use for ingestion, health passes, supersession, conflict review, and structured fallback when search misses.
---

# Knowbox maintenance

Read `SCHEMA.md` at `KNOWBOX_ROOT` or `~/knowbox` before changing notes. Check
the installed binary with `knowbox version` and use `knowbox COMMAND --help`
for current flags. Knowbox releases ship native binaries for Linux, macOS and
Windows.

Agents must make every note change through the `knowbox` CLI. Never create, edit,
or replace note files with file tools, shell commands, or an editor. Use
`knowbox new` to create notes and `knowbox edit` to change an existing note.
Use `knowbox touch`, `knowbox verify`, `knowbox link` or
`knowbox supersede OLD --with NEW` for their respective changes.

## Ingest a document

1. Decide whether readers will seek individual facts or consult the document
   as a unit. Write separately searchable facts as atomic notes. Keep manuals
   and other documents consulted as a unit intact in `refs/`.
2. For a reference document, create a `type: reference` note with a
   `## Section map` body. Give each major heading one line about what it covers,
   including any known staleness. `knowbox refs` reports unmapped documents.
3. Search before each new note. If one already makes the same claim, follow
   the edit boundary below. Put the claim in the first sentence, add a traceable
   source and a routing summary, and describe when to read it in `triggers:`.
4. Use `knowbox new --type … --domain … --title … --summary … --body-stdin`
   with the complete Markdown body. Include relevant `[[kb-NNNN]]` links and
   why they matter. Use `--body` for a short inline body or `--no-edit` to skip
   the interactive editor. Supply the full body when creating an agent note.
5. Record the review with `knowbox raw-mark FILE N --note ID` after ingestion,
   or pass `--rejected "reason"`. Keep the raw entry for provenance. Check the
   result with `knowbox lint`.

## Lint and health

- Investigate errors and warnings in `knowbox lint`. Only user-approved
  historical `created-vs-log` findings are waivable. On a replica without
  scheduled maintenance, set `KNOWBOX_NO_HEARTBEAT=1`.
- Check freshness with `knowbox stale` and `knowbox show ID`. Re-check the
  claim against its source before using `knowbox verify ID --evidence "…"`.
  `EDITED-SINCE-VERIFY` means the note changed after its last verification.
- Use `knowbox expand ID` to inspect inbound links. If a note has no links
  or clear route from the index, sharpen the summary or review it for merger.
- Keep the routing index around 200 lines. When a domain dominates it, discuss
  a domain map and its intended coverage before reorganizing the notes.
- Use `knowbox churn --days N` to identify notes with repeated content edits.
  A rapidly changing fact may need a pointer to its live source. Use
  `knowbox raw-backlog` and `knowbox refs` to find open proposals and missing maps.

## Supersede and merge

- If the first-sentence claim is still true, keep its ID and use
  `knowbox edit ID`. Supply `--base-sha1` from `show --json` when saving an
  earlier draft so a concurrent edit is detected.
- If the claim changes, create or identify the successor and run
  `knowbox supersede OLD --with NEW`. It links the two notes and retires the
  old claim. The retired note is still reachable by ID.
- For overlapping notes, keep a single live claim. Review any details worth
  retaining in the stronger note with Giang before superseding the other.

## Conflicts judge pass

1. Refresh embeddings with the configured provider when approved for this KB.
   Run `knowbox conflicts --json` and skip pairs already recorded under
   `raw/conflicts-*.md`. Read both notes before deciding.
2. Choose the verdict that describes the evidence.
   - `duplicate` — retain the stronger claim and supersede the duplicate.
   - `contradiction` — check primary evidence and apply the edit boundary.
   - `complementary` — give the reason to link both notes.
   - `false-positive` — leave both notes as they are.
3. Run `knowbox verdict A B --verdict KIND --reason "…"`. Use `--link`
   for complementary pairs and `--winner ID` to retain one duplicate or
   contradictory claim. The command records the decision as `> verdict:`
   in the conflicts file.

## Structured search fallback

Search for a distinctive term after reading the routing index. Treat partial,
typo-corrected, hybrid and semantic-only hits as leads to verify. Search uses
lexical matches and trigram substrings, typo correction, gated semantic fusion
or rescue, then refs filenames and headings. A full miss is recorded locally.

1. Walk nearby links with `knowbox expand ID --depth 2`, or add `--expand`
   to a lexical search to show links near the top results. Use `knowbox vsearch`
   for an explicit semantic query when the configured provider is appropriate.
2. Try a synonym in the routing fields. For example, run
   `rg -i 'aliases:.*word|triggers:.*word' "$kb_root/notes"` with `kb_root`
   pointing to the active KB. A distinctive error string or path can also
   make a better trigram query.
3. Open matching notes with `knowbox show ID` to confirm the claim. If you
   establish a new durable fact, ingest it using the workflow above.

Ordinary indexed searches use a temporary snapshot when the saved index is
stale. With `--no-write`, an indexed search reports `stale_index` until
`knowbox reindex` publishes a fresh index. Fresh searches and misses can finish
while a writer holds the lock.

## CLI reference

- `knowbox init [--no-git] [DIR]` creates a KB.
- `knowbox reindex` records detected manual edits and publishes `index.md` and the FTS database.
- `knowbox search QUERY [--expand] [--local] [--no-fix] [--json]` runs layered retrieval.
- `knowbox show ID_OR_SLUG [--json]` prints the note and reports edits since verification.
  Use `show --path PATH` to open the exact path from a JSON result.
- `knowbox expand ID_OR_SLUG [--depth 1|2] [--json]` reports authored links and backlinks.
- `knowbox graph --json` reports all notes and authored connections.
- `knowbox history ID --json` reports a note's history and provenance.
- `knowbox timeline --json` reports change events across the KB.
- `knowbox status --json` reports the KB's Git status and last commit.
- `knowbox serve [--read-only] [--port N]` starts the loopback API.
  Pass `--assets DIRECTORY [--open]` for the separately packaged Explorer.
- `knowbox list [--domain DOMAIN] [--type TYPE]` lists notes.
- `knowbox recent [--limit N] [--here|--project PROJECT]` lists recent sessions.
- `knowbox refs [FILTER]` shows document headings and section maps.
- `knowbox stale [--days N]` reports stale and unverified claims.
- `knowbox churn [--days N]` ranks content edits from Git history.
- `knowbox new --type TYPE --domain DOMAIN --title TITLE --body-stdin` creates a note.
  Repeat `--related ID` to link the new note to existing notes in both directions.
- `knowbox edit ID [--base-sha1 SHA] [--body-stdin] [--set FIELD=VALUE]` edits a note.
  Use `--source-stdin` for a complete note or open the editor from a terminal.
- `knowbox link A B` links both notes.
- `knowbox verdict A B --verdict KIND --reason TEXT` records a verdict.
- `knowbox next-id` previews the next ID before allocation.
- `knowbox touch ID` updates the note's date and leaves the change for `sweep`
  or the next event commit.
- `knowbox verify ID... --evidence "…"` records a verification.
- `knowbox supersede OLD --with NEW` retires a claim and links its successor.
  Add `--reason "…"` to record why in the journal and commit.
- `knowbox sweep` audits note changes, refreshes stale indexes, and commits pending Git changes.
- `knowbox lint` checks notes, derived data, heartbeats and untracked entries.
  Use `--file PATH` or `--stdin` to check a draft and `--against ID` when editing.
- `knowbox usage [--days N] [--rollup]` reports local opens, note exposures, and search outcomes.
- `knowbox raw-backlog` lists unreviewed proposals.
- `knowbox raw-mark FILE N --note ID` marks a proposal ingested. Use
  `--rejected "reason"` to record rejection.
- `knowbox embed [--force]` refreshes the vector cache.
- `knowbox vsearch QUERY` queries the vector cache with the embedding provider.
- `knowbox conflicts --json` lists similarity candidates using cached vectors.
  Tune with `--top N` and `--min-cos SCORE`.
- `knowbox context --project P --host H [--explain]` produces the bounded session brief.
- `knowbox map --json` reports the knowledge map. `--project-misses` sends recent
  missed queries to the configured provider for placement.
- `knowbox metrics --json` reads measured daily lint, trust and backlog counts.
- `knowbox version` prints the installed version.

### Environment

- Use `--no-write` on read commands to disable usage and miss logging.
  An indexed search returns a stale-index status if it needs `reindex`.
- `KNOWBOX_ROOT` selects the KB directory. An invalid override is an error.
- `--client CLIENT` overrides `KNOWBOX_CLIENT` for journal events and usage.
  `KNOWBOX_HOST` sets the recorded machine. `KNOWBOX_SESSION` sets the
  session when the harness does not supply one.
- `KNOWBOX_EMBED_URL` and `KNOWBOX_API_KEY` configure the provider.
  `KNOWBOX_EMBED_MODEL` defaults to `text-embedding-v4`. Unset the URL for
  offline searches.
- `KNOWBOX_NO_FUSION=1` disables partial-match reranking. Semantic rescue can
  still run after a miss. `KNOWBOX_NO_RECENCY=1` disables age weighting in fusion.
- `KNOWBOX_NO_HEARTBEAT=1` disables both lint heartbeat checks.
- `KNOWBOX_LOCK_TIMEOUT` sets the writer lock wait in seconds, default 120.
- `EDITOR` selects the editor for an interactive private draft with frontmatter
  and body. Knowbox fills `id`, `created` and `updated` on save.
- `HTTPS_PROXY` and `NO_PROXY` configure CONNECT proxy routing for HTTPS
  embedding requests. Numeric loopback HTTP connects directly.
