"""knowbox notes: frontmatter parsing, guarded rewriting, note inventory,
ids, project detection, the append-only journal and the git auto-commit.

Everything note-shaped lives here. Paths late-bind (paths.NOTES inside
functions) so cmd_init re-rooting via paths._use_kb keeps working.
"""
import os
import re
import subprocess
import sys
from datetime import date

import paths
from paths import ID_RE
from sysutil import _atomic_write_bytes

# ---------- frontmatter (minimal YAML: scalars + flat lists) ----------

class MalformedNote(Exception):
    """A note that opens a frontmatter block (`---`) but never closes it (T7).

    Refuse-to-publish: all_notes() skips these (they reach neither the FTS DB,
    index.md, nor the embeddings) and records them in PARSE_FAILURES; `knowbox lint`
    reports each one as an ERROR. `knowbox show` still prints the raw file, so a
    malformed note stays readable and fixable."""

    def __init__(self, path):
        self.path = path
        super().__init__(f"{path}: unterminated frontmatter (no closing '---')")


def parse_note(path):
    """Return (meta dict, body str). meta values are str or list[str].

    A leading UTF-8 BOM is tolerated (stripped for detection/parsing only —
    the rewrite helpers preserve it on disk). Raises MalformedNote when the
    file starts a frontmatter block that never closes."""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    if text.startswith("\ufeff"):
        text = text[1:]
    if not text.startswith("---"):
        return {}, text
    end = text.find("\n---", 3)
    if end == -1:
        raise MalformedNote(path)
    fm, body = text[3:end].strip("\n"), text[end + 4:].lstrip("\n")
    meta, key = {}, None
    for line in fm.split("\n"):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        m = re.match(r"^([A-Za-z_]+):\s*(.*)$", line)
        if m:
            key, raw = m.group(1), m.group(2).strip()
            if raw == "":
                meta[key] = []          # block list follows (maybe)
            else:
                meta[key] = _scalar(raw)
            continue
        m = re.match(r"^\s+-\s+(.*)$", line)
        if m and key is not None:
            cur = meta.get(key)
            if not isinstance(cur, list):
                cur = [] if cur in (None, "") else [str(cur)]
            cur.append(_scalar(m.group(1).strip()))
            meta[key] = cur
    return meta, body


def _scalar(raw):
    if raw.startswith("[") and raw.endswith("]"):
        inner = raw[1:-1].strip()
        if not inner:
            return []
        return [_scalar(x.strip()) for x in _split_csv(inner)]
    if (raw.startswith('"') and raw.endswith('"')) or \
       (raw.startswith("'") and raw.endswith("'")):
        return raw[1:-1]
    return raw


def _split_csv(s):
    out, cur, q = [], "", None
    for ch in s:
        if q:
            cur += ch
            if ch == q:
                q = None
        elif ch in "\"'":
            q = ch
            cur += ch
        elif ch == ",":
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out



# ---------- guarded frontmatter rewriting (P4 / T5) ----------
#
# Writers must NEVER regex the whole file: a note BODY may legitimately
# contain lines like `updated: 2020-01-01` inside code fences, and the old
# whole-file re.sub rewrote them whenever the frontmatter lacked the key.
# All key updates are applied to lines strictly INSIDE the frontmatter
# slice, on bytes, preserving a leading UTF-8 BOM and the line-ending style.

