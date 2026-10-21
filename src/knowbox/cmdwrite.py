"""knowbox cmdwrite: the mutating note commands — new/touch/verify/supersede
(every one locks the full write span; supersede is two-phase with rollback),
plus the read-side show/list/recent.
"""
import os
import re
import shlex
import subprocess
import sys
from datetime import date

import paths
from _flags import _flags
from index import build_db, build_index
from notes import (MalformedNote, _aslist, _asstr, _rewrite_frontmatter,
                   _rewrite_fm_bytes, all_notes, append_log, detect_project,
                   find_path, git_commit, next_id, parse_note)
from paths import ENUMS, ID_RE
from sysutil import IS_NT, _atomic_write_bytes, _locked


def cmd_show(args):
    if not args:
        sys.exit("usage: knowbox show ID|SLUG")
    p = find_path(args[0])
    if p == "AMBIGUOUS":
        sys.exit(2)
    if not p:
        sys.exit(f"not found: {args[0]}")
    # deliberately RAW (never parse_note, P4/T7): a malformed note —
    # unterminated frontmatter, BOM, CRLF — must stay readable so it can be
    # diagnosed and fixed. The edited-since-verify flag below parses
    # DEFENSIVELY on top of that: MalformedNote → no flag, raw dump only.
    try:
        meta, _b = parse_note(p)
        ver, upd = (str(meta.get(k) or "") for k in ("verified", "updated"))
        if ver and upd > ver:
            print(f"knowbox show: WARNING edited after last verification "
                  f"(updated {upd} > verified {ver}) — content changed since "
                  f"it was last confirmed; re-verify before relying on it",
                  file=sys.stderr)
    except MalformedNote:
        pass
    print(open(p, encoding="utf-8").read())


def cmd_list(args):
    dom = typ = None
    for i, a in enumerate(args):
        if a == "--domain" and i + 1 < len(args):
            dom = args[i + 1]
        if a == "--type" and i + 1 < len(args):
            typ = args[i + 1]
    for rel, meta, _b in all_notes():
        if dom and str(meta.get("domain", "")).lower() != dom.lower():
            continue
        if typ and meta.get("type") != typ:
            continue
        print(f"{meta.get('id','?'):8} [{meta.get('type','?'):9}] "
              f"{str(meta.get('domain','?')):12} {meta.get('title','?')}")


def cmd_recent(args):
    """knowbox recent [--limit N] [--here] [--project P] — newest session notes."""
    opts = _flags(args)
    try:
        limit = int(opts.get("limit", "8") or 8)
    except ValueError:
        sys.exit("knowbox recent: --limit must be an integer")
    proj = opts.get("project") or (detect_project() if "here" in opts else None)
    rows = []
    for rel, meta, _b in all_notes():
        if meta.get("type") != "session":
            continue
        p = str(meta.get("project", "?"))
        if proj and p != proj:
            continue
        rows.append((str(meta.get("created", "")), str(meta.get("id", "?")),
                     p, _asstr(meta.get("title")), _asstr(meta.get("summary")),
                     rel))
    if not rows:
        scope = f" for project {proj!r}" if proj else ""
        print(f"no session notes{scope} yet. Save one on request: "
              "knowbox new --type session --domain sessions "
              "--title \"<date> <topic>\" (template: SCHEMA.md 'Session notes')")
        return
    rows.sort(key=lambda r: (r[0], r[1]), reverse=True)
    scope = f" (project: {proj})" if proj else " (all projects)"
    print(f"knowbox recent — newest session notes first{scope}")
    for created, nid, p, title, summary, rel in rows[:limit]:
        print(f"{nid}  {created}  [{p}]  {title}")
        print(f"    {summary}\n    {rel}")


