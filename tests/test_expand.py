"""Opt-in derived-graph expansion — kb-0110 gap #4 (IWE / Basic-Memory pattern).

Design under test (src/knowbox/cli.py: _build_graph / _print_neighborhood /
_print_search_expansion / cmd_expand):
  - Adjacency is derived per invocation from AUTHOR-WRITTEN links only
    (frontmatter related:/supersedes: + body [[kb-XXXX]]); duplicate pairs
    fold to the strongest rel (supersedes > related > mention); dead ids are
    dropped; malformed notes are never nodes.
  - Both surfaces are strictly OPT-IN: without `--expand` the search output
    is byte-identical (goldens untouched by construction), and expansion
    rows are indented so search_ids (`^kb-`) can never match them.
  - Search path: top-3 displayed hits seed depth-1 expansion; dedup vs
    displayed ids and across seeds; ≤4 rows/seed, ≤8 total. Rescue path
    never expands. Superseded neighbours are shown, tagged `(superseded)`.
  - Filesystem only: ZERO HTTP/embedding calls on any expansion surface.

House style: subprocess kb_run only — never import the CLI module in the pytest
process; the pure-helper probe runs its import in a SUBPROCESS.

Fixture graph (on top of conftest MINI_NOTES links 9001↔9002, 9004↔9005,
9008→9001, 9009→9001):
    kb-9110 →related kb-9111 →related kb-9112     (depth-2 chain)
    kb-9113 body-only [[kb-9001]]                 (mention edge)
    kb-9114 related: [kb-8888]                    (dead id → dropped)
    kb-9120/9121/9122 "zebra … hub"               (cap fixtures)
    kb-9102 unterminated frontmatter              (malformed → never a node)
"""
import hashlib
import http.server
import json
import math
import os
import re
import subprocess
import threading
from pathlib import Path

import pytest

from conftest import CLI_SRC, UV, _fm, build_sandbox, kb_run, search_ids

REPO = Path(__file__).resolve().parent.parent

HEADER = "(graph neighbours of top hits"
EXPAND_ROW = re.compile(r"^    (kb-\d{4})\b", re.M)   # indented id lines only


def expand_ids(stdout):
    return EXPAND_ROW.findall(stdout)


def _add(root, nid, slug, title, body="", **kw):
    (root / "notes" / f"{nid}-{slug}.md").write_text(
        _fm(nid=nid, title=title, **kw) + body, encoding="utf-8")


@pytest.fixture(scope="module")
def sbx(tmp_path_factory):
    root = build_sandbox(tmp_path_factory.mktemp("expand") / "kb",
                         corpus="mini", git=False,
                         extra_notes=("unterminated",))   # kb-9102 malformed
    _add(root, "kb-9110", "chain-alpha", "Chain note alpha",
         related=("kb-9111",), body="Alpha links to beta.\n")
    _add(root, "kb-9111", "chain-beta", "Chain note beta",
         related=("kb-9112",), body="Beta links to gamma.\n")
    _add(root, "kb-9112", "chain-gamma", "Chain note gamma",
         body="Gamma ends the chain.\n")
    _add(root, "kb-9113", "mention-only", "Mention-only note",
         body="See [[kb-9001]] for the fact.\n")
    _add(root, "kb-9114", "dead-link", "Note with a dead related id",
         related=("kb-8888",), body="Points at nothing.\n")
    _add(root, "kb-9120", "zebra-alpha-hub", "zebra alpha hub",
         related=("kb-9001", "kb-9002", "kb-9003", "kb-9005", "kb-9006",
                  "kb-9008"),
         body="Zebra hub alpha.\n")
    _add(root, "kb-9121", "zebra-beta-hub", "zebra beta hub",
         related=("kb-9006", "kb-9007", "kb-9008", "kb-9009", "kb-9110",
                  "kb-9111"),
         body="Zebra hub beta.\n")
    _add(root, "kb-9122", "zebra-gamma-hub", "zebra gamma hub",
         related=("kb-9110", "kb-9111", "kb-9112"),
         body="Zebra hub gamma.\n")
    kb_run("reindex", root=root, expect_rc=0)
    return root


