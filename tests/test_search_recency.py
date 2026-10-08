"""Default-on recency weighting inside the fusion path — kb-0110 gap #2.

Design under test (src/knowbox/cli.py: _recency_multiplier / _apply_recency /
_fusion_pass / RECENCY_* constants):
  - Post-fusion multiplier on the RRF score: max(FLOOR, 2 ** (-(age -
    OFFSET) / HALF_LIFE)), age from `updated:` (fallback `created:`),
    missing/malformed → neutral 1.0, future dates clamped to 1.0 (never a
    boost — the ES decay-inversion pitfall). Fusion path ONLY: AND/exact,
    typo, semantic-rescue rows never see it.
  - BLOCKWISE re-sort: vector-only rows stay below every lexical row even
    when a fresh vec-only score (1.0 × 0.5/61 ≈ 0.00820) exceeds a floored
    lexical one (0.5 × 1/62 ≈ 0.00806) — a global sort would invert
    kb-0109's structural guarantee at note ages > ~110d.
  - Labels-not-scores (kb-0038): the multiplier is never printed; the
    hybrid label gains "+ recency" only when recency actually reordered.
  - Kill switch KB_NO_RECENCY (KB_NO_FUSION pattern) restores today's
    ordering AND I/O profile byte-identically.

House style: subprocess kb_run only — never import the CLI module in the pytest
process; pure-helper probes run their import in a SUBPROCESS.

Dates are RELATIVE (today - N days) and sit deep inside the flat bands
(≤30 → exactly 1.0; ≥120 → exactly 0.5) so a ±1d midnight drift cannot
flip a band. Every E2E test first pins its KB_NO_RECENCY=1 baseline.

Deliberately not covered (see plan):
  - zero-HTTP AND path: recency lives strictly behind `if not exact:` and
    adds no network code; pinned by test_search_fusion.py (a).
  - E2E parse_note explosion mid-fusion: not deterministically reachable
    (search() self-heals index drift before returning rows); the per-row
    except → neutral multiplier is code-reviewed, the outer degrade-to-
    lexical is pinned by test_search_fusion.py (h).
"""
import hashlib
import http.server
import json
import math
import os
import re
import subprocess
import threading
from datetime import date, timedelta
from pathlib import Path

import pytest

from conftest import CLI_SRC, UV, _fm, build_sandbox, kb_run, search_ids

REPO = Path(__file__).resolve().parent.parent

DIM = 64
Q = ("delta", "qqyy")            # OR path: qqyy matches nothing → exact=False
QTEXT = " ".join(Q)
OLD_LABEL = "(hybrid re-rank: lexical + semantic agreement)"
REC_LABEL = "(hybrid re-rank: lexical + semantic + recency)"


# --------------------------------------------------------------------------
# deterministic vectors + mock embedding server (test_search_fusion pattern)
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
    s = math.sqrt(max(0.0, 1 - c * c))
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


def _env(srv, **extra):
    return {"KNOWBOX_EMBED_URL": srv.url, **extra}


def _write_store(root, note_vecs):
    (root / ".kb-embed.json").write_text(json.dumps({
        "model": "text-embedding-v4",
        "notes": {nid: {"h": "fixture", "v": v}
                  for nid, v in note_vecs.items()}}), encoding="utf-8")


# --------------------------------------------------------------------------
# sandbox helpers — per-test isolation, RELATIVE dates
# --------------------------------------------------------------------------

def _aged(days):
    """ISO date `days` in the past (negative → future)."""
    return (date.today() - timedelta(days=days)).isoformat()


def _sbx(tmp_path, name="sbx"):
    return build_sandbox(tmp_path / name, corpus="mini", git=False)


def _add(root, nid, slug, title, body="", **kw):
    (root / "notes" / f"{nid}-{slug}.md").write_text(
        _fm(nid=nid, title=title, **kw) + body, encoding="utf-8")


def _add_raw(root, fname, text):
    (root / "notes" / fname).write_text(text, encoding="utf-8")


def _finish(root, store):
    kb_run("reindex", root=root, expect_rc=0)
    _write_store(root, store)
    return root


QV = _vec(QTEXT)
# vector-only tail note: no lexical match for Q, cos 0.90 (≥0.45 gate)
EXT_BODY = "Unrelated zebrafinch content, nothing to match here.\n"


def _ext(root, nid="kb-9132", slug="ext-vector", age=5, **kw):
    _add(root, nid, slug, "External vector fixture", EXT_BODY,
         updated=_aged(age), **kw)


