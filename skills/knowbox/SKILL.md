---
name: knowbox
description: "Maintain the ~/knowbox knowledge base. Use when ingesting a document/article/transcript into the KB, when running KB lint or a health/prune pass, when superseding or merging stale notes, or when a KB query needs structured fallback search. Also serves as the full knowbox CLI command reference (last section). Do NOT use for simply reading one known note (just Read it) or for creating a single quick note (use `knowbox new` directly)."
---

# KB maintenance skill

Conventions live in `~/knowbox/SCHEMA.md` — read it before any operation below.
`knowbox` CLI is on PATH (uv-tool shim at `~/.local/bin/knowbox`, installed from
`qdgiang/knowbox`; update via `uv tool install --reinstall git+https://github.com/qdgiang/knowbox`).

## Ingest a document

1. Classify: atomic knowledge (split into notes) vs reference doc (keep whole).
   Test: would two different tasks each want *part* of this doc? → split. Is it a
   guide/manual consulted as a unit? → refs/ + section map.
2. Reference path: `mv` the doc to `~/knowbox/refs/`, then
   `knowbox new --type reference --domain <d> --title "<doc> section map"` with a
   `## Section map` body: one line per H2, what's inside, ⚠ staleness warnings.
3. Atomic path: extract each durable fact as its own note via `knowbox new`. For each:
   GATE with `knowbox search` first (update > create). First sentence = the fact.
   Set `source:` to the original (raw/ path or URL).
4. Cross-link `related:` in the body link block (2-3, each with a reason).
5. `knowbox reindex && knowbox lint`.

## Lint / health pass

1. `knowbox lint` — fix every ERROR **and WARN**: the baseline is 0 errors /
   0 warnings (kb-0102), so any finding is actionable signal. The only
   sanctioned silence: user-approved `created-vs-log` lines in
   `~/knowbox/lint-waivers.md` (print as uncounted `WAIVED` entries) — never add a
   waiver to make a finding disappear.
2. Freshness sweep: `rg -l '^updated: <old-year>' ~/knowbox/notes` — for each, is the
   fact still true? Re-verify or mark `status: superseded`. Prefer a *different
   model/session* for judging contradictions — self-lint shares blind spots.
3. Orphan sweep: notes with no inbound `related:` from anywhere and no matching
   index summary → merge into a neighbor or sharpen the summary.
4. Index size check: index.md must stay < ~200 lines. Past that, split a domain
   into its own folder + MOC note.
5. `git -C ~/knowbox log --oneline | head` — recent churn on one note = instability;
   consider whether it captures a moving target that should be a live pointer.

Auxiliary CLI commands useful in a health pass (not in the core loop above):
`knowbox stale [--days N]` (rank live notes by staleness, 90d default; flags STALE / NEVER-VERIFIED / EDITED-SINCE-VERIFY — the last means `updated > verified`: re-verify before relying),
`knowbox churn [--days N]` (rank by git content-edit frequency — powers step 5
beyond `head`), `knowbox refs [FILTER]` (heading index over refs/; UNMAPPED docs
need a section-map note), `knowbox verify ID... --evidence "…"` (stamp a note
`status: verified` after re-checking its claim against live state).

## Supersede / merge

- Fact changed → use the CLI, never a manual recipe: `knowbox supersede OLD --with NEW`
  (create NEW with `knowbox new` first if needed). It links `supersedes:` on NEW,
  retires OLD (`status: superseded` + body banner, excluded from search, still
  addressable by id), and commits+pushes once.
- Boundary rule (SCHEMA.md): edit + `knowbox touch` only while the note's
  first-sentence claim stays TRUE; the moment the claim changes meaning
  (version, path, behavior, decision) → supersede.
- Two notes overlap: merge content into one, `knowbox supersede` the other.

## Conflicts judge pass

Deterministic scanner + reading judgment, split by design: `knowbox conflicts` is
pure embedding cosine (NO LLM in the CLI); verdicts come from an agent that
READS both notes.

1. Refresh + scan: `knowbox embed` (incremental) then `knowbox conflicts --json`
   (wider net: `--top 20`; default `--min-cos 0.70` is calibrated to the
   corpus — 99th percentile of real all-pairs cosines).
2. Skip already-judged pairs: `grep -h "^## pair:" ~/knowbox/raw/conflicts-*.md` —
   the command is stateless by design; prior reviews are the state.
