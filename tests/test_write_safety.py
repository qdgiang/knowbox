r"""Write-path safety (hardening plan P4).

Covers:
- T5 fix: touch/verify only ever rewrite the frontmatter slice — a note body
  containing `updated:`/`status:` lines inside code fences stays byte-identical
  (kb-9008), and a note whose fm lacks `updated:` is refused, not regexed.
- verify inserts `verified:` after `updated:` INSIDE the fm only.
- supersede on a CRLF+BOM note (kb-9012 as OLD): banner present, BOM and CRLF
  preserved byte-wise, `status: superseded` in the fm (the old `\A(---\n…)`
  regex silently no-oped the banner on such files).
- T7 refuse-to-publish: an unterminated-frontmatter note (kb-9102) is named by
  lint ERRORs, excluded from index.md and search by reindex, still printable
  via `knowbox show`, and refused by every writer.
- BOM note now parses: kb-9012 appears in index and search after reindex.

Coverage gap (documented, NOT faked): the interrupted-supersede rollback
(second write fails → OLD restored from memory) cannot be fault-injected
honestly at subprocess level — both notes live in the same directory, so no
reachable state makes write #1 succeed and write #2 fail; every failure mode
reachable through find_path (directory-as-note, unreadable file, missing fm
keys, read-only notes/) trips BEFORE or AT the first write. Those paths ARE
tested below (test_supersede_refuses_*, test_supersede_first_write_failure_*).
The rollback branch itself is 4 lines of restore-from-memory, covered by code
review only.
"""
import datetime

from conftest import build_sandbox, kb_run, note_path, search_ids

TODAY = datetime.date.today().isoformat()


def _split_fm_body(raw: bytes):
    """Split note bytes at the closing frontmatter delimiter (LF-based
    fixtures only here; CRLF notes are asserted byte-wise in their test)."""
    fm, body = raw.split(b"\n---\n", 1)
    return fm.decode("utf-8"), body


# --------------------------------------------------------------------------
# T5: frontmatter-only rewriting
# --------------------------------------------------------------------------

