"""knowbox reports: the read-only analytics commands — refs (heading index),
churn (git edit frequency), conflicts (embedding-cosine pair candidates).
"""
import json
import os
import re
import subprocess
import sys

import paths
from _flags import _flags
from embed import _load_embed
from notes import _asstr, all_notes
from paths import ID_PREFIX_RE, ID_RE


def cmd_refs(args):
    """knowbox refs [FILTER] — heading index over refs/: each doc's word count,
    H2 sections, and the type:reference map note covering it. Docs with no
    map note are flagged UNMAPPED (actionable: SCHEMA.md § Large docs)."""
    opts = _flags(args)
    filt = " ".join(opts.get("_pos", [])).strip().lower()
    if not os.path.isdir(paths.REFS):
        print("knowbox refs: no refs/ directory — nothing ingested yet")
        return
    docs = sorted(f for f in os.listdir(paths.REFS)
                  if f.endswith(".md") and os.path.isfile(os.path.join(paths.REFS, f)))
    if not docs:
        print("knowbox refs: refs/ is empty")
        return
    # map notes: every live type:reference note — match docs by filename
    # appearing in the note's title, summary, or body (section-map pointer).
    maps = []
    for _rel, meta, body in all_notes():
        if str(meta.get("type", "")) != "reference":
            continue
        if str(meta.get("status", "")) == "superseded":
            continue
        hay = (str(meta.get("title", "")) + " " + str(meta.get("summary", ""))
               + " " + body)
        maps.append((str(meta.get("id", "?")), hay))
    rows = []
    for fn in docs:
        try:
            with open(os.path.join(paths.REFS, fn), encoding="utf-8",
                      errors="replace") as fh:
                text = fh.read()
        except OSError as e:
            rows.append((fn, -1, [], [], f"UNREADABLE ({e})"))
            continue
        heads, fence = [], False
        for line in text.splitlines():
            if line.lstrip().startswith("```"):
                fence = not fence
                continue
            if not fence and line.startswith("## "):
                heads.append(line[3:].strip())
        mids = [i for i, hay in maps if fn in hay]
        rows.append((fn, len(text.split()), heads, mids,
                     ",".join(mids) if mids else "UNMAPPED"))
    shown = rows
    if filt:
        shown = [r for r in rows
                 if filt in r[0].lower()
                 or any(filt in h.lower() for h in r[2])
                 or any(filt in m.lower() for m in r[3])]
    print(f"knowbox refs — heading index over refs/ ({len(shown)} of {len(docs)} "
          "docs shown; read via the map note, section by section, never whole)")
    for fn, words, heads, _mids, label in shown:
        wc = f"{words}w" if words >= 0 else "?"
        print(f"\n{fn}  [{wc}]  map: {label}")
        for h in heads[:12]:
            print(f"  - {h[:70]}")
        if len(heads) > 12:
            print(f"  … +{len(heads) - 12} more sections")
    unmapped = [r[0] for r in rows if r[4] == "UNMAPPED"]
    print(f"\n{len(rows)} doc(s), {len(rows) - len(unmapped)} mapped, "
          f"{len(unmapped)} UNMAPPED")
    if unmapped:
        print("act on UNMAPPED: create a type:reference section-map note "
              "(SCHEMA.md § Large docs) — unmapped docs are invisible to "
              "the need-knowledge protocol.")


