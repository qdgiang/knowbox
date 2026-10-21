"""Characterization tests for the writer commands + mini-corpus behaviors.

Everything here passes on the CURRENT CLI (status quo pinned); later phases
extend this file. Push behavior is tested against a local bare fake remote —
no network, no monkeypatching.
"""
import datetime

from conftest import (build_sandbox, git_dir_run, git_run, kb_run, note_path,
                      search_ids)

TODAY = datetime.date.today().isoformat()


def _fm_of(path):
    return path.read_text().split("---")[1]


def test_writer_lifecycle_new_touch_verify_supersede_reindex(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", remote=True)
    bare = root.parent / "remotes" / "origin.git"

    # --- new: file, frontmatter, index, log, commit, PUSH
    r = kb_run("new", "--type", "gotcha", "--domain", "tooling",
               "--title", "widget explodes on Tuesdays",
               "--summary", "widget Tuesday explosion routing hint",
               "--tags", "widget",
               "--body", "The widget explodes on Tuesdays.\n",
               root=root, expect_rc=0)
    created = r.stdout.strip().splitlines()[-1]
    assert created.endswith(".md") and "/notes/" in created
    p = note_path(root, "kb-9013")           # mini has kb-9001..9012
    fm = _fm_of(p)
    assert "status: active" in fm and "type: gotcha" in fm
    assert f"created: {TODAY}" in fm
    assert "kb-9013" in (root / "index.md").read_text()
    assert "create | kb-9013" in (root / "log.md").read_text()
    assert "knowbox: create" in git_run(root, "log", "--oneline", "-1")
    assert "knowbox: create" in git_dir_run(bare, "log", "--oneline", "-1"), \
        "git_commit did not push to the fake remote"

    # --- touch: bumps updated: only
    before = note_path(root, "kb-9001").read_text()
    kb_run("touch", "kb-9001", root=root, expect_rc=0)
    after = note_path(root, "kb-9001").read_text()
    assert f"updated: {TODAY}" in _fm_of(note_path(root, "kb-9001"))
    assert before != after

    # --- verify: flips status + stamps verified:
    kb_run("verify", "kb-9003", "--evidence", "test probe", root=root,
           expect_rc=0)
    fm = _fm_of(note_path(root, "kb-9003"))
    assert "status: verified" in fm and f"verified: {TODAY}" in fm

    # --- supersede: links both ways, retires OLD, excludes from search,
    #     keeps OLD in index with flag
    kb_run("supersede", "kb-9003", "--with", "kb-9013", root=root,
           expect_rc=0)
    old = note_path(root, "kb-9003").read_text()
    fm_old = _fm_of(note_path(root, "kb-9003"))
    fm_new = _fm_of(note_path(root, "kb-9013"))
    assert "status: superseded" in fm_old
    assert "kb-9013" in old and "Superseded" in old       # body banner
    assert "kb-9003" in fm_new  # supersedes: link
    # related: cross-add (kb-0086 leftover fix, 2026-09-11) — the banner's
    # [[kb-9013]] must be covered by OLD's related:, and NEW links back
    rel_old = next(l for l in fm_old.splitlines() if l.startswith("related:"))
    rel_new = next(l for l in fm_new.splitlines() if l.startswith("related:"))
    assert "kb-9013" in rel_old, "OLD.related must carry the successor"
    assert "kb-9003" in rel_new, "NEW.related must carry the predecessor"
    lint_out = kb_run("lint", root=root).stdout          # rc≠0: mini's ERROR
    assert not [l for l in lint_out.splitlines()
                if l.startswith("WARN") and "kb-9003" in l
                and "not in related" in l], \
        "banner link without related: entry must not WARN anymore"
    idx = (root / "index.md").read_text()
    assert "SUPERSEDED" in idx and "kb-9003" in idx
    ids = search_ids(kb_run("search", "migration", root=root).stdout)
    assert "kb-9003" not in ids, "superseded note leaked into search"

    # --- reindex: idempotent
    idx1 = (root / "index.md").read_text()
    kb_run("reindex", root=root, expect_rc=0)
    assert (root / "index.md").read_text() == idx1


def test_git_commit_stages_data_paths(tmp_path):
    # kb-0142 shape: the data repo stages the notes-only path tuple — tests/,
    # pyproject.toml, uv.lock left the staging set with the CLI split
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    (root / "raw" / "transcript.md").write_text("# raw probe\n")
    kb_run("new", "--type", "fact", "--domain", "tooling",
           "--title", "staging probe", "--body", "probe.\n",
           root=root, expect_rc=0)
    stat = git_run(root, "show", "--stat", "--oneline", "HEAD")
    assert "raw/transcript.md" in stat and "notes/kb-9013" in stat


def test_superseded_excluded_session_note_hidden_superseded_flagged(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    idx = (root / "index.md").read_text()
    assert "kb-9006" not in idx, "session note must not enter the index"
    assert "kb-9001" in idx
    assert "SUPERSEDED" in idx and "kb-9004" in idx
    ids = search_ids(kb_run("search", "widget", root=root).stdout)
    assert "kb-9005" in ids and "kb-9004" not in ids


def test_lint_flags_known_mini_issues(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    r = kb_run("lint", root=root)
    out = r.stdout + r.stderr
    assert r.returncode == 1
    assert "kb-9011" in out, "oversized note (>600 words) not flagged"
    assert "kb-9010" in out, "empty source: not flagged"


def test_touch_does_not_rewrite_body_when_frontmatter_has_updated(tmp_path):
    # kb-9008's body contains `updated:`/`status:` lines inside a code fence;
    # fm has updated: → the count=1 regex hits fm first. Safe case TODAY;
    # P4 must keep it safe via frontmatter-only rewriting.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9008")
    body_before = p.read_text().split("---", 2)[2]
    kb_run("touch", "kb-9008", root=root, expect_rc=0)
    body_after = p.read_text().split("---", 2)[2]
    assert body_before == body_after
