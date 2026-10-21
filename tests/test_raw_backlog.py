"""knowbox raw-backlog / raw-mark + lint raw/ health (kb-0149 design).

Design under test: `## ` headings are raw/ entry boundaries (bodies use ###
and deeper only); review state is a `> state: ingested kb-XXXX (date)` /
`> state: rejected <reason>` marker as the FIRST line under the heading;
legacy `> ingested:`/`> rejected:` lines anywhere in the block still resolve
the entry (the distill-era convention puts them at the END of the block).
A legacy marker in the PREAMBLE (above the first heading) resolves nothing
and gets a lint WARN — it skews the janitor's cardinality count.
`## pair:` judge-verdict blocks are exempt. The lint WARNs use the SAME
scanner, so the three surfaces can never disagree.
"""

from conftest import build_sandbox, kb_run

RAW_DOC = """# human notes 2026-10-13

## First idea
body text

### deep dive
nested heading is fine

## Second idea
more body
"""

LEGACY_ABOVE = """# distill-style file
> ingested: kb-9001
## PROPOSAL
orphan preamble marker resolves nothing

## PROPOSAL
no marker — backlog
"""

LEGACY_ENDBLOCK = """## First proposal
body text

> ingested: kb-9001

## Second proposal
no marker — backlog
"""

LEGACY_MIDBLOCK = """## Idea
body line one
> rejected: superseded by kb-9002
trailing body
"""

JUDGE_DOC = """# conflicts review
## pair: kb-9001 kb-9002 (cos 0.81)
> verdict: duplicate — merged into kb-9001
"""


def _write(root, name, text):
    (root / "raw" / name).write_text(text, encoding="utf-8")


def _lint(root):
    """Lint with the mini fixture's intentional findings reindexed away."""
    kb_run("reindex", root=root, expect_rc=0)
    return kb_run("lint", root=root)


def test_backlog_lists_unmarked_entries(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "human.md", RAW_DOC)
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    # mini fixture ships one unreviewed session-distill proposal too
    assert "human.md #1: First idea" in r.stdout
    assert "human.md #2: Second idea" in r.stdout
    assert "session-distill-2026-08-01.md #2" in r.stdout
    assert "session-distill-2026-08-01.md #1" not in r.stdout, \
        "the fixture's ingested block must not list"


def test_backlog_empty_is_clean(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("raw-mark", "session-distill-2026-08-01.md", "2",
           "--note", "kb-9001", root=root, expect_rc=0)
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    assert "0 unreviewed" in r.stdout
    assert "mark after ingest" not in r.stdout


def test_legacy_marker_in_preamble_is_orphan(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "old.md", LEGACY_ABOVE)
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    assert "old.md #1" in r.stdout, "preamble marker resolves no entry"
    assert "old.md #2" in r.stdout
    lint = _lint(root)
    assert "raw/old.md" in lint.stdout and "orphan" in lint.stdout


def test_legacy_marker_at_block_end_resolves(tmp_path):
    """The real distill-era convention: marker at the END of the block."""
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "end.md", LEGACY_ENDBLOCK)
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    assert "end.md #1" not in r.stdout, "end-of-block legacy marker resolves"
    assert "end.md #2" in r.stdout
    lint = _lint(root)
    assert "raw/end.md: 1 unreviewed" in lint.stdout, \
        "only entry #2 counts as backlog — #1 must not double-count"


def test_legacy_midblock_resolves_cleanly(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "mid.md", LEGACY_MIDBLOCK)
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    assert "mid.md #1" not in r.stdout, "mid-block legacy marker still resolves"
    lint = _lint(root)
    assert "raw/mid.md" not in lint.stdout


def test_judge_pair_blocks_exempt(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "conflicts-2026-10-13.md", JUDGE_DOC)
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    assert "conflicts-2026-10-13.md" not in r.stdout
    lint = _lint(root)
    assert "raw/conflicts-2026-10-13.md" not in lint.stdout


def test_raw_mark_inserts_state_line_and_commits(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    _write(root, "human.md", RAW_DOC)
    kb_run("raw-mark", "human.md", "1", "--note", "kb-9001",
           root=root, expect_rc=0)
    text = (root / "raw" / "human.md").read_text()
    lines = text.splitlines()
    h = lines.index("## First idea")
    assert lines[h + 1].startswith("> state: ingested kb-9001 ("), \
        "marker must be the FIRST line under the heading"
    assert "## Second idea" in text, "other entries untouched"
    r = kb_run("raw-backlog", root=root, expect_rc=0)
    assert "human.md #1" not in r.stdout and "human.md #2" in r.stdout
    log = kb_run("raw-mark", "human.md", "1", "--note", "kb-9002", root=root)
    assert log.returncode != 0 and "already marked" in (
        log.stdout + log.stderr)


def test_raw_mark_rejected_form(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "human.md", RAW_DOC)
    kb_run("raw-mark", "human.md", "2", "--rejected", "covered by kb-9001",
           root=root, expect_rc=0)
    assert "> state: rejected covered by kb-9001 (" in \
        (root / "raw" / "human.md").read_text()


def test_raw_mark_refuses_marked_and_bad_args(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "mid.md", LEGACY_MIDBLOCK)
    r = kb_run("raw-mark", "mid.md", "1", "--note", "kb-9001", root=root)
    assert r.returncode != 0 and "already marked" in (r.stdout + r.stderr), \
        "a legacy-resolved entry is already marked — refuse a second marker"
    r = kb_run("raw-mark", "../evil.md", "1", "--note", "kb-9001", root=root)
    assert r.returncode != 0
    r = kb_run("raw-mark", "mid.md", "9", "--note", "kb-9001", root=root)
    assert r.returncode != 0 and "no #9" in (r.stdout + r.stderr)
    r = kb_run("raw-mark", "mid.md", "1", "--note", "kb-9001", "--rejected",
               "x", root=root)
    assert r.returncode != 0, "exactly one of --note/--rejected"


def test_lint_warns_on_human_file_backlog(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _write(root, "human.md", RAW_DOC)
    lint = _lint(root)
    assert "raw/human.md" in lint.stdout and "unreviewed" in lint.stdout
    # the mini fixture's ONE open distill proposal stays under the threshold
    assert "raw/session-distill-2026-08-01.md" not in lint.stdout


def test_lint_distill_file_threshold(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    block = "\n## PROPOSAL\nbody\n"
    _write(root, "session-distill-2026-10-13.md",
           "# big distill file\n" + block * 7)  # 7 open > RAW_BACKLOG_WARN_MIN(5)
    lint = _lint(root)
    assert "raw/session-distill-2026-10-13.md" in lint.stdout
    assert "7 unreviewed proposals" in lint.stdout