def _rewrite_fm_bytes(raw, updates, insert_after=None, after_fm=(), path="?"):
    """Pure transform: return the note file bytes `raw` with the frontmatter
    updates applied — or sys.exit() with a clear refusal, never a partial or
    body-touching rewrite.

    - updates: {key: "value"} → the fm line becomes `key: value` (first
      occurrence INSIDE the fm slice only; body lines are never touched).
    - insert_after: {key: anchor} → when `key` is absent from the fm but is
      in updates, insert `key: value` right after the `anchor:` line (e.g.
      {"verified": "updated"}); an anchor inside the fm slice only.
    - after_fm: extra lines inserted right after the closing '---' (the
      supersede banner), rendered in the file's own line-ending style.
    - A leading UTF-8 BOM and CRLF vs LF endings are detected and preserved.
    - Refusals (non-zero exit, nothing written): frontmatter missing or
      unterminated, not UTF-8, or an updates key with no fm line and no
      insert_after anchor (the T5 case: refuse instead of hitting the body).
    - Known limit (same as the old regex): replacing a key whose value is a
      block list (`key:` + indented `  - item` lines) leaves the continuation
      lines behind. No CLI writer updates a block-list key that way today.
    """
    bom = b"\xef\xbb\xbf" if raw.startswith(b"\xef\xbb\xbf") else b""
    try:
        text = raw[len(bom):].decode("utf-8")
    except UnicodeDecodeError:
        sys.exit(f"knowbox: {path}: not valid UTF-8 — refusing to rewrite")
    nl = "\r\n" if "\r\n" in text else "\n"
    lines = text.split(nl)
    if not lines or lines[0].rstrip("\r") != "---":
        sys.exit(f"knowbox: {path}: no frontmatter — refusing to rewrite (fix the "
                 f"note by hand; the body is never regexed)")
    close = next((i for i in range(1, len(lines))
                  if lines[i].rstrip("\r").startswith("---")), None)
    if close is None:
        sys.exit(f"knowbox: {path}: unterminated frontmatter — refusing to rewrite")
    fm = lines[1:close]
    present = {m.group(1) for m in
               (re.match(r"^([A-Za-z_]+):", ln) for ln in fm) if m}
    out, done = [], set()
    for ln in fm:
        m = re.match(r"^([A-Za-z_]+):", ln)
        key = m.group(1) if m else None
        if key in updates and key not in done:
            out.append(f"{key}: {updates[key]}")
            done.add(key)
        else:
            out.append(ln)
        for ins, anchor in (insert_after or {}).items():
            if (anchor == key and ins in updates
                    and ins not in present and ins not in done):
                out.append(f"{ins}: {updates[ins]}")
                done.add(ins)
    missing = [k for k in updates if k not in done]
    if missing:
        sys.exit(f"knowbox: {path}: frontmatter has no {', '.join(missing)}: "
                 f"line — refusing to rewrite (this is what used to corrupt "
                 f"note bodies; fix the frontmatter by hand)")
    new = lines[:1] + out + lines[close:close + 1] + \
        list(after_fm) + lines[close + 1:]
    return bom + nl.join(new).encode("utf-8")


def _rewrite_frontmatter(path, updates, insert_after=None):
    """Guarded in-place frontmatter rewrite — see _rewrite_fm_bytes."""
    with open(path, "rb") as f:
        raw = f.read()
    _atomic_write_bytes(path, _rewrite_fm_bytes(raw, updates, insert_after,
                                                path=path))


# Refuse-to-publish ledger (T7): absolute paths of notes parse_note()
# rejected. Populated by all_notes(), reported by cmd_lint as ERRORs.
PARSE_FAILURES = []


def all_notes():
    """Yield (relpath, meta, body) for every note in notes/. Skip unreadable
    files; malformed ones (unterminated frontmatter) are recorded in
    PARSE_FAILURES and skipped — they must not reach DB, index, or embeds."""
    for fn in sorted(os.listdir(paths.NOTES)) if os.path.isdir(paths.NOTES) else []:
        if fn.endswith(".md"):
            p = os.path.join(paths.NOTES, fn)
            try:
                meta, body = parse_note(p)
            except MalformedNote as e:
                if p not in PARSE_FAILURES:
                    PARSE_FAILURES.append(p)
                print(f"knowbox: skipping malformed note: {e}", file=sys.stderr)
                continue
            except (OSError, UnicodeDecodeError, ValueError) as e:
                print(f"knowbox: skipping unreadable note {fn}: {e}", file=sys.stderr)
                continue
            yield f"notes/{fn}", meta, body


def _aslist(v):
    if v is None:
        return []
    return v if isinstance(v, list) else [str(v)]


def _asstr(v):
    if isinstance(v, list):
        return ", ".join(map(str, v))
    return "" if v is None else str(v)


def _search_text_parts(meta):
    """(title, summary, tags, aliases, triggers) — the note-text projection
    shared by index.build_db, embed._note_embed_text and kbsearch._vocab.
    Single source: add a field here, all three consumers see it."""
    return (str(meta.get("title", "")), str(meta.get("summary", "")),
            " ".join(map(str, _aslist(meta.get("tags")))),
            " ".join(map(str, _aslist(meta.get("aliases")))),
            " ".join(map(str, _aslist(meta.get("triggers")))))


# ---------- ids ----------

def next_id():
    """High-water mark over filenames ∪ log.md create entries (T4/P9).

    Filenames alone recycled deleted ids — the 2026-08-06 incident: notes
    kb-0023/24/25 were deleted and the next `knowbox new` handed the SAME ids to
    different content (log.md still carries the duplicate create entries —
    lint now WARNs on them). log.md is append-only, so the union can never
    regress even when files are removed. Derived from existing artifacts on
    purpose: no .kb-seq state file that could drift. Runs under cmd_new's
    lock (P7), so the allocate+write window stays atomic."""
    mx = 0
    for fn in os.listdir(paths.NOTES) if os.path.isdir(paths.NOTES) else []:
        m = re.match(r"kb-(\d+)", fn)  # any width — high-water scan, not ID_RE
        if m:
            mx = max(mx, int(m.group(1)))
    if os.path.exists(paths.LOG):
        try:
            with open(paths.LOG, encoding="utf-8") as f:
                for m in re.finditer(r"^## \[[^\]]+\] create \| kb-(\d{4})",
                                     f.read(), re.M):
                    mx = max(mx, int(m.group(1)))
        except OSError:
            pass
    return f"kb-{mx + 1:04d}"


