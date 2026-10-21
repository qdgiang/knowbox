"""knowbox conflicts — deterministic similarity report (hardening plan P12).

Design under test: pure cosine over the embedding store, NO LLM in the CLI
(user decision); live notes only (superseded excluded); loud failures on a
missing/mismatched store; deterministic ordering ((-cos, a, b)) so two runs
are byte-identical; --json key-stable for the judge-pass skill. Vectors are
hand-crafted (plan §3): tiny orthogonal/parallel directions, no network.
"""
import json

from conftest import build_sandbox, kb_run


def _write_embed(root, vectors, model="text-embedding-v4"):
    (root / ".kb-embed.json").write_text(json.dumps(
        {"model": model,
         "notes": {nid: {"h": f"fake-{nid}", "v": v}
                   for nid, v in vectors.items()}}))


PARALLEL_A = [1.0, 0.0, 0.0]
PARALLEL_B = [1.0, 0.001, 0.0]      # cos ≈ 0.9999995 with A
ORTHOGONAL = [0.0, 1.0, 0.0]


def test_expected_pairs_and_order(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write_embed(root, {"kb-9001": PARALLEL_A, "kb-9002": PARALLEL_B,
                        "kb-9003": ORTHOGONAL})
    r = kb_run("conflicts", root=root, expect_rc=0)
    assert "kb-9001" in r.stdout and "kb-9002" in r.stdout
    assert "kb-9003" not in r.stdout, \
        "orthogonal note must not appear at default min-cos"
    assert "cos 0.99" in r.stdout or "cos 1.00" in r.stdout
    # exactly one pair block
    assert r.stdout.count("cos 0.") + r.stdout.count("cos 1.") == 1


def test_superseded_excluded(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    # kb-9004 is superseded in the mini fixture — even a parallel vector
    # must keep it out of the report
    _write_embed(root, {"kb-9001": PARALLEL_A, "kb-9002": PARALLEL_B,
                        "kb-9004": PARALLEL_A})
    r = kb_run("conflicts", root=root, expect_rc=0)
    assert "kb-9004" not in r.stdout


def test_missing_store_fails_loud(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    r = kb_run("conflicts", root=root)
    assert r.returncode != 0
    assert "knowbox embed" in (r.stdout + r.stderr)


def test_model_mismatch_fails_loud(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write_embed(root, {"kb-9001": PARALLEL_A, "kb-9002": PARALLEL_B},
                 model="some-other-model")
    r = kb_run("conflicts", root=root)
    assert r.returncode != 0
    assert "--force" in (r.stdout + r.stderr)


def test_unembedded_corpus_fails_loud(tmp_path):
    # store exists + model matches, but no live note has a vector
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write_embed(root, {"kb-7777": PARALLEL_A})
    r = kb_run("conflicts", root=root)
    assert r.returncode != 0
    assert "knowbox embed" in (r.stdout + r.stderr)


def test_json_keys_and_stability(tmp_path):
    # identical vectors → exact 1.0 ties → the (-cos, a, b) tie-break is
    # fully determined (4-decimal rounding of near-parallel vectors would
    # create artificial ties the unrounded sort legitimately orders by cos)
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write_embed(root, {"kb-9001": PARALLEL_A, "kb-9002": PARALLEL_A,
                        "kb-9003": ORTHOGONAL, "kb-9005": PARALLEL_A})
    a = kb_run("conflicts", "--json", root=root, expect_rc=0)
    b = kb_run("conflicts", "--json", root=root, expect_rc=0)
    assert a.stdout == b.stdout, "two runs must be byte-identical"
    data = json.loads(a.stdout)
    assert isinstance(data, list) and data
    required = {"a", "b", "cos", "a_status", "b_status", "a_updated",
                "b_updated", "a_summary", "b_summary"}
    assert all(required <= set(entry) for entry in data)
    # deterministic order: cos desc, then (a, b) — exact ties here
    keys = [(-e["cos"], e["a"], e["b"]) for e in data]
    assert keys == sorted(keys)
    assert [(e["a"], e["b"]) for e in data] == \
        [("kb-9001", "kb-9002"), ("kb-9001", "kb-9005"),
         ("kb-9002", "kb-9005")]
    assert "kb-9003" not in {e["a"] for e in data} | {e["b"] for e in data}


def test_json_cos_non_increasing(tmp_path):
    # near-parallel (not identical) vectors: rounding ties are fine, but the
    # reported scores must never increase
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write_embed(root, {"kb-9001": PARALLEL_A, "kb-9002": PARALLEL_B,
                        "kb-9005": [1.0, 0.02, 0.0]})
    data = json.loads(kb_run("conflicts", "--json", root=root,
                             expect_rc=0).stdout)
    cos = [e["cos"] for e in data]
    assert cos == sorted(cos, reverse=True), cos
    assert len(data) == 3


def test_top_and_min_cos_flags(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write_embed(root, {"kb-9001": PARALLEL_A, "kb-9002": PARALLEL_A,
                        "kb-9003": ORTHOGONAL, "kb-9005": PARALLEL_A})
    full = json.loads(kb_run("conflicts", "--json", root=root,
                             expect_rc=0).stdout)
    assert len(full) == 3                       # 9001-9002, 9001-9005, 9002-9005
    one = json.loads(kb_run("conflicts", "--json", "--top", "1", root=root,
                            expect_rc=0).stdout)
    assert len(one) == 1 and one[0] == full[0], \
        "--top must cap the list (--json before --top used to eat it)"
    none = kb_run("conflicts", "--min-cos", "1.1", root=root, expect_rc=0)
    assert "no live-note pairs" in none.stdout
    bad = kb_run("conflicts", "--top", "abc", root=root)
    assert bad.returncode != 0
