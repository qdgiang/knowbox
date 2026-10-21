"""Edited-since-verification surfacing: `knowbox show` stderr flag +
`knowbox stale` EDITED-SINCE-VERIFY flag.

Reading (SCHEMA.md): `updated > verified` means the note changed since it
was last confirmed — the "was something else before" signal. Never-verified
notes carry no flag (their status: active/draft already says it); a verified
stamp newer than updated means the edit WAS the verification.
"""
from conftest import build_sandbox, kb_run

_FM = """---
id: {nid}
title: "{title}"
type: fact
domain: tooling
tags: []
aliases: []
triggers:
  - "when testing the edited-since-verify flag"
status: verified
confidence: high
created: 2026-08-01
updated: {updated}
verified: {verified}
supersedes: []
related: []
source: "session"
summary: "{title}, routed."
---
Body.
"""


def _write(root, nid, title, updated, verified):
    slug = f"{nid}-{title.lower().replace(' ', '-')}"
    (root / "notes" / f"{slug}.md").write_text(
        _FM.format(nid=nid, title=title, updated=updated, verified=verified))


def test_show_flags_edited_since_verify(tmp_path):
    root = tmp_path / "kb"
    build_sandbox(root)
    _write(root, "kb-9151", "Fact corrected after verification",
           updated="2026-09-01", verified="2026-08-05")
    r = kb_run("show", "kb-9151", root=root)
    assert "edited after last verification" in r.stderr
    assert "updated 2026-09-01 > verified 2026-08-05" in r.stderr
    assert r.stdout.startswith("---"), "stdout stays the raw note"


def test_show_silent_when_verify_is_current(tmp_path):
    root = tmp_path / "kb"
    build_sandbox(root)
    _write(root, "kb-9152", "Verified and untouched since",
           updated="2026-08-01", verified="2026-08-05")
    r = kb_run("show", "kb-9152", root=root)
    assert "edited after last verification" not in r.stderr


def test_show_silent_when_never_verified(tmp_path):
    # mini corpus kb-9001: status active, no verified: — no flag (status
    # already carries the not-yet-confirmed signal; flagging every such
    # note would be noise)
    root = tmp_path / "kb"
    build_sandbox(root)
    r = kb_run("show", "kb-9001", root=root)
    assert "edited after last verification" not in r.stderr


def test_stale_flags_edited_since_verify(tmp_path):
    root = tmp_path / "kb"
    build_sandbox(root)
    _write(root, "kb-9151", "Fact corrected after verification",
           updated="2026-09-01", verified="2026-08-05")
    _write(root, "kb-9152", "Verified and untouched since",
           updated="2026-08-01", verified="2026-08-05")
    r = kb_run("stale", root=root)
    line = next(l for l in r.stdout.splitlines() if "kb-9151" in l)
    assert "EDITED-SINCE-VERIFY" in line
    clean = next(l for l in r.stdout.splitlines() if "kb-9152" in l)
    assert "EDITED-SINCE-VERIFY" not in clean
