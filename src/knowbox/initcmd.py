"""knowbox initcmd: the init command (KB skeleton + git setup) and the
version helper. Named initcmd — an `init.py` reads as a mistyped __init__.py.
"""
import os
import subprocess
import sys

from _flags import _flags
from index import build_db, build_index
from notes import append_log, git_commit
from paths import _env, _looks_like_kb, _use_kb
from sysutil import _locked


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

raw/ (staging, append-only): `## ` headings delimit entries (bodies use
### and deeper ONLY). Reviewed entries carry a marker as the FIRST line
under the heading: `> state: ingested kb-XXXX (date)` / `> state: rejected
<reason>` — `knowbox raw-backlog` lists unmarked entries, `knowbox raw-mark
<file> <N> --note kb-XXXX | --rejected "reason"` stamps them. Never delete
entries; legacy `> ingested:`/`> rejected:` lines still resolve an entry.
"""


def cmd_init(args):
    """Create an empty KB skeleton (default ~/knowbox). Refuses to touch anything
    that already looks like a KB or a non-empty directory."""
    fl = _flags(args)
    target = fl.get("_first") or os.path.expanduser("~/knowbox")
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
        git_commit("knowbox: init — empty KB skeleton")
    print(f"knowbox: initialized empty KB at {target}"
          + ("" if git_ok else " (no git repo)"))
    if not _env("ROOT"):
        print("knowbox: this KB is the default root (~/knowbox). Point elsewhere "
              "with KNOWBOX_ROOT=<dir>.")
