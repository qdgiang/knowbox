"""Golden-master index build + lint report against the frozen corpus.

index_build.txt pins build_index output byte-for-byte (P4/P11 change the
⚠SUPERSEDED flag rendering → intentional golden diff then).
lint_report.json pins every WARN/ERROR message line (relative paths, so it is
sandbox-independent) plus the summary line.
"""
import re

from conftest import golden_assert, golden_json_assert, kb_run


def test_index_build_golden(frozen_sandbox):
    text = (frozen_sandbox / "index.md").read_text()
    # normalize a generation timestamp if build_index ever adds one
    text = re.sub(r"\d{4}-\d{2}-\d{2}T[\d:]+Z?", "<ts>", text)
    golden_assert("index_build.txt", text)


def test_lint_report_golden(frozen_sandbox):
    r = kb_run("lint", root=frozen_sandbox)
    # corpus-v1 (frozen pre-P5): 3 oversized-note errors + 24 verified-without-
    # date errors (the P5 flip) — prod itself is clear of the latter.
    assert r.returncode == 1, "corpus-v1 has known lint errors"
    out = r.stdout + r.stderr
    errors = [l.strip() for l in out.splitlines() if l.startswith("ERROR")]
    warnings = [l.strip() for l in out.splitlines() if l.startswith("WARN")]
    summary = [l.strip() for l in out.splitlines()
               if "across" in l and "notes" in l]
    golden_json_assert("lint_report.json",
                       {"errors": errors, "warnings": warnings,
                        "summary": summary})
