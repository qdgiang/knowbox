"""Staleness detection (hardening plan P8, audit finding T2).

`.kb.db` now carries a kb_meta table (path, content-sha1, bytes) written at
every rebuild; _freshness_report() compares it against the notes on disk,
index.md's listed ids against the ids on disk (orphans), and index.md's bytes
against freshly generated lines (hand-edits). Consumers:

- `knowbox search` self-heals: rebuild once per invocation + one stderr note (D6).
  Unlocked by design — readers never take the flock; unique-tmp + atomic
  replace make concurrent rebuilds last-writer-wins.
- `knowbox lint` turns each failure mode into an ERROR naming `knowbox reindex`,
  replacing the old substring-presence WARN (SCHEMA's orphan claim is now
  actually machine-checked).

sha1, not mtime: git clone/rsync/touch all make mtime lie — see the P8
decision note in ~/kb/notes.
"""
from conftest import build_sandbox, kb_run, note_path


def _lint(root):
    r = kb_run("lint", root=root)
    return r, (r.stdout + r.stderr).splitlines()


def test_fresh_state_no_stale_errors_and_silent_search(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    _r, lines = _lint(root)
    assert not any("stale" in l or "orphan" in l or "content drift" in l
                   for l in lines), lines
    s = kb_run("search", "grubby", root=root, expect_rc=0)
    assert "stale" not in s.stderr, "fresh index must not announce a rebuild"


def test_missing_db_is_stale_error(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    (root / ".kb.db").unlink()
    r, lines = _lint(root)
    assert r.returncode == 1
    assert any(l.startswith("ERROR") and "FTS db missing" in l
               for l in lines), lines


def test_lint_names_drifted_note_path(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    p = note_path(root, "kb-9001")
    p.write_text(p.read_text() + "\nDrifted behind the CLI's back.\n")
    r, lines = _lint(root)
    assert r.returncode == 1
    hits = [l for l in lines if l.startswith("ERROR") and "drifted" in l]
    assert hits, lines
    assert p.name in hits[0] and "knowbox reindex" in hits[0], hits[0]


def test_deleted_note_counts_as_drift(tmp_path):
    # kb_meta has a row the disk no longer matches — deletions must not
    # silently shrink the index's idea of the corpus
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    note_path(root, "kb-9003").unlink()
    _r, lines = _lint(root)
    assert any(l.startswith("ERROR") and "drifted" in l for l in lines), lines


def test_superseded_note_edit_detected(tmp_path):
    # kb_meta covers EVERY notes/*.md file, not just FTS-indexed ones:
    # drift detection must not depend on parse/index status
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    p = note_path(root, "kb-9004")          # superseded
    p.write_text(p.read_text() + "\nEdited while retired.\n")
    _r, lines = _lint(root)
    assert any(l.startswith("ERROR") and "drifted" in l for l in lines), lines


def test_hand_edited_index_summary_is_content_drift(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    idx = root / "index.md"
    text = idx.read_text()
    edited = text.replace("routed.", "hand-edited routing.", 1)
    assert edited != text
    idx.write_text(edited)
    r, lines = _lint(root)
    assert r.returncode == 1
    assert any(l.startswith("ERROR") and "content drift" in l
               for l in lines), lines


def test_orphan_index_entry_named(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    idx = root / "index.md"
    idx.write_text(idx.read_text()
                   + "\n- [kb-9999 · ghost entry](notes/kb-9999-ghost.md): x")
    r, lines = _lint(root)
    assert r.returncode == 1
    hits = [l for l in lines if l.startswith("ERROR") and "orphan" in l]
    assert hits and "kb-9999" in hits[0], lines


def test_search_rebuilds_once_then_lint_agrees(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    p = note_path(root, "kb-9002")
    p.write_text(p.read_text() + "\nZORKREBUILD token.\n")
    r1 = kb_run("search", "ZORKREBUILD", root=root)
    assert "kb-9002" in r1.stdout
    assert "stale" in r1.stderr and "rebuilt" in r1.stderr
    r2 = kb_run("search", "ZORKREBUILD", root=root)
    assert "kb-9002" in r2.stdout and "stale" not in r2.stderr
    # the search-triggered rebuild healed the derived state: lint agrees
    _r, lines = _lint(root)
    assert not any("stale" in l for l in lines), lines
