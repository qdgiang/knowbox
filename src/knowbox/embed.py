"""knowbox embed: embedding store, semantic search, RRF fusion, recency
weighting, and the fusion/search-graph tuning constants.

EMBED_URL/EMBED_MODEL are import-time reads (unchanged behavior); the KB root
late-binds via paths.EMBED inside functions.
"""
import json
import os

import paths
from paths import _env
from notes import (MalformedNote, _aslist, _days_since, _search_text_parts,
                   all_notes, parse_note)


# ---------- embeddings (semantic recall; AI-provider text-embedding-v4) ----

EMBED_URL = _env("EMBED_URL")   # required for embed ops — no vendor default
EMBED_MODEL = _env("EMBED_MODEL") or "text-embedding-v4"

# Gated RRF fusion (fixed k=60 — adaptive weighting was evaluated and
# refuted). The vector channel fires ONLY on the OR/partial lexical path,
# so the precision-first guarantee holds structurally:
# with w_vec=0.5 a vector-only doc at rank 1 scores 0.5/61 ≈ 0.0082, below
# EVERY lexical doc (worst rank 6 → 1/66 ≈ 0.0152) — vector-only rows append
# to the tail and can never interleave, while dual-channel agreement re-ranks
# (lex#3 + vec#1 ≈ 0.0241 > lex#1 alone ≈ 0.0164). cos floors per
# calibration (real hits ≥0.45, noise ≤0.21): ≥0.40 to participate at all,
# ≥0.45 for vector-only docs — the 0.40–0.45 "likely-absent" band surfaces
# only with independent lexical support.
RRF_K = 60
RRF_W_LEX = 1.0
RRF_W_VEC = 0.5
FUSE_COS_FLOOR = 0.40           # gate to enter fusion at all
FUSE_COS_FLOOR_VEC_ONLY = 0.45  # stricter gate for docs with no lexical support
FUSE_VEC_ONLY_CAP = 2
FUSE_TIMEOUT = 10               # interactive path: never hang on the 90s default

# Recency weighting (ES function_score decay + OpenClaw post-merge
# exponential): post-fusion multiplier on the RRF score, fusion path ONLY —
# AND/exact, typo, semantic-rescue and pure-BM25 rows never see it (Yahoo
# query-gated-freshness precedent). mult = max(FLOOR, 2 ** (-(age - OFFSET)
# / HALF_LIFE)) with age in days from `updated:` (fallback `created:`,
# cmd_stale precedent); missing/unparseable → neutral 1.0 (fail toward
# evergreen). Age clamped ≥ 0: future dates give exactly 1.0, never a boost
# (guards the ES decay-inversion pitfall). OFFSET = grace band where fresh
# notes are indistinguishable; FLOOR = an old-but-correct note is never
# buried. Applied BLOCKWISE (lexical rows before vector-only) so the
# structural "vector-only never outranks lexical" guarantee survives the
# multiplier — under a global sort a fresh vec-only row (1.0 × 0.5/61 ≈
# 0.0082) WOULD overtake a floored lex#2 (0.5 × 1/62 ≈ 0.0081). Kill switch:
# KNOWBOX_NO_RECENCY (KNOWBOX_NO_FUSION pattern). Params are an inferred
# starting point — recalibrate against miss-log soak data.
RECENCY_HALF_LIFE_DAYS = 90
RECENCY_OFFSET_DAYS = 30
RECENCY_FLOOR = 0.5

# Opt-in derived-graph expansion (IWE / Basic-Memory pattern): adjacency over
# AUTHOR-WRITTEN links only — no LLM extraction, no graph DB, no cache;
# rebuilt per invocation from all_notes(). Benchmarks: graph expansion hurts
# single-hop lookups and helps only multi-hop/relational queries → STRICTLY
# opt-in (`knowbox expand`, `knowbox search --expand`); default search output
# stays byte-identical (precision-first; goldens untouched by construction).
# Caps mirror Basic-Memory's depth-1 / max-per-level defaults.
EXPAND_MAX_DEPTH = 2            # `knowbox expand --depth` ceiling
EXPAND_LEVEL_CAP = 12           # `knowbox expand`: max neighbours per level
EXPAND_SEARCH_SEEDS = 3         # search --expand: top-N displayed hits seed it
EXPAND_SEARCH_PER_SEED = 4      # search --expand: max neighbours per seed
EXPAND_SEARCH_TOTAL = 8         # search --expand: max neighbour rows total

_EDGE_RANK = {"supersedes": 0, "related": 1, "mention": 2}  # fold precedence


def _embed_token():
    return os.environ.get("KNOWBOX_API_KEY", "").strip()