# --------------------------------------------------------------------------
# kb search --expand
# --------------------------------------------------------------------------

def test_no_flag_output_clean_and_stable(sbx):
    r1 = kb_run("search", "grubby", "kernel", "cmdline", root=sbx, expect_rc=0)
    r2 = kb_run("search", "grubby", "kernel", "cmdline", root=sbx, expect_rc=0)
    assert r1.stdout == r2.stdout                    # byte-identical
    assert HEADER not in r1.stdout                   # opt-in: no flag no block
    assert expand_ids(r1.stdout) == []               # no indented id rows
    assert search_ids(r1.stdout) == ["kb-9001"]


def test_search_expand_labels_and_backlinks(sbx):
    r = kb_run("search", "grubby", "kernel", "cmdline", "--expand",
               root=sbx, expect_rc=0)
    assert search_ids(r.stdout) == ["kb-9001"]       # hits block unchanged
    assert HEADER in r.stdout
    # seed kb-9001: out 9002, then backlinks sorted; per-seed cap 4 cuts 9120
    assert expand_ids(r.stdout) == ["kb-9002", "kb-9008", "kb-9009",
                                    "kb-9113"]
    assert "via related: kb-9001" in r.stdout
    assert "← backlink: kb-9001" in r.stdout


def test_search_expand_dedups_displayed_ids(sbx):
    # OR path: both grubby notes are hits → neither may reappear as neighbour
    r = kb_run("search", "grubby", "qqzzqq", "--expand", root=sbx, expect_rc=0)
    assert set(search_ids(r.stdout)) == {"kb-9001", "kb-9002"}
    ids = expand_ids(r.stdout)
    assert "kb-9001" not in ids and "kb-9002" not in ids
    # seed order = rank order [9002, 9001]: 9120 backlinks 9002 (row 1),
    # then 9001's backlinks sorted; per-seed cap 4 keeps all four here
    assert ids == ["kb-9120", "kb-9008", "kb-9009", "kb-9113"]
    assert "← backlink: kb-9002" in r.stdout


def test_superseded_neighbour_shown_tagged(sbx):
    r = kb_run("search", "widget", "--expand", root=sbx, expect_rc=0)
    ids = expand_ids(r.stdout)
    assert "kb-9004" in ids                          # retired note surfaces…
    line = next(l for l in r.stdout.splitlines()
                if l.startswith("    kb-9004"))
    assert line.endswith("(superseded)")             # …but never silently
    assert "via supersedes: kb-9005" in r.stdout


def test_per_seed_cap_is_four(sbx):
    r = kb_run("search", "zebra", "alpha", "--expand", root=sbx, expect_rc=0)
    assert search_ids(r.stdout) == ["kb-9120"]       # single exact hit
    # hub related: 6 targets, sorted → first 4 kept, deterministic
    assert expand_ids(r.stdout) == ["kb-9001", "kb-9002", "kb-9003",
                                    "kb-9005"]


def test_total_cap_is_eight(sbx):
    r = kb_run("search", "zebra", "hub", "--expand", root=sbx, expect_rc=0)
    hits = set(search_ids(r.stdout))
    assert hits == {"kb-9120", "kb-9121", "kb-9122"}
    ids = expand_ids(r.stdout)
    assert len(ids) == 8                             # 3 hubs would give 11
    assert hits.isdisjoint(ids)
    assert set(ids) <= {"kb-9001", "kb-9002", "kb-9003", "kb-9005",
                        "kb-9006", "kb-9007", "kb-9008", "kb-9009",
                        "kb-9110", "kb-9111", "kb-9112"}


def test_own_id_path_expands(sbx):
    plain = kb_run("search", "kb-9001", root=sbx, expect_rc=0)
    assert HEADER not in plain.stdout
    r = kb_run("search", "kb-9001", "--expand", root=sbx, expect_rc=0)
    assert HEADER in r.stdout
    assert "kb-9002" in expand_ids(r.stdout)


def test_rescue_path_never_expands(sbx):
    r = kb_run("search", "zzzqqqabsent", "--expand", root=sbx, expect_rc=0)
    assert HEADER not in r.stdout
    assert expand_ids(r.stdout) == []


