"""knowbox kbsearch: BM25 lexical search (dual-tokenizer FTS5 + trigram
fallback), zero-hit rescue layers (typo fix, refs grep), and the derived
author-written link graph with its expand printers.

Search rows are 5-tuples (id, path, title, snippet, bm25) shared with
embed.py's fusion; tuning constants (EXPAND_*, _EDGE_RANK) live in embed.py
(the constants block) and are imported here.
"""
import os
import re
import sqlite3
import sys

import paths
from paths import ID_PREFIX_RE, ID_RE, WIKILINK_RE
from embed import (_EDGE_RANK, EXPAND_LEVEL_CAP, EXPAND_SEARCH_PER_SEED,
                   EXPAND_SEARCH_SEEDS, EXPAND_SEARCH_TOTAL)
from index import (BM25_WEIGHTS, KB_COLS, KB_TRI_COLS, _freshness_report,
                   build_db, build_index)
from notes import _aslist, _search_text_parts, all_notes


def _fts_query(text):
    stop = {"a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "did",
            "do", "does", "for", "from", "has", "have", "how", "i", "if", "in",
            "is", "it", "its", "just", "me", "my", "no", "not", "of", "on", "or",
            "so", "than", "that", "the", "then", "this", "to", "too", "very",
            "was", "we", "were", "when", "will", "with", "won", "t", "you", "your",
            "something", "anything", "thing", "stuff", "way", "kind", "sort",
            "quite", "really", "actually", "maybe", "please",
            "need", "needs", "needed", "want", "wants", "wanted"}
    toks = [t for t in re.findall(r"[\w'./#-]+", text.lower())
            if t not in stop or "/" in t or "." in t]  # keep path-ish tokens
    if not toks:  # query was pure stopwords — fall back to unfiltered
        toks = re.findall(r"[\w'./#-]+", text.lower())
    # prefix expansion only for tokens >= 3 chars (short prefixes like "co*"
    # explode into unrelated matches once diacritics are stripped) — applied
    # by search() when it builds the quoted FTS5 terms.
    return toks


def search(text, limit=6):
    """BM25 search. AND-first, OR-fallback (announced). Returns (rows, exact),
    where exact=False means no note matched all query terms."""
    # P8 (D6): never serve a stale index silently. Content-sha1 freshness
    # check (~ms at this corpus size); on drift, rebuild once per invocation.
    # Deliberately UNLOCKED: readers never take the flock, and unique-tmp +
    # atomic-replace make concurrent rebuilds last-writer-wins, never corrupt.
    # The rebuild does NOT commit — a resulting index.md change rides the next
    # write command (or an explicit `knowbox reindex`).
    if not _freshness_report()["fresh"]:
        build_db()
        build_index()
        print("(index was stale — rebuilt from current notes)", file=sys.stderr)
    toks = _fts_query(text)
    if not toks:
        return [], True
    # FTS5 prefix syntax is `"term"*` — the star must sit OUTSIDE the quotes
    quoted = ['"%s"*' % t.replace('"', '""') if len(t) >= 3
              else '"%s"' % t.replace('"', '""') for t in toks]
    for query, exact, tri in ((" AND ".join(quoted), True, False),
                              (" OR ".join(quoted), False, True)):
        rows = _fts_run(query, toks, limit, trigram_fallback=tri)
        if rows:
            return rows, exact
    return [], True


def _fts_run(q, toks, limit, trigram_fallback=True):
    try:
        c = sqlite3.connect(paths.DB)
    except sqlite3.Error:
        return []
    out = []
    try:
        out = _fts_exec(c, q, toks, limit, trigram_fallback)
    except sqlite3.Error:
        # corrupt/stale index (DatabaseError is OperationalError's parent):
        # rebuild once and retry before giving up
        c.close()
        try:
            build_db()
            c = sqlite3.connect(paths.DB)
            out = _fts_exec(c, q, toks, limit, trigram_fallback)
        except sqlite3.Error as e:
            print(f"knowbox: FTS error ({e}) — run 'knowbox reindex'", file=sys.stderr)
            return []
    c.close()
    return out


