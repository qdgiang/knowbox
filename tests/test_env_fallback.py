"""KNOWBOX_*/KB_* env fallback policy (kb-0142 decision 5).

KNOWBOX_* is primary; KB_* is honored as the deprecated one-release fallback
(drop at 2.0). When both are set, KNOWBOX_* wins.
"""
from conftest import build_sandbox, kb_run


def test_kb_root_honored_when_knowbox_root_unset(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    r = kb_run("next-id", root=root,
               env_extra={"KNOWBOX_ROOT": None, "KB_ROOT": str(root)},
               expect_rc=0)
    assert r.stdout.strip() == "kb-9013"


def test_knowbox_root_wins_when_both_set(tmp_path):
    a = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    b = tmp_path / "does-not-exist"
    # KB_ROOT points at the real sandbox; KNOWBOX_ROOT at a bogus dir —
    # primary wins, so the bogus dir must trigger the loud exit 2.
    r = kb_run("next-id", root=a,
               env_extra={"KNOWBOX_ROOT": str(b), "KB_ROOT": str(a)})
    assert r.returncode == 2
    assert "KNOWBOX_ROOT" in (r.stdout + r.stderr)
