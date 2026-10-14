"""knowbox maintain: reindex + lint (with waivers) + the raw/ staging
backlog commands. Owns the raw/ entry scanner — raw-backlog, raw-mark and
lint share it so all three always agree.
"""
import os
import re
import subprocess
import sys
from datetime import date, datetime

import paths
from _flags import _flags
from index import _freshness_report, build_db, build_index
from notes import (PARSE_FAILURES, _aslist, _asstr, _days_since, all_notes,
                   append_log, git_commit, next_id)
from paths import (DATE_RE, ENUMS, HEARTBEAT_MAX_AGE_DAYS, ID_RE, REQUIRED,
                   _env)
from sysutil import IS_NT, _atomic_write_bytes, _locked


# Lint waivers (zero-warning baseline, 2026-09-08): `lint-waivers.md` at the
# KB root silences specific (note-id, category) warnings that can never be
# fixed in the data — append-only log.md history (hand-made pre-journal notes,
# the 2026-08-06 id recycling). POLICY: only `created-vs-log` is waivable;
# word-budget/summary/triggers/related warnings are content — fix them, never
# waive them. New waivers require user approval (SCHEMA.md "Lint"). A waiver
# that suppresses nothing is itself a WARN, so the file self-cleans.
WAIVER_CATEGORIES = {"created-vs-log"}
WAIVER_RE = re.compile(
    r"^(kb-\d{4})\s+([a-z-]+)\s+(\d{4}-\d{2}-\d{2})\s+(\S.*)$")


def _load_waivers():
    """Parse lint-waivers.md → ({(id, category): "date reason"}, [malformed]).

    Waiver lines look like `kb-XXXX <category> <ISO-date> <reason>` (they may
    sit inside a fenced code block — fences/prose are just non-matching lines).
    Blank lines, #-comments and other prose are ignored; a line STARTING with
    kb-XXXX that fails to parse is reported malformed (no silent typos).
    """
    waivers, malformed = {}, []
    path = os.path.join(paths.KB, "lint-waivers.md")
    if not os.path.exists(path):
        return waivers, malformed
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return waivers, malformed
    for ln in text.splitlines():
        s = ln.strip()
        if not s or s.startswith("#"):
            continue
        m = WAIVER_RE.match(s)
        if m:
            nid, cat, date, reason = m.groups()
            if cat in WAIVER_CATEGORIES:
                waivers[(nid, cat)] = f"{date} {reason}"
            else:
                malformed.append(f"unknown category {cat!r} in line "
                                 f"'{s}' (waivable: "
                                 f"{', '.join(sorted(WAIVER_CATEGORIES))})")
        elif re.match(r"^kb-\d{4}(\s|$)", s):
            malformed.append(f"line '{s}' is not "
                             "`kb-XXXX <category> <date> <reason>`")
    return waivers, malformed


# ---------- raw/ staging backlog (kb-0149 design) ----------
# raw/ files are append-only entry streams: a `## ` heading starts an entry,
# the entry runs to the next `## `/EOF (bodies use ### and deeper ONLY — the
# "no nested-level heading" rule). Review state is a marker as the FIRST
# line under the heading: `> state: ingested kb-XXXX (date)` /
# `> state: rejected <reason>`. Legacy markers (`> ingested:` /
# `> rejected:` ANYWHERE in the block, incl. at the block's end — the
# distill-era form) still resolve the entry; new writes use only the new
# form. A legacy marker in the PREAMBLE (above the first heading) resolves
# nothing but skews the janitor's cardinality count → lint WARN.

_RAW_HEADING_RE = re.compile(r"^## (?!#)(.*)$")
_RAW_STATE_RE = re.compile(r"^> state: (ingested|rejected)\b")
_RAW_LEGACY_RE = re.compile(r"^> (?:ingested|rejected):")
# `## pair: kb-XXXX kb-YYYY` conflict-judge verdicts are review records, not
# ingest candidates — exempt by prefix, not by filename, so the parser stays
# content-driven (any file may hold judge blocks).
_RAW_ENTRY_EXEMPT_PREFIXES = ("pair:",)
# lint WARN threshold for session-distill-* files only (the distill pipeline's
# own janitor alert already watches the total; lint only flags a file when the
# pile-up is well past routine review cadence). Human-note files warn at 1.
RAW_BACKLOG_WARN_MIN = 5