def cmd_new(args):
    opts = _flags(args)
    missing = [f for f in ("type", "domain", "title") if f not in opts]
    if missing:
        sys.exit(f"knowbox new needs: {missing}")
    for f in ("type", "status", "confidence"):
        if opts.get(f) and opts[f] not in ENUMS[f]:
            sys.exit(f"knowbox new: {f}={opts[f]!r} not in {sorted(ENUMS[f])}")
    with _locked():  # P7: whole write span (id, note, rebuilds, log, commit)
        nid = next_id()
        slug = re.sub(r"[^a-z0-9]+", "-", opts["title"].lower()).strip("-")[:60]
        fn = f"{nid}-{slug}.md"
        today = date.today().isoformat()
        triggers = [t.strip() for t in opts.get("triggers", "").split("|")
                    if t.strip()]
        fm = [
            "---",
            f"id: {nid}",
            f"title: \"{opts['title']}\"",
            f"type: {opts['type']}",
            f"domain: {opts['domain'].lower()}",
        ]
        if opts["type"] == "session":
            fm.append(f"project: {opts.get('project') or detect_project()}")
        fm += [
            f"tags: [{opts.get('tags', '')}]",
            f"aliases: [{opts.get('aliases', '')}]",
        ]
        if triggers:
            fm.append("triggers:")
            fm += [f"  - \"{t}\"" for t in triggers]
        fm += [
            f"status: {opts.get('status', 'active')}",
            f"confidence: {opts.get('confidence', 'medium')}",
            f"created: {today}",
            f"updated: {today}",
            "supersedes: []",
            "related: []",
            f"source: \"{opts.get('source', '')}\"",
            f"summary: \"{opts.get('summary', 'TODO: what question does this answer?')}\"",
            "---",
            "",
        ]
        if "body" in opts:
            body = opts["body"]
        elif opts.get("body-stdin"):
            body = sys.stdin.read()
        else:
            body = ("FIRST SENTENCE = the load-bearing fact.\n\n"
                    "Details, commands, evidence.\n\n"
                    "## Related\n- [[kb-xxxx]] — why\n")
        path = os.path.join(paths.NOTES, fn)
        # newline="\n": notes are LF everywhere; _rewrite_fm_bytes
        # preserves a note's existing endings, so starting LF keeps the corpus
        # uniform even when created on Windows
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(fm) + body.rstrip("\n") + "\n")
        interactive = ("body" not in opts and not opts.get("body-stdin")
                       and "--no-edit" not in args and sys.stdin.isatty())
        if not interactive:
            build_db()
            build_index()
            append_log("create", nid, opts["title"])
            git_commit(f"knowbox: create {nid} — {opts['title']}")
    if interactive:
        # $EDITOR may take minutes — the flock must NOT be held across it
        # (P7). The note file already exists with its id, so a concurrent
        # `knowbox new` cannot collide; re-lock for the derived rebuilds only.
        try:  # $EDITOR may contain arguments ("vim -u NONE")
            # POSIX shlex eats backslashes — an EDITOR like
            # C:\Program Files\... would be mangled; and vi doesn't exist on
            # native Windows (notepad does). The except below still catches a
            # missing editor with the edit-by-hand fallback.
            ed = os.environ.get("EDITOR", "notepad" if IS_NT else "vi")
            subprocess.run(shlex.split(ed, posix=not IS_NT) + [path])
        except (FileNotFoundError, ValueError) as e:
            print(f"knowbox: could not launch $EDITOR ({e}) — edit {path} by hand",
                  file=sys.stderr)
        with _locked():
            build_db()
            build_index()
            append_log("create", nid, opts["title"])
            git_commit(f"knowbox: create {nid} — {opts['title']}")
    print(path)


