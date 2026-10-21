"""Lint waiver mechanism (zero-warning baseline, 2026-09-08).

`lint-waivers.md` at the KB root suppresses (note-id, category) warnings for
the created-vs-log family only — append-only log.md history that can never be
fixed in the data (hand-made pre-journal notes, the 2026-08-06 id recycling).
These tests pin: suppression + the informational WAIVED section, the
category policy (word-budget & co are NOT waivable), stale/malformed waiver
hygiene (the file can't rot silently), and the summary-line format the
janitor parses.
"""
import re

from conftest import build_sandbox, kb_run

SUMMARY_RE = re.compile(r"^(\d+) errors, (\d+) warnings across (\d+) notes$",
                        re.M)


def _handmade_note(root, nid, slug, created="2026-08-05"):
    """A well-formed note with NO log.md create entry and created AFTER the
    first log date → exactly one created-vs-log WARN, nothing else."""
    (root / "notes" / f"{nid}-{slug}.md").write_text(
        "---\n"
        f"id: {nid}\n"
        f'title: "Hand-made note {nid}"\n'
        "type: fact\ndomain: tooling\ntags: []\naliases: []\n"
        "triggers:\n  - \"testing the lint waiver mechanism\"\n"
        "status: active\nconfidence: high\n"
        f"created: {created}\nupdated: {created}\n"
        "supersedes: []\nrelated: []\nsource: \"session\"\n"
        'summary: "Fixture for the waiver tests."\n---\nBody.\n',
        encoding="utf-8")


def _clean_sandbox(tmp_path):
    """mini corpus minus the deliberate oversized ERROR, plus two hand-made
    notes; reindexed so the P8 freshness checks stay quiet."""
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "notes" / "kb-9011-oversized-note.md").unlink()
    _handmade_note(root, "kb-9013", "handmade-thirteen")
    _handmade_note(root, "kb-9014", "handmade-fourteen")
    kb_run("reindex", root=root, expect_rc=0)
    return root


def _lint(root):
    r = kb_run("lint", root=root, expect_rc=0)   # 0 errors in this fixture
    out = (r.stdout + r.stderr)
    m = SUMMARY_RE.search(out)
    assert m, f"summary line format changed — janitor regex would break:\n{out}"
    return out, int(m.group(1)), int(m.group(2))


def test_baseline_without_waiver_file_warns_both(tmp_path):
    root = _clean_sandbox(tmp_path)
    out, nerr, nwarn = _lint(root)
    assert nerr == 0
    assert "kb-9013" in out and "no create entry" in out
    warns_9013 = [l for l in out.splitlines()
                  if l.startswith("WARN") and "kb-9013" in l]
    warns_9014 = [l for l in out.splitlines()
                  if l.startswith("WARN") and "kb-9014" in l]
    assert len(warns_9013) == 1 and len(warns_9014) == 1
    assert "WAIVED" not in out, "no waiver file → no WAIVED section"


def test_waiver_suppresses_only_the_waived_note(tmp_path):
    root = _clean_sandbox(tmp_path)
    _out, _e, nwarn_before = _lint(root)
    (root / "lint-waivers.md").write_text(
        "# Lint waivers — see SCHEMA.md. Prose and comments are ignored.\n"
        "kb-9013 created-vs-log 2026-09-08 hand-made test fixture note\n",
        encoding="utf-8")
    out, nerr, nwarn_after = _lint(root)
    assert nerr == 0
    assert nwarn_after == nwarn_before - 1, "waived warning must leave the count"
    # the waived note's forensic WARN is gone from the WARN lines…
    assert not [l for l in out.splitlines()
                if l.startswith("WARN") and "kb-9013" in l]
    # …but visible in the informational WAIVED section (not silent deletion)
    assert "WAIVED (1)" in out and "kb-9013" in out.split("WAIVED (1)")[1]
    # the NON-waived note with the SAME category still warns
    assert [l for l in out.splitlines()
            if l.startswith("WARN") and "kb-9014" in l and "no create entry" in l]