3. For each remaining pair, READ BOTH notes (`knowbox show`) and assign exactly
   one verdict:
   - `duplicate` → merge content into the stronger note, `knowbox supersede` the other.
   - `contradiction` → determine the truth (live checks where possible), then
     supersede or edit per SCHEMA's edit-vs-supersede boundary rule.
   - `complementary` → add each other to `related:` with reasons + `knowbox touch` both.
   - `false-positive` → no action.
4. Record the review in `raw/conflicts-YYYY-MM-DD.md` — one block per pair:
   `## pair: kb-XXXX kb-YYYY (cos 0.NN)`, then `> verdict: <one of the four>`,
   then 1-3 sentences of reasoning + actions taken. raw/ is append-only; the
   `## pair:` / `> verdict:` marker lines are exactly what step 2 greps.
5. Apply actions via the knowbox CLI (each write commits+pushes; the raw/ review
   file rides along in the same commit).

## Structured search fallback

`knowbox search` is layered: BM25 (AND-first, OR-degrading, trigram substrings) →
on zero lexical hits, typo correction (unique edit-distance-1 candidate,
announced) → gated semantic fusion/rescue (cos-labeled) → refs/ filename +
heading grep. Only an all-layer miss lands in `.search-misses.log`. Its
labels mark degraded confidence — `(partial match…)`, `(hybrid re-rank:
lexical + semantic agreement)` or `…+ recency`, `(semantic-only)`,
`(typo-corrected: 'x'→'y')` (an out-of-domain word can be "corrected" into
KB vocabulary) —
read those hits critically. Partial-match results are recency-weighted
(`updated:`, 30d grace / 90d half-life / 0.5 floor; `KNOWBOX_NO_RECENCY=1`
disables, `KNOWBOX_NO_FUSION=1` disables the vector channel).

When it still misses (vocabulary mismatch):
1. Walk the link graph from any note you DID find — relational misses often
   resolve one hop away: `knowbox expand ID [--depth 2]` (related / supersedes /
   backlinks, with provenance) or `knowbox search … --expand` (depth-1 neighbours
   of top hits). Explicit pure-vector search: `knowbox vsearch "…"`.
2. `rg -il -- 'synonym' ~/knowbox/notes` across aliases/triggers lines:
   `rg -i 'aliases:.*word|triggers:.*word' ~/knowbox/notes` — and retry `knowbox search`
   with a distinctive fragment of an error string or path (trigram matches
   substrings).
3. Still nothing → it's genuinely absent. File it if it's now known (`knowbox new`).

## Full CLI reference

The workflow sections above cover the maintenance loops; this table is the
complete command surface (knowbox 3.1.0).

**Read-side** (no writes): `search "…" [--expand]` — layered hybrid, labels
mark degraded confidence · `expand ID [--depth 2]` — link graph
(related/supersedes/backlinks) · `vsearch "…"` — explicit pure-vector ·
`show ID|SLUG` — raw dump, warns on stderr when `updated > verified`
(edited since last confirmation) · `list [--domain D] [--type T]` ·
`recent [--limit N] [--here|--project P]` — session-continuity notes.

**Write-side** (each commits+pushes): `new --type … --domain … --title "…"`
(headless: `EDITOR=true knowbox new --no-edit` or `--body-stdin`, then fill
the stub + touch/reindex) · `touch ID` — bump `updated:` after a manual edit ·
`verify ID... --evidence "…"` — stamp `verified:` + flip status ·
`supersede OLD --with NEW` — retire, link, banner, one commit · `reindex` —
rebuild index.md + FTS after manual edits.

**Reports / maintenance**: `lint` (0/0 baseline) · `stale` · `churn
[--days N]` · `refs [FILTER]` (UNMAPPED = needs a section-map note) ·
`conflicts [--top N] [--min-cos X] [--json]` · `raw-backlog` (unreviewed raw/
entries) · `raw-mark FILE N --note kb-XXXX | --rejected "reason"` ·
`embed [--force]` (rebuild note embeddings).

**Setup / plumbing**: `init` (create a fresh KB root) · `next-id`.

**Env vars** (all `KNOWBOX_`-prefixed): `KNOWBOX_ROOT` — KB location override
(fails loudly if not a directory) · `KNOWBOX_EMBED_URL` + `KNOWBOX_API_KEY` —
required for embed/vsearch/conflicts (provider-agnostic; `KNOWBOX_EMBED_MODEL`
defaults to text-embedding-v4) · `KNOWBOX_NO_FUSION` / `KNOWBOX_NO_RECENCY` —
search kill switches · `KNOWBOX_NO_HEARTBEAT` — disable lint's janitor
dead-man check.