def _scan_raw_file(path):
    """Parse one raw/ file → (entries, orphan_markers). entries: list of
    dicts — file (basename), n (1-based entry number), title (heading
    text), resolved (bool). orphan_markers: count of legacy
    `> ingested:`/`> rejected:` lines in the PREAMBLE (before the first
    `## ` heading) — they resolve no entry but the janitor's cardinality
    grep counts them, silently skewing its backlog number."""
    try:
        with open(path, encoding="utf-8") as f:
            lines = f.read().splitlines()
    except (OSError, UnicodeDecodeError):
        return [], 0
    entries = []
    orphans = 0
    seen_heading = False
    for i, ln in enumerate(lines):
        m = _RAW_HEADING_RE.match(ln)
        if not m:
            if not seen_heading and _RAW_LEGACY_RE.match(ln):
                orphans += 1
            continue
        seen_heading = True
        title = m.group(1).strip()
        if any(title.lower().startswith(p) for p in _RAW_ENTRY_EXEMPT_PREFIXES):
            continue
        j = i + 1
        end = len(lines)
        while j < end and not _RAW_HEADING_RE.match(lines[j]):
            j += 1
        block = lines[i + 1:j]
        new_form = bool(block) and bool(_RAW_STATE_RE.match(block[0]))
        legacy = any(_RAW_LEGACY_RE.match(b) for b in block)
        entries.append({"file": os.path.basename(path),
                        "n": len(entries) + 1, "title": title,
                        "resolved": new_form or legacy})
    return entries, orphans


def _raw_backlog():
    """(entries, orphans_by_file) across raw/*.md, oldest file first
    (stable scan order — raw-mark and lint must agree with raw-backlog
    listings)."""
    entries, orphans = [], {}
    if os.path.isdir(paths.RAW):
        for fn in sorted(os.listdir(paths.RAW)):
            if fn.endswith(".md"):
                ents, n_orph = _scan_raw_file(os.path.join(paths.RAW, fn))
                entries.extend(ents)
                if n_orph:
                    orphans[fn] = n_orph
    return entries, orphans


def cmd_raw_backlog(_args):
    entries, _ = _raw_backlog()
    open_ = [e for e in entries if not e["resolved"]]
    print(f"raw backlog: {len(open_)} unreviewed of {len(entries)} entries")
    for e in open_:
        print(f"  {e['file']} #{e['n']}: {e['title']}")
    if open_:
        print("\nmark after ingest: knowbox raw-mark <file> <N> "
              "--note kb-XXXX | --rejected \"reason\"")


