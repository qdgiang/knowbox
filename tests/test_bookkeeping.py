"""Bookkeeping (hardening plan P9): T4 id high-water, created-vs-log lint
forensics, T9 churn keyed by id with date-only bumps skipped, T10 reindex
no-op skip (with the dirty-staged-paths caveat from plan §0.4).

The mini fixture seeds log.md with create entries dated 2026-08-01 (matching
every MINI note's created:) — that is also the first-log-date boundary for
the pre-log-era exemption.
"""
import re

from conftest import build_sandbox, git_run, kb_run, note_path


def _lint_out(root):
    r = kb_run("lint", root=root)
    return r, (r.stdout + r.stderr).splitlines()


def _handmade_note(root, nid, slug, created):
    (root / "notes" / f"{nid}-{slug}.md").write_text(
        "---\n"
        f"id: {nid}\n"
        f'title: "Hand-made note {nid}"\n'
        "type: fact\ndomain: tooling\ntags: []\naliases: []\n"
        "triggers:\n  - \"testing the created-vs-log forensics\"\n"
        "status: active\nconfidence: high\n"
        f"created: {created}\nupdated: {created}\n"
        "supersedes: []\nrelated: []\nsource: \"session\"\n"
        'summary: "Fixture for P9 lint forensics."\n---\nBody.\n',
        encoding="utf-8")


# --------------------------------------------------------------------------
# T4: next_id high-water over filenames ∪ log create entries
# --------------------------------------------------------------------------