def cmd_churn(args):
    """knowbox churn [--days N] — rank notes by git CONTENT-edit frequency
    (default window 90d). T9/P9: keyed by kb-id, so renames aggregate into
    one row; date-only bumps are skipped — a commit whose ENTIRE change to a
    note is `updated:`/`verified:` lines is maintenance noise, not claim
    churn (pure renames with no content lines are skipped too)."""
    opts = _flags(args)
    try:
        window = int(opts.get("days", "90"))
    except ValueError:
        sys.exit("knowbox churn: --days must be an integer")
    if window < 1:
        sys.exit("knowbox churn: --days must be >= 1")
    # B4 (kb-0117): encoding="utf-8" — text=True alone decodes the captured
    # note DIFFS with the locale codec (cp1252 on Windows) and this corpus is
    # full of em-dashes/Vietnamese → UnicodeDecodeError. B6: pin diff.noprefix
    # and diff.external off — the parser below expects a/ b/ prefixes and the
    # built-in diff. B5: git absent → clean exit, not a traceback.
    try:
        res = subprocess.run(
            ["git", "-C", paths.KB, "-c", "core.quotePath=false",
             "-c", "diff.noprefix=false", "-c", "diff.external=", "log",
             f"--since={window} days ago", "-U0", "--no-color", "-M",
             "--pretty=format:%x01%cs", "--", "notes/"],
            capture_output=True, text=True, timeout=60,
            encoding="utf-8", errors="replace")
    except FileNotFoundError:
        sys.exit("knowbox churn: git not found")
    except subprocess.TimeoutExpired:
        sys.exit("knowbox churn: git log timed out")
    if res.returncode != 0:
        sys.exit(f"knowbox churn: git log failed: {res.stderr.strip()}")
    DATE_ONLY = re.compile(r"^[+-](updated|verified): ")
    counts, days_touched = {}, {}
    commits_with_edits = set()
    state = {"day": None, "commit": 0, "file": None, "lines": [],
             "last_from": None}

    def _strip(p):
        return p[2:] if p[:2] in ("a/", "b/") else p

    def flush():
        """Close the current (commit, file) diff: count it unless every
        changed line is a date bump (or there were no content lines)."""
        if state["file"] is not None and state["lines"] and \
                any(not DATE_ONLY.match(l) for l in state["lines"]):
            m = ID_PREFIX_RE.search(state["file"])
            key = m.group(0) if m else state["file"]
            counts[key] = counts.get(key, 0) + 1
            days_touched.setdefault(key, set()).add(state["day"])
            commits_with_edits.add(state["commit"])
        state["file"], state["lines"] = None, []

    for line in res.stdout.splitlines():
        if line.startswith("\x01"):
            flush()
            state["day"] = line[1:]
            state["commit"] += 1
        elif line.startswith("diff --git"):
            flush()
        elif line.startswith("--- "):
            state["last_from"] = _strip(line[4:].strip())
        elif line.startswith("+++ "):
            flush()
            p = _strip(line[4:].strip())
            state["file"] = state["last_from"] if p == "/dev/null" else p
        elif line.startswith(("+", "-")) and state["file"]:
            state["lines"].append(line)
        # @@ / index / similarity / rename headers: ignored
    flush()
    if not counts:
        print(f"knowbox churn: no note content edits in the last {window} days "
              "(date-only updated:/verified: bumps skipped)")
        return
    titles = {str(m.get("id", "?")): _asstr(m.get("title"))
              for _rel, m, _b in all_notes()}
    rows = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
    hot = [k for k, c in rows if c >= 5]
    print(f"knowbox churn — content edits per note id, last {window} days "
          "(HIGH-CHURN at >=5; date-only bumps skipped, renames aggregated)")
    print(f"{'commits':>7} {'days':>5}  note")
    for key, c in rows:
        if key in titles:
            label = f"{key}  {titles[key][:56]}"
        elif ID_RE.match(key):
            label = f"{key}  (note deleted or renamed away)"
        else:
            label = f"{key}  (not a top-level note)"
        mark = "  HIGH-CHURN" if c >= 5 else ""
        print(f"{c:>7} {len(days_touched[key]):>5}  {label}{mark}")
    print(f"\n{len(rows)} note id(s) edited across "
          f"{len(commits_with_edits)} commit(s); high-churn: {len(hot)}")
    if hot:
        print("high-churn notes keep changing: consider splitting them, or moving "
              "the stable core into refs/ with a section map.")