def test_unknown_category_is_rejected_and_warns(tmp_path):
    # word-budget is content — never waivable (policy: SCHEMA.md)
    root = _clean_sandbox(tmp_path)
    (root / "notes" / "kb-9015-wordy-note.md").write_text(
        "---\nid: kb-9015\ntitle: \"Wordy note over the warn budget\"\n"
        "type: fact\ndomain: tooling\ntags: []\naliases: []\n"
        "triggers:\n  - \"testing that word budget is not waivable\"\n"
        "status: active\nconfidence: high\ncreated: 2026-08-01\n"
        "updated: 2026-08-01\nsupersedes: []\nrelated: []\n"
        "source: \"session\"\nsummary: \"Wordy fixture.\"\n---\n"
        "First sentence is the fact.\n\n"
        + " ".join(f"filler{i}" for i in range(470)) + "\n",
        encoding="utf-8")
    (root / "lint-waivers.md").write_text(
        "kb-9015 word-budget 2026-09-08 trying to waive a content warning\n",
        encoding="utf-8")
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("lint", root=root)
    out = r.stdout + r.stderr
    assert "unknown category 'word-budget'" in out, \
        "waiving a non-waivable category must WARN, not suppress"
    assert [l for l in out.splitlines()
            if l.startswith("WARN") and "kb-9015" in l and "words" in l], \
        "the word-budget warning must still fire"


def test_stale_waiver_warns(tmp_path):
    root = _clean_sandbox(tmp_path)
    (root / "lint-waivers.md").write_text(
        # dead id — nothing to suppress
        "kb-9999 created-vs-log 2026-09-08 this note does not exist\n"
        # real note, but it has no created-vs-log warning
        "kb-9001 created-vs-log 2026-09-08 this note is journal-consistent\n",
        encoding="utf-8")
    out, _e, _w = _lint(root)
    stale = [l for l in out.splitlines() if "stale waiver" in l]
    assert len(stale) == 2, f"both dead-id and no-op waivers must WARN:\n{out}"
    assert any("kb-9999" in l for l in stale)
    assert any("kb-9001" in l for l in stale)


def test_malformed_waiver_lines_warn(tmp_path):
    root = _clean_sandbox(tmp_path)
    (root / "lint-waivers.md").write_text(
        "# comment lines are ignored\n"
        "Prose without a leading kb-id is ignored too.\n"
        "kb-9013 created-vs-log not-a-date reason here\n"   # bad date
        "kb-9013 created-vs-log 2026-09-08\n",              # missing reason
        encoding="utf-8")
    out, _e, _w = _lint(root)
    malformed = [l for l in out.splitlines()
                 if l.startswith("WARN") and "lint-waivers.md" in l]
    assert len(malformed) == 2, f"both bad lines must WARN:\n{out}"
    # …and because neither line parsed, kb-9013's warning is NOT suppressed
    assert [l for l in out.splitlines()
            if l.startswith("WARN") and "kb-9013" in l and "no create entry" in l]


def test_error_cannot_be_waived(tmp_path):
    # Audit N4 pin (2026-09-11): waivers route WARNs only. A waiver naming a
    # note that carries an ERROR must leave the ERROR firing — and, having
    # suppressed nothing, itself WARN as stale.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    (root / "lint-waivers.md").write_text(
        "kb-9011 created-vs-log 2026-09-08 attempt to silence an ERROR note\n",
        encoding="utf-8")
    r = kb_run("lint", root=root)   # mini keeps kb-9011 oversized → rc nonzero
    out = r.stdout + r.stderr
    assert [l for l in out.splitlines()
            if l.startswith("ERROR") and "kb-9011" in l], \
        "the oversized-note ERROR must fire despite the waiver line"
    assert any("stale waiver" in l and "kb-9011" in l for l in out.splitlines()), \
        "a waiver that suppressed nothing must WARN as stale"


def test_waiver_mechanism_inert_on_frozen_corpus(frozen_sandbox):
    # The frozen corpus has no lint-waivers.md → the mechanism must be fully
    # inert: no WAIVED section, no waiver hygiene warnings. (Byte-identity of
    # the whole report is pinned by test_index_lint_golden.py's golden.)
    r = kb_run("lint", root=frozen_sandbox)
    out = r.stdout + r.stderr
    assert "WAIVED" not in out
    assert "lint-waivers.md" not in out
    assert "stale waiver" not in out