def cmd_raw_mark(args):
    opts = _flags(args)
    pos = opts.get("_pos", [])
    note, rejected = opts.get("note", ""), opts.get("rejected", "")
    if len(pos) != 2 or (bool(note) == bool(rejected)):
        sys.exit("usage: knowbox raw-mark <file> <N> --note kb-XXXX | "
                 "--rejected \"reason\"")
    fname, n_s = pos
    if "/" in fname or "\\" in fname or fname in (".", "..") or \
            not fname.endswith(".md"):
        sys.exit(f"knowbox raw-mark: bad raw filename {fname!r} "
                 "(basename of a .md file inside raw/)")
    path = os.path.join(paths.RAW, fname)
    if not os.path.isfile(path):
        sys.exit(f"not found: raw/{fname}")
    try:
        n = int(n_s)
    except ValueError:
        sys.exit(f"knowbox raw-mark: entry number must be an integer, "
                 f"got {n_s!r}")
    if note and not ID_RE.match(note):
        sys.exit(f"knowbox raw-mark: --note must be a kb-id, got {note!r}")
    entries, _ = _scan_raw_file(path)
    if not (1 <= n <= len(entries)):
        sys.exit(f"knowbox raw-mark: {fname} has {len(entries)} entries, "
                 f"no #{n}")
    e = entries[n - 1]
    if e["resolved"]:
        sys.exit(f"knowbox raw-mark: {fname} #{n} ({e['title']!r}) is "
                 "already marked")
    marker = (f"> state: ingested {note} ({date.today().isoformat()})"
              if note else
              f"> state: rejected {rejected} ({date.today().isoformat()})")
    with _locked():
        with open(path, "rb") as f:
            lines = f.read().decode("utf-8").splitlines(keepends=True)
        # nth non-exempt entry heading, re-found byte-exactly under the
        # lock (n is the global entry index, so count ALL entry headings —
        # not just same-title ones — then verify the title)
        seen, at = 0, None
        for i, ln in enumerate(lines):
            m = _RAW_HEADING_RE.match(ln.rstrip("\r\n"))
            if not m:
                continue
            title = m.group(1).strip()
            if any(title.lower().startswith(p)
                   for p in _RAW_ENTRY_EXEMPT_PREFIXES):
                continue
            seen += 1
            if seen == n:
                at = i
                break
        if at is None:
            sys.exit(f"knowbox raw-mark: entry #{n} vanished mid-write — "
                     "retry (file changed under us?)")
        found = lines[at].rstrip("\r\n")
        if _RAW_HEADING_RE.match(found).group(1).strip() != e["title"]:
            sys.exit(f"knowbox raw-mark: entry #{n} title changed mid-write "
                     f"(now {found!r}) — retry")
        lines.insert(at + 1, marker + "\n")
        _atomic_write_bytes(path, "".join(lines).encode("utf-8"))
        git_commit(f"knowbox: raw-mark {fname} #{n}")
    print(f"marked {fname} #{n}: {e['title']} -> {marker[2:]}")


