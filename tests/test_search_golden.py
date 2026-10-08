"""Golden-master search ranking against the frozen corpus (tests/fixtures/corpus-v1).

History: "settings.json" and "kb-0042" pinned the tokenchars-gluing and
own-id misses — both FLIPPED by P10a (dual tokenizer + own-id short-circuit).
"consoleblankk" pins the P10b typo layer: it must rank exactly like
"consoleblank" (unique edit-distance-1 correction, announced in output but
invisible to the id list).
Golden format: {query: [ids in rank order]}. Ids+rank only — never scores or
snippets (sqlite-version fragile). Regenerate: tests/tools/capture_goldens.py.
"""
from conftest import GOLDENS, golden_json_assert, kb_run, search_ids

QUERIES = [
    "grubby kernel cmdline",
    "zellij COPR",
    "clamshell thermal baseline",
    "consoleblank",
    "tty_tickets",
    "bin/kb",
    "e2e-data-ai-project",
    "FTS5 bm25 index",
    "ai-box.vn proxy model routing",
    "docker group root equivalent chroot",
    "how to stop laptop sleeping when lid closed",
    "yum package install policy",
    "zellij freeze herdr",
    "distill proposal promotion",
    "settings.json",
    "kb-0042",
    "consoleblankk",
    "zzq-nonexistent-token qwxvb",
]


def test_search_ranking_golden(frozen_sandbox):
    results = {}
    for q in QUERIES:
        r = kb_run("search", q, root=frozen_sandbox)
        results[q] = search_ids(r.stdout)
    # sanity: well-formed topical queries must actually hit (guard against a
    # golden silently recording all-empty results from a broken sandbox)
    assert results["grubby kernel cmdline"], "topical query returned nothing — sandbox broken?"
    golden_json_assert("search_ranking.json", results)


def test_search_total_miss_appends_miss_log(frozen_sandbox):
    r = kb_run("search", "zzq-nonexistent-token qwxvb", root=frozen_sandbox)
    assert not search_ids(r.stdout)
    log = frozen_sandbox / ".search-misses.log"
    assert log.exists(), "total miss did not create .search-misses.log"
    assert "zzq-nonexistent-token" in log.read_text()


def test_search_is_stable_across_runs(frozen_sandbox):
    a = kb_run("search", "zellij freeze herdr", root=frozen_sandbox)
    b = kb_run("search", "zellij freeze herdr", root=frozen_sandbox)
    assert search_ids(a.stdout) == search_ids(b.stdout)