def _fts_exec(c, q, toks, limit, trigram_fallback=True):
    out = []
    # weights derive from index.KB_COLS/BM25_WEIGHTS — a column add/relayout
    # can't silently desync weights from DDL (review finding A).
    weights = ", ".join(str(BM25_WEIGHTS.get(cn, 0.0)) for cn in KB_COLS)
    # P10a dual tokenizer: query BOTH tables — kb (tokenchars '-_.#/':
    # path-ish tokens stay whole) and kb_d (default unicode61: compounds
    # split) — then merge by id keeping each id's BEST rank. bm25 scores are
    # not comparable across tokenizers, so (rank, table order) is the merge
    # key: deterministic, kb wins ties. A pre-P10 db without kb_d degrades
    # gracefully to kb-only.
    best = {}   # path-or-id key → (rank, table_order, row)
    for ti, table in enumerate(("kb", "kb_d")):
        try:
            rows = list(c.execute(
                "SELECT id, path, title, snippet(%s, %d, '>>>', '<<<', '…',"
                " 12), bm25(%s, %s) FROM %s WHERE %s MATCH ?"
                " ORDER BY bm25(%s, %s) LIMIT ?"
                % (table, KB_COLS.index("body"), table, weights, table,
                   table, table, weights),
                (q, limit)))
        except sqlite3.Error:
            if ti == 0:
                raise            # primary table broken → outer rebuild path
            continue             # kb_d absent (pre-P10 db): kb results stand
        for rank, r in enumerate(rows):
            key = r[1] or r[0]
            if key not in best or (rank, ti) < (best[key][0], best[key][1]):
                best[key] = (rank, ti, r)
    out = [e[2] for e in sorted(best.values(), key=lambda x: (x[0], x[1]))]
    out = out[:limit]
    # trigram substring fallback: only on the OR path, after the lexical index
    # has fully missed (substring matching is noisy — never an "exact" answer)
    if not out and toks and trigram_fallback:
        tq = " OR ".join('"%s"' % t.replace('"', '""') for t in toks if len(t) >= 3)
        if tq:
            for r in c.execute(
                    "SELECT id, path, title, snippet(kb_tri, %d, '>>>',"
                    " '<<<', '…', 12), bm25(kb_tri) FROM kb_tri"
                    " WHERE kb_tri MATCH ? ORDER BY bm25(kb_tri) LIMIT ?"
                    % KB_TRI_COLS.index("body"),
                    (tq, limit)):
                out.append(r)
    return out


# ---------- P10b: typo expansion + refs-grep (zero-hit rescue layers) ----------

def _vocab():
    """Set of lowercase tokens present in live notes — the typo-correction
    dictionary. Built in-memory per zero-hit search (~ms at this corpus
    size). Includes both compound tokens (as the kb tokenizer sees them) and
    punctuation-split parts (as kb_d sees them), so a typo of either form
    can be corrected."""
    vocab = set()
    for _rel, meta, body in all_notes():
        text = " ".join([*_search_text_parts(meta), body])
        for tok in re.findall(r"[\w'./#-]+", text.lower()):
            vocab.add(tok)
            vocab.update(p for p in re.split(r"[^a-z0-9]+", tok) if p)
    vocab.discard("")
    return vocab


def _edits1(tok):
    """All edit-distance-1 variants of tok: deletions, substitutions,
    transpositions, insertions (lowercase alnum alphabet)."""
    letters = "abcdefghijklmnopqrstuvwxyz0123456789"
    splits = [(tok[:i], tok[i:]) for i in range(len(tok) + 1)]
    out = set()
    for a, b in splits:
        if b:
            out.add(a + b[1:])                              # deletion
            out.update(a + ch + b[1:] for ch in letters)    # substitution
        if len(b) > 1:
            out.add(a + b[1] + b[0] + b[2:])                # transposition
        out.update(a + ch + b for ch in letters)            # insertion
    out.discard(tok)
    return out