def cmd_lint(_args):
    errs, warns = [], []
    waivers, waiver_malformed = _load_waivers()
    waived, waiver_hits = [], set()

    def _warn_waivable(cat, nid, rel, msg):
        if (nid, cat) in waivers:
            waived.append(f"{rel}: {msg}")
            waiver_hits.add((nid, cat))
        else:
            warns.append(f"{rel}: {msg}")

    ids, titles = {}, {}
    succ, superseded_notes = {}, []   # P11 no-successor WARN bookkeeping
    # created-vs-log forensics (T4/P9): log.md's create entries are the CLI's
    # machine-stamped birth records. Mismatch/no-entry are WARNs, not ERRORs —
    # prod reality at adoption (2026-09-07): kb-0014's created: 2026-08-02 vs
    # its log entry 2026-08-03 (crossed midnight) would otherwise be a
    # permanent ERROR; the plan's "lint no-worse" rule wins. Pre-log-era
    # notes (created on or before the first log entry date — the journal
    # started mid-day 2026-08-02) are exempt: it cannot know them.
    log_creates, first_log_date = {}, None
    if os.path.exists(paths.LOG):
        try:
            with open(paths.LOG, encoding="utf-8") as f:
                log_text = f.read()
        except OSError:
            log_text = ""
        dm = re.search(r"^## \[(\d{4}-\d{2}-\d{2})\]", log_text, re.M)
        first_log_date = dm.group(1) if dm else None
        for m in re.finditer(
                r"^## \[(\d{4}-\d{2}-\d{2})\] create \| (kb-\d{4})",
                log_text, re.M):
            log_creates.setdefault(m.group(2), []).append(m.group(1))
    n_parsed = 0
    for rel, meta, body in all_notes():
        n_parsed += 1
        nid = str(meta.get("id", "?"))
        fn = os.path.basename(rel)
        if not ID_RE.match(nid):
            errs.append(f"{rel}: bad id {nid!r}")
        if nid in ids:
            errs.append(f"{rel}: duplicate id {nid} (also {ids[nid]})")
        ids[nid] = rel
        t = str(meta.get("title", ""))
        if t in titles:
            warns.append(f"{rel}: duplicate title (also {titles[t]})")
        titles[t] = rel
        if not fn.startswith(nid + "-"):
            errs.append(f"{rel}: filename must start with {nid}-")
        for f in REQUIRED:
            if f not in meta or meta[f] in ("", None, []):
                errs.append(f"{rel}: missing {f}")
        for f, allowed in ENUMS.items():
            if f in meta and meta[f] not in allowed:
                errs.append(f"{rel}: {f}={meta[f]!r} not in {sorted(allowed)}")
        for f in ("created", "updated"):
            if f in meta and not DATE_RE.match(str(meta[f])):
                errs.append(f"{rel}: {f} not ISO date: {meta[f]!r}")
        for f in ("related", "supersedes"):
            for r in _aslist(meta.get(f)):
                if not ID_RE.match(str(r)):
                    warns.append(f"{rel}: {f} entry {r!r} is not a kb-id")
        for s in _aslist(meta.get("supersedes")):
            succ.setdefault(str(s), nid)
        if str(meta.get("status")) == "superseded":
            superseded_notes.append((rel, nid))
        s = str(meta.get("summary", ""))
        if len(s) > 220:
            warns.append(f"{rel}: summary {len(s)} chars (>220; it is the index line)")
        if "TODO" in s:
            warns.append(f"{rel}: summary still has TODO")
        words = len(body.split())
        if words > 600 and meta.get("type") != "reference":
            errs.append(f"{rel}: {words} words (>600; split it)")
        elif words > 450 and meta.get("type") != "reference":
            warns.append(f"{rel}: {words} words (budget is 400)")
        body_links = set(re.findall(r"\[\[(kb-\d{4})\]\]", body))
        related = {str(r) for r in _aslist(meta.get("related"))}
        for l in sorted(body_links - related):
            warns.append(f"{rel}: body links [[{l}]] but it is not in related:")
        if str(meta.get("source", "")) == "":
            warns.append(f"{rel}: empty source:")
        cr, up = str(meta.get("created", "")), str(meta.get("updated", ""))
        if DATE_RE.match(cr) and DATE_RE.match(up) and up < cr:
            errs.append(f"{rel}: updated ({up}) < created ({cr})")
        # verified-date integrity (P4; the missing-date WARN flips to ERROR
        # once the P5 sweep clears the backlog)
        ver = meta.get("verified")
        has_ver = ver not in (None, "", [])
        if has_ver and not DATE_RE.match(str(ver)):
            errs.append(f"{rel}: verified not ISO date: {ver!r}")
        if meta.get("status") == "verified" and not has_ver:
            # P5 flip (2026-09-06): WARN while the 24-note backlog existed;
            # backlog is zero, so an undated verified claim is now an error.
            errs.append(f"{rel}: status: verified but no verified: date")
        # D3 interim policy: triggers: strongly recommended → WARN on missing
        if not _aslist(meta.get("triggers")):
            warns.append(f"{rel}: missing triggers: "
                         "(WHEN should an agent read this?)")
        # created-vs-log integrity (T4/P9) — see the forensics comment above.
        # This family is waivable via lint-waivers.md (append-only history).
        dates = log_creates.get(nid)
        if dates:
            if len(dates) > 1:
                _warn_waivable("created-vs-log", nid, rel,
                               f"log.md has {len(dates)} create entries "
                               f"for {nid} (historical id recycling — see the "
                               "P9 gotcha note)")
            if str(meta.get("created", "")) not in dates:
                _warn_waivable("created-vs-log", nid, rel,
                               f"created: {meta.get('created')} matches "
                               f"no log.md create date for {nid} "
                               f"({', '.join(sorted(set(dates)))})")
        elif first_log_date and str(meta.get("created", "")) > first_log_date:
            _warn_waivable("created-vs-log", nid, rel,
                           "no create entry in log.md (created after "
                           "the journal started — hand-made note?)")
    # dead cross-links
    for rel, meta, _b in all_notes():
        for f in ("related", "supersedes"):
            for r in _aslist(meta.get(f)):
                if ID_RE.match(str(r)) and r not in ids:
                    errs.append(f"{rel}: {f} points at missing {r}")
    # P11: a superseded note no successor claims is a dead end in the index
    # (its flag renders bare ⚠SUPERSEDED with nowhere to route)
    for rel, nid in superseded_notes:
        if nid not in succ:
            warns.append(f"{rel}: superseded but no note lists it in "
                         "supersedes: (no successor — index flag stays bare)")
    # refuse-to-publish (P4/T7): malformed notes never reach index/FTS/embeds
    for pf in PARSE_FAILURES:
        errs.append(f"{os.path.relpath(pf, paths.KB).replace(os.sep, '/')}: "
                    f"unterminated frontmatter "
                    f"(excluded from index/search)")
    n_disk = len([fn for fn in os.listdir(paths.NOTES) if fn.endswith(".md")]) \
        if os.path.isdir(paths.NOTES) else 0
    if n_parsed < n_disk:
        # redundant on purpose: fires even if the ledger somehow stayed empty
        errs.append(f"{n_disk - n_parsed} of {n_disk} notes/*.md failed to "
                    f"parse — excluded from index/search (see errors above)")
    # index/DB freshness (P8): content-sha1 drift vs kb_meta, orphan index
    # entries, hand-edited index.md — replaces the old substring-presence
    # WARN, so SCHEMA's "orphan index entries" claim is now truly checked.
    rep = _freshness_report()
    if rep["db_missing"]:
        errs.append("index/DB stale: FTS db missing, corrupt, or pre-kb_meta "
                    "schema → run `knowbox reindex`")
    if rep["drifted"]:
        shown = ", ".join(rep["drifted"][:5])
        more = (f" +{len(rep['drifted']) - 5} more"
                if len(rep["drifted"]) > 5 else "")
        errs.append(f"index/DB stale: {len(rep['drifted'])} note(s) drifted "
                    f"({shown}{more}) → run `knowbox reindex`")
    if rep["orphans"]:
        errs.append(f"index.md orphan entries: {', '.join(rep['orphans'])} "
                    "— no note on disk → run `knowbox reindex`")
    if rep["index_drift"] and not (rep["db_missing"] or rep["drifted"]
                                   or rep["orphans"]):
        errs.append("index.md missing or content drift (hand-edited?) → "
                    "run `knowbox reindex`")
    # raw/ staging health (kb-0149): `## ` headings are entry boundaries —
    # entries need a review marker or they are backlog. Uses the SAME
    # scanner as raw-backlog/raw-mark so all three always agree. Knowledge
    # stays per-entry (the WARN names the file + count, not the backlog
    # total), so `lint-waivers.md`-style suppression is per-file.
    entries, orphans = _raw_backlog()
    for fn, n_orph in sorted(orphans.items()):
        warns.append(f"raw/{fn}: {n_orph} orphan legacy marker"
                     f"{'s' if n_orph != 1 else ''} in the preamble (above "
                     "the first `## ` heading) — they resolve no entry but "
                     "skew the janitor's backlog count; attach each to its "
                     "entry's block")
    raw_by_file = {}
    for e in entries:
        raw_by_file.setdefault(e["file"], []).append(e)
    for fn in sorted(raw_by_file):
        open_ = [e for e in raw_by_file[fn] if not e["resolved"]]
        if not open_:
            continue
        if fn.startswith("session-distill-"):
            # distill backlog has its own janitor alert + raw-backlog
            # listing; a per-file lint WARN would double-signal every file
            if len(open_) > RAW_BACKLOG_WARN_MIN:
                warns.append(
                    f"raw/{fn}: distill backlog: {len(open_)} unreviewed "
                    f"proposals (> {RAW_BACKLOG_WARN_MIN}) — promote or "
                    "reject via the knowbox skill + `knowbox raw-mark`")
        else:
            warns.append(
                f"raw/{fn}: {len(open_)} unreviewed entr"
                f"{'ies' if len(open_) != 1 else 'y'} — ingest via the "
                "knowbox skill, then `knowbox raw-mark <file> <N> "
                "--note kb-XXXX | --rejected \"reason\"`")
    # janitor dead-man's switch (kb-0014): the weekly cron run touches
    # .kb-heartbeat; missing/old means the cron is dead — fail loudly.
    # F17 (kb-0117): the janitor lives on the Fedora server only — a replica
    # clone (native Windows, or any secondary machine) has no cron and the
    # heartbeat is gitignored, so a MISSING heartbeat there must not make lint
    # permanently red. Opt out with KNOWBOX_NO_HEARTBEAT=1; on Windows a
    # never-present heartbeat auto-exempts (an EXISTING but stale one still
    # errors — someone installed a janitor there and it died).
    hb = os.path.join(paths.KB, ".kb-heartbeat")
    hb_disabled = _env("NO_HEARTBEAT") not in ("", "0", "false", "no")
    hb_expected = not hb_disabled and (not IS_NT or os.path.exists(hb))
    if not hb_expected:
        pass  # replica client without a janitor — heartbeat not expected
    elif not os.path.exists(hb):
        errs.append("janitor heartbeat missing — weekly cron dead? run: "
                    "bash ~/dotfiles/scripts/knowbox-janitor.sh (see kb-0014)")
    else:
        age_days = (datetime.now().timestamp() - os.path.getmtime(hb)) / 86400
        if age_days > HEARTBEAT_MAX_AGE_DAYS:
            errs.append(f"janitor heartbeat stale ({age_days:.0f}d > "
                        f"{HEARTBEAT_MAX_AGE_DAYS}d) — weekly cron dead? run: "
                        f"bash ~/dotfiles/scripts/knowbox-janitor.sh (see kb-0014)")
    # Waiver hygiene: malformed lines, unknown categories, and waivers that
    # suppressed nothing (data since fixed, note deleted, id typo'd) are
    # themselves WARNs — the file must never rot silently.
    for bad in waiver_malformed:
        warns.append(f"lint-waivers.md: {bad}")
    for wnid, wcat in sorted(waivers):
        if (wnid, wcat) not in waiver_hits:
            warns.append(f"lint-waivers.md: stale waiver {wnid} {wcat} — "
                         "suppressed nothing this run; remove the line")
    for w in warns:
        print(f"WARN  {w}")
    for e in errs:
        print(f"ERROR {e}")
    print(f"\n{len(errs)} errors, {len(warns)} warnings "
          f"across {len(ids)} notes")
    if waived:
        # Informational only — NOT counted in the summary line above (the
        # janitor and the goldens parse that exact format).
        print(f"\nWAIVED ({len(waived)}) via lint-waivers.md — historical, "
              "not counted:")
        for w in waived:
            print(f"  {w}")
    sys.exit(1 if errs else 0)