# --------------------------------------------------------------------------
# kb expand
# --------------------------------------------------------------------------

def test_expand_depth1_sections(sbx):
    r = kb_run("expand", "kb-9001", root=sbx, expect_rc=0)
    assert r.stdout.startswith("kb-9001 — grubby syncs the kernel cmdline")
    assert "  notes/kb-9001-grubby-syncs-kernel-cmdline.md" in r.stdout
    assert "links to:" in r.stdout
    assert re.search(r"^    related\s+kb-9002\b", r.stdout, re.M)
    assert "linked from (backlinks):" in r.stdout
    assert re.search(r"^    mention\s+kb-9113\b", r.stdout, re.M)
    assert "depth 2" not in r.stdout                 # default depth is 1


def test_expand_depth2_new_neighbours_only(sbx):
    d1 = kb_run("expand", "kb-9110", root=sbx, expect_rc=0)
    assert "kb-9112" not in d1.stdout and "depth 2" not in d1.stdout
    d2 = kb_run("expand", "kb-9110", "--depth", "2", root=sbx, expect_rc=0)
    assert "depth 2 (new neighbours, excluding the above):" in d2.stdout
    assert "kb-9112" in d2.stdout
    assert "(via kb-9111)" in d2.stdout              # 2-hop provenance


@pytest.mark.parametrize("bad", ["3", "0", "-1", "x", ""])
def test_expand_bad_depth_rejected(sbx, bad):
    args = ["expand", "kb-9001", "--depth"] + ([bad] if bad else [])
    r = kb_run(*args, root=sbx)
    assert r.returncode != 0
    assert "--depth must be 1 or 2" in r.stderr


def test_expand_slug_unknown_and_noargs(sbx):
    r = kb_run("expand", "grubby", root=sbx, expect_rc=0)
    assert r.stdout.startswith("kb-9001 — ")         # slug resolution
    r = kb_run("expand", "kb-9999", root=sbx)
    assert r.returncode != 0
    assert "no note for 'kb-9999'" in r.stderr
    r = kb_run("expand", root=sbx)
    assert r.returncode != 0
    assert "usage: knowbox expand" in r.stderr


def test_expand_superseded_input_warns(sbx):
    r = kb_run("expand", "kb-9004", root=sbx, expect_rc=0)
    assert "(note: kb-9004 is superseded" in r.stdout
    assert re.search(r"^    superseded-by\s+kb-9005\b", r.stdout, re.M)
    assert re.search(r"^    related\s+kb-9005\b", r.stdout, re.M)  # folded fm


def test_expand_isolated_note(sbx):
    r = kb_run("expand", "kb-9010", root=sbx, expect_rc=0)
    assert "(no author-written links)" in r.stdout


def test_expand_deterministic(sbx):
    a = kb_run("expand", "kb-9001", "--depth", "2", root=sbx, expect_rc=0)
    b = kb_run("expand", "kb-9001", "--depth", "2", root=sbx, expect_rc=0)
    assert a.stdout == b.stdout
    c = kb_run("search", "zebra", "hub", "--expand", root=sbx, expect_rc=0)
    d = kb_run("search", "zebra", "hub", "--expand", root=sbx, expect_rc=0)
    assert c.stdout == d.stdout


# --------------------------------------------------------------------------
# malformed notes, zero-HTTP, adjacency probe
# --------------------------------------------------------------------------

def test_malformed_note_fail_soft(sbx):
    # kb-9102 (unterminated frontmatter) is in this sandbox: never a node,
    # never a crash, never in output.
    r = kb_run("expand", "kb-9001", root=sbx, expect_rc=0)
    assert "Traceback" not in r.stderr and "kb-9102" not in r.stdout
    r = kb_run("search", "grubby", "kernel", "cmdline", "--expand",
               root=sbx, expect_rc=0)
    assert "Traceback" not in r.stderr and "kb-9102" not in r.stdout


# --- zero-HTTP: minimal mock embedding server (fusion-test pattern) --------

DIM = 64