def _search(root, srv, **env):
    return kb_run("search", *Q, root=root, env_extra=_env(srv, **env),
                  expect_rc=0)


# --------------------------------------------------------------------------
# (1)+(8-labels) recency reorders: aged lex#1 sinks below fresh lex#2
# --------------------------------------------------------------------------

def _reorder_corpus(tmp_path):
    root = _sbx(tmp_path)
    _add(root, "kb-9130", "aged-reorder", "Aged reorder fixture",
         "delta delta delta aged note body.\n", updated=_aged(200))
    _add(root, "kb-9131", "fresh-reorder", "Fresh reorder fixture",
         "delta fresh note body.\n", updated=_aged(5))
    _ext(root)
    return _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})


def test_recency_demotes_aged_below_fresh(tmp_path, mock_embed):
    root = _reorder_corpus(tmp_path)
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    assert search_ids(base.stdout) == ["kb-9130", "kb-9131", "kb-9132"]
    assert OLD_LABEL in base.stdout            # fusion fired (vec tail added)

    on = _search(root, mock_embed)
    # OLD: 0.5 × 1/61 = 0.008197; FRESH: 1.0 × 1/62 = 0.016129; EXT: 0.008197
    # — OLD/EXT tie exactly; blockwise key keeps OLD above the vec-only row.
    assert search_ids(on.stdout) == ["kb-9131", "kb-9130", "kb-9132"]
    assert REC_LABEL in on.stdout
    assert OLD_LABEL not in on.stdout
    # labels-not-scores: the multiplier (or any decimal) is never printed
    assert not re.search(r"\d\.\d", on.stdout)


# --------------------------------------------------------------------------
# (2) kill switch: inert, deterministic, no life outside fusion
# --------------------------------------------------------------------------

def test_kill_switch_inert_and_deterministic(tmp_path, mock_embed):
    root = _reorder_corpus(tmp_path)
    k1 = _search(root, mock_embed, KB_NO_RECENCY="1")
    k2 = _search(root, mock_embed, KB_NO_RECENCY="1")
    on = _search(root, mock_embed)
    assert k1.stdout == k2.stdout                       # byte-identical
    assert search_ids(on.stdout) != search_ids(k1.stdout)  # switch matters
    nof_1 = _search(root, mock_embed, KB_NO_FUSION="1")
    nof_2 = _search(root, mock_embed, KB_NO_FUSION="1", KB_NO_RECENCY="1")
    assert nof_1.stdout == nof_2.stdout   # recency has no life outside fusion
    assert search_ids(nof_1.stdout) == ["kb-9130", "kb-9131"]  # lexical only


# --------------------------------------------------------------------------
# (3) offset band: ages ≤30d are indistinguishable
# --------------------------------------------------------------------------

