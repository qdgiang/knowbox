"""`knowbox init` skeleton semantics (knowbox split kb-0142).

Init creates a fresh KB (notes/refs/raw + .gitkeep, log.md, SCHEMA stub,
.gitignore, .gitattributes, index.md+DB under the lock) and a git repo with
repo-local identity kb/kb@localhost and one commit — never a remote.
"""
import shutil

import pytest

from conftest import build_sandbox, git_run, kb_run

GIT = shutil.which("git")


def test_init_creates_skeleton_with_lf_endings(tmp_path):
    root = tmp_path / "kb"
    kb_run("init", str(root), root=tmp_path, expect_rc=0)
    for d in ("notes", "refs", "raw"):
        assert (root / d).is_dir() and (root / d / ".gitkeep").exists()
    for f in ("index.md", "log.md", "SCHEMA.md", ".gitignore",
              ".gitattributes", ".kb.db"):
        assert (root / f).exists(), f"missing {f}"
    # LF everywhere — load-bearing on Windows (kb-0142 Phase D)
    for f in ("index.md", "log.md", "SCHEMA.md", ".gitignore", ".gitattributes"):
        assert b"\r" not in (root / f).read_bytes(), f"CR byte in {f}"


def test_init_git_repo_local_identity_one_commit_no_remote(tmp_path):
    pytest.mark.skipif(not GIT, reason="git not on PATH")
    root = tmp_path / "kb"
    kb_run("init", str(root), root=tmp_path, expect_rc=0)
    assert (root / ".git").is_dir()
    assert git_run(root, "config", "user.name").strip() == "kb"
    assert git_run(root, "config", "user.email").strip() == "kb@localhost"
    assert git_run(root, "rev-list", "--count", "HEAD").strip() == "1"
    first = git_run(root, "log", "-1", "--format=%s").strip()
    assert first == "kb: init — empty KB skeleton"
    assert git_run(root, "ls-files").split(), "first commit carries the skeleton"
    assert git_run(root, "remote").strip() == "", "init never adds a remote"


def test_init_refuses_existing_kb(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    r = kb_run("init", str(root), root=tmp_path)
    assert r.returncode != 0


def test_init_refuses_non_empty_dir(tmp_path):
    root = tmp_path / "kb"
    root.mkdir()
    (root / "stray.txt").write_text("x\n")
    r = kb_run("init", str(root), root=tmp_path)
    assert r.returncode != 0


def test_init_no_git_skips_repo(tmp_path):
    root = tmp_path / "kb"
    kb_run("init", "--no-git", str(root), root=tmp_path, expect_rc=0)
    assert not (root / ".git").exists()
    assert (root / "index.md").exists() and (root / ".kb.db").exists()


def test_init_round_trip_lint_new_search_reindex(tmp_path):
    root = tmp_path / "kb"
    kb_run("init", str(root), root=tmp_path, expect_rc=0)
    r = kb_run("lint", root=root, env_extra={"KNOWBOX_NO_HEARTBEAT": "1"})
    assert r.returncode == 0 and "0 errors, 0 warnings" in r.stdout
    r = kb_run("new", "--type", "fact", "--domain", "test",
               "--title", "round trip note", "--body", "first body\n",
               root=root, expect_rc=0)
    assert "kb-0001" in r.stdout
    r = kb_run("search", "round trip", root=root, expect_rc=0)
    assert "kb-0001" in r.stdout
    r = kb_run("reindex", root=root, expect_rc=0)
    assert "no changes" in r.stdout
