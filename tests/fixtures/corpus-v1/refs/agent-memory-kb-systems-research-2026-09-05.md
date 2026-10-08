# Agent-memory KB systems research — 2026-09-05

Deep-research report (fan-out web search → 26 sources → 127 claims → 25 adversarially
verified, 3-vote panels: 22 confirmed, 3 refuted). Question: how do other practitioners
set up persistent knowledge bases for AI coding agents with open-source, CLI-first,
plain-markdown + git stacks comparable to ~/kb — specifically for conflict resolution,
supersession, cross-note synthesis, stale derived indexes, and silent maintenance
failures. Companion section-map note: see `notes/` (type: reference, created 2026-09-05).

Stats: 6 search angles · 110 agent calls · sources are primary (repos/docs/papers)
unless marked blog/forum. Verification votes noted per finding.

## 1. Executive summary

Practitioner systems comparable to ~/kb (markdown + YAML frontmatter + CLI + FTS/BM25 +
git) converge on one architecture: **plain markdown is the authoritative,
human-reviewable store; every index (SQLite FTS5, embeddings, knowledge graph) is
derived, rebuildable, and explicitly subordinate.** No surveyed system abandons gitops
reviewability — ~/kb's core design is field consensus, not idiosyncratic (validates
kb-0011's 2026-08-02 survey at greater depth).

For ~/kb's verified weak points, the field's shipped answers:

| Weak point | External solution pattern | Systems |
|---|---|---|
| (4) Silently stale derived index | Automatic reconciliation: file watchers, content-hash change detection, transactional atomic replacement, `doctor`-style file↔DB consistency check | basic-memory, sqlite-memory, ai-memory |
| (1)/(2) No conflict precedence; never-used supersede | Automated write-time resolution: LLM tool-call ADD/UPDATE/DELETE/NOOP over top-10 embedding-similar notes; bi-temporal soft-invalidation (never delete) | Mem0, Zep/Graphiti, Mem0g, ai-memory |
| (3) No cross-note synthesis / overlap detection | Typed-relation grammar parsed into a derived graph + multi-hop traversal; embedding-clustered near-duplicate detection | basic-memory, SemDeDup |
| Deferred contradiction lint | First-class `memory_lint` tool: declared `contradicts:` edges (zero-LLM) + optional LLM pass; lint output as git-committable markdown; rule-promotion suggestions | ai-memory |
| (5) Silent maintenance-loop failure | Dead man's switch: job must ping; absence of ping alerts | healthchecks.io pattern |

## 2. Finding: architecture consensus (confidence high, votes 3-0 merged)

Markdown-as-truth + derived-rebuildable-index is the consensus for
agent-consumption-first KBs; embeddings/vector/graph layers are optional add-ons
deferred until measured need.

- **cross-agent-memory**: "Markdown and Git reviewable, portable, and authoritative …
  the control condition and source of truth"; indexes "derived from the Git layer and
  rebuildable" (FTS/BM25 first); explicitly defers embeddings/vector/graph until a
  repo-specific benchmark justifies them.
- **basic-memory**: "Plain Markdown files store all knowledge … the database serves as
  a secondary index"; "Just files plus a local SQLite index … the source of truth is
  always your files." Fixed frontmatter defaults (title/type/permalink/tags) plus
  pattern-parsed body structure; hybrid full-text + vector ranking (FastEmbed,
  optional jina-reranker-v1-tiny-en) layered over the markdown.
- **ai-memory**: "Plain markdown on disk as source of truth … Persistent index in
  SQLite + FTS5 … No reindex on every restart."
- **sqlite-memory**: markdown files as source of truth indexed into one portable .db,
  hybrid FTS5 + vector retrieval, markdown-aware chunking.
- **memweave / memsearch**: SQLite/vector index treated as a purely derived cache;
  deleting it loses nothing.

## 3. Finding: stale derived indexes (confidence high, votes merged incl. one 2-1)

Solved by **automatic reconciliation loops + explicit diagnostics commands**, not by
remembering to reindex:

