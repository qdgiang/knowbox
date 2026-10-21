"""Known-bug characterization tests — each xfail(strict) pinned one audit finding.

The fixing phase REMOVED the marker in the same commit (red→green on GitHub):
- T1 → test_crontab.py (P2)
- T2 stale index served silently → FIXED in P8 (self-heal assertion here;
  full coverage in test_freshness.py)
- T4 next_id recycles deleted ids → FIXED in P9 (log high-water, below;
  full coverage in test_bookkeeping.py)
- T5 touch rewrites BODY when fm lacks updated: → FIXED in P4 (refusal
  assertion here)

All audit xfails are now flipped; this file keeps the flipped regressions.
"""
from conftest import build_sandbox, kb_run, note_path


def test_search_sees_direct_file_edit(tmp_path):
    # T2 FIXED in P8: freshness = content sha1 (kb_meta inside .kb.db), not
    # DB existence. A behind-the-CLI edit is found by the NEXT search, which
    # rebuilds inline and says so on stderr — then stays quiet once fresh.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    p = note_path(root, "kb-9001")
    p.write_text(p.read_text() + "\nCANARYZORK token added behind the CLI's back.\n")
    r = kb_run("search", "CANARYZORK", root=root)
    assert "kb-9001" in r.stdout
    assert "stale" in r.stderr and "rebuilt" in r.stderr, \
        "self-heal must announce itself on stderr"
    r2 = kb_run("search", "CANARYZORK", root=root)
    assert "kb-9001" in r2.stdout
    assert "stale" not in r2.stderr, "fresh index must rebuild silently"


def test_next_id_does_not_recycle_deleted_ids(tmp_path):
    # T4 FIXED in P9: next_id = max(filename ids ∪ log.md create ids) + 1.
    # The mini fixture seeds log.md create entries, so the log half of the
    # union still knows kb-9012 after its file is deleted.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    note_path(root, "kb-9012").unlink()
    r = kb_run("next-id", root=root, expect_rc=0)
    assert r.stdout.strip() == "kb-9013"     # must NOT return kb-9012


def test_touch_never_rewrites_body(tmp_path):
    # T5 FIXED in P4: cmd_touch no longer regexes the whole file. A note
    # whose frontmatter lacks `updated:` is now REFUSED (non-zero exit, no
    # write at all) instead of silently rewriting a body line — the body
    # stays byte-identical.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("no_updated",))
    p = note_path(root, "kb-9101")
    before = p.read_bytes()
    r = kb_run("touch", "kb-9101", root=root)
    assert r.returncode != 0, \
        "touch must refuse (non-zero) a note whose fm lacks updated:"
    assert "updated" in (r.stdout + r.stderr), "refusal must name the problem"
    assert p.read_bytes() == before, \
        "refused touch must leave the file byte-identical"
