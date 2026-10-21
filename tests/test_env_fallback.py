"""KB_* env fallback removed at 2.0.0 (kb-0142 decision 5, 2026-10-09).

Regression pin: with KNOWBOX_ROOT unset, a KB_ROOT pointing at a valid KB
must be IGNORED — against an empty fake HOME the CLI must exit 2 with the
init gate message, never silently use the KB_ROOT dir.
"""
from conftest import build_sandbox

from test_resolution import _fake_home_env, _fake_home_run


def test_kb_root_ignored_at_2_0(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    home = tmp_path / "home"
    home.mkdir()
    env = _fake_home_env(home)
    env["KB_ROOT"] = str(root)
    import subprocess
    from conftest import CLI_SRC, UV
    r = subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "--script",
         str(CLI_SRC), "next-id"],
        capture_output=True, text=True, env=env,
        stdin=subprocess.DEVNULL, timeout=180)
    assert r.returncode == 2
    out = r.stdout + r.stderr
    assert "knowbox init" in out and "KNOWBOX_ROOT" in out