def cmd_touch(args):
    opts = _flags(args)
    if "supersede" in opts:
        sys.exit("deprecated: use `knowbox supersede OLD --with NEW` "
                 "(atomic, links both notes)")
    if not args or args[0].startswith("--"):
        sys.exit("usage: knowbox touch ID")
    with _locked():  # P7: whole write span under the lock (was: none)
        p = find_path(args[0])
        if p == "AMBIGUOUS":
            sys.exit(2)
        if not p:
            sys.exit(f"not found: {args[0]}")
        # guarded rewrite (P4/T5): frontmatter slice only. A note whose fm
        # lacks `updated:` is REFUSED (exit≠0, no write) — the old whole-file
        # regex would have rewritten a `updated:` line inside a body fence.
        _rewrite_frontmatter(p, {"updated": date.today().isoformat()})
        build_db()
        build_index()
        append_log("update", os.path.basename(p).split("-")[0] + "-"
                   + os.path.basename(p).split("-")[1]
                   if "-" in os.path.basename(p)
                   else os.path.basename(p), "touched")
        git_commit(f"knowbox: touch {args[0]}")
    print(p)


def cmd_reindex(_args):
    with _locked():  # P7: rebuild + log + commit are one critical section
        old_index = None
        if os.path.exists(paths.INDEX):
            try:
                with open(paths.INDEX, encoding="utf-8") as f:
                    old_index = f.read()
            except OSError:
                pass
        build_db()
        n = build_index()
        # T10/P9: skip the log entry + commit when NOTHING changed — but a
        # dirty staged path (e.g. a hand-edited SCHEMA.md) must still commit
        # even when the regenerated index is byte-identical. The CLI itself
        # ships separately (knowbox repo push + uv tool reinstall) and no
        # longer rides these commits.
        new_index = None
        try:
            with open(paths.INDEX, encoding="utf-8") as f:
                new_index = f.read()
        except OSError:
            pass
        staged = paths.TRACKED_PATHS
        # git missing → clean message, and a FAILED status must
        # count as dirty — the old unchecked rc read a non-git tree as clean
        # and silently skipped the commit
        try:
            r = subprocess.run(
                ["git", "-C", paths.KB, "status", "--porcelain", "--", *staged],
                capture_output=True, text=True,
                encoding="utf-8", errors="replace")
            dirty = (r.stdout.strip() if r.returncode == 0
                     else "git-status-failed")
        except FileNotFoundError:
            print("knowbox: git not found — treating tree as dirty",
                  file=sys.stderr)
            dirty = "git-missing"
        if old_index == new_index and not dirty:
            print("reindex: no changes")
            return
        append_log("reindex", "—", f"{n} notes")
        git_commit("knowbox: reindex")
    print(f"index.md + FTS rebuilt from {n} notes")


def cmd_next_id(_args):
    print(next_id())


def cmd_verify(args):
    """knowbox verify ID... [--evidence "…"] — record a re-verification with evidence."""
    opts = _flags(args)
    ids = opts.get("_pos", [])
    if not ids:
        sys.exit("usage: knowbox verify ID... [--evidence \"one-line evidence\"]")
    ev = opts.get("evidence", "re-verified")
    touched = []
    with _locked():  # P7: whole batch (fm rewrites + rebuilds + commit) locked
        for nid in ids:
            p = find_path(nid)
            if p == "AMBIGUOUS":
                continue  # hint already printed; no contradictory "not found"
            if not p:
                print(f"not found: {nid}")
                continue
            try:
                meta, _b = parse_note(p)
            except MalformedNote as e:
                print(f"skipping {nid}: {e}")
                continue
            today = date.today().isoformat()
            if str(meta.get("status")) == "superseded":
                print(f"skipping {nid}: superseded notes are not verified")
                continue
            if "status" not in meta or "updated" not in meta:
                print(f"skipping {nid}: frontmatter missing status:/updated: — "
                      "fix the note first")
                continue
            # guarded rewrite (P4): fm slice only; `verified:` is inserted
            # right after `updated:` when absent — never after a body line.
            _rewrite_frontmatter(p, {"updated": today, "status": "verified",
                                     "verified": today},
                                 insert_after={"verified": "updated"})
            append_log("verify", nid, ev)
            touched.append(nid)
        if touched:
            build_db()
            build_index()
            git_commit(f"knowbox: verify {', '.join(touched)} — {ev}")
    if touched:
        print(f"verified: {', '.join(touched)} (pushed if remote exists)")


