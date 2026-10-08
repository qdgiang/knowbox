#!/usr/bin/env -S uv run --quiet --python 3.12 --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""knowbox — knowledge base CLI. Runs under uv-managed Python 3.12 (PEP 723), never system python. Stdlib only.
Cross-platform: Linux/macOS/native Windows (uv tool shim). Install:
  uv tool install git+https://github.com/qdgiang/knowbox

Commands:
  knowbox search QUERY... [--expand]
                              BM25 full-text (AND-first, OR-degrading, + trigram)
                              + gated semantic fusion & recency on the partial
                              path; typo→semantic→refs rescue on a miss.
                              --expand appends depth-1 link neighbours (opt-in)
  knowbox expand ID|SLUG [--depth 1|2]
                              author-written link neighbourhood (related /
                              supersedes / backlinks) — derived per invocation
  knowbox show ID|SLUG        print one note
  knowbox list [--domain D] [--type T]
  knowbox new --type T --domain D --title "..." [--tags a,b] [--aliases a,b]
         [--status S] [--confidence C] [--source S] [--summary S]
         [--triggers "t1|t2"] [--body TEXT | --body-stdin] [--no-edit]
  knowbox recent [--limit N] [--here] [--project P]
                            newest session notes first (continuity entry point)
  knowbox touch ID             bump updated
  knowbox verify ID... [--evidence "…"]  re-verify: stamps verified:, status:verified
  knowbox supersede OLD --with NEW  atomic supersede: links both notes, retires OLD
  knowbox embed [--force]      build/update note embeddings (semantic recall)
  knowbox vsearch QUERY...     semantic (vector) search over embedded notes
  knowbox reindex             regenerate index.md + FTS db + git commit
  knowbox lint                structural health checks
  knowbox stale [--days N]    rank live notes by staleness (default threshold 90d)
  knowbox churn [--days N]    rank notes by git edit frequency (default window 90d)
  knowbox refs [FILTER]       heading index over refs/: docs + H2 sections +
                              their type:reference map note (UNMAPPED = act)
  knowbox conflicts [--top N] [--min-cos X] [--json]
                              similar live-note pairs (embedding cosine) —
                              candidates for the judge pass (SKILL.md), not verdicts
  knowbox next-id             print next free id
  knowbox init [DIR] [--no-git]  create an empty KB skeleton (default ~/kb)
  knowbox version             print version

Environment: KNOWBOX_ROOT (KB dir; default ~/kb), KNOWBOX_LOCK_TIMEOUT,
  KNOWBOX_EMBED_URL, KNOWBOX_EMBED_MODEL, KNOWBOX_NO_FUSION,
  KNOWBOX_NO_RECENCY, KNOWBOX_NO_HEARTBEAT.
  Deprecated fallback: each KB_* name is honored when its KNOWBOX_* twin is
  unset (one release; dropped at 2.0). ANTHROPIC_AUTH_TOKEN (or the ai-box
  token in ~/.claude/settings.json) authorizes embed/vsearch/fusion calls.
