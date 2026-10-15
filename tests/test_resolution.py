"""Root resolution matrix (hardening plan P1; knowbox split kb-0142).

KNOWBOX_ROOT set → honored (any dir name) / bogus → loud exit 2. KB_* env
spellings were removed at 2.0.0 (test_env_fallback.py pins that KB_ROOT is
ignored). Unset → ~/knowbox with loud validation: fake-HOME tests pin
UV_PYTHON_INSTALL_DIR + the real UV_CACHE_DIR so uv resolves its managed
CPython without the real HOME (manual smoke gate 2026-10-08: rc 2 against an
empty fake HOME, interpreter found via the pin).
"""
import os
import subprocess

from conftest import CLI_SRC, UV, build_sandbox, kb_run

UV_PY_DIR = "/home/giang/.local/share/uv/python"
UV_CACHE = os.path.expanduser("~/.cache/uv")


def _fake_home_env(home):
    env = {k: v for k, v in os.environ.items()
           if k not in ("KNOWBOX_ROOT", "KB_ROOT")}
    env.update({"HOME": str(home),
                "UV_PYTHON_INSTALL_DIR": UV_PY_DIR,
                "UV_CACHE_DIR": UV_CACHE,
                "KNOWBOX_API_KEY": "test-token",
                "KNOWBOX_EMBED_URL": "http://127.0.0.1:9/embeddings"})
    return env


def _fake_home_run(*args, home, timeout=180):
    return subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "--script",
         str(CLI_SRC), *[str(a) for a in args]],
        capture_output=True, text=True, env=_fake_home_env(home),
        stdin=subprocess.DEVNULL, timeout=timeout)


def test_kb_root_honored_for_dir_not_named_kb(tmp_path):
    # the pre-P1 bug: a copy outside a dir named "kb" silently used ~/knowbox
    root = build_sandbox(tmp_path / "sandbox-root", corpus="mini", git=False)
    r = kb_run("next-id", root=root, expect_rc=0)
    assert r.stdout.strip() == "kb-9013"


def test_kb_root_bogus_exits_loudly_never_falls_back(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    # script exists, but KNOWBOX_ROOT points at a nonexistent dir → must
    # exit 2 loudly instead of silently falling back to ~/knowbox
    r = kb_run("next-id", root=root,
               env_extra={"KNOWBOX_ROOT": str(tmp_path / "does-not-exist")})
    assert r.returncode == 2
    assert "KNOWBOX_ROOT" in (r.stdout + r.stderr)


def test_fake_home_fallback_existing_kb(tmp_path):
    # unset root → ~/knowbox: a KB at fake-HOME/knowbox is found without any env var
    home = tmp_path / "home"
    root = build_sandbox(home / "knowbox", corpus="mini", git=False)
    r = _fake_home_run("next-id", home=home)
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "kb-9013"


def test_fake_home_fallback_no_kb_gate_exit2(tmp_path):
    home = tmp_path / "home"
    home.mkdir()
    r = _fake_home_run("next-id", home=home)
    assert r.returncode == 2
    out = r.stdout + r.stderr
    assert "knowbox init" in out and "KNOWBOX_ROOT" in out


def test_fake_home_fallback_dir_without_notes_gate_exit2(tmp_path):
    home = tmp_path / "home"
    (home / "knowbox").mkdir(parents=True)   # exists but no notes/ → not a KB
    r = _fake_home_run("next-id", home=home)
    assert r.returncode == 2
    assert "knowbox init" in (r.stdout + r.stderr)


def test_fake_home_init_creates_skeleton(tmp_path):
    home = tmp_path / "home"
    home.mkdir()
    r = _fake_home_run("init", home=home)
    assert r.returncode == 0, r.stderr
    root = home / "knowbox"
    assert (root / "notes").is_dir() and (root / "index.md").exists()
    r2 = _fake_home_run("next-id", home=home)
    assert r2.returncode == 0 and r2.stdout.strip() == "kb-0001"