- **basic-memory**: File Watcher detects changes → re-parses → updates DB → resolves
  forward references → updates search index. Ships `basic-memory doctor` (file↔DB
  consistency check, also an MCP tool `basic_memory_diagnostics`) + `reindex`.
  Caveat (refuted claim bounds this): watcher automation only runs while watch
  mode/MCP server is alive — manual `sync` still needed otherwise. Shipped pattern is
  belt-and-suspenders: watcher + periodic doctor/hash-check.
- **sqlite-memory**: content-hash change detection on every write; unchanged content
  skipped; modified content atomically replaces old chunks/embeddings/FTS rows inside
  SQLite SAVEPOINT transactions (one failed file rolls back cleanly). Caveat: hash
  detection fires only when sync is invoked (cron/startup/watch) — plain in-place
  edits outside the sync path are NOT covered (0-3 refutation).
- **ai-memory**: index commits in the same transaction as the data
  ("No background-task-indexing-after-return" — an invariant derived from
  basic-memory bugs #763/#578) + watcher reconciling hand edits (vim/Obsidian).

Transferable to ~/kb: per-note (mtime, sha1) in `.kb.db`; `search`/`lint` compare
against filesystem → auto-rebuild or ERROR. This is exactly the audit's H1 fix,
independently shipped by three systems.

## 4. Finding: write-time conflict resolution — Mem0 (confidence high, 3-0 ×2)

Mem0 resolves conflicting/overlapping memories **at write time** via an LLM tool-call
choosing one of four operations — ADD, UPDATE, DELETE ("removal of memories
contradicted by new information"), NOOP — instead of a separate classifier. Conflict
detection is **embedding-scoped**: "the system first retrieves the top s semantically
similar memories using vector embeddings" (s=10). Contradiction checking runs against
a similarity-restricted candidate set, not the whole store — the cost-control answer
to a deferred contradiction lint.

Directly imitable for ~/kb: at `kb new`/edit (or as a periodic pass), retrieve top-10
cosine-similar notes from the embeddings the CLI already computes, and have the agent
choose ADD/UPDATE/SUPERSEDE/NOOP with the candidates in context.

Qualification: Mem0's OSS implementation has diverged from its paper (issue #4956,
Apr 2026, unconfirmed, alleges v3 ingest degraded to ADD-only; graph memory removed
from OSS v3) — cite the mechanism from the paper (ECAI 2025, arXiv:2504.19413), not
the current OSS default.

## 5. Finding: supersession without deletion (confidence high, votes 3-0 merged)

The dominant precedence rule across systems: **soft-invalidate, never delete**,
automated at write/sweep time:

- **Zep/Graphiti**: LLM compares each new edge against semantically related existing
  edges to find contradictions, then invalidates older edges — bi-temporal model,
  four timestamps per fact including `valid_at` / `invalid_at`; "prioritizes new
  information when determining edge invalidation." Retired facts stay in the graph,
  stamped `invalid_at`, and searches downrank them (Graphiti issue #1728).
- **Mem0g**: "An LLM-based update resolver determines if certain relationships should
  be obsolete, marking them as invalid rather than physically removing them to enable
  temporal reasoning" (soft-delete PR #4188; cite the paper — removed from OSS v3).
- **ai-memory**: "Versioned in place: old `is_latest=false`, new `supersedes=old_id`";
  retrieval surfaces only `is_latest=1` pages; forget sweep tombstones with
  `superseded_at`.
- **MADR (ADR-style decision logs)**: status enum
  `proposed|rejected|accepted|deprecated|superseded` + mandatory "superseded by
  ADR-NNNN" pointer — supersession encoded in frontmatter as a machine-readable
  precedence link. ~/kb's schema already matches this; the gap is automation, not
  design.

Transferable: ~/kb's `status: superseded` + `supersedes:` IS this pattern — the fix
is making it the automatic outcome of a fact-change (write-time detection per §4),
not a command nobody runs.

## 6. Finding: cross-note synthesis via typed relations (confidence high, 3-0 ×3)

**basic-memory** achieves synthesis in plain markdown with a lightweight grammar
parsed by syntax pattern anywhere in the document (NOTE-FORMAT.md):

- Categorized Observations: `- [category] fact`
- Open-ended typed Relations: `- relation_type [[Target Entity]]` — any text is a
  valid relation type ("conventions, not a fixed set", default `relates_to`);
  forward references resolve when the target is created.
- Headings conventional but not required → structure survives free-form editing.
- Agents traverse multi-hop chains with `build_context`, which "follows links between
  notes to assemble relevant information from across your knowledge base."

Transferable: ~/kb's `[[kb-…]]` wikilinks + `related:` are the same substrate;
adding optional relation types (e.g. `- contradicts [[kb-0010]]`,
`- supersedes-detail [[kb-0077]]`) and a graph-traversal command would give
multi-note synthesis without adopting the tool.

Also relevant: **SemDeDup** (arXiv:2303.09540) — embed → k-means cluster → pairwise
cosine only within clusters → flag pairs above threshold. The cheap algorithm for a
`kb similar`/`kb conflicts` overlap report at corpus scale.

## 7. Finding: memory_lint — the contradiction lint we deferred (confidence medium, 3-0)

**ai-memory** ships it as a first-class agent tool:

- `memory_lint` runs stale-page detection, contradiction finding, and rule-suggestion
  checks.
- Zero-LLM path: declared `contradicts:` frontmatter edges ("No LLM involved: the
  declaration IS the signal") + optional LLM contradiction pass when a provider is
  configured.
- Lint output lands as markdown **inside the wiki itself** (`wiki/_lint/`) —
  reviewable, git-committable.
- Rule-suggestion path = distill-promotion analog: "a rule looks durable enough to
  copy into CLAUDE.md or AGENTS.md."
- Feedback channel: `memory_feedback` stale/wrong rows feed unresolved findings into
  the next lint pass.

Medium confidence: single-system, single-vendor evidence; zero-LLM path only catches
*declared* contradictions — undeclared semantic conflicts still need an LLM pass.

## 8. Finding: alternative precedence mechanisms (confidence medium, split votes)

Where write-time LLM resolution is unwanted:

- **cross-agent-memory** (2-1): promotion gated on **verifiable provenance** — "a
  seeded claim must resolve against a commit, pull request, test, run, or recorded
  owner decision"; unresolvable claims become dated historical checkpoints, not
  current truth; agent-proposed updates need live evidence or owner review. Caveat:
  single-author, ~6-week-old, zero-adoption repo, instruction-level enforcement;
  a companion claim about paired supersedes/superseded_by frontmatter was refuted 0-3.
  Treat as design idea. (Maps to ~/kb's existing `source:` + `verified:` +
  `confidence:` fields — same idea, already in our schema, unenforced.)
- **basic-memory** (3-0): tool-level overwrite safety — `write_note` errors on
  existing paths unless overwrite explicitly enabled; `edit_note` append/prepend
  auto-creates. No note-level precedence rule documented.
- **sqlite-memory** (2-1): block-level last-write-wins CRDT merges concurrent
  multi-agent edits. LWW is an explicit precedence rule (newest wins) but discards
  the losing write; does not resolve semantic contradictions between distinct notes.

## 9. Silent maintenance failure (weak point 5)

**healthchecks.io dead man's switch**: each successful cron run must actively ping a
unique URL; the *absence* of an expected ping raises the alert. Covers the exact
failure mode that killed ~/kb's lint cron (job never runs → passive logs show
nothing). Self-hostable or SaaS; a local equivalent: cron job touches a heartbeat
file, and `kb lint` ERRORs when the heartbeat is older than the schedule interval.

## 10. Caveats and refuted claims

- Source-strength asymmetry: strongest evidence is primary/code-verifiable (Mem0 &
  Zep papers; basic-memory, sqlite-memory, ai-memory repos/docs), but vendor papers
  carry self-reported benchmarks — mechanisms cited, not efficacy numbers.
- Refuted 0-3: cross-agent-memory does NOT use paired `supersedes`/`superseded_by`
  frontmatter lifecycle ops as claimed.
- Refuted 1-2: basic-memory's index is NOT updated automatically on any file change
  without watch mode running.
- Refuted 0-3: sqlite-memory does NOT guarantee derived-store consistency for
  in-place edits outside its sync path.
- Nothing surveyed fully solves semantic contradiction detection without an LLM in
  the loop. No-LLM options are declared edges (ai-memory) or recency rules (LWW
  CRDT) — both cheap to adopt, neither detects *undeclared* conflicts.
- Practitioner signal (forum, lower weight): claude-mem (~45k stars) is append-only
  with NO invalidation mechanism — the most popular tool in the space has the same
  hole ~/kb's audit found; invalidation is an open question there too.

## 11. Open questions

1. Cost (latency/tokens/false positives) of a Mem0-style write-time resolution pass
   retrofitted onto an existing markdown KB — no surveyed source reports lint/ingest
   cost at corpus scale.
2. Retrieval-quality impact of soft-invalidated note graveyards over time — do
   downranked tombstones degrade answers or context budgets in agents that read
   notes whole?
3. No surveyed system combines git-native human review (PR-based approval of memory
   updates) with automated contradiction detection end-to-end. The hybrid (agent
   opens a PR that supersedes a note and cites the contradicting evidence) appears
   unbuilt — closest to ~/kb's gitops requirement.
4. Beyond dead-man's-switch and ai-memory's feedback-fed lint, what mechanisms make
   scheduled maintenance loops fail loudly?

## 12. Sources

Primary (repos/docs/papers):
- https://github.com/basicmachines-co/basic-memory (+ NOTE-FORMAT.md, docs.basicmemory.com/reference/technical-information, /reference/mcp-tools-reference, /concepts/observations-and-relations)
- https://arxiv.org/html/2504.19413v1 — Mem0 paper (ECAI 2025)
- https://arxiv.org/html/2501.13956v1 — Zep/Graphiti paper + https://github.com/getzep/graphiti
- https://github.com/akitaonrails/ai-memory + author blog post 2026-05-23
- https://github.com/sqliteai/sqlite-memory
- https://github.com/VladimirGutuev/cross-agent-memory/blob/main/RESEARCH.md
- https://arxiv.org/html/2303.09540v3 — SemDeDup
- https://arxiv.org/abs/2502.12110 — A-MEM; https://arxiv.org/abs/2606.10677 — Infini Memory
- https://docs.mem0.ai/core-concepts/memory-operations/update + https://github.com/mem0ai/mem0
- https://adr.github.io/madr/ (MADR 4.0.0)
- https://docs.cognee.ai/examples/contradiction-handling (opt-in pipeline stage, off by default)
- https://healthchecks.io/docs/monitoring_cron_jobs/
- https://apenwarr.ca/log/20181113-mtime-comparison-considered-harmful
- https://github.com/zilliztech/memsearch + https://github.com/thedotmack/claude-mem/issues/249

Blogs/forums (lower weight):
- https://dev.to/codingsimba/the-markdown-file-that-beat-a-50m-vector-database-…-3d14
- https://towardsdatascience.com/memweave-zero-infra-ai-agent-memory-…/ + levelup.gitconnected.com equivalent
- https://www.marknagelberg.com/what-i-learned-building-ai-agents-on-top-of-the-obsidian-cli/ (one-call-at-a-time CLI bottlenecks agent loops)
- https://tianpan.co/blog/2026-04-14/agent-memory-garbage-collection (truth-maintenance: negation/temporal-supersession/value-conflict/antonym checks)
- https://news.ycombinator.com/item?id=49508317 ("Agent memory as a file format")
- reddit r/AI_Agents statement-graph thread; r/ClaudeCode claude-mem thread (one reddit fetch failed — 400 after anti-bot workaround attempt; that angle is under-covered)