def cmd_conflicts(args):
    """knowbox conflicts [--top N] [--min-cos X] [--json] — deterministic
    similarity report over live embedded notes (P12). High-cosine pairs are
    CANDIDATES for duplicate/contradiction/complementary review — NOT
    verdicts. Deliberately no LLM in the CLI (user decision D-conflicts,
    2026-09-05): the judge pass lives in SKILL.md and records verdicts as
    `> verdict:` markers in raw/conflicts-YYYY-MM-DD.md (D5). This command
    stays stateless — skipping already-judged pairs is the skill's job
    (grep prior reviews), not the scanner's.

    --min-cos default 0.70 calibrated 2026-09-07 against the real store:
    99th percentile of 3916 pairs ≈ 0.698 (noise below, signal above)."""
    opts = _flags(args)
    as_json = "--json" in args
    try:
        top = int(opts.get("top", "10"))
    except ValueError:
        sys.exit("knowbox conflicts: --top must be an integer")
    if top < 1:
        sys.exit("knowbox conflicts: --top must be >= 1")
    try:
        min_cos = float(opts.get("min-cos", "0.70"))
    except ValueError:
        sys.exit("knowbox conflicts: --min-cos must be a number")
    if not os.path.exists(paths.EMBED):
        sys.exit("knowbox conflicts: no embeddings (.kb-embed.json) — "
                 "run `knowbox embed` first")
    store = _load_embed()
    if not store:
        sys.exit("knowbox conflicts: embeddings are for a different model or "
                 "unreadable — run `knowbox embed --force`")
    # restrict to LIVE notes on disk: superseded (dead knowledge), malformed
    # (refuse-to-publish) and deleted ids never enter the report; notes
    # without an embedding yet simply don't participate (run `knowbox embed`)
    info = {}
    for rel, meta, _body in all_notes():
        if meta.get("status") == "superseded":
            continue
        nid = str(meta.get("id", "?"))
        ent = store.get(nid)
        if isinstance(ent, dict) and ent.get("v"):
            info[nid] = (rel, meta, ent["v"])
    if len(info) < 2:
        sys.exit(f"knowbox conflicts: only {len(info)} embedded live note(s) — "
                 "run `knowbox embed` first")
    ids = sorted(info)
    norms = {i: (sum(x * x for x in info[i][2]) ** 0.5) or 1.0 for i in ids}
    pairs = []
    for ai, a in enumerate(ids):
        va = info[a][2]
        for b in ids[ai + 1:]:
            vb = info[b][2]
            cos = sum(x * y for x, y in zip(va, vb)) / (norms[a] * norms[b])
            if cos >= min_cos:
                pairs.append((cos, a, b))
    # deterministic order: score desc, then the id pair — two runs are
    # byte-identical (a pinned test)
    pairs.sort(key=lambda t: (-t[0], t[1], t[2]))
    pairs = pairs[:top]
    if as_json:
        print(json.dumps(
            [{"a": a, "b": b, "cos": round(c, 4),
              "a_status": str(info[a][1].get("status", "")),
              "b_status": str(info[b][1].get("status", "")),
              "a_updated": str(info[a][1].get("updated", "")),
              "b_updated": str(info[b][1].get("updated", "")),
              "a_summary": str(info[a][1].get("summary", "")),
              "b_summary": str(info[b][1].get("summary", ""))}
             for c, a, b in pairs], indent=2, ensure_ascii=False))
        return
    if not pairs:
        print(f"no live-note pairs at cos >= {min_cos:.2f} "
              f"({len(info)} embedded notes scanned)")
        return
    print(f"knowbox conflicts — {len(pairs)} candidate pair(s) at cos >= "
          f"{min_cos:.2f} ({len(info)} embedded live notes).")
    print("Candidates, NOT verdicts — judge pass: SKILL.md 'Conflicts judge "
          "pass' → raw/conflicts-DATE.md\n")
    for c, a, b in pairs:
        print(f"  cos {c:.4f}")
        for nid in (a, b):
            rel, meta, _v = info[nid]
            s = " ".join(_asstr(meta.get("summary")).split())[:110]
            print(f"    {nid}  [{meta.get('status', '?')} · upd "
                  f"{meta.get('updated', '?')}]  {rel}\n        {s}")
        print()
