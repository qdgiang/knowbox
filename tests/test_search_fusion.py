"""Gated RRF fusion (BM25 × vector) — kb-0107 survey gap #1.

Design under test (src/knowbox/cli.py constants block + _rrf_fuse/_fusion_pass):
  - The vector channel fires ONLY on the OR/partial lexical path
    (exact=False). AND-path hits make ZERO HTTP requests — the kbx-style
    strong-signal fast path, adapted (our dual-tokenizer bm25 scores are not
    comparable, so `exact` is the honest strong signal).
  - w_lex=1.0 / w_vec=0.5 at fixed k=60 keeps vector-only docs below every
    lexical doc (tail-append, never interleave) while dual-channel agreement
    re-ranks. Cos floors (kb-0024 calibration): ≥0.40 to participate, ≥0.45
    for vector-only, cap 2.
  - Fail-soft: no store / kill-switch / dead endpoint → today's lexical-only
    output, rc=0, no traceback. Existing goldens never see fusion (sandboxes
    ship without .kb-embed.json and conftest blackholes KNOWBOX_EMBED_URL).

House style: subprocess kb_run only — never import the CLI module in the pytest
process. The pure-helper unit test (case i) runs its import in a SUBPROCESS.

Mock server vectors are deterministic sha1-seeded unit vectors, so cos
targets are crafted exactly: v(c) = c·qvec + √(1−c²)·orth.
"""
import hashlib
import http.server
import json
import math
import os
import subprocess
import threading
from pathlib import Path

import pytest

from conftest import CLI_SRC, UV, build_sandbox, kb_run, search_ids

REPO = Path(__file__).resolve().parent.parent

DIM = 64
OR_QUERY = ("grubby", "qqzzqq")        # OR path: lex order [kb-9002, kb-9001]
OR_QTEXT = " ".join(OR_QUERY)
EXACT_QUERY = ("grubby", "kernel", "cmdline")   # AND path: kb-9001, exact


# --------------------------------------------------------------------------
# deterministic vectors + mock embedding server
# --------------------------------------------------------------------------

def _vec(text):
    """Unit vector seeded from a sha1 chain — stable across runs/machines."""
    h = hashlib.sha1(text.encode()).digest()
    raw = []
    while len(raw) < DIM:
        h = hashlib.sha1(h).digest()
        raw.extend(b / 255.0 - 0.5 for b in h)
    n = math.sqrt(sum(x * x for x in raw)) or 1.0
    return [x / n for x in raw]


def _at_cos(qvec, c, seed):
    """Unit vector with exact cosine c against the unit vector qvec."""
    u = _vec(seed)
    dot = sum(a * b for a, b in zip(u, qvec))
    orth = [a - dot * b for a, b in zip(u, qvec)]
    n = math.sqrt(sum(x * x for x in orth)) or 1.0
    orth = [x / n for x in orth]
    s = math.sqrt(max(0.0, 1.0 - c * c))
    return [c * a + s * b for a, b in zip(qvec, orth)]


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


@pytest.fixture(scope="module")
def mock_embed():
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
    srv.request_count = 0
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    srv.url = f"http://127.0.0.1:{srv.server_port}/embeddings"
    yield srv
    srv.shutdown()


