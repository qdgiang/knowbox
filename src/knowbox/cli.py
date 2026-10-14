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
  knowbox raw-backlog         list unreviewed raw/ entries (## heading blocks)
  knowbox raw-mark <file> <N> --note kb-XXXX | --rejected "reason"
                              stamp an entry's `> state:` review marker
  knowbox next-id             print next free id
  knowbox init [DIR] [--no-git]  create an empty KB skeleton (default ~/knowbox)
  knowbox version             print version

Environment: KNOWBOX_ROOT (KB dir; default ~/knowbox), KNOWBOX_LOCK_TIMEOUT,
  KNOWBOX_EMBED_URL, KNOWBOX_EMBED_MODEL, KNOWBOX_NO_FUSION,
  KNOWBOX_NO_RECENCY, KNOWBOX_NO_HEARTBEAT.
  ANTHROPIC_AUTH_TOKEN (or the ai-box token in ~/.claude/settings.json)
  authorizes embed/vsearch/fusion calls.
"""
import os
import sys

# PEP 723 script mode (`uv run --script src/knowbox/cli.py`): the package dir
# is not on sys.path for a direct script run, and helper modules are imported
# as top-level modules — add the dir explicitly. Works identically for the
# installed entry point (the dir is already importable; the insert is a
# harmless no-op) and for direct `python cli.py`.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# Flat-name compatibility: probe tests (test_portability, test_expand,
# test_search_fusion, test_search_recency) load cli.py via SourceFileLoader
# and touch private helpers on ITS namespace — re-export every moved name
# explicitly (import * drops underscore names).
import paths
from paths import (                          # noqa: F401
    _KB_FROM_ENV, _env, _use_kb, _looks_like_kb,
    ENUMS, REQUIRED, DATE_RE, ID_RE, HEARTBEAT_MAX_AGE_DAYS)
from sysutil import (                        # noqa: F401
    IS_NT, _replace_retry, _atomic_write_bytes, _tmp_in_kb,
    _swap_into_place, LOCK_TIMEOUT_DEFAULT, _lock_acquire, _lock_release,
    _locked)
from notes import (                          # noqa: F401
    MalformedNote, parse_note, _rewrite_fm_bytes, _rewrite_frontmatter,
    PARSE_FAILURES, all_notes, _aslist, _asstr, next_id, find_path,
    detect_project, append_log, git_commit, _days_since)
from index import (                          # noqa: F401
    _note_hashes, build_db, _index_lines, build_index, _freshness_report)
from embed import (                          # noqa: F401
    EMBED_MODEL, _embed_texts, _note_embed_text, _load_embed, _semantic,
    _rrf_fuse, _recency_multiplier, _apply_recency, _fusion_pass,
    FUSE_TIMEOUT, _EDGE_RANK, EXPAND_LEVEL_CAP, EXPAND_SEARCH_SEEDS,
    EXPAND_SEARCH_PER_SEED, EXPAND_SEARCH_TOTAL, EXPAND_MAX_DEPTH)
# NOTE: the 8 root globals (KB/NOTES/REFS/RAW/DB/INDEX/LOG/EMBED) are NOT
# imported here — cmd_init re-roots them via paths._use_kb, so every use in
# this file must late-bind as paths.X (a from-import would freeze a stale
# snapshot at import time).
from kbsearch import (                       # noqa: F401
    _fts_query, search, _fts_run, _fts_exec, _vocab, _edits1, _typo_fix,
    _refs_grep, _build_graph, _fmt_nb, _print_neighborhood,
    _print_search_expansion)





from cmdsearch import cmd_expand, cmd_search
from cmdwrite import (cmd_list, cmd_new, cmd_next_id, cmd_recent, cmd_reindex,
                      cmd_show, cmd_supersede, cmd_touch, cmd_verify)
from initcmd import _version, cmd_init
from maintain import cmd_lint, cmd_raw_backlog, cmd_raw_mark
from reports import cmd_churn, cmd_conflicts, cmd_refs, cmd_stale
from sembed import cmd_embed, cmd_vsearch

CMDS = {"search": cmd_search, "expand": cmd_expand, "show": cmd_show,
        "list": cmd_list,
        "recent": cmd_recent, "new": cmd_new, "touch": cmd_touch,
        "verify": cmd_verify,
        "supersede": cmd_supersede, "embed": cmd_embed,
        "vsearch": cmd_vsearch, "reindex": cmd_reindex,
        "lint": cmd_lint, "stale": cmd_stale, "churn": cmd_churn,
        "refs": cmd_refs,
        "conflicts": cmd_conflicts, "next-id": cmd_next_id,
        "raw-backlog": cmd_raw_backlog, "raw-mark": cmd_raw_mark,
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
    # root is ~/knowbox — every command except `init` refuses to run against a
    # directory that doesn't look like a KB instead of silently auto-creating.
    if cmd != "init" and not _KB_FROM_ENV and not _looks_like_kb(paths.KB):
        print(f"knowbox: {paths.KB} is not a knowledge base (no notes/ dir) — "
              f"run `knowbox init` to create one, or export "
              f"KNOWBOX_ROOT=<dir> to point at an existing KB",
              file=sys.stderr)
        sys.exit(2)
    CMDS[cmd](args[1:])


if __name__ == "__main__":
    main()

