"""Search quality (hardening plan P10).

10a (this file today): own-id short-circuit + dual tokenizer.
  - `kb search kb-XXXX` returns THAT note as the single result (superseded
    included, labeled) before any FTS runs — the id column is UNINDEXED, so
    ids never matched lexically, and notes merely MENTIONING an id used to
    outrank the note itself.
  - kb_d: same columns as kb but default unicode61 (no tokenchars) —
    `~/.claude/settings.json` indexes as claude/settings/json, so querying a
    path part finds the compound. Both tables are queried; results merge by
    (best rank, kb-first tie-break), deduplicated by path.
10b (this file, second section): typo expansion on zero-hit + refs-grep
fallback. Chain order on a lexical miss: typo → semantic → refs-grep →
miss-log. Typo correction is CONSERVATIVE: only a UNIQUE edit-distance-1
vocabulary candidate is substituted; ambiguous tokens (e.g. 'gruby' → both
'grub' and 'grubby') are left alone for an honest miss.

The frozen-corpus goldens pin the two former pinned misses ("settings.json",
"kb-0042" — flipped by 10a) and the typo layer ("consoleblankk" must rank
exactly like "consoleblank").
"""
from conftest import build_sandbox, kb_run, note_path, search_ids


# --------------------------------------------------------------------------
# own-id short-circuit
# --------------------------------------------------------------------------

def test_own_id_lookup_returns_only_that_note(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "kb-9001", root=root, expect_rc=0)
    assert search_ids(r.stdout) == ["kb-9001"], r.stdout


def test_own_id_lookup_returns_superseded_with_label(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "kb-9004", root=root, expect_rc=0)
    assert search_ids(r.stdout) == ["kb-9004"], r.stdout
    assert "SUPERSEDED" in r.stdout


def test_own_id_lookup_malformed_note_labeled_not_crash(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False,
                         extra_notes=("unterminated",))
    r = kb_run("search", "kb-9102", root=root, expect_rc=0)
    assert "kb-9102" in r.stdout and "MALFORMED" in r.stdout


def test_unknown_id_falls_through_to_body_mentions(tmp_path):
    # kb-9999 has no file — the query degrades to normal FTS, which finds
    # the note whose BODY mentions the id
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9001")
    p.write_text(p.read_text() + "\nSee also [[kb-9999]] for context.\n")
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "kb-9999", root=root, expect_rc=0)
    assert "kb-9001" in search_ids(r.stdout), r.stdout


# --------------------------------------------------------------------------
# dual tokenizer
# --------------------------------------------------------------------------

def test_path_part_query_finds_compound(tmp_path):
    # the tokenchars-gluing miss in miniature: with ONLY the glued tokenizer,
    # `~/.claude/settings.json` is one token and the query "settings.json"
    # cannot prefix-match it; kb_d splits the compound and finds it
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9001")
    p.write_text(p.read_text() +
                 "\nConfig lives in ~/.claude/settings.json on this box.\n")
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "settings.json", root=root, expect_rc=0)
    assert "kb-9001" in search_ids(r.stdout), r.stdout


def test_glued_token_query_still_works(tmp_path):
    # the kb table's tokenchars must keep whole-compound queries exact:
    # "bin/kb" still hits a note containing that path
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9002")
    p.write_text(p.read_text() + "\nThe CLI lives at ~/kb/bin/kb.\n")
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "bin/kb", root=root, expect_rc=0)
    assert "kb-9002" in search_ids(r.stdout), r.stdout


def test_dual_merge_has_no_duplicate_rows(tmp_path):
    # both tokenizers answer "grubby kernel" — each id must appear ONCE
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "grubby kernel", root=root, expect_rc=0)
    ids = search_ids(r.stdout)
    assert ids and len(ids) == len(set(ids)), ids


def test_pre_p10_db_without_kb_d_degrades_to_kb_only(tmp_path):
    # graceful degradation: drop kb_d from a built db — searches must still
    # work off the primary table (the exact state a prod db is in between
    # shipping the CLI and the next reindex)
    import sqlite3
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    c = sqlite3.connect(root / ".kb.db")
    c.execute("DROP TABLE kb_d")
    c.commit()
    c.close()
    r = kb_run("search", "grubby kernel", root=root, expect_rc=0)
    assert "kb-9001" in search_ids(r.stdout), r.stdout
    assert "FTS error" not in r.stderr


def test_search_deterministic_byte_identical(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    a = kb_run("search", "widget configuration", root=root, expect_rc=0)
    b = kb_run("search", "widget configuration", root=root, expect_rc=0)
    assert a.stdout == b.stdout


# --------------------------------------------------------------------------
# 10b: typo expansion
# --------------------------------------------------------------------------

def test_typo_corrected_when_candidate_unique(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    # "widgett" → "widget" by deletion — the UNIQUE distance-1 vocab hit
    r = kb_run("search", "widgett", root=root, expect_rc=0)
    assert "typo-corrected" in r.stdout and "'widgett'→'widget'" in r.stdout
    ids = search_ids(r.stdout)
    assert "kb-9005" in ids and "kb-9003" in ids, ids
    assert "kb-9004" not in ids, "superseded note leaked into corrected search"


def test_typo_ambiguous_candidate_not_corrected(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    # "gruby" has TWO distance-1 vocab neighbors in mini ("grub" tag and
    # "grubby") → conservative: no correction, honest miss path instead
    r = kb_run("search", "gruby", root=root, expect_rc=0)
    assert "typo-corrected" not in r.stdout
    assert not search_ids(r.stdout)


# --------------------------------------------------------------------------
# 10b: refs-grep fallback + miss-log discipline
# --------------------------------------------------------------------------

def test_refs_grep_points_at_unindexed_doc(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    # refs/mini-guide.md mentions MotherDuck but refs/ is outside FTS —
    # a total lexical+typo+semantic miss must still point at the doc
    r = kb_run("search", "motherduck", root=root, expect_rc=0)
    assert "refs/ mentions" in r.stdout
    assert "refs/mini-guide.md" in r.stdout
    # a refs hit is NOT a total miss: the miss-log stays untouched
    assert not (root / ".search-misses.log").exists()


def test_total_miss_still_logs_after_all_layers(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    r = kb_run("search", "qwxvb zzz nonexistent", root=root, expect_rc=0)
    assert "no matches" in r.stdout
    log = root / ".search-misses.log"
    assert log.exists() and "qwxvb" in log.read_text()
