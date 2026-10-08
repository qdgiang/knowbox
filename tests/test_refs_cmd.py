"""`knowbox refs` heading index (P10b stretch, built early on user go-ahead 2026-09-11).

Read-only command: refs/ docs → word count, H2 sections (code-fence aware),
and the type:reference map note covering each doc. UNMAPPED docs are the
actionable signal (SCHEMA.md § Large docs: every refs doc needs a section map).
"""
from conftest import build_sandbox, kb_run


def _refs_out(root, *extra):
    r = kb_run("refs", *extra, root=root, expect_rc=0)
    return r.stdout + r.stderr


def test_mini_unmapped_doc_flagged_with_headings(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    out = _refs_out(root)
    assert "mini-guide.md" in out
    assert "- Setup" in out, "H2 sections must be listed"
    assert "UNMAPPED" in out and "act on UNMAPPED" in out
    assert "1 doc(s), 0 mapped, 1 UNMAPPED" in out


def test_frozen_corpus_docs_map_to_their_reference_notes(frozen_sandbox):
    out = _refs_out(frozen_sandbox)
    assert "12 doc(s)" in out
    block = out.split("claude-harness-audit-2026-08-05.md")[1]
    assert "map: kb-0045" in block.splitlines()[0], \
        "the harness-audit section-map note must be linked to its doc"
    block = out.split("2026-08-24-herdr-plugins-productivity.md")[1]
    assert "map: kb-0072" in block.splitlines()[0]


def test_filter_narrows_by_filename(frozen_sandbox):
    out = _refs_out(frozen_sandbox, "gastown-research")
    assert "gastown-research-2026-08-09.md" in out
    assert "herdr-guide.md" not in out
    assert "1 of 12 docs shown" in out


def test_code_fenced_hash_lines_are_not_headings(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "refs" / "fenced.md").write_text(
        "# Fenced\n\n## Real Section\n\n```md\n## NotAHeading\n```\n",
        encoding="utf-8")
    out = _refs_out(root)
    assert "- Real Section" in out
    assert "NotAHeading" not in out
    assert "2 doc(s), 0 mapped, 2 UNMAPPED" in out


def test_empty_refs_dir(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "refs" / "mini-guide.md").unlink()
    out = _refs_out(root)
    assert "refs/ is empty" in out


def test_help_lists_refs(frozen_sandbox):
    r = kb_run("--help", root=frozen_sandbox, expect_rc=0)
    assert "knowbox refs [FILTER]" in r.stdout
