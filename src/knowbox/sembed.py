"""knowbox sembed: the embed/vsearch commands — thin orchestration over
embed.py's store/search helpers.
"""
import hashlib
import json
import os
import sys

import paths
from embed import EMBED_MODEL, _embed_texts, _load_embed, _note_embed_text, _semantic
from notes import all_notes
from sysutil import _locked, _swap_into_place, _tmp_in_kb


def cmd_embed(args):
    """knowbox embed [--force] — build/update per-note embeddings (semantic recall)."""
    force = "--force" in args
    try:
        _embed_texts(["warmup"])  # fail fast on auth/network before scanning
    except Exception as e:
        sys.exit(f"knowbox embed: embedding endpoint unavailable ({e})")
    store = _load_embed()
    todo, seen = [], set()
    for rel, meta, body in all_notes():
        if meta.get("status") == "superseded":
            continue
        nid = str(meta.get("id"))
        seen.add(nid)
        text = _note_embed_text(meta, body)
        h = hashlib.sha1(text.encode()).hexdigest()
        cur = store.get(nid)
        if not force and cur and cur.get("h") == h:
            continue
        todo.append((nid, text, h))
    if not todo:
        print(f"embeddings up to date ({len(seen)} notes, model={EMBED_MODEL})")
        return
    updates = {}
    for i in range(0, len(todo), 16):
        batch = todo[i:i + 16]
        for (nid, text, h), vec in zip(batch, _embed_texts([t[1] for t in batch])):
            updates[nid] = {"h": h, "v": vec}
    # P7: the lock covers ONLY the merge+write — never the HTTP phase above
    # (a 90 s urlopen timeout inside the flock would wedge every kb writer).
    # Re-read the store under the lock and prune against CURRENT on-disk ids:
    # the scan snapshot may be stale by now (a concurrent `knowbox new` may have
    # landed mid-network), so pruning by the old `seen` set could delete a
    # live note's fresh embedding.
    with _locked():
        store = _load_embed()
        live = {str(m.get("id")) for _rel, m, _b in all_notes()
                if m.get("status") != "superseded"}
        for nid in [k for k in store if k not in live]:
            del store[nid]
        store.update(updates)
        tmp = _tmp_in_kb(".kb-embed.json.")
        try:
            with open(tmp, "w", encoding="utf-8") as f:
                json.dump({"model": EMBED_MODEL, "notes": store}, f)
        except BaseException:
            try:
                os.unlink(tmp)
            except OSError:
                pass
            raise
        _swap_into_place(tmp, paths.EMBED)
    print(f"embedded {len(todo)} note(s); {len(store)} total "
          f"(model={EMBED_MODEL})")


def cmd_vsearch(args):
    if not args:
        sys.exit("usage: knowbox vsearch QUERY...")
    if not os.path.exists(paths.EMBED):
        sys.exit("no embeddings yet — run `knowbox embed` first")
    if not _load_embed():
        sys.exit("embeddings are for a different model or unreadable — "
                 "run `knowbox embed --force`")
    rows = _semantic(" ".join(args))
    if not rows:
        print("no semantic matches.")
        return
    for score, nid, rel, title in rows:
        print(f"{nid}  {rel}  [{score:.3f}]\n    {title}\n")