def test_offset_band_no_reorder(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add(root, "kb-9133", "band-one", "Band fixture one",
         "delta delta delta band one body.\n", updated=_aged(1))
    _add(root, "kb-9134", "band-two", "Band fixture two",
         "delta band two body.\n", updated=_aged(28))
    _ext(root)
    _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    on = _search(root, mock_embed)
    assert search_ids(base.stdout) == ["kb-9133", "kb-9134", "kb-9132"]
    assert on.stdout == base.stdout        # both multipliers exactly 1.0
    assert OLD_LABEL in on.stdout and REC_LABEL not in on.stdout


# --------------------------------------------------------------------------
# (4) floor: two ancient notes both sit at exactly 0.5 → RRF order kept
# --------------------------------------------------------------------------

def test_floor_keeps_ancient_in_rrf_order(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add(root, "kb-9135", "floor-one", "Floor fixture one",
         "delta delta delta floor one body.\n", updated=_aged(200))
    _add(root, "kb-9136", "floor-two", "Floor fixture two",
         "delta floor two body.\n", updated=_aged(400))
    _ext(root)
    _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    on = _search(root, mock_embed)
    assert search_ids(base.stdout) == ["kb-9135", "kb-9136", "kb-9132"]
    assert on.stdout == base.stdout        # uniform 0.5 → stable sort
    assert REC_LABEL not in on.stdout


# --------------------------------------------------------------------------
# (5) future dates never boost (ES decay-inversion guard)
# --------------------------------------------------------------------------

def test_future_dated_never_boosted(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add(root, "kb-9137", "today-fixture", "Today fixture",
         "delta delta delta today body.\n", updated=_aged(0))
    # unclamped bug would give FUT 2**(40/90) ≈ 1.366 → 1.366/62 ≈ 0.0220
    # > TODAY 1/61 ≈ 0.0164 → inversion; the clamp pins both at 1.0.
    _add(root, "kb-9138", "future-fixture", "Future fixture",
         "delta future body.\n", updated=_aged(-10))
    _ext(root)
    _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    on = _search(root, mock_embed)
    assert search_ids(base.stdout) == ["kb-9137", "kb-9138", "kb-9132"]
    assert on.stdout == base.stdout
    assert REC_LABEL not in on.stdout


# --------------------------------------------------------------------------
# (6a) missing `updated:` → falls back to `created:` (distinguishable: an
#      OLD created date must DEMOTE — neutral-1.0 handling would not reorder)
# --------------------------------------------------------------------------

def test_missing_updated_falls_back_to_created(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add_raw(root, "kb-9139-no-updated-field.md",
             "---\nid: kb-9139\ntitle: \"Fallback fixture without updated\"\n"
             "type: fact\ndomain: tooling\ntags: []\naliases: []\n"
             "status: active\nconfidence: high\n"
             f"created: {_aged(200)}\n"
             "supersedes: []\nrelated: []\nsource: \"session\"\n"
             "summary: \"Created-date fallback fixture.\"\n---\n"
             "delta delta delta fallback body.\n")
    _add(root, "kb-9131", "fresh-reorder", "Fresh reorder fixture",
         "delta fresh note body.\n", updated=_aged(5))
    _ext(root)
    _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    assert search_ids(base.stdout) == ["kb-9139", "kb-9131", "kb-9132"]
    on = _search(root, mock_embed)
    # created 200d ago → floor 0.5 → demoted below the fresh note
    assert search_ids(on.stdout) == ["kb-9131", "kb-9139", "kb-9132"]
    assert REC_LABEL in on.stdout


# --------------------------------------------------------------------------
# (6b) malformed date → neutral 1.0, fusion stays intact
# --------------------------------------------------------------------------

def test_malformed_date_is_neutral(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add(root, "kb-9140", "bad-date", "Bad date fixture",
         "delta delta delta bad date body.\n", updated="soon")
    _add(root, "kb-9131", "fresh-reorder", "Fresh reorder fixture",
         "delta fresh note body.\n", updated=_aged(5))
    _ext(root)
    _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    on = _search(root, mock_embed)
    assert search_ids(base.stdout) == ["kb-9140", "kb-9131", "kb-9132"]
    assert on.stdout == base.stdout        # None age → 1.0 → no movement
    assert OLD_LABEL in on.stdout and REC_LABEL not in on.stdout


# --------------------------------------------------------------------------
# (7) Q7 REGRESSION PIN — blockwise guarantee: a floored lex#2 (0.5 × 1/62
#     ≈ 0.008065) must stay ABOVE a fresh vec-only row (1.0 × 0.5/61 ≈
#     0.008197). A naive global sort inverts this; this test fails then.
# --------------------------------------------------------------------------

def test_vec_only_never_outranks_lexical_under_recency(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add(root, "kb-9141", "lex-fresh", "Lexical fresh fixture",
         "delta delta delta lex fresh body.\n", updated=_aged(5))
    _add(root, "kb-9142", "lex-ancient", "Lexical ancient fixture",
         "delta lex ancient body.\n", updated=_aged(200))
    _ext(root)
    _finish(root, {"kb-9132": _at_cos(QV, 0.90, "orth-rec-1")})
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    assert search_ids(base.stdout) == ["kb-9141", "kb-9142", "kb-9132"]
    on = _search(root, mock_embed)
    assert search_ids(on.stdout) == ["kb-9141", "kb-9142", "kb-9132"]
    assert REC_LABEL not in on.stdout      # nothing moved
    line = next(l for l in on.stdout.splitlines() if l.startswith("kb-9132"))
    assert line.endswith("(semantic-only)")
    line = next(l for l in on.stdout.splitlines() if l.startswith("kb-9142"))
    assert not line.endswith("(semantic-only)")


# --------------------------------------------------------------------------
# (8) recency DOES reorder inside the vector-only tail block
# --------------------------------------------------------------------------

def test_vec_only_block_reorders_within_tail(tmp_path, mock_embed):
    root = _sbx(tmp_path)
    _add(root, "kb-9141", "lex-fresh", "Lexical fresh fixture",
         "delta delta delta lex fresh body.\n", updated=_aged(5))
    _ext(root, nid="kb-9143", slug="ext-aged", age=200)
    _ext(root, nid="kb-9144", slug="ext-fresh", age=5)
    _finish(root, {
        "kb-9143": _at_cos(QV, 0.90, "orth-rec-a"),   # vec rank 1, ancient
        "kb-9144": _at_cos(QV, 0.80, "orth-rec-b"),   # vec rank 2, fresh
    })
    base = _search(root, mock_embed, KB_NO_RECENCY="1")
    assert search_ids(base.stdout) == ["kb-9141", "kb-9143", "kb-9144"]
    on = _search(root, mock_embed)
    # aged EXT: 0.5 × 0.5/61 ≈ 0.0041 < fresh EXT: 1.0 × 0.5/62 ≈ 0.0082
    assert search_ids(on.stdout) == ["kb-9141", "kb-9144", "kb-9143"]
    assert REC_LABEL in on.stdout
    for nid in ("kb-9143", "kb-9144"):
        line = next(l for l in on.stdout.splitlines() if l.startswith(nid))
        assert line.endswith("(semantic-only)")


# --------------------------------------------------------------------------
# (9)+(10) pure helpers — import runs in a SUBPROCESS (house rule)
# --------------------------------------------------------------------------

def _probe(root, tmp_path, name, body):
    script = tmp_path / name
    script.write_text(
        "import importlib.machinery, importlib.util, json, os\n"
        "p = os.environ['KNOWBOX_CLI_SRC']\n"
        "spec = importlib.util.spec_from_file_location(\n"
        "    'kbmod', p, loader=importlib.machinery.SourceFileLoader('kbmod', p))\n"
        "m = importlib.util.module_from_spec(spec)\n"
        "spec.loader.exec_module(m)\n" + body,
        encoding="utf-8")
    r = subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "python", str(script)],
        capture_output=True, text=True, timeout=120,
        env={**os.environ, "KNOWBOX_ROOT": str(root),
             "KNOWBOX_CLI_SRC": str(CLI_SRC)})
    assert r.returncode == 0, r.stderr
    return json.loads(r.stdout)


def test_recency_multiplier_pure_probe(tmp_path):
    root = _sbx(tmp_path)
    out = _probe(root, tmp_path, "mult_probe.py",
                 "ages = [None, -365, -5, 0, 30, 31, 60, 120, 1000]\n"
                 "vals = {str(a): m._recency_multiplier(a) for a in ages}\n"
                 "sweep = [m._recency_multiplier(a) for a in range(0, 401)]\n"
                 "print(json.dumps({'vals': vals,\n"
                 "    'mono': all(x >= y for x, y in zip(sweep, sweep[1:])),\n"
                 "    'bounded': all(0.5 <= v <= 1.0 for v in sweep)}))\n")
    v = out["vals"]
    assert v["None"] == 1.0 and v["-365"] == 1.0 and v["-5"] == 1.0
    assert v["0"] == 1.0 and v["30"] == 1.0          # offset band exact
    assert v["31"] == 2.0 ** (-1 / 90)               # same float expression
    assert v["60"] == 2.0 ** (-30 / 90)
    assert v["120"] == 0.5                           # exact power of two
    assert v["1000"] == 0.5                          # floor holds
    assert out["mono"] and out["bounded"]


def test_apply_recency_pure_probe(tmp_path):
    root = _sbx(tmp_path)
    out = _probe(root, tmp_path, "apply_probe.py",
                 "rows = [('kb-A', 'notes/a.md', 'A', 'sa', 1.0 / 61),\n"
                 "        ('kb-B', 'notes/b.md', 'B', 'sb', 1.0 / 62),\n"
                 "        ('kb-C', 'notes/c.md', 'C', None, 0.05)]\n"
                 "vo = {'notes/c.md'}\n"
                 "r1, c1 = m._apply_recency(rows, [200, 5, 5], vo)\n"
                 "r2, c2 = m._apply_recency(rows, [5, 5, 5], vo)\n"
                 "r3, c3 = m._apply_recency(rows, [200, 400, 5], vo)\n"
                 "print(json.dumps({'ids1': [r[0] for r in r1], 'c1': c1,\n"
                 "    'ids2': [r[0] for r in r2], 'c2': c2,\n"
                 "    'ids3': [r[0] for r in r3], 'c3': c3}))\n")
    # C's raw 0.05 dwarfs every lexical score — a global sort would put it
    # first; the blockwise key must keep it last regardless.
    assert out["ids1"] == ["kb-B", "kb-A", "kb-C"] and out["c1"]
    assert out["ids2"] == ["kb-A", "kb-B", "kb-C"] and not out["c2"]
    assert out["ids3"] == ["kb-A", "kb-B", "kb-C"] and not out["c3"]
