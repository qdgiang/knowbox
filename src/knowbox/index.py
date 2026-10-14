"""knowbox index: the derived read models — FTS database (.kb.db), the
human-facing index.md, and the P8 freshness oracle that keeps them honest.

Pure functions of the notes corpus; rebuilt wholesale by `knowbox reindex`.
Paths late-bind (paths.DB / paths.INDEX inside functions).
"""
import hashlib
import os
import re
import sqlite3

import paths
from paths import ID_PREFIX_RE
from notes import all_notes, _aslist, _search_text_parts
from sysutil import _tmp_in_kb, _swap_into_place

# FTS schema single-source: column order here must match every DDL/INSERT
# below and every bm25-weight/snippet-ordinal derivation in kbsearch.
KB_COLS = ("id", "path", "title", "typ", "domain", "tags", "aliases",
           "triggers", "summary", "body")
# bm25 weights by column name; id/path unlisted → 0.0. Derived string in
# kbsearch keeps KB_COLS order: "0.0, 0.0, 10.0, 1.0, 1.0, 5.0, 8.0, 6.0,
# 4.0, 1.0".
BM25_WEIGHTS = {"title": 10.0, "typ": 1.0, "domain": 1.0, "tags": 5.0,
                "aliases": 8.0, "triggers": 6.0, "summary": 4.0, "body": 1.0}
KB_TRI_COLS = ("id", "path", "title", "body")

def _note_hashes():
    """{relpath: (sha1, nbytes)} for every .md file under notes/ — ALL of
    them, including superseded and malformed ones (drift detection must not
    depend on parse/index status). Content sha1, not mtime: git clone, rsync
    and touch all make mtime lie (P8 decision note)."""
    out = {}
    for fn in sorted(os.listdir(paths.NOTES)) if os.path.isdir(paths.NOTES) else []:
        if not fn.endswith(".md"):
            continue
        p = os.path.join(paths.NOTES, fn)
        try:
            with open(p, "rb") as f:
                data = f.read()
        except OSError:
            continue   # unreadable: absent here but in kb_meta → flags drift
        out["notes/" + fn] = (hashlib.sha1(data).hexdigest(), len(data))
    return out


def build_db():
    # build into a uniquely-named temp file, then atomic-swap: readers keep
    # the old inode, concurrent rebuilds never share a tmp path (T3/P7), and
    # a killed rebuild can never leave a corrupt .kb.db in place
    tmp = _tmp_in_kb(".kb.db.")
    try:
        c = sqlite3.connect(tmp)
        c.execute("CREATE VIRTUAL TABLE kb USING fts5(id UNINDEXED, path UNINDEXED,"
                  " " + ", ".join(KB_COLS[2:]) + ","
                  " tokenize=\"unicode61 tokenchars '-_.#/'\")")
        c.execute("CREATE VIRTUAL TABLE kb_tri USING fts5(id UNINDEXED, path UNINDEXED,"
                  " " + ", ".join(KB_TRI_COLS[2:]) + ", tokenize='trigram')")
        # P10a dual tokenizer: kb_d has the SAME columns but FTS5's default
        # unicode61 (no tokenchars) — punctuation SPLITS, so path compounds
        # like `~/.claude/settings.json` index as claude/settings/json and
        # become findable by their parts (the kb table glues them into one
        # token; see the P10a gotcha note). Both tables are queried and
        # merged by best rank in _fts_exec.
        c.execute("CREATE VIRTUAL TABLE kb_d USING fts5(id UNINDEXED, path UNINDEXED,"
                  " " + ", ".join(KB_COLS[2:]) + ")")
        for rel, meta, body in all_notes():
            if meta.get("status") == "superseded":
                continue  # dead knowledge out of the search path
            title, summary, tags, aliases, triggers = _search_text_parts(meta)
            row = (str(meta.get("id", "")), rel, title,
                   str(meta.get("type", "")), str(meta.get("domain", "")),
                   tags, aliases, triggers, summary, body)
            c.execute("INSERT INTO kb VALUES (?,?,?,?,?,?,?,?,?,?)", row)
            c.execute("INSERT INTO kb_d VALUES (?,?,?,?,?,?,?,?,?,?)", row)
            c.execute("INSERT INTO kb_tri VALUES (?,?,?,?)",
                      (row[0], row[1], row[2], row[9]))
        # kb_meta (P8): the freshness oracle — content sha1 + size for every
        # note file on disk, written at every rebuild. Lives INSIDE .kb.db so
        # the index and its freshness metadata can never disagree with each
        # other; both are derived, gitignored, and rebuilt together.
        c.execute("CREATE TABLE kb_meta (path TEXT PRIMARY KEY, sha1 TEXT,"
                  " bytes INTEGER NOT NULL)")
        c.executemany("INSERT INTO kb_meta VALUES (?,?,?)",
                      [(p, h, b) for p, (h, b) in _note_hashes().items()])
        c.commit()
        c.close()
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise
    _swap_into_place(tmp, paths.DB)