def cmd_stale(args):
    """knowbox stale [--days N] — rank live notes by staleness (default threshold 90d)."""
    opts = _flags(args)
    try:
        threshold = int(opts.get("days", "90"))
    except ValueError:
        sys.exit("knowbox stale: --days must be an integer")
    if threshold < 1:
        sys.exit("knowbox stale: --days must be >= 1")
    rows = []
    for _rel, meta, _body in all_notes():
        if str(meta.get("status", "")) == "superseded":
            continue
        upd = str(meta.get("updated") or meta.get("created") or "")
        age = _days_since(upd)
        ver = str(meta.get("verified") or "")
        flags = []
        if age is not None and age >= threshold:
            flags.append("STALE")
        if not ver and age is not None and age >= threshold // 2:
            flags.append("NEVER-VERIFIED")
        rows.append((age if age is not None else -1, "+".join(flags),
                     meta.get("id", "?"), str(meta.get("status", "?")),
                     upd or "?", ver or "never", _asstr(meta.get("title"))))
    if not rows:
        print("knowbox stale: no live notes found")
        return
    rows.sort(key=lambda r: (-r[0], r[2]))
    print(f"knowbox stale — live notes ranked by staleness (threshold: {threshold}d, "
          f"never-verified flag at {threshold // 2}d)")
    print(f"{'flag':16} {'id':8} {'status':9} {'updated':11} {'age':>4} {'verified':11} title")
    for age, flags, nid, status, upd, ver, title in rows:
        print(f"{flags:16} {nid:8} {status:9} {upd:11} {age:>3}d {ver:11} {title[:58]}")
    flagged = [r for r in rows if r[1]]
    print(f"\n{len(rows)} live note(s); oldest update {rows[0][0]}d ago; "
          f"{len(flagged)} flagged")
    if flagged:
        print("act on flags: re-verify with 'knowbox verify ID --evidence …', "
              "update, or supersede.")