# ---------- project detection (session notes) ----------

def find_path(id_or_slug):
    # ids/slugs only — reject path separators and traversal outright
    if not re.match(r"^[A-Za-z0-9._-]+$", id_or_slug) or ".." in id_or_slug:
        return None
    if ID_RE.match(id_or_slug):
        for fn in sorted(os.listdir(paths.NOTES)):
            if fn.startswith(id_or_slug + "-") or fn == id_or_slug + ".md":
                return os.path.join(paths.NOTES, fn)
    # slug match (containment-checked: never resolves outside notes/)
    cand = os.path.join(paths.NOTES, id_or_slug if id_or_slug.endswith(".md")
                        else id_or_slug + ".md")
    if os.path.exists(cand) and \
            os.path.realpath(cand).startswith(os.path.realpath(paths.NOTES) + os.sep):
        return cand
    hits = [fn for fn in sorted(os.listdir(paths.NOTES)) if id_or_slug in fn]
    if len(hits) > 1:
        print(f"knowbox: ambiguous slug {id_or_slug!r} matches: "
              + ", ".join(h.split("-")[0] + "-" + h.split("-")[1]
                          for h in hits[:6])
              + " — use the full kb-id", file=sys.stderr)
        # sentinel: callers must exit(2) without printing a contradictory
        # "not found" on top of this message
        return "AMBIGUOUS"
    if hits:
        return os.path.join(paths.NOTES, hits[0])
    return None


# ---------- project detection (session notes) ----------

def detect_project(start=None):
    """Nearest enclosing git repo's name; 'machine' for $HOME; cwd basename
    otherwise. Used to stamp/filter type:session notes across projects."""
    d = os.path.realpath(start or os.getcwd())
    cur = d
    while True:
        if os.path.exists(os.path.join(cur, ".git")):
            return os.path.basename(cur)
        parent = os.path.dirname(cur)
        if parent == cur:
            break
        cur = parent
    home = os.path.realpath(os.path.expanduser("~"))
    return "machine" if d == home else os.path.basename(d)


def append_log(op, id_, title):
    entry = f"## [{date.today().isoformat()}] {op} | {id_} | {title}\n"
    # newline="\n" (kb-0117 D9): keep the journal LF on every platform
    with open(paths.LOG, "a", encoding="utf-8", newline="\n") as f:
        f.write(entry)


def git_commit(msg):
    try:
        # staged paths filtered to existing ones (git add fatals on an
        # unmatched pathspec); the CLI ships separately via the knowbox
        # repo, so the data repo's commits carry notes-side files only.
        stage = [p for p in paths.TRACKED_PATHS
                 if os.path.exists(os.path.join(paths.KB, p))]
        # encoding="utf-8" (kb-0117 B4): git stderr can carry UTF-8 paths —
        # text=True alone decodes with the locale codec (cp1252 on Windows)
        r = subprocess.run(["git", "-C", paths.KB, "add", *stage],
                           check=False, capture_output=True, text=True,
                           timeout=60,
                           encoding="utf-8", errors="replace")
        if r.returncode != 0:
            print(f"knowbox: git add failed: {r.stderr.strip()[:120]}",
                  file=sys.stderr)
        r = subprocess.run(["git", "-C", paths.KB, "commit", "-q", "-m", msg],
                           check=False, capture_output=True, text=True,
                           timeout=60,
                           encoding="utf-8", errors="replace")
        if r.returncode not in (0, 1):  # 1 = nothing to commit
            print(f"knowbox: git commit failed: {r.stderr.strip()[:120]}",
                  file=sys.stderr)
        # auto-push when a remote exists; fail soft (offline is fine) but LOUD
        if subprocess.run(["git", "-C", paths.KB, "remote"], capture_output=True,
                          text=True, encoding="utf-8",
                          errors="replace").stdout.strip():
            r = subprocess.run(["git", "-C", paths.KB, "push", "-q"], check=False,
                               capture_output=True, text=True, timeout=60,
                               encoding="utf-8", errors="replace")
            if r.returncode != 0:
                print(f"knowbox: push failed (offline?): {r.stderr.strip()[:120]}",
                      file=sys.stderr)
    except FileNotFoundError:
        print("knowbox: git not found — skipping auto-commit", file=sys.stderr)
    except subprocess.TimeoutExpired:
        print("knowbox: git op timed out (offline?) — will retry next write",
              file=sys.stderr)


def _days_since(iso):
    try:
        return (date.today() - date.fromisoformat(str(iso).strip())).days
    except (ValueError, TypeError):
        return None