def _sandbox(tmp_path, name="sbx"):
    root = build_sandbox(tmp_path / name, corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    return root


def _write_store(root, note_vecs):
    """note_vecs: {nid: vector} — h values are irrelevant to the search path."""
    (root / ".kb-embed.json").write_text(json.dumps({
        "model": "text-embedding-v4",
        "notes": {nid: {"h": "fixture", "v": v}
                  for nid, v in note_vecs.items()}}), encoding="utf-8")


def _env(srv, **extra):
    return {"KNOWBOX_EMBED_URL": srv.url, **extra}


# --------------------------------------------------------------------------
# (a) AND path — strong lexical signal never touches the network
# --------------------------------------------------------------------------

def test_and_path_makes_zero_requests(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    _write_store(root, {"kb-9001": _vec(" ".join(EXACT_QUERY))})
    before = mock_embed.request_count
    r = kb_run("search", *EXACT_QUERY, root=root,
               env_extra=_env(mock_embed), expect_rc=0)
    assert mock_embed.request_count - before == 0
    assert "(partial match" not in r.stdout      # exact hit, no label
    assert search_ids(r.stdout) == ["kb-9001"]


# --------------------------------------------------------------------------
# (b) OR path — exactly one query-embedding call, byte-identical across runs
# --------------------------------------------------------------------------

def test_or_path_one_request_and_deterministic(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    _write_store(root, {"kb-9001": _vec(OR_QTEXT)})
    before = mock_embed.request_count
    r1 = kb_run("search", *OR_QUERY, root=root,
                env_extra=_env(mock_embed), expect_rc=0)
    r2 = kb_run("search", *OR_QUERY, root=root,
                env_extra=_env(mock_embed), expect_rc=0)
    assert mock_embed.request_count - before == 2   # exactly 1 per search
    assert r1.stdout == r2.stdout                   # byte-identical


# --------------------------------------------------------------------------
# (c) dual-channel agreement re-ranks (kb-9001: lex#2 + vec#1 → #1)
# --------------------------------------------------------------------------

def test_dual_channel_agreement_reranks(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    _write_store(root, {"kb-9001": _vec(OR_QTEXT)})   # cos 1.0 with query
    base = kb_run("search", *OR_QUERY, root=root,
                  env_extra=_env(mock_embed, KB_NO_FUSION="1"), expect_rc=0)
    assert search_ids(base.stdout) == ["kb-9002", "kb-9001"]  # lex order
    assert "(hybrid re-rank" not in base.stdout
    fused = kb_run("search", *OR_QUERY, root=root,
                   env_extra=_env(mock_embed), expect_rc=0)
    assert search_ids(fused.stdout) == ["kb-9001", "kb-9002"]  # promoted
    assert "(partial match" in fused.stdout
    assert "(hybrid re-rank: lexical + semantic agreement)" in fused.stdout
    assert "(semantic-only)" not in fused.stdout    # both have lexical support


# --------------------------------------------------------------------------
# (d) vector-only doc at cos 0.50 → tail-appended, tagged, snippet synthesized
# --------------------------------------------------------------------------

def test_vector_only_appended_with_tag(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    qv = _vec(OR_QTEXT)
    _write_store(root, {"kb-9005": _at_cos(qv, 0.50, "orth-d")})
    r = kb_run("search", *OR_QUERY, root=root,
               env_extra=_env(mock_embed), expect_rc=0)
    assert search_ids(r.stdout) == ["kb-9002", "kb-9001", "kb-9005"]
    line = next(l for l in r.stdout.splitlines() if l.startswith("kb-9005"))
    assert line.endswith("(semantic-only)")
    assert "(hybrid re-rank" in r.stdout
    # snippet synthesized from summary (parse_note path)
    assert "New widget configuration, routed." in r.stdout


# --------------------------------------------------------------------------
# (e) vector-only doc in the 0.40–0.45 band is vetoed (kb-0024 calibration)
# --------------------------------------------------------------------------

def test_vector_only_below_045_vetoed(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    qv = _vec(OR_QTEXT)
    _write_store(root, {"kb-9005": _at_cos(qv, 0.42, "orth-e")})
    r = kb_run("search", *OR_QUERY, root=root,
               env_extra=_env(mock_embed), expect_rc=0)
    assert search_ids(r.stdout) == ["kb-9002", "kb-9001"]   # no kb-9005
    assert "(semantic-only)" not in r.stdout
    assert "(hybrid re-rank" not in r.stdout                # fusion was a no-op


def test_vector_only_cap_is_two(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    qv = _vec(OR_QTEXT)
    _write_store(root, {
        "kb-9005": _at_cos(qv, 0.90, "orth-1"),
        "kb-9006": _at_cos(qv, 0.80, "orth-2"),
        "kb-9007": _at_cos(qv, 0.70, "orth-3"),   # 3rd vector-only → capped
    })
    r = kb_run("search", *OR_QUERY, root=root,
               env_extra=_env(mock_embed), expect_rc=0)
    ids = search_ids(r.stdout)
    assert ids[:2] == ["kb-9002", "kb-9001"]      # lexical block untouched
    assert set(ids[2:]) <= {"kb-9005", "kb-9006"} # top-2 by cos appended
    assert "kb-9007" not in ids
    assert ids.count("kb-9005") + ids.count("kb-9006") == 2


# --------------------------------------------------------------------------
# (f)+(g) kill-switch and no-store are byte-identical to lexical-only, 0 HTTP
# --------------------------------------------------------------------------

def test_kill_switch_and_no_store_are_inert(tmp_path, mock_embed):
    root = _sandbox(tmp_path)
    no_store = kb_run("search", *OR_QUERY, root=root,
                      env_extra=_env(mock_embed), expect_rc=0)
    _write_store(root, {"kb-9001": _vec(OR_QTEXT)})
    before = mock_embed.request_count
    killed = kb_run("search", *OR_QUERY, root=root,
                    env_extra=_env(mock_embed, KB_NO_FUSION="1"),
                    expect_rc=0)
    assert mock_embed.request_count - before == 0  # gate checked before HTTP
    assert killed.stdout == no_store.stdout        # byte-identical


# --------------------------------------------------------------------------
# (h) dead endpoint + store present → fail-soft to lexical-only
# --------------------------------------------------------------------------

def test_blackholed_endpoint_fails_soft(tmp_path):
    root = _sandbox(tmp_path)
    _write_store(root, {"kb-9001": _vec(OR_QTEXT)})
    # no KNOWBOX_EMBED_URL override: conftest blackholes it to 127.0.0.1:9
    r = kb_run("search", *OR_QUERY, root=root, expect_rc=0, timeout=120)
    assert "Traceback" not in r.stderr
    assert search_ids(r.stdout) == ["kb-9002", "kb-9001"]
    assert "(hybrid re-rank" not in r.stdout


# --------------------------------------------------------------------------
# (i) _rrf_fuse pure arithmetic — import runs in a SUBPROCESS (house rule)
# --------------------------------------------------------------------------

def test_rrf_fuse_pure_helper(tmp_path):
    root = _sandbox(tmp_path)
    script = tmp_path / "fuse_probe.py"
    script.write_text(
        "import importlib.machinery, importlib.util, json, os\n"
        "p = os.environ['KNOWBOX_CLI_SRC']\n"
        "spec = importlib.util.spec_from_file_location(\n"
        "    'kbmod', p, loader=importlib.machinery.SourceFileLoader('kbmod', p))\n"
        "m = importlib.util.module_from_spec(spec)\n"
        "spec.loader.exec_module(m)\n"
        "lex = [('kb-A', 'notes/a.md', 'A', 'sa', -9.0),\n"
        "       ('kb-B', 'notes/b.md', 'B', 'sb', -8.0)]\n"
        "sem = [(0.50, 'kb-B', 'notes/b.md', 'B'),\n"
        "       (0.50, 'kb-C', 'notes/c.md', 'C'),\n"
        "       (0.44, 'kb-D', 'notes/d.md', 'D'),\n"
        "       (0.90, 'kb-E', 'notes/e.md', 'E'),\n"
        "       (0.80, 'kb-F', 'notes/f.md', 'F')]\n"
        "rows, vec_only = m._rrf_fuse(lex, sem, limit=6)\n"
        "print(json.dumps({'ids': [r[0] for r in rows],\n"
        "                  'vec_only': sorted(vec_only),\n"
        "                  'snips': [r[3] for r in rows]}))\n",
        encoding="utf-8")
    r = subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "python", str(script)],
        capture_output=True, text=True, timeout=120,
        # KNOWBOX_CLI_SRC = the CLI under test; KNOWBOX_ROOT = sandbox so
        # module-level KB resolution never touches production data.
        env={**os.environ, "KNOWBOX_ROOT": str(root),
             "KNOWBOX_CLI_SRC": str(CLI_SRC)})
    assert r.returncode == 0, r.stderr
    out = json.loads(r.stdout)
    # B: lex#2 + vec#1 → 1/62 + 0.5/61 ≈ 0.0243 outranks A: 1/61 ≈ 0.0164.
    # Vector-only: D vetoed (0.44 < 0.45); cap 2 by cos → E(0.90), F(0.80);
    # C(0.50) is third → excluded. Tail order by fused score: E (vec rank 4,
    # 0.5/64) before F (vec rank 5, 0.5/65).
    assert out["ids"] == ["kb-B", "kb-A", "kb-E", "kb-F"]
    assert out["vec_only"] == ["notes/e.md", "notes/f.md"]
    assert out["snips"][2] is None and out["snips"][3] is None  # caller fills