"""
import hashlib
import json
import os
import re
import shlex
import sqlite3
import subprocess
import sys
import tempfile
import time
from contextlib import contextmanager
from datetime import date, datetime

# Windows portability (kb-0117 audit, 2026-09-22): fcntl is Unix-only — a
# top-level import killed EVERY command (even readers) on native Windows.
# msvcrt.locking is the Win32 analog; byte-range locks there are mandatory,
# not advisory, but only kb touches .kb.lock, so the semantics are equivalent.
IS_NT = os.name == "nt"
if IS_NT:
    import msvcrt
else:
    import fcntl

# A3 (kb-0117): on native Windows, piped stdout/stderr falls back to the
# locale codec (cp1252) and the first em-dash raises UnicodeEncodeError —
# fatal for agent harnesses, which ALWAYS pipe. Force UTF-8 on all three
# streams; no-op elsewhere (Python on Linux/macOS is already UTF-8).
if IS_NT:
    for _s in (sys.stdin, sys.stdout, sys.stderr):
        try:
            _s.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):
            pass  # already detached / replaced — nothing sensible to do

def _env(suffix):
    """KNOWBOX_<suffix> wins; KB_<suffix> is the deprecated fallback (one
    release — dropped at 2.0). Returns "" when neither is set."""
    v = os.environ.get(f"KNOWBOX_{suffix}", "").strip()
    if v:
        return v
    return os.environ.get(f"KB_{suffix}", "").strip()


KB = _env("ROOT")
if KB:
    # explicit override (tests, sandboxes, worktrees). NEVER fall back
    # silently — a bogus KNOWBOX_ROOT must fail loudly, not operate on ~/kb.
    if not os.path.isdir(KB):
        print(f"knowbox: KNOWBOX_ROOT={KB!r} is not a directory — refusing to "
              f"fall back to ~/kb (fix KNOWBOX_ROOT or unset it)",
              file=sys.stderr)
        sys.exit(2)
    KB = os.path.realpath(KB)
    _KB_FROM_ENV = True
else:
    KB = os.path.realpath(os.path.expanduser("~/kb"))
    _KB_FROM_ENV = False


def _use_kb(root):
    global KB, NOTES, REFS, DB, INDEX, LOG, EMBED
    KB = root
    NOTES = os.path.join(KB, "notes")
    REFS = os.path.join(KB, "refs")
    DB = os.path.join(KB, ".kb.db")
    INDEX = os.path.join(KB, "index.md")
    LOG = os.path.join(KB, "log.md")
    EMBED = os.path.join(KB, ".kb-embed.json")


_use_kb(KB)


def _looks_like_kb(root):
    return os.path.isdir(root) and os.path.isdir(os.path.join(root, "notes"))

ENUMS = {
    "type": {"fact", "gotcha", "howto", "decision", "reference", "project",
             "session"},
    "status": {"draft", "active", "verified", "superseded"},
    "confidence": {"high", "medium", "low"},
}
REQUIRED = ["id", "title", "type", "domain", "status", "confidence",
            "created", "updated", "summary"]
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ID_RE = re.compile(r"^kb-\d{4}$")
# janitor dead-man's switch: weekly cron touches .kb-heartbeat (kb-0014).
# Weekly schedule + 1 day grace.
HEARTBEAT_MAX_AGE_DAYS = 8


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

def _replace_retry(src, dst, attempts=5, delay=0.1):
    """os.replace with a short retry (kb-0117 C7). POSIX replace-over-an-open-
    file always succeeds (readers keep the old inode); on Windows it raises
    PermissionError while ANY process holds dst open without FILE_SHARE_DELETE
    — a concurrent kb reader mid-parse, an editor, an AV scan. Reader windows
    are milliseconds, so retrying almost always lands; the last error raises.
    Retries only on nt — a POSIX PermissionError is a real permission problem
    and retrying would just delay the traceback."""
    for i in range(attempts):
        try:
            os.replace(src, dst)
            return
        except PermissionError:
            if not IS_NT or i == attempts - 1:
                raise
            time.sleep(delay)


def _atomic_write_bytes(path, data):
    """Write bytes via tempfile.mkstemp(same dir) + os.replace: readers never
    see a half-written note, and a failed write never truncates the original."""
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path) or ".",
                               prefix=".kb-write-")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        os.chmod(tmp, os.stat(path).st_mode & 0o7777)  # mkstemp makes 0600
        _replace_retry(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


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
    for fn in sorted(os.listdir(NOTES)) if os.path.isdir(NOTES) else []:
        if fn.endswith(".md"):
            p = os.path.join(NOTES, fn)
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
    for fn in os.listdir(NOTES) if os.path.isdir(NOTES) else []:
        m = re.match(r"kb-(\d+)", fn)
        if m:
            mx = max(mx, int(m.group(1)))
    if os.path.exists(LOG):
        try:
            with open(LOG, encoding="utf-8") as f:
                for m in re.finditer(r"^## \[[^\]]+\] create \| kb-(\d{4})",
                                     f.read(), re.M):
                    mx = max(mx, int(m.group(1)))
        except OSError:
            pass
    return f"kb-{mx + 1:04d}"


LOCK_TIMEOUT_DEFAULT = 120.0
_LOCK_DEPTH = 0


def _lock_acquire(lf):
    """Non-blocking exclusive lock on the open lock-file handle; raises
    OSError on contention on BOTH platforms (the retry loop in _locked
    depends on that). msvcrt locks a 1-byte range at the current position —
    seek(0) first so acquire/release always address the same range."""
    if IS_NT:
        lf.seek(0)
        msvcrt.locking(lf.fileno(), msvcrt.LK_NBLCK, 1)
    else:
        fcntl.flock(lf, fcntl.LOCK_EX | fcntl.LOCK_NB)


def _lock_release(lf):
    if IS_NT:
        lf.seek(0)
        msvcrt.locking(lf.fileno(), msvcrt.LK_UNLCK, 1)
    else:
        fcntl.flock(lf, fcntl.LOCK_UN)


@contextmanager
def _locked():
    """Exclusive lock for EVERY write command (T3/P7): note writes, derived
    rebuilds, log append and git commit are one critical section, so parallel
    agent sessions serialize instead of interleaving index/log/commits.

    Waits up to KB_LOCK_TIMEOUT seconds (default 120; unparsable or ≤0 values
    fall back to the default) with non-blocking flock retries, then exits
    loudly — a hung writer can never wedge a live session silently. Re-entrant
    within one process: a nested call rides the outer lock (two separate fds
    on the same file would flock-contend even in-process). Readers
    (search/show/list/lint/stale/churn) never lock.

    Documented trade-off: git_commit (incl. push, 60 s timeout) runs INSIDE
    the lock — worst case a concurrent writer waits ~60 s during a network
    hang. Chosen because serialized commit+push keeps log/index/commits
    consistent, and pushes here are ~1 s. `knowbox embed` is the exception: it
    locks only its final merge+write, never the HTTP phase.
    """
    global _LOCK_DEPTH
    if _LOCK_DEPTH:
        _LOCK_DEPTH += 1
        try:
            yield
        finally:
            _LOCK_DEPTH -= 1
        return
    try:
        timeout = float(_env("LOCK_TIMEOUT"))
    except ValueError:
        timeout = LOCK_TIMEOUT_DEFAULT
    if timeout <= 0:
        timeout = LOCK_TIMEOUT_DEFAULT
    lock_path = os.path.join(KB, ".kb.lock")
    lf = open(lock_path, "a")
    deadline = time.monotonic() + timeout
    while True:
        try:
            _lock_acquire(lf)
            break
        except OSError:
            if time.monotonic() >= deadline:
                lf.close()
                # F18 (kb-0117): lsof doesn't exist on Windows
                probe = (f"handle.exe {lock_path}" if IS_NT
                         else f"lsof {lock_path}")
                sys.exit(f"knowbox: lock held >{timeout:g}s by another knowbox process — "
                         f"check: {probe}")
            time.sleep(0.05)
    _LOCK_DEPTH = 1
    try:
        yield
    finally:
        _LOCK_DEPTH = 0
        _lock_release(lf)
        lf.close()


def find_path(id_or_slug):
    # ids/slugs only — reject path separators and traversal outright
    if not re.match(r"^[A-Za-z0-9._-]+$", id_or_slug) or ".." in id_or_slug:
        return None
    if ID_RE.match(id_or_slug):
        for fn in sorted(os.listdir(NOTES)):
            if fn.startswith(id_or_slug + "-") or fn == id_or_slug + ".md":
                return os.path.join(NOTES, fn)
    # slug match (containment-checked: never resolves outside notes/)
    cand = os.path.join(NOTES, id_or_slug if id_or_slug.endswith(".md")
                        else id_or_slug + ".md")
    if os.path.exists(cand) and \
            os.path.realpath(cand).startswith(os.path.realpath(NOTES) + os.sep):
        return cand
    hits = [fn for fn in sorted(os.listdir(NOTES)) if id_or_slug in fn]
    if len(hits) > 1:
        print(f"knowbox: ambiguous slug {id_or_slug!r} matches: "
              + ", ".join(h.split("-")[0] + "-" + h.split("-")[1]
                          for h in hits[:6])
              + " — use the full kb-id", file=sys.stderr)
        return None
    if hits:
        return os.path.join(NOTES, hits[0])
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


# ---------- FTS ----------

def _tmp_in_kb(prefix):
    """Uniquely-named temp file in the KB root (T3/P7). The old FIXED `X.tmp`
    names raced: two concurrent rebuilds removed-then-built into the SAME
    path, corrupting both. mkstemp names never collide."""
    fd, tmp = tempfile.mkstemp(dir=KB, prefix=prefix, suffix=".tmp")
    os.close(fd)
    return tmp


def _swap_into_place(tmp, dest):
    """chmod (mkstemp makes 0600) + atomic replace; clean the tmp on failure."""
    try:
        os.chmod(tmp, 0o644)
        _replace_retry(tmp, dest)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def _note_hashes():
    """{relpath: (sha1, nbytes)} for every .md file under notes/ — ALL of
    them, including superseded and malformed ones (drift detection must not
    depend on parse/index status). Content sha1, not mtime: git clone, rsync
    and touch all make mtime lie (P8 decision note)."""
    out = {}
    for fn in sorted(os.listdir(NOTES)) if os.path.isdir(NOTES) else []:
        if not fn.endswith(".md"):
            continue
        p = os.path.join(NOTES, fn)
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
                  " title, typ, domain, tags, aliases, triggers, summary, body,"
                  " tokenize=\"unicode61 tokenchars '-_.#/'\")")
        c.execute("CREATE VIRTUAL TABLE kb_tri USING fts5(id UNINDEXED, path UNINDEXED,"
                  " title, body, tokenize='trigram')")
        # P10a dual tokenizer: kb_d has the SAME columns but FTS5's default
        # unicode61 (no tokenchars) — punctuation SPLITS, so path compounds
        # like `~/.claude/settings.json` index as claude/settings/json and
        # become findable by their parts (the kb table glues them into one
        # token; see the P10a gotcha note). Both tables are queried and
        # merged by best rank in _fts_exec.
        c.execute("CREATE VIRTUAL TABLE kb_d USING fts5(id UNINDEXED, path UNINDEXED,"
                  " title, typ, domain, tags, aliases, triggers, summary, body)")
        for rel, meta, body in all_notes():
            if meta.get("status") == "superseded":
                continue  # dead knowledge out of the search path
            tags = " ".join(map(str, _aslist(meta.get("tags"))))
            aliases = " ".join(map(str, _aslist(meta.get("aliases"))))
            triggers = " ".join(map(str, _aslist(meta.get("triggers"))))
            row = (str(meta.get("id", "")), rel, str(meta.get("title", "")),
                   str(meta.get("type", "")), str(meta.get("domain", "")),
                   tags, aliases, triggers, str(meta.get("summary", "")), body)
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
    _swap_into_place(tmp, DB)


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
    # explode into unrelated matches once diacritics are stripped)
    return toks, toks


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
    qterms, toks = _fts_query(text)
    if not qterms:
        return [], True
    # FTS5 prefix syntax is `"term"*` — the star must sit OUTSIDE the quotes
    quoted = ['"%s"*' % t.replace('"', '""') if len(t) >= 3
              else '"%s"' % t.replace('"', '""') for t in qterms]
    for query, exact, tri in ((" AND ".join(quoted), True, False),
                              (" OR ".join(quoted), False, True)):
        rows = _fts_run(query, toks, limit, trigram_fallback=tri)
        if rows:
            return rows, exact
    return [], True


def _fts_run(q, toks, limit, trigram_fallback=True):
    try:
        c = sqlite3.connect(DB)
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
            c = sqlite3.connect(DB)
            out = _fts_exec(c, q, toks, limit, trigram_fallback)
        except sqlite3.Error as e:
            print(f"knowbox: FTS error ({e}) — run 'knowbox reindex'", file=sys.stderr)
            return []
    c.close()
    return out


def _fts_exec(c, q, toks, limit, trigram_fallback=True):
    out = []
    weights = "0.0, 0.0, 10.0, 1.0, 1.0, 5.0, 8.0, 6.0, 4.0, 1.0"  # id,path,title,typ,domain,tags,aliases,triggers,summary,body
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
                "SELECT id, path, title, snippet(%s, 9, '>>>', '<<<', '…',"
                " 12), bm25(%s, %s) FROM %s WHERE %s MATCH ?"
                " ORDER BY bm25(%s, %s) LIMIT ?"
                % (table, table, weights, table, table, table, weights),
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
                    "SELECT id, path, title, snippet(kb_tri, 3, '>>>',"
                    " '<<<', '…', 12), bm25(kb_tri) FROM kb_tri"
                    " WHERE kb_tri MATCH ? ORDER BY bm25(kb_tri) LIMIT ?",
                    (tq, limit)):
                out.append(r)
    return out


# ---------- index + log + git ----------

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
    refs = sorted(f for f in os.listdir(REFS) if not f.startswith(".")) \
        if os.path.isdir(REFS) else []
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
    _swap_into_place(tmp, INDEX)
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
    if not os.path.exists(DB):
        rep["db_missing"] = True
        return rep
    try:
        # C8 (kb-0117): plain connect, NOT a hand-built read-only `file:` URI —
        # on Windows that form depends on backslash tolerance and any ?/#/% in
        # the path breaks URI parsing SILENTLY into db_missing=True
        # (rebuild-on-every-search forever). Freshness only ever SELECTs; the
        # os.path.exists guard above already ran.
        c = sqlite3.connect(DB)
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
    if not os.path.exists(INDEX):
        rep["index_drift"] = True
        return rep
    try:
        idx = open(INDEX, encoding="utf-8").read()
    except OSError:
        rep["index_drift"] = True
        return rep
    on_disk_ids = set()
    for p in disk:
        m = re.match(r"kb-\d{4}", os.path.basename(p))
        if m:
            on_disk_ids.add(m.group(0))
    listed = re.findall(r"^- \[(kb-\d{4}) ·", idx, re.M)
    rep["orphans"] = sorted({i for i in listed if i not in on_disk_ids})
    if not rep["orphans"] and not rep["drifted"]:
        # byte-exact content check only when nothing else explains the diff
        rep["index_drift"] = idx != "\n".join(_index_lines()[0])
    rep["fresh"] = not (rep["drifted"] or rep["orphans"]
                        or rep["index_drift"])
    return rep


def append_log(op, id_, title):
    entry = f"## [{date.today().isoformat()}] {op} | {id_} | {title}\n"
    # newline="\n" (kb-0117 D9): keep the journal LF on every platform
    with open(LOG, "a", encoding="utf-8", newline="\n") as f:
        f.write(entry)


def git_commit(msg):
    try:
        # staged paths filtered to existing ones (git add fatals on an
        # unmatched pathspec); the CLI ships separately via the knowbox
        # repo, so the data repo's commits carry notes-side files only.
        paths = [p for p in ("notes", "refs", "raw", "index.md", "log.md",
                             "SCHEMA.md", ".gitignore", ".gitattributes",
                             "lint-waivers.md")
                 if os.path.exists(os.path.join(KB, p))]
        # encoding="utf-8" (kb-0117 B4): git stderr can carry UTF-8 paths —
        # text=True alone decodes with the locale codec (cp1252 on Windows)
        r = subprocess.run(["git", "-C", KB, "add", *paths],
                           check=False, capture_output=True, text=True,
                           encoding="utf-8", errors="replace")
        if r.returncode != 0:
            print(f"knowbox: git add failed: {r.stderr.strip()[:120]}",
                  file=sys.stderr)
        r = subprocess.run(["git", "-C", KB, "commit", "-q", "-m", msg],
                           check=False, capture_output=True, text=True,
                           encoding="utf-8", errors="replace")
        if r.returncode not in (0, 1):  # 1 = nothing to commit
            print(f"knowbox: git commit failed: {r.stderr.strip()[:120]}",
                  file=sys.stderr)
        # auto-push when a remote exists; fail soft (offline is fine) but LOUD
        if subprocess.run(["git", "-C", KB, "remote"], capture_output=True,
                          text=True, encoding="utf-8",
                          errors="replace").stdout.strip():
            r = subprocess.run(["git", "-C", KB, "push", "-q"], check=False,
                               capture_output=True, text=True, timeout=60,
                               encoding="utf-8", errors="replace")
            if r.returncode != 0:
                print(f"knowbox: push failed (offline?): {r.stderr.strip()[:120]}",
                      file=sys.stderr)
    except (FileNotFoundError, subprocess.TimeoutExpired):
        print("knowbox: git push timed out (offline?) — will retry next write",
              file=sys.stderr)


# ---------- embeddings (semantic recall; aibox text-embedding-v4) ----------

EMBED_URL = _env("EMBED_URL") or "https://api.ai-box.vn/v1/embeddings"
EMBED_MODEL = _env("EMBED_MODEL") or "text-embedding-v4"

# Gated RRF fusion (kb-0107 survey gap #1; fixed k=60 — adaptive weighting
# refuted 0-3, kb-0107 §3). The vector channel fires ONLY on the OR/partial
# lexical path, so kb-0038's precision-first guarantee holds structurally:
# with w_vec=0.5 a vector-only doc at rank 1 scores 0.5/61 ≈ 0.0082, below
# EVERY lexical doc (worst rank 6 → 1/66 ≈ 0.0152) — vector-only rows append
# to the tail and can never interleave, while dual-channel agreement re-ranks
# (lex#3 + vec#1 ≈ 0.0241 > lex#1 alone ≈ 0.0164). cos floors per kb-0024
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

# Recency weighting (kb-0110 gap #2 MODEST GO; canonical design per deep-dive
# §7 — ES function_score decay + OpenClaw post-merge exponential): post-fusion
# multiplier on the RRF score, fusion path ONLY — AND/exact, typo, semantic-
# rescue and pure-BM25 rows never see it (Yahoo query-gated-freshness
# precedent, kb-0110 §7). mult = max(FLOOR, 2 ** (-(age - OFFSET) / HALF_LIFE))
# with age in days from `updated:` (fallback `created:`, cmd_stale precedent);
# missing/unparseable → neutral 1.0 (fail toward evergreen). Age clamped ≥ 0:
# future dates give exactly 1.0, never a boost (guards the ES decay-inversion
# pitfall). OFFSET = grace band where fresh notes are indistinguishable;
# FLOOR = an old-but-correct note is never buried (kb-0038). Applied
# BLOCKWISE (lexical rows before vector-only) so kb-0109's structural
# "vector-only never outranks lexical" guarantee survives the multiplier —
# under a global sort a fresh vec-only row (1.0 × 0.5/61 ≈ 0.0082) WOULD
# overtake a floored lex#2 (0.5 × 1/62 ≈ 0.0081). Kill switch: KB_NO_RECENCY
# (KB_NO_FUSION pattern). Params are kb-0110's inferred starting point —
# recalibrate against miss-log soak data.
RECENCY_HALF_LIFE_DAYS = 90
RECENCY_OFFSET_DAYS = 30
RECENCY_FLOOR = 0.5

# Opt-in derived-graph expansion (kb-0110 gap #4 CONDITIONAL GO; IWE /
# Basic-Memory pattern): adjacency over AUTHOR-WRITTEN links only — no LLM
# extraction, no graph DB, no cache; rebuilt per invocation from all_notes()
# (~110 notes). Benchmarks (kb-0110 §3): graph expansion hurts single-hop
# lookups and helps only multi-hop/relational queries → STRICTLY opt-in
# (`knowbox expand`, `knowbox search --expand`); default search output stays
# byte-identical (kb-0038 precision-first; goldens untouched by
# construction). Caps mirror Basic-Memory's depth-1 / max-per-level defaults.
EXPAND_MAX_DEPTH = 2            # `knowbox expand --depth` ceiling
EXPAND_LEVEL_CAP = 12           # `knowbox expand`: max neighbours per level
EXPAND_SEARCH_SEEDS = 3         # search --expand: top-N displayed hits seed it
EXPAND_SEARCH_PER_SEED = 4      # search --expand: max neighbours per seed
EXPAND_SEARCH_TOTAL = 8         # search --expand: max neighbour rows total

_EDGE_RANK = {"supersedes": 0, "related": 1, "mention": 2}  # fold precedence


def _embed_token():
    tok = os.environ.get("ANTHROPIC_AUTH_TOKEN") or \
          os.environ.get("ANTHROPIC_API_KEY")
    if tok:
        return tok
    try:  # same aibox token Claude Code uses (settings.json env block)
        with open(os.path.expanduser("~/.claude/settings.json"),
                  encoding="utf-8") as f:
            return json.load(f).get("env", {}).get("ANTHROPIC_AUTH_TOKEN", "")
    except (OSError, ValueError):
        return ""


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
    tok = _embed_token()
    if not tok:
        raise RuntimeError("no auth token (set ANTHROPIC_AUTH_TOKEN or use the "
                           "aibox profile)")
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
    parts = [str(meta.get("title", "")), str(meta.get("summary", "")),
             " ".join(map(str, _aslist(meta.get("tags")))),
             " ".join(map(str, _aslist(meta.get("aliases")))),
             " ".join(map(str, _aslist(meta.get("triggers")))),
             body[:2000]]
    return "\n".join(p for p in parts if p)


def _cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = sum(x * x for x in a) ** 0.5
    nb = sum(y * y for y in b) ** 0.5
    return dot / (na * nb) if na and nb else 0.0


def _load_embed():
    if not os.path.exists(EMBED):
        return {}
    try:
        with open(EMBED, encoding="utf-8") as f:
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
    """Pure: recency multiplier for a note age in days (kb-0110 §9 design).

    None (missing/unparseable date) → neutral 1.0, fail toward evergreen;
    negative ages clamp to 0 → 1.0 (future dates never boost — the ES
    decay-inversion pitfall, kb-0110 §7); age ≤ RECENCY_OFFSET_DAYS →
    exactly 1.0; then exponential half-life decay bounded at RECENCY_FLOOR.
    Never printed anywhere (kb-0038 labels-not-scores).
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
    below every lexical row, so kb-0109's structural guarantee survives the
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
    the AND path stays offline/instant/golden-stable (kb-0038 precision-first;
    analog of kbx's strong-signal fast path). Fused rows are then recency-
    weighted (default-on; KB_NO_RECENCY disables — see RECENCY_* constants).
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
        # neutral multiplier (age None). KB_NO_RECENCY restores the pre-
        # recency I/O profile exactly (parse only when snip is None).
        recency_on = not _env("NO_RECENCY")
        ages = [None] * len(fused)
        for i, (nid, rel, title, snip, score) in enumerate(fused):
            meta = body = None
            if snip is None or recency_on:
                try:
                    meta, body = parse_note(os.path.join(KB, rel))
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
        _swap_into_place(tmp, EMBED)
    print(f"embedded {len(todo)} note(s); {len(store)} total "
          f"(model={EMBED_MODEL})")


def cmd_vsearch(args):
    if not args:
        sys.exit("usage: knowbox vsearch QUERY...")
    if not os.path.exists(EMBED):
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


# ---------- commands ----------

# ---------- P10b: typo expansion + refs-grep (zero-hit rescue layers) ----------

def _vocab():
    """Set of lowercase tokens present in live notes — the typo-correction
    dictionary. Built in-memory per zero-hit search (~ms at this corpus
    size). Includes both compound tokens (as the kb tokenizer sees them) and
    punctuation-split parts (as kb_d sees them), so a typo of either form
    can be corrected."""
    vocab = set()
    for _rel, meta, body in all_notes():
        text = " ".join([
            str(meta.get("title", "")), str(meta.get("summary", "")),
            " ".join(map(str, _aslist(meta.get("tags")))),
            " ".join(map(str, _aslist(meta.get("aliases")))),
            " ".join(map(str, _aslist(meta.get("triggers")))),
            body])
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
    if not toks or not os.path.isdir(REFS):
        return out
    for fn in sorted(os.listdir(REFS)):
        if not fn.endswith(".md") or fn.startswith("."):
            continue
        try:
            with open(os.path.join(REFS, fn), encoding="utf-8",
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
            m = re.match(r"(kb-\d{4})", os.path.basename(rel))
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
        for t in (set(re.findall(r"\[\[(kb-\d{4})\]\]", body)) & known) - {nid}:
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
    if len(args) == 1 and re.match(r"^kb-\d+$", args[0].lower()):
        p = find_path(args[0].lower())
        if p:
            # forward slashes on every platform (kb-0117 E10): relpath emits
            # backslashes on Windows, breaking goldens and path conventions
            rel = os.path.relpath(p, KB).replace(os.sep, "/")
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
    _qt, toks = _fts_query(query)
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
           if r[0] >= 0.40]  # rescue
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
        with open(os.path.join(KB, ".search-misses.log"), "a",
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
    m = re.match(r"(kb-\d{4})", os.path.basename(p)) if p else None
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


def cmd_show(args):
    if not args:
        sys.exit("usage: knowbox show ID|SLUG")
    p = find_path(args[0])
    if not p:
        sys.exit(f"not found: {args[0]}")
    # deliberately RAW (never parse_note, P4/T7): a malformed note —
    # unterminated frontmatter, BOM, CRLF — must stay readable so it can be
    # diagnosed and fixed.
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
        path = os.path.join(NOTES, fn)
        # newline="\n" (kb-0117 D9): notes are LF everywhere; _rewrite_fm_bytes
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
            git_commit(f"kb: create {nid} — {opts['title']}")
    if interactive:
        # $EDITOR may take minutes — the flock must NOT be held across it
        # (P7). The note file already exists with its id, so a concurrent
        # `knowbox new` cannot collide; re-lock for the derived rebuilds only.
        try:  # $EDITOR may contain arguments ("vim -u NONE")
            # kb-0117 F16: POSIX shlex eats backslashes — an EDITOR like
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
            git_commit(f"kb: create {nid} — {opts['title']}")
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
        git_commit(f"kb: touch {args[0]}")
    print(p)


def cmd_reindex(_args):
    with _locked():  # P7: rebuild + log + commit are one critical section
        old_index = None
        if os.path.exists(INDEX):
            try:
                with open(INDEX, encoding="utf-8") as f:
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
            with open(INDEX, encoding="utf-8") as f:
                new_index = f.read()
        except OSError:
            pass
        staged = ("notes", "refs", "raw", "index.md", "log.md", "SCHEMA.md",
                  ".gitignore", ".gitattributes", "lint-waivers.md")
        # B5 (kb-0117): git missing → clean message, and a FAILED status must
        # count as dirty — the old unchecked rc read a non-git tree as clean
        # and silently skipped the commit
        try:
            r = subprocess.run(
                ["git", "-C", KB, "status", "--porcelain", "--", *staged],
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
        git_commit("kb: reindex")
    print(f"index.md + FTS rebuilt from {n} notes")


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
    path = os.path.join(KB, "lint-waivers.md")
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
    if os.path.exists(LOG):
        try:
            with open(LOG, encoding="utf-8") as f:
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
        errs.append(f"{os.path.relpath(pf, KB).replace(os.sep, '/')}: "
                    f"unterminated frontmatter "
                    f"(excluded from index/search)")
    n_disk = len([fn for fn in os.listdir(NOTES) if fn.endswith(".md")]) \
        if os.path.isdir(NOTES) else 0
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
    # janitor dead-man's switch (kb-0014): the weekly cron run touches
    # .kb-heartbeat; missing/old means the cron is dead — fail loudly.
    # F17 (kb-0117): the janitor lives on the Fedora server only — a replica
    # clone (native Windows, or any secondary machine) has no cron and the
    # heartbeat is gitignored, so a MISSING heartbeat there must not make lint
    # permanently red. Opt out with KB_NO_HEARTBEAT=1; on Windows a
    # never-present heartbeat auto-exempts (an EXISTING but stale one still
    # errors — someone installed a janitor there and it died).
    hb = os.path.join(KB, ".kb-heartbeat")
    hb_disabled = _env("NO_HEARTBEAT") not in ("", "0", "false", "no")
    hb_expected = not hb_disabled and (not IS_NT or os.path.exists(hb))
    if not hb_expected:
        pass  # replica client without a janitor — heartbeat not expected
    elif not os.path.exists(hb):
        errs.append("janitor heartbeat missing — weekly cron dead? run: "
                    "bash ~/dotfiles/scripts/kb-janitor.sh (see kb-0014)")
    else:
        age_days = (datetime.now().timestamp() - os.path.getmtime(hb)) / 86400
        if age_days > HEARTBEAT_MAX_AGE_DAYS:
            errs.append(f"janitor heartbeat stale ({age_days:.0f}d > "
                        f"{HEARTBEAT_MAX_AGE_DAYS}d) — weekly cron dead? run: "
                        f"bash ~/dotfiles/scripts/kb-janitor.sh (see kb-0014)")
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
            git_commit(f"kb: verify {', '.join(touched)} — {ev}")
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
        # related: cross-add (kb-0086 leftover, fixed 2026-09-11): the banner
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
        git_commit(f"kb: supersede {oid} → {nid}")
    print(f"{oid} superseded by {nid}")


def _days_since(iso):
    try:
        return (date.today() - date.fromisoformat(str(iso).strip())).days
    except (ValueError, TypeError):
        return None


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


def cmd_refs(args):
    """knowbox refs [FILTER] — heading index over refs/: each doc's word count,
    H2 sections, and the type:reference map note covering it. Docs with no
    map note are flagged UNMAPPED (actionable: SCHEMA.md § Large docs)."""
    opts = _flags(args)
    filt = " ".join(opts.get("_pos", [])).strip().lower()
    if not os.path.isdir(REFS):
        print("knowbox refs: no refs/ directory — nothing ingested yet")
        return
    docs = sorted(f for f in os.listdir(REFS)
                  if f.endswith(".md") and os.path.isfile(os.path.join(REFS, f)))
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
            with open(os.path.join(REFS, fn), encoding="utf-8",
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
            ["git", "-C", KB, "-c", "core.quotePath=false",
             "-c", "diff.noprefix=false", "-c", "diff.external=", "log",
             f"--since={window} days ago", "-U0", "--no-color", "-M",
             "--pretty=format:%x01%cs", "--", "notes/"],
            capture_output=True, text=True,
            encoding="utf-8", errors="replace")
    except FileNotFoundError:
        sys.exit("knowbox churn: git not found")
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
            m = re.search(r"kb-\d{4}", state["file"])
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
    if not os.path.exists(EMBED):
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


def _version():
    try:
        from importlib import metadata
        return metadata.version("knowbox")
    except Exception:
        return "knowbox dev (script mode)"


_GITATTR_INIT = "* text=auto eol=lf\n"
_GITIGNORE_INIT = """\
# knowbox data repo — derived files (regenerated from notes/ by the CLI)
.kb.db
.kb.db-wal
.kb.db-shm
.kb-embed.json
.kb.lock
.lint-cron.log
.embed-cron.log
.kb-heartbeat
__pycache__/
"""
_SCHEMA_STUB = """\
# SCHEMA — note conventions

Frontmatter (YAML-ish, one key per line):
  id: kb-NNNN (issued by `knowbox new`; never reuse)
  title / type: fact|reference|procedure|decision|session|project
  domain / status: active|draft|superseded|verified / created / updated
  summary: one line — this is the index routing text
Optional: tags, aliases, related ([[id]] links), supersedes, source,
confidence, triggers, verified.

Body: free markdown. Large docs go whole into refs/ with a type: reference
section-map note here. Full contract: https://github.com/qdgiang/knowbox
"""


def cmd_init(args):
    """Create an empty KB skeleton (default ~/kb). Refuses to touch anything
    that already looks like a KB or a non-empty directory."""
    fl = _flags(args)
    target = fl.get("_first") or os.path.expanduser("~/kb")
    target = os.path.realpath(target)
    if _looks_like_kb(target):
        sys.exit(f"knowbox: {target} already looks like a KB (has notes/) — "
                 f"refusing to init over it")
    if os.path.isdir(target) and os.listdir(target):
        sys.exit(f"knowbox: {target} is not empty — refusing to init")
    os.makedirs(target, exist_ok=True)
    _use_kb(target)
    for sub in ("notes", "refs", "raw"):
        d = os.path.join(target, sub)
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, ".gitkeep"), "a").close()
    def _w(path, text):
        # newline="\n" (kb-0117 D9): skeleton files stay LF everywhere
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    if not os.path.exists(os.path.join(target, "SCHEMA.md")):
        _w(os.path.join(target, "SCHEMA.md"), _SCHEMA_STUB)
    _w(os.path.join(target, ".gitignore"), _GITIGNORE_INIT)
    _w(os.path.join(target, ".gitattributes"), _GITATTR_INIT)
    append_log("init", "—", f"KB skeleton created by {_version()}")
    with _locked():
        build_db()
        build_index()
    # git setup AFTER the derived-file build so the first commit also carries
    # the generated index.md (db/embed stay untracked via .gitignore)
    git_ok = False
    if not fl.get("no-git"):
        try:
            subprocess.run(["git", "--version"], check=True,
                           capture_output=True)
            git_ok = True
        except (OSError, subprocess.CalledProcessError):
            print("knowbox: git not found — skipping repo setup "
                  "(KB works without it; no auto-commits)", file=sys.stderr)
    if git_ok:
        def _git(*argv):
            r = subprocess.run(["git", "-C", target, *argv], check=False,
                               capture_output=True, text=True,
                               encoding="utf-8", errors="replace")
            if r.returncode != 0:
                sys.exit(f"knowbox: git {' '.join(argv[:1])} failed: "
                         f"{r.stderr.strip()[:200]}")
        _git("init", "-q", "-b", "main")
        # repo-local identity (kb-0007): global user.name/email may be unset
        _git("config", "user.name", "kb")
        _git("config", "user.email", "kb@localhost")
        git_commit("kb: init — empty KB skeleton")
    print(f"knowbox: initialized empty KB at {target}"
          + ("" if git_ok else " (no git repo)"))
    if not _env("ROOT"):
        print("knowbox: this KB is the default root (~/kb). Point elsewhere "
              "with KNOWBOX_ROOT=<dir>.")


def _flags(args):
    out, i = {}, 0
    while i < len(args):
        a = args[i]
        if a.startswith("--"):
            k = a[2:]
            if "=" in k:
                k, v = k.split("=", 1)
                out[k] = v
                i += 1
            elif k in ("body-stdin", "no-edit", "json", "no-git"):
                # boolean flags: presence = true, never consume the next arg
                # ("json" added at P12 — `--json --top 1` used to eat --top)
                out[k] = True
                i += 1
            elif i + 1 < len(args):
                out[k] = args[i + 1]
                i += 2
            else:
                out[k] = ""  # dangling flag: let caller validation catch it
                i += 1
        else:
            out.setdefault("_pos", []).append(a)
            i += 1
    if "_pos" in out and out["_pos"]:
        out["_first"] = out["_pos"][0]
    return out


CMDS = {"search": cmd_search, "expand": cmd_expand, "show": cmd_show,
        "list": cmd_list,
        "recent": cmd_recent, "new": cmd_new, "touch": cmd_touch,
        "verify": cmd_verify,
        "supersede": cmd_supersede, "embed": cmd_embed,
        "vsearch": cmd_vsearch, "reindex": cmd_reindex,
        "lint": cmd_lint, "stale": cmd_stale, "churn": cmd_churn,
        "refs": cmd_refs,
        "conflicts": cmd_conflicts, "next-id": cmd_next_id,
        "init": cmd_init}


def main(argv=None):
    args = list(sys.argv[1:]) if argv is None else list(argv)
    if not args or args[0] in ("-h", "--help", "help"):
        print(__doc__)
        return
    if args[0] in ("version", "--version", "-V"):
        print(_version())
        return
    cmd = args[0]
    if cmd not in CMDS:
        sys.exit(f"unknown command {cmd!r}. try: {', '.join(CMDS)}")
    # Loud fallback validation (locked decision): with KNOWBOX_ROOT unset the
    # root is ~/kb — every command except `init` refuses to run against a
    # directory that doesn't look like a KB instead of silently auto-creating.
    if cmd != "init" and not _KB_FROM_ENV and not _looks_like_kb(KB):
        print(f"knowbox: {KB} is not a knowledge base (no notes/ dir) — "
              f"run `knowbox init` to create one, or export "
              f"KNOWBOX_ROOT=<dir> to point at an existing KB",
              file=sys.stderr)
        sys.exit(2)
    CMDS[cmd](args[1:])


if __name__ == "__main__":
    main()