def _vec(text):
    h = hashlib.sha1(text.encode()).digest()
    raw = []
    while len(raw) < DIM:
        h = hashlib.sha1(h).digest()
        raw.extend(b / 255.0 - 0.5 for b in h)
    n = math.sqrt(sum(x * x for x in raw)) or 1.0
    return [x / n for x in raw]


class _Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.server.request_count += 1
        length = int(self.headers.get("Content-Length", 0))
        payload = json.loads(self.rfile.read(length))
        texts = payload["input"]
        if isinstance(texts, str):
            texts = [texts]
        body = json.dumps({"data": [{"index": i, "embedding": _vec(t)}
                                    for i, t in enumerate(texts)]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def test_expansion_makes_zero_http_calls(tmp_path):
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
    srv.request_count = 0
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    try:
        root = build_sandbox(tmp_path / "http", corpus="mini", git=False)
        kb_run("reindex", root=root, expect_rc=0)
        (root / ".kb-embed.json").write_text(json.dumps({
            "model": "text-embedding-v4",
            "notes": {"kb-9001": {"h": "fixture",
                                  "v": _vec("grubby kernel cmdline")}}}),
            encoding="utf-8")
        env = {"KNOWBOX_EMBED_URL": f"http://127.0.0.1:{srv.server_port}/embeddings"}
        # exact-path search (fusion gated off) + expand: graph must not call
        r = kb_run("search", "grubby", "kernel", "cmdline", "--expand",
                   root=root, env_extra=env, expect_rc=0)
        assert "kb-9002" in expand_ids(r.stdout)     # expansion really ran
        r = kb_run("expand", "kb-9001", root=root, env_extra=env, expect_rc=0)
        assert "kb-9002" in r.stdout
        assert srv.request_count == 0
    finally:
        srv.shutdown()


def test_build_graph_pure_helper(sbx, tmp_path):
    script = tmp_path / "graph_probe.py"
    script.write_text(
        "import importlib.machinery, importlib.util, json, os\n"
        "p = os.environ['KNOWBOX_CLI_SRC']\n"
        "spec = importlib.util.spec_from_file_location(\n"
        "    'kbmod', p, loader=importlib.machinery.SourceFileLoader('kbmod', p))\n"
        "m = importlib.util.module_from_spec(spec)\n"
        "spec.loader.exec_module(m)\n"
        "adj, info = m._build_graph()\n"
        "print(json.dumps({'out9001': adj['kb-9001']['out'],\n"
        "                  'in9001': adj['kb-9001']['in'],\n"
        "                  'out9004': adj['kb-9004']['out'],\n"
        "                  'in9004': adj['kb-9004']['in'],\n"
        "                  'out9005': adj['kb-9005']['out'],\n"
        "                  'out9113': adj['kb-9113']['out'],\n"
        "                  'out9114': adj['kb-9114']['out'],\n"
        "                  'status9004': info['kb-9004']['status'],\n"
        "                  'has9102': 'kb-9102' in adj}))\n",
        encoding="utf-8")
    r = subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "python", str(script)],
        capture_output=True, text=True, timeout=120,
        # KNOWBOX_CLI_SRC = the CLI under test; KNOWBOX_ROOT = sandbox so the
        # module-level KB resolution never touches production data.
        env={**os.environ, "KNOWBOX_ROOT": str(sbx),
             "KNOWBOX_CLI_SRC": str(CLI_SRC)})
    assert r.returncode == 0, r.stderr
    out = json.loads(r.stdout)
    # fm related + body [[kb-9002]] fold to the strongest rel (related)
    assert out["out9001"] == {"kb-9002": "related"}
    assert out["in9001"] == {"kb-9002": "related", "kb-9008": "related",
                             "kb-9009": "related", "kb-9113": "mention",
                             "kb-9120": "related"}
    # supersede banner mention folds under fm related; supersedes beats related
    assert out["out9004"] == {"kb-9005": "related"}
    assert out["in9004"] == {"kb-9005": "supersedes"}
    assert out["out9005"] == {"kb-9004": "supersedes"}
    assert out["out9113"] == {"kb-9001": "mention"}   # body-only edge
    assert out["out9114"] == {}                       # dead id dropped
    assert out["status9004"] == "superseded"
    assert out["has9102"] is False                    # malformed never a node
