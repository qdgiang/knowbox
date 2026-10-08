"""Janitor e2e + lint heartbeat checks (hardening plan P2; knowbox split kb-0142).

The janitor (~/dotfiles/scripts/kb-janitor.sh since the split) is tested
end-to-end against a mini sandbox: real bash, the CLI under test via a
knowbox_shim as KNOWBOX_BIN, a TG_BIN shim that records alert calls to a
file. No network, no Telegram, no cron.
"""
import os
import subprocess
import time

import pytest

from conftest import DOTFILES_JANITOR, build_sandbox, kb_run, knowbox_shim

JANITOR = DOTFILES_JANITOR
_SKIP = pytest.mark.skipif(not JANITOR.exists(),
                           reason="kb-janitor.sh not present")


@_SKIP
def test_janitor_bash_syntax():
    r = subprocess.run(["bash", "-n", str(JANITOR)],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr


def test_dotfiles_scripts_bash_syntax():
    scripts = JANITOR.parent.glob("*.sh")
    checked = 0
    for s in scripts:
        r = subprocess.run(["bash", "-n", str(s)],
                           capture_output=True, text=True)
        assert r.returncode == 0, f"{s}: {r.stderr}"
        checked += 1
    # dotfiles may be absent on other boxes — checked==0 is fine there


def _tg_shim(dirpath):
    log = dirpath / "tg-calls.log"
    shim = dirpath / "tg-send-shim.sh"
    shim.write_text(f'#!/usr/bin/env bash\necho "$@" >> "{log}"\n')
    shim.chmod(0o755)
    return shim, log


def _run_janitor(root, tmp_path, knowbox_bin=None, env_over=None):
    shim, tglog = _tg_shim(tmp_path)
    kb = knowbox_bin or str(knowbox_shim(tmp_path / "shimbin"))
    env = {**os.environ, "KB_DIR": str(root), "KNOWBOX_BIN": kb,
           "TG_BIN": str(shim), "REPORTS_DIR": str(tmp_path / "reports")}
    env.update(env_over or {})
    r = subprocess.run(["bash", str(JANITOR)],
                       capture_output=True, text=True, env=env, timeout=300)
    return r, tglog


@_SKIP
def test_janitor_happy_path_no_alert(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    # make lint 0E/0W — the baseline the alerts are keyed to (kb-0102):
    # kb-9011 is the deliberate oversized ERROR, kb-9010 the deliberate
    # empty-source WARN (both stay in the fixture for the lint tests)
    (root / "notes" / "kb-9011-oversized-note.md").unlink()
    (root / "notes" / "kb-9010-empty-source.md").unlink()
    kb_run("reindex", root=root, expect_rc=0)   # P8: no DB = stale-ERROR; prod
    # always has one, so build it — this test is about clean CONTENT lint
    hb = root / ".kb-heartbeat"          # seeded fresh by build_sandbox
    mtime_before = hb.stat().st_mtime
    r, tglog = _run_janitor(root, tmp_path)
    assert r.returncode == 0, r.stdout + r.stderr
    assert hb.stat().st_mtime >= mtime_before, "run must refresh the heartbeat"
    assert not tglog.exists(), "clean lint (0E/0W) must not send an alert"
    # log block format preserved (the old cron line's shape)
    assert r.stdout.startswith("== ") and "knowbox lint exit=0" in r.stdout
    assert "--- knowbox stale ---" in r.stdout
    assert "--- knowbox churn ---" in r.stdout
    assert "unreviewed proposals: 1 (of 2 total)" in r.stdout


@_SKIP
def test_janitor_warnings_only_alert(tmp_path):
    # Zero-warning baseline flip (2026-09-08): with 0 errors but ≥1 warning
    # the janitor must still ping — any finding is actionable signal.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "notes" / "kb-9011-oversized-note.md").unlink()   # drop the ERROR
    # kb-9010 (empty source:) stays → exactly 1 warning, 0 errors
    kb_run("reindex", root=root, expect_rc=0)
    r, tglog = _run_janitor(root, tmp_path)
    assert r.returncode == 0, "warnings are not janitor failure"
    assert (root / ".kb-heartbeat").exists(), "lint ran → heartbeat"
    assert tglog.exists(), "warnings-only must alert on a 0W baseline"
    alert = tglog.read_text()
    assert "0 error(s), 1 warning(s)" in alert and ".lint-cron.log" in alert


@_SKIP
def test_janitor_seed_run_missing_heartbeat_alerts_then_creates(tmp_path):
    # the prod seed-run scenario: heartbeat absent → lint ERRORs on it →
    # alert fires AND the successful run creates the heartbeat (exit 0).
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "notes" / "kb-9011-oversized-note.md").unlink()
    kb_run("reindex", root=root, expect_rc=0)   # P8: derive DB+index first,
    # so the missing heartbeat stays the ONE error this test alerts on
    hb = root / ".kb-heartbeat"
    hb.unlink()
    r, tglog = _run_janitor(root, tmp_path)
    assert r.returncode == 0
    assert hb.exists(), "seed run must create the heartbeat"
    assert tglog.exists() and "1 error(s)" in tglog.read_text(), \
        "missing heartbeat is itself the 1 error the seed run alerts on"


@_SKIP
def test_janitor_lint_errors_alert_but_heartbeat(tmp_path):
    # mini_kb ships with kb-9011 oversized → exactly 1 lint ERROR (P8: reindex
    # first so the missing-DB stale-ERROR doesn't inflate the count)
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    r, tglog = _run_janitor(root, tmp_path)
    assert r.returncode == 0, "content errors are not janitor failure"
    assert (root / ".kb-heartbeat").exists(), \
        "lint ran to completion → heartbeat even with errors"
    assert tglog.exists(), "lint errors must alert"
    alert = tglog.read_text()
    assert "1 error(s)" in alert and ".lint-cron.log" in alert


@_SKIP
def test_janitor_tg_delivery_failure_witnessed_not_fatal(tmp_path):
    # Audit S1 (2026-09-11): a dead alert channel must not fail the run, and
    # must leave an ALERT-DELIVERY-FAILED witness in stdout (.lint-cron.log) —
    # healthy runs send nothing, so silence alone is ambiguous.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)   # mini keeps kb-9011 → 1 ERROR
    r, tglog = _run_janitor(root, tmp_path, env_over={"TG_BIN": "/bin/false"})
    assert r.returncode == 0, "delivery failure never fails the run"
    assert (root / ".kb-heartbeat").exists(), "lint ran → heartbeat regardless"
    assert "ALERT-DELIVERY-FAILED" in r.stdout


@_SKIP
def test_janitor_failure_no_heartbeat_loud_alert(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    hb = root / ".kb-heartbeat"
    hb.unlink()
    r, tglog = _run_janitor(root, tmp_path, knowbox_bin="/bin/false")
    assert r.returncode == 1, "janitor must fail when lint cannot run"
    assert not hb.exists(), "failed run must NOT touch the heartbeat"
    assert tglog.exists() and "FAILED" in tglog.read_text()


@_SKIP
def test_janitor_distill_backlog_alert_threshold(tmp_path):
    # Backlog alert (2026-09-29): unreviewed proposals > BACKLOG_MAX must ping;
    # mini corpus ships 1 unreviewed of 2 → BACKLOG_MAX=0 trips, default 30 doesn't.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "notes" / "kb-9011-oversized-note.md").unlink()
    kb_run("reindex", root=root, expect_rc=0)
    r, tglog = _run_janitor(root, tmp_path, env_over={"TG_BIN": "/bin/true"})
    assert r.returncode == 0 and "unreviewed proposals: 1" in r.stdout

    shim, tglog = _tg_shim(tmp_path)
    r, _ = _run_janitor(root, tmp_path,
                        env_over={"TG_BIN": str(shim), "BACKLOG_MAX": "0"})
    assert r.returncode == 0, r.stdout + r.stderr
    assert tglog.exists() and "distill backlog 1 unreviewed proposals (> 0)" in tglog.read_text()


@_SKIP
def test_janitor_reports_sweep_archives_old_done_handoffs(tmp_path):
    # Reports sweep (2026-09-27): done handoffs >30d old → archive/;
    # active handoffs and fresh done ones are left alone.
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / "notes" / "kb-9011-oversized-note.md").unlink()
    kb_run("reindex", root=root, expect_rc=0)
    reports = tmp_path / "reports"
    (reports / "handoffs").mkdir(parents=True)
    done = reports / "handoffs" / "2026-01-01-old-done.md"
    done.write_text("---\nstatus: done\n---\n# old done\n")
    active = reports / "handoffs" / "2026-01-01-old-active.md"
    active.write_text("---\nstatus: active\n---\n# old active\n")
    fresh = reports / "handoffs" / "2026-09-20-fresh-done.md"
    fresh.write_text("---\nstatus: done\n---\n# fresh done\n")
    old = time.time() - 40 * 86400
    for f in (done, active):
        os.utime(f, (old, old))
    r, tglog = _run_janitor(root, tmp_path,
                            env_over={"TG_BIN": "/bin/true",
                                      "REPORTS_DIR": str(reports)})
    assert r.returncode == 0, r.stdout + r.stderr
    assert "archived done handoffs (>30d): 1" in r.stdout
    assert (reports / "handoffs" / "archive" / done.name).exists()
    assert active.exists(), "active handoffs are never archived"
    assert fresh.exists(), "done handoffs <30d stay in place"


def test_lint_heartbeat_fresh_is_silent(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    r = kb_run("lint", root=root)
    assert "heartbeat" not in (r.stdout + r.stderr)


def test_lint_heartbeat_missing_errors(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    (root / ".kb-heartbeat").unlink()
    r = kb_run("lint", root=root)
    assert "heartbeat missing" in (r.stdout + r.stderr)


def test_lint_heartbeat_stale_errors(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    old = time.time() - 9 * 86400
    os.utime(root / ".kb-heartbeat", (old, old))
    r = kb_run("lint", root=root)
    assert "heartbeat stale" in (r.stdout + r.stderr)