def _typo_fix(toks):
    """(corrected_toks, [(wrong, right), …]) — for each query token ≥4 chars
    that is NOT in the index vocabulary, look for edit-distance-1 vocabulary
    words. EXACTLY ONE candidate → substitute; zero or several → leave the
    token alone (conservative: an ambiguous 'correction' is worse than an
    honest miss — e.g. 'gruby' has both 'grub' and 'grubby' at distance 1)."""
    vocab = _vocab()
    fixes, out = [], []
    for t in toks:
        if len(t) >= 4 and t not in vocab:
            hits = sorted(_edits1(t) & vocab)
            if len(hits) == 1:
                out.append(hits[0])
                fixes.append((t, hits[0]))
                continue
        out.append(t)
    return out, fixes


def _refs_grep(toks):
    """Last resort before the miss-log (P10b): refs/ is deliberately OUTSIDE
    the FTS index (large docs live whole, reached via their section-map
    note) — grep filenames, H1/H2 headings and the lead of each doc so a
    total miss can still point at a reference. Returns display lines."""
    toks = [t for t in toks if len(t) >= 3]
    out = []
    if not toks or not os.path.isdir(paths.REFS):
        return out
    for fn in sorted(os.listdir(paths.REFS)):
        if not fn.endswith(".md") or fn.startswith("."):
            continue
        try:
            with open(os.path.join(paths.REFS, fn), encoding="utf-8",
                      errors="replace") as f:
                text = f.read()
        except OSError:
            continue
        headings = [l.lstrip("#").strip() for l in text.splitlines()
                    if l.startswith("# ") or l.startswith("## ")]
        hit_heads = [h for h in headings if any(t in h.lower() for t in toks)]
        if hit_heads:
            out.append(f"refs/{fn}: " + " / ".join(hit_heads[:3]))
        elif any(t in fn.lower() for t in toks) or \
                any(t in text[:2048].lower() for t in toks):
            out.append(f"refs/{fn}: (filename or lead section)")
    return out


# ---------- derived link graph (kb-0110 gap #4; opt-in expansion) ----------

def _build_graph():
    """In-memory adjacency over author-written links (kb-0110 gap #4).
    Returns (adj, info):
        adj[id]  = {"out": {nb: rel}, "in": {nb: rel}},
                   rel in {related, supersedes, mention}
        info[id] = {"path": relpath, "title": str, "status": str}
    Edges: frontmatter related:/supersedes: + body [[kb-XXXX]] wikilinks
    (same regex as lint). A pair asserted through several channels folds to
    the strongest rel (_EDGE_RANK) — supersede banners never duplicate a
    related: edge. Ids outside the corpus are dropped (lint ERRORs on them);
    malformed notes are never nodes (all_notes skips them). Filesystem only,
    zero HTTP; rebuilt per invocation — derived and disposable by design.
    """
    nodes = []
    for rel, meta, body in all_notes():
        nid = str(meta.get("id") or "").strip()
        if not ID_RE.match(nid):  # id-less/mismatched file: fall back to name
            m = ID_PREFIX_RE.match(os.path.basename(rel))
            nid = m.group(1) if m else ""
        if nid:
            nodes.append((nid, rel, meta, body))
    info = {nid: {"path": rel, "title": str(meta.get("title") or nid),
                  "status": str(meta.get("status") or "")}
            for nid, rel, meta, _b in nodes}
    adj = {nid: {"out": {}, "in": {}} for nid in info}
    known = set(info)

    def _fold(d, k, rel_):
        if k not in d or _EDGE_RANK.get(rel_, 9) < _EDGE_RANK.get(d[k], 9):
            d[k] = rel_

    for nid, _rel, meta, body in nodes:
        o = adj[nid]["out"]
        for chan in ("related", "supersedes"):
            for t in {str(x).strip() for x in _aslist(meta.get(chan))} & known:
                if t != nid:
                    _fold(o, t, chan)
                    _fold(adj[t]["in"], nid, chan)
        for t in (set(WIKILINK_RE.findall(body)) & known) - {nid}:
            _fold(o, t, "mention")
            _fold(adj[t]["in"], nid, "mention")
    return adj, info