def _index_lines():
    """Pure generation of index.md's exact content (P8): the freshness check
    compares these lines byte-for-byte against the file on disk, so
    generation must live in ONE function shared by writer and checker.
    Returns (lines, n)."""
    notes = list(all_notes())
    # P11: successor map — a superseded entry's flag points at the note whose
    # supersedes: lists it, so the index routes dead entries to live ones
    succ = {}
    for _rel, m, _b in notes:
        for s in _aslist(m.get("supersedes")):
            succ[str(s)] = str(m.get("id", "?"))
    by_domain = {}
    n = 0
    for rel, meta, body in notes:
        if meta.get("type") == "session":
            continue  # session notes are continuity logs — reached via
            # `knowbox recent`, not via the topic-routing index
        dom = str(meta.get("domain", "misc")).lower() or "misc"
        by_domain.setdefault(dom, []).append((rel, meta))
        n += 1
    lines = [
        "# KB Index",
        "",
        "> Personal knowledge base. **Read this file first, then open only the",
        "> note(s) you need.** Conventions: SCHEMA.md. CLI: `knowbox search/show/new`.",
        "> Entries: `- [id · title](path): summary`. Generated — do not edit by hand.",
        "",
        f"**{n} notes** · domains: "
        + ", ".join(f"[{d}](#{d})" for d in sorted(by_domain)),
        "",
    ]
    for dom in sorted(by_domain):
        lines += [f"## {dom}", ""]
        for rel, meta in sorted(by_domain[dom], key=lambda x: str(x[1].get("id", ""))):
            st = meta.get("status", "")
            flag = ""
            if st == "superseded":
                s = succ.get(str(meta.get("id", "")))
                flag = f" ⚠SUPERSEDED→[[{s}]]" if s else " ⚠SUPERSEDED"
            lines.append(
                f"- [{meta.get('id','?')} · {meta.get('title','?')}]({rel}): "
                f"{meta.get('summary','—')}{flag}")
        lines.append("")
    # refs section: whole docs, browse by section map notes (type reference)
    refs = sorted(f for f in os.listdir(paths.REFS) if not f.startswith(".")) \
        if os.path.isdir(paths.REFS) else []
    if refs:
        lines += ["## refs/ — large docs (read via their section-map note)", ""]
        lines += [f"- `refs/{f}`" for f in refs]
        lines.append("")
    return lines, n


def build_index():
    lines, n = _index_lines()
    # atomic write with a UNIQUE tmp name (T3/P7): a concurrent reader
    # following "read index.md first" must never see a half-written index,
    # and two concurrent rebuilds must never share the tmp path
    tmp = _tmp_in_kb(".index.md.")
    try:
        # newline="\n" (kb-0117 D9): text-mode writes translate to CRLF on
        # Windows — index.md must stay LF on every platform (repo-wide LF is
        # also pinned by .gitattributes)
        with open(tmp, "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(lines))
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise
    _swap_into_place(tmp, paths.INDEX)
    return n


def _freshness_report():
    """P8: is every derived artifact in sync with the notes on disk?

    Compares (a) the content sha1 of every notes/*.md against the kb_meta
    table inside .kb.db — content hash, NOT mtime (git clone, rsync and
    touch all make mtime lie); (b) the ids listed in index.md against the
    ids on disk (orphans); (c) index.md's bytes against freshly generated
    lines (hand-edits). Never raises: a missing, corrupt or pre-P8-schema
    DB simply reports stale. Returns a dict with keys: fresh, db_missing,
    drifted (relpaths), orphans (ids), index_drift.
    """
    rep = {"fresh": False, "db_missing": False,
           "drifted": [], "orphans": [], "index_drift": False}
    disk = _note_hashes()
    if not os.path.exists(paths.DB):
        rep["db_missing"] = True
        return rep
    try:
        # C8 (kb-0117): plain connect, NOT a hand-built read-only `file:` URI —
        # on Windows that form depends on backslash tolerance and any ?/#/% in
        # the path breaks URI parsing SILENTLY into db_missing=True
        # (rebuild-on-every-search forever). Freshness only ever SELECTs; the
        # os.path.exists guard above already ran.
        c = sqlite3.connect(paths.DB)
        try:
            meta_rows = {r[0]: (r[1], r[2]) for r in
                         c.execute("SELECT path, sha1, bytes FROM kb_meta")}
        finally:
            c.close()
    except sqlite3.Error:
        rep["db_missing"] = True   # corrupt or pre-P8 schema → same remedy
        return rep
    rep["drifted"] = sorted(p for p in set(disk) | set(meta_rows)
                            if disk.get(p) != meta_rows.get(p))
    if not os.path.exists(paths.INDEX):
        rep["index_drift"] = True
        return rep
    try:
        idx = open(paths.INDEX, encoding="utf-8").read()
    except OSError:
        rep["index_drift"] = True
        return rep
    on_disk_ids = set()
    for p in disk:
        m = ID_PREFIX_RE.match(os.path.basename(p))
        if m:
            on_disk_ids.add(m.group(0))
    listed = re.findall(r"^- \[" + ID_PREFIX_RE.pattern + r" ·", idx, re.M)
    rep["orphans"] = sorted({i for i in listed if i not in on_disk_ids})
    if not rep["orphans"] and not rep["drifted"]:
        # byte-exact content check only when nothing else explains the diff
        rep["index_drift"] = idx != "\n".join(_index_lines()[0])
    rep["fresh"] = not (rep["drifted"] or rep["orphans"]
                        or rep["index_drift"])
    return rep