def test_touch_updates_fm_only_body_bytes_identical(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9008")           # body has updated:/status: lines
    body_before = p.read_bytes().split(b"\n---\n", 1)[1]
    kb_run("touch", "kb-9008", root=root, expect_rc=0)
    raw = p.read_bytes()
    fm, body_after = _split_fm_body(raw)
    assert body_after == body_before, \
        "touch modified bytes below the frontmatter"
    assert f"updated: {TODAY}" in fm
    assert b"updated: 2020-01-01" in body_after   # fence line untouched
    assert b"status: draft" in body_after


def test_verify_updates_fm_only_and_inserts_verified_after_updated(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9008")           # body has updated:/status: lines
    body_before = p.read_bytes().split(b"\n---\n", 1)[1]
    kb_run("verify", "kb-9008", "--evidence", "P4 probe", root=root,
           expect_rc=0)
    raw = p.read_bytes()
    fm, body_after = _split_fm_body(raw)
    assert body_after == body_before, \
        "verify modified bytes below the frontmatter"
    lines = fm.split("\n")
    assert f"updated: {TODAY}" in lines
    assert "status: verified" in lines
    assert f"verified: {TODAY}" in lines
    assert lines.index(f"verified: {TODAY}") == \
        lines.index(f"updated: {TODAY}") + 1, \
        "verified: must be inserted right after updated: inside the fm"
    assert b"updated: 2020-01-01" in body_after
    assert b"status: draft" in body_after


def test_verify_reverify_updates_existing_verified_in_place(tmp_path):
    # kb-9002 already has `verified: 2026-08-05` → replaced in the fm,
    # no second verified: line appears.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9002")
    kb_run("verify", "kb-9002", "--evidence", "re-probe", root=root,
           expect_rc=0)
    fm, _body = _split_fm_body(p.read_bytes())
    assert fm.count("verified:") == 1
    assert f"verified: {TODAY}" in fm


def test_verify_still_skips_superseded(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9004")
    before = p.read_bytes()
    r = kb_run("verify", "kb-9004", root=root, expect_rc=0)
    assert "skipping" in r.stdout and "superseded" in r.stdout
    assert p.read_bytes() == before


# --------------------------------------------------------------------------
# supersede: CRLF+BOM preservation, banner anchoring, two-phase refusals
# --------------------------------------------------------------------------

def test_supersede_crlf_bom_old_note_byte_faithful(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    old = note_path(root, "kb-9012")         # CRLF + BOM fixture
    new = note_path(root, "kb-9001")
    kb_run("supersede", "kb-9012", "--with", "kb-9001", root=root,
           expect_rc=0)
    raw = old.read_bytes()
    assert raw.startswith(b"\xef\xbb\xbf"), "BOM lost by supersede"
    assert raw.count(b"\n") == raw.count(b"\r\n"), \
        "line endings changed: every \\n must belong to a \\r\\n"
    assert b"\r\n\r\n> **Superseded by [[kb-9001]]" in raw, \
        "banner missing (must anchor at the parsed fm end, CRLF-safe)"
    assert b"status: superseded\r\n" in raw
    assert f"updated: {TODAY}".encode() in raw
    nraw = new.read_bytes()
    assert b"supersedes: [kb-9012]" in nraw
    assert f"updated: {TODAY}".encode() in nraw


def test_supersede_refuses_before_any_write_when_new_lacks_updated(tmp_path):
    # kb-9101's fm has no `updated:` → the NEW-side rewrite refuses; because
    # both new contents are computed BEFORE the first write, OLD is untouched.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("no_updated",))
    old = note_path(root, "kb-9001")
    new = note_path(root, "kb-9101")
    ob, nb = old.read_bytes(), new.read_bytes()
    r = kb_run("supersede", "kb-9001", "--with", "kb-9101", root=root)
    assert r.returncode != 0
    assert old.read_bytes() == ob, \
        "OLD must be untouched when the NEW-side rewrite refuses"
    assert new.read_bytes() == nb


def test_supersede_refuses_malformed_old_before_any_write(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("unterminated",))
    bad = note_path(root, "kb-9102")
    other = note_path(root, "kb-9001")
    bb, ob = bad.read_bytes(), other.read_bytes()
    r = kb_run("supersede", "kb-9102", "--with", "kb-9001", root=root)
    assert r.returncode != 0
    assert "unterminated frontmatter" in (r.stdout + r.stderr)
    assert bad.read_bytes() == bb and other.read_bytes() == ob


def test_supersede_first_write_failure_leaves_both_untouched(tmp_path):
    # Honest fault injection: a read-only notes/ dir makes the FIRST
    # atomic write (mkstemp) fail → clean non-zero exit, both notes
    # byte-identical ("nothing changed" branch).
    # The SECOND-write-failure → restore-OLD-from-memory rollback branch is
    # NOT reachable from a subprocess-level test: both notes live in the
    # same directory, so no permission/state makes write #1 succeed and
    # write #2 fail without a mid-run race; injecting it would require a
    # fault-injection hook inside the CLI (deliberately not added). That
    # branch stays covered by code review only — documented, not faked.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    old = note_path(root, "kb-9003")
    new = note_path(root, "kb-9001")
    ob, nb = old.read_bytes(), new.read_bytes()
    notes_dir = root / "notes"
    notes_dir.chmod(0o555)
    try:
        r = kb_run("supersede", "kb-9003", "--with", "kb-9001", root=root)
    finally:
        notes_dir.chmod(0o755)
    assert r.returncode != 0
    assert "nothing changed" in (r.stdout + r.stderr)
    assert old.read_bytes() == ob and new.read_bytes() == nb


# --------------------------------------------------------------------------
# T7: unterminated frontmatter — refuse-to-publish, stay readable
# --------------------------------------------------------------------------

def test_lint_names_unterminated_frontmatter(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("unterminated",))
    r = kb_run("lint", root=root)
    out = r.stdout + r.stderr
    assert r.returncode == 1
    err_lines = [l for l in out.splitlines() if l.startswith("ERROR")]
    assert any("kb-9102" in l and "unterminated frontmatter" in l
               for l in err_lines), err_lines
    # redundant count ERROR: 13 files on disk, 12 parsed
    assert any("1 of 13" in l for l in err_lines), err_lines


def test_reindex_excludes_unterminated_from_index_and_search(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("unterminated",))
    kb_run("reindex", root=root, expect_rc=0)
    idx = (root / "index.md").read_text()
    assert "kb-9102" not in idx
    assert "UNTERMINATEDZORK" not in idx
    r = kb_run("search", "UNTERMINATEDZORK", root=root)
    assert not search_ids(r.stdout), "malformed note leaked into search"
    assert "no matches" in r.stdout


def test_show_still_prints_unterminated_note(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("unterminated",))
    r = kb_run("show", "kb-9102", root=root, expect_rc=0)
    assert "UNTERMINATEDZORK" in r.stdout
    assert "Unterminated frontmatter fixture" in r.stdout


def test_writers_refuse_unterminated_frontmatter(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("unterminated",))
    p = note_path(root, "kb-9102")
    before = p.read_bytes()
    r = kb_run("touch", "kb-9102", root=root)
    assert r.returncode != 0
    assert p.read_bytes() == before
    r = kb_run("verify", "kb-9102", root=root, expect_rc=0)
    assert "skipping" in r.stdout           # verify skips (never refuses hard)
    assert p.read_bytes() == before


# --------------------------------------------------------------------------
# BOM note now parses → visible in index + search
# --------------------------------------------------------------------------

def test_bom_note_parses_into_index_and_search(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    assert "kb-9012" in (root / "index.md").read_text(), \
        "BOM note missing from index — parse_note must tolerate the BOM"
    ids = search_ids(kb_run("search", "CRLF BOM", root=root).stdout)
    assert "kb-9012" in ids


# --------------------------------------------------------------------------
# P5 flip: verified-without-date is an ERROR once the backlog is zero
# --------------------------------------------------------------------------

def test_lint_errors_on_verified_without_date(tmp_path):
    # Was a WARN while the 24-note prod backlog existed (P4); P5 cleared the
    # backlog and flipped it — an undated verified claim is now an error.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9002")           # ships with verified: 2026-08-05
    p.write_text(p.read_text().replace("verified: 2026-08-05\n", ""))
    r = kb_run("lint", root=root)
    out = r.stdout + r.stderr
    assert r.returncode == 1
    assert any(l.startswith("ERROR") and "kb-9002" in l
               and "no verified: date" in l for l in out.splitlines()), out