def _embed_call(payload, tok, timeout=90):
    import urllib.request
    req = urllib.request.Request(
        EMBED_URL, data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {tok}",
                 "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.load(r)
    except urllib.error.HTTPError as e:
        detail = e.read(300).decode(errors="replace").replace("\n", " ")
        raise RuntimeError(f"HTTP {e.code}: {detail}") from None


def _embed_texts(texts, timeout=90):
    if not EMBED_URL:
        raise RuntimeError("no embeddings endpoint (set KNOWBOX_EMBED_URL)")
    tok = _embed_token()
    if not tok:
        raise RuntimeError("no auth token (set KNOWBOX_API_KEY)")
    try:  # batch first
        data = _embed_call({"model": EMBED_MODEL, "input": texts}, tok,
                           timeout=timeout)
        items = sorted(data["data"], key=lambda d: d.get("index", 0))
        return [d["embedding"] for d in items]
    except RuntimeError:
        if len(texts) == 1:
            raise
        out = []  # proxy rejected the batch — one request per text
        for t in texts:
            d = _embed_call({"model": EMBED_MODEL, "input": t}, tok,
                            timeout=timeout)
            out.append(d["data"][0]["embedding"])
        return out


def _note_embed_text(meta, body):
    title, summary, tags, aliases, triggers = _search_text_parts(meta)
    parts = [title, summary, tags, aliases, triggers, body[:2000]]
    return "\n".join(p for p in parts if p)


def _cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = sum(x * x for x in a) ** 0.5
    nb = sum(y * y for y in b) ** 0.5
    return dot / (na * nb) if na and nb else 0.0


def _load_embed():
    if not os.path.exists(paths.EMBED):
        return {}
    try:
        with open(paths.EMBED, encoding="utf-8") as f:
            s = json.load(f)
        if s.get("model") == EMBED_MODEL:
            return s.get("notes", {})
    except (OSError, ValueError):
        pass
    return {}


def _semantic(query, limit=6, timeout=90):
    """Vector-similarity rescue for BM25 misses. Fail-soft: [] on any error."""
    try:
        store = _load_embed()
        if not store:
            return []
        qv = _embed_texts([query], timeout=timeout)[0]
        info = {}
        for rel, meta, _b in all_notes():
            if meta.get("status") == "superseded":
                continue
            info[str(meta.get("id"))] = (rel, str(meta.get("title", "")))
        scored = []
        for nid, ent in store.items():
            if isinstance(ent, dict) and "v" in ent and nid in info:
                scored.append((_cosine(qv, ent["v"]), nid))
        scored.sort(reverse=True)
        return [(s, nid) + info[nid] for s, nid in scored[:limit]]
    except Exception:
        return []


def _rrf_fuse(lex_rows, sem_rows, limit=6):
    """Fixed-k reciprocal rank fusion (see constants block for the design).

    Pure + total: empty sem_rows → lexical order unchanged.
    lex_rows: (id, path, title, snippet, bm25), rank-ordered — keyed by
    path-or-id, same convention as _fts_exec's dual-table merge.
    sem_rows: (cos, nid, rel, title), rank-ordered and ALREADY floor-filtered
    at FUSE_COS_FLOOR — keyed by rel (path) to match the lexical key space.
    score = RRF_W_LEX/(RRF_K+r_lex) + RRF_W_VEC/(RRF_K+r_vec), ranks 1-based.
    Vector-only docs (no lexical support) additionally need cos ≥
    FUSE_COS_FLOOR_VEC_ONLY and are capped at FUSE_VEC_ONLY_CAP; the weight
    arithmetic alone then keeps them below every lexical doc.
    Returns (display_rows, vec_only_keys): display rows are 5-tuples matching
    the cmd_search print loop (vector-only rows carry snippet=None for the
    caller to fill); slot 4 carries the fused RRF score — the print loop
    ignores it (labels-not-scores), recency weighting consumes it.
    vec_only_keys are path-or-id keys of tail-appended rows.
    Tie-break (-score, lex_rank_or_999, key) is deterministic.
    """
    scores, lex_rank, info = {}, {}, {}
    for r, row in enumerate(lex_rows, start=1):
        key = row[1] or row[0]
        lex_rank[key] = r
        scores[key] = scores.get(key, 0.0) + RRF_W_LEX / (RRF_K + r)
        info[key] = (row[0], row[1], row[2], row[3])
    vec_cos = {}
    for r, (cos, nid, rel, title) in enumerate(sem_rows, start=1):
        key = rel or nid
        vec_cos[key] = cos
        scores[key] = scores.get(key, 0.0) + RRF_W_VEC / (RRF_K + r)
        if key not in info:
            info[key] = (nid, rel, title, None)
    vec_only = sorted(((cos, key) for key, cos in vec_cos.items()
                       if key not in lex_rank
                       and cos >= FUSE_COS_FLOOR_VEC_ONLY),
                      key=lambda ck: (-ck[0], ck[1]))
    allowed = {key for _cos, key in vec_only[:FUSE_VEC_ONLY_CAP]}
    ranked = sorted(((k, s) for k, s in scores.items()
                     if k in lex_rank or k in allowed),
                    key=lambda ks: (-ks[1], lex_rank.get(ks[0], 999), ks[0]))
    out = []
    for key, s in ranked[:limit]:
        nid, rel, title, snip = info[key]
        out.append((nid, rel, title, snip, s))
    out_keys = {row[1] or row[0] for row in out}
    return out, allowed & out_keys


def _recency_multiplier(age_days):
    """Pure: recency multiplier for a note age in days.

    None (missing/unparseable date) → neutral 1.0, fail toward evergreen;
    negative ages clamp to 0 → 1.0 (future dates never boost — the ES
    decay-inversion pitfall); age ≤ RECENCY_OFFSET_DAYS →
    exactly 1.0; then exponential half-life decay bounded at RECENCY_FLOOR.
    Never printed anywhere (labels-not-scores convention).
    """
    if age_days is None:
        return 1.0
    age = max(0, age_days)
    if age <= RECENCY_OFFSET_DAYS:
        return 1.0
    return max(RECENCY_FLOOR,
               2.0 ** (-(age - RECENCY_OFFSET_DAYS) / RECENCY_HALF_LIFE_DAYS))


def _apply_recency(rows, ages, vec_only=frozenset()):
    """Pure: scale each fused row's score (slot 4) by its recency multiplier
    and re-sort BLOCKWISE — the key's first component pins vector-only rows
    below every lexical row, so the structural guarantee survives the
    multiplier without arithmetic (a global sort would let a fresh vec-only
    row at 1.0×0.5/61 ≈ 0.0082 overtake a floored lex#2 at 0.5×1/62 ≈ 0.0081).
    Stable sort → uniform multipliers provably preserve the incoming RRF
    order. ages parallels rows (age in days, or None → neutral).
    Returns (rows, changed); changed is True only when the displayed id
    order actually moved.
    """
    out = [(nid, rel, title, snip, score * _recency_multiplier(age))
           for (nid, rel, title, snip, score), age in zip(rows, ages)]
    before = [r[0] for r in out]
    out.sort(key=lambda r: ((r[1] or r[0]) in vec_only, -r[4]))
    return out, [r[0] for r in out] != before


def _fusion_pass(query, rows, limit=6):
    """Gated hybrid fusion — called ONLY on the OR/partial lexical path, so
    the AND path stays offline/instant/golden-stable (precision-first;
    analog of kbx's strong-signal fast path). Fused rows are then recency-
    weighted (default-on; KNOWBOX_NO_RECENCY disables — see RECENCY_* constants).
    Returns (rows, changed, vec_only_keys, recency_changed); ANY failure
    (kill-switch, no/mismatched embed store, network, timeout, bug in this
    code) degrades to the input rows unchanged.
    """
    try:
        if _env("NO_FUSION"):
            return rows, False, set(), False
        if not _load_embed():  # no store / model mismatch → zero HTTP, inert
            return rows, False, set(), False
        sem = [r for r in _semantic(query, limit=limit, timeout=FUSE_TIMEOUT)
               if r[0] >= FUSE_COS_FLOOR]
        if not sem:
            return rows, False, set(), False
        fused, vec_only = _rrf_fuse(rows, sem, limit=limit)
        # One parse_note per row, shared between vector-only snippet
        # synthesis and the recency age (both read frontmatter). Per-row
        # fail-soft: an unreadable row gets the title as snippet and a
        # neutral multiplier (age None). KNOWBOX_NO_RECENCY restores the pre-
        # recency I/O profile exactly (parse only when snip is None).
        recency_on = not _env("NO_RECENCY")
        ages = [None] * len(fused)
        for i, (nid, rel, title, snip, score) in enumerate(fused):
            meta = body = None
            if snip is None or recency_on:
                try:
                    meta, body = parse_note(os.path.join(paths.KB, rel))
                except (MalformedNote, OSError):
                    meta = None
            if snip is None:
                if meta is not None:
                    snip = (str(meta.get("summary") or "").strip()
                            or " ".join(body.split())[:160] or title)
                else:
                    snip = title
                fused[i] = (nid, rel, title, snip, score)
            if recency_on and meta is not None:
                ages[i] = _days_since(
                    str(meta.get("updated") or meta.get("created") or ""))
        recency_changed = False
        if recency_on:
            fused, recency_changed = _apply_recency(fused, ages, vec_only)
        # changed = FINAL display order vs lexical input: a recency-only
        # reorder must still fire the hybrid label.
        changed = [r[0] for r in fused] != [r[0] for r in rows]
        return fused, changed, vec_only, recency_changed
    except Exception:
        return rows, False, set(), False
