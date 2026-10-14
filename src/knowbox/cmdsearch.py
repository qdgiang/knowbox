"""knowbox cmdsearch: the search/expand commands — own-id short-circuit,
fusion on the partial path, the P10b zero-hit rescue chain, miss-log.
"""
import os
import re
import sys
from datetime import datetime

import paths
from paths import ID_PREFIX_RE
from _flags import _flags
from embed import (EXPAND_MAX_DEPTH, FUSE_COS_FLOOR, FUSE_TIMEOUT,
                   _fusion_pass, _semantic)
from kbsearch import (_build_graph, _fts_query, _print_neighborhood,
                      _print_search_expansion, _refs_grep, _typo_fix, search)
from notes import MalformedNote, find_path, parse_note


def cmd_search(args):
    # --expand is stripped MANUALLY: cmd_search never uses _flags (the query
    # is the raw arg join), so the flag must not leak into the query text.
    expand = "--expand" in args
    args = [a for a in args if a != "--expand"]
    if not args:
        sys.exit("usage: knowbox search QUERY... [--expand]")
    # P10a own-id short-circuit: `knowbox search kb-0042` must return kb-0042 —
    # the FTS id column is UNINDEXED, so before this a bare id either missed
    # or ranked notes that merely MENTION the id above the note itself.
    # Superseded notes are returned too (labeled) — dead knowledge stays
    # addressable by id. Unknown ids fall through to normal search (body
    # mentions can still hit).
    # own-id short-circuit uses a deliberately LENIENT id pattern (kb-\d+,
    # any width) — users type ids before they hit 4 digits; paths.ID_PREFIX_RE
    # is for corpus files, not user queries.
    if len(args) == 1 and re.match(r"^kb-\d+$", args[0].lower()):
        p = find_path(args[0].lower())
        if p == "AMBIGUOUS":
            sys.exit(2)  # hint printed; do NOT fall through to full-text
        if p:
            # forward slashes on every platform (kb-0117 E10): relpath emits
            # backslashes on Windows, breaking goldens and path conventions
            rel = os.path.relpath(p, paths.KB).replace(os.sep, "/")
            try:
                meta, body = parse_note(p)
            except MalformedNote:
                print(f"{args[0].lower()}  {rel} ⚠MALFORMED\n"
                      "    unterminated frontmatter — `knowbox show` prints it "
                      "raw\n")
                return
            label = (" ⚠SUPERSEDED" if meta.get("status") == "superseded"
                     else "")
            snip = " ".join(body.split())[:160]
            print(f"{meta.get('id', args[0].lower())}  {rel}{label}\n"
                  f"    {meta.get('title', '?')}\n    {snip}\n")
            if expand:  # own-id is the highest-value expansion case
                nid = str(meta.get("id") or args[0].lower())
                _print_search_expansion([nid], {nid})
            return
    query = " ".join(args)
    rows, exact = search(query)
    if rows:
        fused, hybrid, vec_only, recency = rows, False, set(), False
        if not exact:
            # OR/partial path only: lexical is uncertain, so consult the
            # vector channel (gated, fail-soft — see _fusion_pass). AND-path
            # hits never leave the machine.
            fused, hybrid, vec_only, recency = _fusion_pass(query, rows)
            print("(partial match — no note contains all query terms)")
            if hybrid:
                print("(hybrid re-rank: lexical + semantic + recency)"
                      if recency else
                      "(hybrid re-rank: lexical + semantic agreement)")
        for id_, path, title, snip, _score in fused:
            snip = " ".join(snip.split())
            tag = " (semantic-only)" if (path or id_) in vec_only else ""
            print(f"{id_}  {path}{tag}\n    {title}\n    {snip}\n")
        if expand:
            _print_search_expansion([r[0] for r in fused],
                                    {r[0] for r in fused})
        return
    # P10b fallback chain on a lexical zero-hit: typo → semantic → refs-grep
    # → miss-log. Typo first because a corrected lexical hit is exact
    # knowledge; semantic rescue is fuzzier; refs-grep only points at docs
    # the FTS index deliberately doesn't cover.
    toks = _fts_query(query)
    fixed, fixes = _typo_fix(toks)
    if fixes:
        rows, exact = search(" ".join(fixed))
        if rows:
            print("(typo-corrected: "
                  + ", ".join(f"'{a}'→'{b}'" for a, b in fixes) + ")")
            if not exact:
                print("(partial match — no note contains all query terms)")
            for id_, path, title, snip, _score in rows:
                snip = " ".join(snip.split())
                print(f"{id_}  {path}\n    {title}\n    {snip}\n")
            return
    sem = [r for r in _semantic(query, limit=3, timeout=FUSE_TIMEOUT)
           if r[0] >= FUSE_COS_FLOOR]  # rescue
    if sem:  # calibration (kb-0024): real hits ≥0.45, noise ≤0.21
        print("(BM25 missed — semantic matches; cos <0.45 is likely-absent "
              "territory, verify before relying on these:)")
        for score, nid, rel, title in sem:
            print(f"{nid}  {rel}  [cos {score:.3f}]\n    {title}\n")
        return
    refhits = _refs_grep(toks)
    if refhits:
        print("refs/ mentions (not FTS-indexed — find the section-map note "
              "via `knowbox search` on the doc title):")
        for h in refhits:
            print(f"    {h}")
        return
    try:  # miss counter: ground truth for gaps NO layer covers
        # newline="\n" (kb-0117 D9): keep the log LF cross-platform
        with open(os.path.join(paths.KB, ".search-misses.log"), "a",
                  encoding="utf-8", newline="\n") as f:
            f.write(f"{datetime.now().isoformat(timespec='seconds')}\t"
                    f"{query}\n")
    except OSError:
        pass
    print("no matches. try: a synonym | broader term | `knowbox list`")


def cmd_expand(args):
    """knowbox expand ID|SLUG [--depth 1|2] — author-written link neighbourhood."""
    opts = _flags(args)
    target = opts.get("_first")
    if not target:
        sys.exit("usage: knowbox expand ID|SLUG [--depth 1|2]")
    try:
        depth = int(opts.get("depth", "1"))
    except ValueError:
        sys.exit("knowbox expand: --depth must be 1 or 2")
    if not 1 <= depth <= EXPAND_MAX_DEPTH:
        sys.exit(f"knowbox expand: --depth must be 1 or {EXPAND_MAX_DEPTH}")
    p = find_path(target)
    if p == "AMBIGUOUS":
        sys.exit(2)
    m = ID_PREFIX_RE.match(os.path.basename(p)) if p else None
    if not p or not m:
        sys.exit(f"knowbox expand: no note for {target!r}")
    nid = m.group(1)
    try:
        adj, info = _build_graph()
    except Exception as e:  # fail-soft: clean message, never a traceback
        sys.exit(f"knowbox expand: could not build link graph ({e})")
    if nid not in info:
        sys.exit(f"knowbox expand: no note for {target!r}")
    if info[nid]["status"] == "superseded":
        print(f"(note: {nid} is superseded — showing its links; "
              "prefer the successor)")
    _print_neighborhood(nid, adj, info, depth)