def _fmt_nb(rel_, nb, info, via=None):
    tag = " (superseded)" if info[nb]["status"] == "superseded" else ""
    v = f" (via {via})" if via else ""
    return f"    {rel_:<13} {nb}  {info[nb]['title']}{tag}{v}"


def _print_neighborhood(nid, adj, info, depth):
    """`knowbox expand` renderer: seed header, links-to / backlinks sections, then
    (depth 2) frontier neighbours that are new. Deterministic: sorted ids,
    out-before-in; per-level cap EXPAND_LEVEL_CAP."""
    print(f"{nid} — {info[nid]['title']}")
    print(f"  {info[nid]['path']}")
    node = adj[nid]
    outs = sorted(node["out"].items())          # [(nb, rel)]
    ins = [(nb, "superseded-by" if r == "supersedes" else r)
           for nb, r in sorted(node["in"].items())]
    if not outs and not ins:
        print("\n(no author-written links)")
        return
    kept_out, kept_in = outs[:EXPAND_LEVEL_CAP], ins[:EXPAND_LEVEL_CAP]
    if kept_out:
        print("\n  links to:")
        for nb, r in kept_out:
            print(_fmt_nb(r, nb, info))
    if kept_in:
        print("\n  linked from (backlinks):")
        for nb, r in kept_in:
            print(_fmt_nb(r, nb, info))
    if depth >= 2:
        seen = {nid} | set(node["out"]) | set(node["in"])
        rows2 = []
        for f in [nb for nb, _ in kept_out] + [nb for nb, _ in kept_in]:
            fn = adj.get(f)
            if not fn:
                continue
            for nb, r in sorted(fn["out"].items()):
                if nb not in seen:
                    rows2.append((nb, r, f))
                    seen.add(nb)
            for nb, r in sorted(fn["in"].items()):
                if nb not in seen:
                    rows2.append((nb, "superseded-by" if r == "supersedes"
                                  else r, f))
                    seen.add(nb)
        if rows2:
            print("\n  depth 2 (new neighbours, excluding the above):")
            for nb, r, via in rows2[:EXPAND_LEVEL_CAP]:
                print(_fmt_nb(r, nb, info, via=via))


def _print_search_expansion(seed_ids, displayed):
    """Opt-in graph block for `knowbox search --expand`: depth-1 author-written
    link neighbours of the top displayed hits, deduped against those hits and
    across seeds (first seed wins), provenance-labelled, hard-capped. Rows are
    INDENTED so search_ids/goldens can never match them. Fail-soft: ANY error
    skips the block and search still exits 0. Filesystem only, zero HTTP."""
    try:
        seeds = [s for s in seed_ids[:EXPAND_SEARCH_SEEDS] if s]
        if not seeds:
            return
        adj, info = _build_graph()
        rows, seen = [], set(displayed) | set(seeds)
        for s in seeds:
            node = adj.get(s)
            if not node:
                continue
            n = 0
            for direction in ("out", "in"):
                for nb in sorted(node[direction]):
                    if len(rows) >= EXPAND_SEARCH_TOTAL:
                        break
                    if nb in seen or n >= EXPAND_SEARCH_PER_SEED:
                        continue
                    rel_ = node[direction][nb]
                    if direction == "out":
                        label = f"via {rel_}: {s}"
                    elif rel_ == "supersedes":
                        label = f"supersedes: {s}"
                    else:
                        label = f"← backlink: {s}"
                    rows.append((nb, label))
                    seen.add(nb)
                    n += 1
                if len(rows) >= EXPAND_SEARCH_TOTAL:
                    break
            if len(rows) >= EXPAND_SEARCH_TOTAL:
                break
        if not rows:
            return
        print("(graph neighbours of top hits — depth 1, opt-in --expand:)")
        for nb, label in rows:
            tag = " (superseded)" if info[nb]["status"] == "superseded" else ""
            print(f"    {nb}  {info[nb]['path']}{tag}")
            print(f"        {label} — {info[nb]['title']}")
    except Exception:
        pass