def cmd_supersede(args):
    """knowbox supersede OLD --with NEW — atomic supersede, both notes linked."""
    opts = _flags(args)
    pos = opts.get("_pos", [])
    if not pos or "with" not in opts:
        sys.exit("usage: knowbox supersede OLD --with NEW")
    with _locked():  # P7: whole two-phase supersede under the lock
        po, pn = find_path(pos[0]), find_path(opts["with"])
        if po == "AMBIGUOUS" or pn == "AMBIGUOUS":
            sys.exit(2)
        if not po:
            sys.exit(f"not found: {pos[0]}")
        if not pn:
            sys.exit(f"not found: {opts['with']}")
        if os.path.realpath(po) == os.path.realpath(pn):
            sys.exit("OLD and NEW resolve to the same note")
        try:
            mo, _bo = parse_note(po)
            mn, _bn = parse_note(pn)
        except MalformedNote as e:
            sys.exit(f"knowbox supersede: {e} — fix the note first")
        oid, nid = str(mo.get("id", "?")), str(mn.get("id", "?"))
        if str(mo.get("status")) == "superseded":
            sys.exit(f"{oid} is already superseded")
        if str(mn.get("status")) == "superseded":
            sys.exit(f"{nid} is itself superseded — pick a live note as NEW")
        today = date.today().isoformat()
        sup = [str(x) for x in _aslist(mn.get("supersedes"))]
        if oid not in sup:
            sup.append(oid)
        banner = (f"> **Superseded by [[{nid}]] — kept for history, excluded "
                  "from search.**")
        # Two-phase safety (P4): read BOTH originals and compute BOTH new
        # contents first — every refusal (missing/unterminated fm, missing
        # keys) happens here, while nothing on disk has changed. Then write
        # via mkstemp+os.replace; if the second write fails, restore the
        # first file from memory. Banner insertion anchors at the parsed fm
        # end (the old `\A(---\n…---\n)` regex silently no-oped on CRLF+BOM).
        try:
            raw_o = open(po, "rb").read()
            raw_n = open(pn, "rb").read()
        except OSError as e:
            sys.exit(f"knowbox supersede: cannot read notes: {e}")
        # related: cross-add: the banner
        # puts [[NEW]] in OLD's body, so OLD.related must carry NEW (lint:
        # body links ⊆ related:); NEW.related gets OLD for symmetry.
        rel_o = [str(x) for x in _aslist(mo.get("related"))]
        if nid not in rel_o:
            rel_o.append(nid)
        rel_n = [str(x) for x in _aslist(mn.get("related"))]
        if oid not in rel_n:
            rel_n.append(oid)
        new_o = _rewrite_fm_bytes(raw_o,
                                  {"status": "superseded", "updated": today,
                                   "related": f"[{', '.join(rel_o)}]"},
                                  after_fm=("", banner), path=po)
        new_n = _rewrite_fm_bytes(raw_n,
                                  {"supersedes": f"[{', '.join(sup)}]",
                                   "updated": today,
                                   "related": f"[{', '.join(rel_n)}]"},
                                  path=pn)
        try:
            _atomic_write_bytes(po, new_o)
        except OSError as e:
            sys.exit(f"knowbox supersede: writing {po} failed ({e}) — nothing "
                     "changed")
        try:
            _atomic_write_bytes(pn, new_n)
        except OSError as e:
            try:
                _atomic_write_bytes(po, raw_o)   # roll OLD back from memory
            except OSError as e2:
                sys.exit(f"knowbox supersede: writing NEW failed ({e}) AND "
                         f"restoring OLD failed ({e2}) — {po} is modified; "
                         "recover via git")
            sys.exit(f"knowbox supersede: writing {pn} failed ({e}) — OLD "
                     "restored byte-identical, nothing changed")
        build_db()
        build_index()
        append_log("supersede", oid, f"superseded by {nid}")
        git_commit(f"knowbox: supersede {oid} → {nid}")
    print(f"{oid} superseded by {nid}")