def test_next_id_high_water_baseline(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    r = kb_run("next-id", root=root, expect_rc=0)
    assert r.stdout.strip() == "kb-9013"


def test_new_after_delete_does_not_recycle(tmp_path):
    # full CLI cycle: create → delete the file by hand → create again.
    # The second create must NOT reuse the deleted id (2026-08-06 incident:
    # kb-0023/24/25 were re-issued to different content).
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    r = kb_run("new", "--type", "fact", "--domain", "test",
               "--title", "ephemeral note", "--body", "x\n",
               root=root, expect_rc=0)
    fn = r.stdout.strip().split("/")[-1]
    assert re.match(r"kb-9013", fn)
    (root / "notes" / fn).unlink()
    r2 = kb_run("new", "--type", "fact", "--domain", "test",
                "--title", "next note after deletion", "--body", "y\n",
                root=root, expect_rc=0)
    fn2 = r2.stdout.strip().split("/")[-1]
    assert fn2.startswith("kb-9014"), f"id recycled: got {fn2}"


# --------------------------------------------------------------------------
# created-vs-log lint forensics (WARN class — see cmd_lint comment for why
# not ERROR: prod's kb-0014 midnight-crossing would be a permanent ERROR)
# --------------------------------------------------------------------------

def test_lint_warns_created_vs_log_mismatch(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9001")
    # earlier date keeps `updated < created` ERROR out of the picture
    p.write_text(p.read_text().replace("created: 2026-08-01",
                                       "created: 2026-07-15", 1))
    _r, lines = _lint_out(root)
    hits = [l for l in lines if "matches no log.md create date" in l]
    assert hits and hits[0].startswith("WARN"), hits
    assert "kb-9001" in hits[0]


def test_lint_warns_duplicate_create_entries(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    with (root / "log.md").open("a", encoding="utf-8") as f:
        f.write("## [2026-08-01] create | kb-9001 | second birth\n")
    _r, lines = _lint_out(root)
    hits = [l for l in lines if "create entries" in l]
    assert hits and hits[0].startswith("WARN") and \
        "2 create entries" in hits[0], hits


def test_lint_warns_no_create_entry_after_journal_start(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _handmade_note(root, "kb-9201", "hand-made", "2026-09-01")
    _r, lines = _lint_out(root)
    hits = [l for l in lines if "no create entry" in l]
    assert hits and any("kb-9201" in l for l in hits), hits


def test_lint_prelog_era_note_exempt(tmp_path):
    # created BEFORE the first log entry date → the journal cannot know it;
    # silent (this exemption covers prod's kb-0002..0009 early batch)
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _handmade_note(root, "kb-9202", "ancient", "2026-07-01")
    _r, lines = _lint_out(root)
    hits = [l for l in lines if "no create entry" in l]
    assert not any("kb-9202" in l for l in hits), hits


# --------------------------------------------------------------------------
# T9: churn keyed by id, date-only bumps skipped
# --------------------------------------------------------------------------

def test_churn_skips_date_only_and_aggregates_renames(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    # 1) CLI touch → its commit's note diff is ONLY the updated: line → skip
    kb_run("touch", "kb-9001", root=root, expect_rc=0)
    # 2) real content edit → counts
    p1 = note_path(root, "kb-9001")
    p1.write_text(p1.read_text() + "\nA real content change.\n")
    git_run(root, "add", "-A")
    git_run(root, "commit", "-q", "-m", "content edit kb-9001")
    # 3) pure rename of kb-9002 → no content lines → skip
    p2 = note_path(root, "kb-9002")
    git_run(root, "mv", str(p2), str(p2.parent / "kb-9002-renamed-slug.md"))
    git_run(root, "commit", "-q", "-m", "rename kb-9002")
    # 4) content edit of the RENAMED file → counts under id kb-9002
    p2n = note_path(root, "kb-9002")
    p2n.write_text(p2n.read_text() + "\nRenamed and edited.\n")
    git_run(root, "add", "-A")
    git_run(root, "commit", "-q", "-m", "content edit kb-9002 after rename")

    r = kb_run("churn", "--days", "30", root=root, expect_rc=0)
    lines = r.stdout.splitlines()
    row9001 = next(l for l in lines if "kb-9001" in l)
    row9002 = next(l for l in lines if "kb-9002" in l)
    # baseline is 1 for the fixture-init commit (a real creation edit); the
    # date-only touch and the pure rename must NOT add to it — a regression
    # would show 3, not 2
    assert row9001.split()[0] == "2", f"touch must be skipped: {row9001}"
    assert row9002.split()[0] == "2", f"rename must not count: {row9002}"
    assert "kb-9002-renamed-slug" not in r.stdout, "rows must key by id"
    # commits with content edits: init + the two manual edits (touch and
    # rename commits excluded)
    assert "across 3 commit(s)" in r.stdout, r.stdout


def test_churn_verify_status_change_counts(tmp_path):
    # verify flips status: too — NOT a date-only bump; it is a real state
    # change and must count (the skip rule covers updated:/verified: only)
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    kb_run("verify", "kb-9003", "--evidence", "churn probe",
           root=root, expect_rc=0)
    r = kb_run("churn", "--days", "30", root=root, expect_rc=0)
    row = next(l for l in r.stdout.splitlines() if "kb-9003" in l)
    # fixture-init creation + the verify commit = 2
    assert row.split()[0] == "2", row


# --------------------------------------------------------------------------
# T10: reindex no-op skip (+ §0.4 dirty-staged-paths caveat)
# --------------------------------------------------------------------------

def test_reindex_noop_skips_log_and_commit(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    kb_run("reindex", root=root, expect_rc=0)      # first run ships
    c1 = git_run(root, "rev-list", "--count", "HEAD").strip()
    log1 = (root / "log.md").read_text()
    r = kb_run("reindex", root=root, expect_rc=0)
    assert "no changes" in r.stdout
    assert git_run(root, "rev-list", "--count", "HEAD").strip() == c1, \
        "no-op reindex must not commit"
    assert (root / "log.md").read_text() == log1, \
        "no-op reindex must not append a log line"


def test_reindex_dirty_schema_still_commits(tmp_path):
    # plan §0.4 (kb-0142 shape): a dirty STAGED path must ride the reindex
    # commit even when the regenerated index is identical — the CLI itself no
    # longer lives in the data repo, so SCHEMA.md stands in for bin/kb
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    kb_run("reindex", root=root, expect_rc=0)
    sp = root / "SCHEMA.md"
    sp.write_text(sp.read_text() + "\n<!-- ship me -->\n")
    r = kb_run("reindex", root=root, expect_rc=0)
    assert "no changes" not in r.stdout
    assert "<!-- ship me -->" in git_run(root, "show", "HEAD:SCHEMA.md"), \
        "dirty staged SCHEMA.md did not ride the reindex commit"
