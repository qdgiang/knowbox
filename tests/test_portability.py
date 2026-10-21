r"""Windows portability Phase 1 (kb-0117 audit, 2026-09-22).

Two layers, because native-Windows branches (msvcrt, stdio reconfigure,
notepad default) CANNOT execute on this Linux box — the same honest-gap
posture as test_write_safety's rollback branch:

1. SOURCE FENCE — regression fences on the CLI source so a future edit can't
   silently reintroduce a Unix-only pattern (top-level fcntl, a text writer
   without newline=, direct os.replace, hand-built file: URIs). Reading the
   source is not importing it — house rule intact.
2. BEHAVIOR — subprocess probes for the new platform-neutral helpers
   (_replace_retry retry/reraise semantics, _lock_acquire contention on the
   POSIX branch), plus end-to-end LF writes, the F17 heartbeat env gate, the
   B5 non-git reindex path, and the launcher/attributes artifacts.

Windows-only branches covered by fence + code review ONLY (documented gap):
msvcrt locking, stdio reconfigure, notepad/shlex posix=False, normcase.
"""
import json
import os
import subprocess
from pathlib import Path

from conftest import CLI_SRC, REPO, UV, build_sandbox, kb_run, note_path

# Post-split (paths.py + sysutil.py, refactor step 1): fences point at the
# OWNING module's source; ALL_SRC aggregates every knowbox source file for
# fences that are location-independent (counts across the whole package).
KB_SRC = CLI_SRC.read_text(encoding="utf-8")
SYSUTIL_SRC = (CLI_SRC.parent / "sysutil.py").read_text(encoding="utf-8")
ALL_SRC = "".join(p.read_text(encoding="utf-8")
                  for p in sorted(CLI_SRC.parent.glob("*.py")))

PROBE_HEADER = (
    "import importlib.machinery, importlib.util, json, os, tempfile\n"
    "p = os.environ['KNOWBOX_CLI_SRC']\n"
    "spec = importlib.util.spec_from_file_location(\n"
    "    'kbmod', p, loader=importlib.machinery.SourceFileLoader('kbmod', p))\n"
    "m = importlib.util.module_from_spec(spec)\n"
    "spec.loader.exec_module(m)\n"
)


def _probe(tmp_path, root, body, timeout=120):
    """Run probe code against the real module IN A SUBPROCESS (house rule:
    never import the CLI in the pytest process). KNOWBOX_ROOT = sandbox so
    module-level KB resolution never touches production data."""
    script = tmp_path / "probe.py"
    script.write_text(PROBE_HEADER + body, encoding="utf-8")
    return subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "python", str(script)],
        capture_output=True, text=True, timeout=timeout,
        env={**os.environ, "KNOWBOX_ROOT": str(root),
             "KNOWBOX_CLI_SRC": str(CLI_SRC)})


# --------------------------------------------------------------------------
# 1. source fence
# --------------------------------------------------------------------------

def test_fence_no_top_level_fcntl():
    # A1: a top-level `import fcntl` kills EVERY command on Windows at module
    # load. It must live only inside the else: branch of the IS_NT guard.
    # Owner: sysutil.py (the IS_NT guard + both platform imports moved there).
    lines = SYSUTIL_SRC.splitlines()
    for i, ln in enumerate(lines):
        if ln.strip() == "import fcntl":
            assert ln.startswith((" ", "\t")), \
                f"line {i+1}: fcntl imported unconditionally (A1 regression)"
    assert "import msvcrt" in SYSUTIL_SRC
    assert "IS_NT = os.name == \"nt\"" in SYSUTIL_SRC


def test_fence_stdio_reconfigure_present():
    # A3: piped cp1252 stdout crashes on the first em-dash — agent harnesses
    # ALWAYS pipe, so this is a hard blocker on native Windows.
    assert 'reconfigure(encoding="utf-8"' in SYSUTIL_SRC


def test_fence_text_writers_pin_lf():
    # D9: index.md, log.md, miss-log, note-create must all pass newline="\n".
    # Aggregated across ALL module sources: the writers live in different
    # modules post-split (A4), no single module owns ≥4 sites.
    assert ALL_SRC.count('newline="\\n"') >= 4


def test_fence_no_direct_os_replace():
    # C7: every swap goes through _replace_retry (the Windows replace-over-
    # open-file retry); the only os.replace( left is inside the helper.
    # Owner: sysutil.py.
    assert SYSUTIL_SRC.count("os.replace(") == 1
    assert "_replace_retry(tmp, path)" in SYSUTIL_SRC      # _atomic_write_bytes
    assert "_replace_retry(tmp, dest)" in SYSUTIL_SRC      # _swap_into_place


def test_fence_no_hand_built_sqlite_uri():
    # C8: file:{DB}?mode=ro breaks silently on Windows paths (?/#/% or
    # backslash edge) → db_missing=True → rebuild-forever loop. Aggregate:
    # the DB readers move between modules as the split proceeds.
    assert "?mode=ro" not in ALL_SRC


def test_fence_editor_and_heartbeat_gates():
    # Post-commands-split: the editor default moved to cmdwrite.py (cmd_new),
    # heartbeat gate to maintain.py (cmd_lint), git decodes to notes.py /
    # reports.py — no single module owns them all; aggregate (A4 pattern).
    assert "posix=not IS_NT" in ALL_SRC          # F16 shlex backslash trap
    assert '"notepad" if IS_NT else "vi"' in ALL_SRC
    assert '_env("NO_HEARTBEAT")' in ALL_SRC    # F17 replica-client gate
    assert 'encoding="utf-8", errors="replace"' in ALL_SRC  # B4 git decodes


def test_fence_gitattributes_shipped_by_git_commit():
    # the new repo file must ride the auto-commit path list, or it never ships.
    # Owner: paths.py TRACKED_PATHS (single source since the Batch-2
    # consolidation; git_commit stages it verbatim).
    assert '".gitattributes"' in (CLI_SRC.parent / "paths.py") \
        .read_text(encoding="utf-8")


# --------------------------------------------------------------------------
# 2. behavior — helper probes (platform-neutral logic)
# --------------------------------------------------------------------------

def test_replace_retry_retries_then_succeeds(tmp_path):
    root = build_sandbox(tmp_path / "kb")
    r = _probe(tmp_path, root, (
        "d = tempfile.mkdtemp()\n"
        "src = os.path.join(d, 's'); dst = os.path.join(d, 'd')\n"
        "open(src, 'w').write('payload'); open(dst, 'w').write('old')\n"
        "calls = {'n': 0}\n"
        "real = os.replace\n"
        "def flaky(a, b):\n"
        "    calls['n'] += 1\n"
        "    if calls['n'] < 3:\n"
        "        raise PermissionError('simulated AV/reader hold')\n"
        "    return real(a, b)\n"
        # A3 (post-split): _replace_retry reads IS_NT from ITS OWN module —
        # mutate the owning module (sysutil, already in sys.modules via
        # cli.py's import), not the kbmod re-export.
        "import sysutil\n"
        "sysutil.IS_NT = True\n"
        "os.replace = flaky\n"
        "try:\n"
        "    m._replace_retry(src, dst, attempts=5, delay=0)\n"
        "finally:\n"
        "    os.replace = real\n"
        "print(json.dumps({'calls': calls['n'], 'content': open(dst).read()}))\n"
    ))
    assert r.returncode == 0, r.stderr
    out = json.loads(r.stdout.strip().splitlines()[-1])
    assert out == {"calls": 3, "content": "payload"}


def test_replace_retry_reraises_immediately_off_nt(tmp_path):
    root = build_sandbox(tmp_path / "kb")
    r = _probe(tmp_path, root, (
        "d = tempfile.mkdtemp()\n"
        "src = os.path.join(d, 's'); dst = os.path.join(d, 'd')\n"
        "open(src, 'w').write('payload')\n"
        "calls = {'n': 0}\n"
        "def always(a, b):\n"
        "    calls['n'] += 1\n"
        "    raise PermissionError('a POSIX perms problem is real — no retry')\n"
        "import sysutil\n"
        "sysutil.IS_NT = False\n"
        "os.replace = always\n"
        "raised = False\n"
        "try:\n"
        "    m._replace_retry(src, dst, attempts=5, delay=0)\n"
        "except PermissionError:\n"
        "    raised = True\n"
        "print(json.dumps({'raised': raised, 'calls': calls['n']}))\n"
    ))
    assert r.returncode == 0, r.stderr
    out = json.loads(r.stdout.strip().splitlines()[-1])
    assert out == {"raised": True, "calls": 1}


def test_use_kb_reroots_all_eight_paths_globals(tmp_path):
    # A5: paths.py owns all 8 root globals; _use_kb re-root must propagate to
    # every one of them (cmd_init depends on it — a half-propagated root would
    # write notes to the new KB and the DB to the old one).
    root = build_sandbox(tmp_path / "kb")
    r = _probe(tmp_path, root, (
        "import paths\n"
        "d = tempfile.mkdtemp()\n"
        "m._use_kb(d)\n"
        "exp = {\n"
        "    'KB': d,\n"
        "    'NOTES': os.path.join(d, 'notes'),\n"
        "    'REFS': os.path.join(d, 'refs'),\n"
        "    'RAW': os.path.join(d, 'raw'),\n"
        "    'DB': os.path.join(d, '.kb.db'),\n"
        "    'INDEX': os.path.join(d, 'index.md'),\n"
        "    'LOG': os.path.join(d, 'log.md'),\n"
        "    'EMBED': os.path.join(d, '.kb-embed.json'),\n"
        "}\n"
        "got = {k: getattr(paths, k) for k in exp}\n"
        "print(json.dumps({'ok': got == exp}))\n"
    ))
    assert r.returncode == 0, r.stderr
    out = json.loads(r.stdout.strip().splitlines()[-1])
    assert out == {"ok": True}


def test_lock_acquire_contention_roundtrip_posix(tmp_path):
    # POSIX branch of the A1 shim (the branch this box can actually run):
    # two fds on one lock file — second non-blocking acquire must raise
    # OSError, and succeed after release. (flock conflicts across separate
    # open file descriptions even within one process.)
    root = build_sandbox(tmp_path / "kb")
    r = _probe(tmp_path, root, (
        "d = tempfile.mkdtemp(); lp = os.path.join(d, '.kb.lock')\n"
        "lf1 = open(lp, 'a'); lf2 = open(lp, 'a')\n"
        "m._lock_acquire(lf1)\n"
        "try:\n"
        "    m._lock_acquire(lf2); contended = False\n"
        "except OSError:\n"
        "    contended = True\n"
        "m._lock_release(lf1)\n"
        "m._lock_acquire(lf2)\n"
        "m._lock_release(lf2)\n"
        "print('OK' if contended else 'NO-CONTENTION')\n"
    ))
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip().splitlines()[-1] == "OK"


# --------------------------------------------------------------------------
# 3. behavior — end-to-end through the real CLI
# --------------------------------------------------------------------------

def test_writers_emit_lf_only(tmp_path):
    root = build_sandbox(tmp_path / "kb")
    r = kb_run("new", "--type", "fact", "--domain", "tooling",
               "--title", "LF check note", "--no-edit",
               "--body", "Body — em-dash and ✓ stay UTF-8, LF only.\n",
               root=root, expect_rc=0)
    note = Path(r.stdout.strip().splitlines()[-1])
    kb_run("reindex", root=root, expect_rc=0)
    for target in (note, root / "index.md", root / "log.md"):
        data = target.read_bytes()
        assert b"\r" not in data, f"{target.name} contains CR bytes"


def test_heartbeat_env_gate(tmp_path):
    # F17: replica clients have no janitor — KNOWBOX_NO_HEARTBEAT=1 silences
    # ONLY the heartbeat check; the missing-heartbeat ERROR stays for the
    # server. (Deprecated KB_* fallback coverage lives in test_env_fallback.)
    root = build_sandbox(tmp_path / "kb")
    (root / ".kb-heartbeat").unlink()
    r1 = kb_run("lint", root=root)
    assert "janitor heartbeat missing" in r1.stdout + r1.stderr
    r2 = kb_run("lint", root=root, env_extra={"KNOWBOX_NO_HEARTBEAT": "1"})
    assert "janitor heartbeat" not in r2.stdout + r2.stderr


def test_reindex_without_git_treats_tree_as_dirty(tmp_path):
    # B5: a failed `git status` (non-git tree) must NOT read as clean and
    # silently skip the log entry — old code never checked the rc.
    root = build_sandbox(tmp_path / "kb", git=False)
    r = kb_run("reindex", root=root, expect_rc=0)
    assert "no changes" not in r.stdout
    assert "rebuilt" in r.stdout
    # the git-missing/git-status failure is announced, not swallowed
    assert "git" in (r.stdout + r.stderr).lower()


def test_churn_handles_utf8_diffs(tmp_path):
    # B4: churn decodes captured note diffs — this corpus is full of
    # em-dashes; on Windows text=True alone would decode them as cp1252.
    root = build_sandbox(tmp_path / "kb")
    p = note_path(root, "kb-9001")
    p.write_text(p.read_text(encoding="utf-8")
                 + "\nAn em-dash edit — ütf-8 ✓.\n", encoding="utf-8")
    kb_run("touch", "kb-9001", root=root, expect_rc=0)   # ships the edit
    r = kb_run("churn", root=root, expect_rc=0)
    assert "kb-9001" in r.stdout


# --------------------------------------------------------------------------
# 4. artifacts
# --------------------------------------------------------------------------

def test_gitattributes_pins_lf():
    attrs = (REPO / ".gitattributes").read_text(encoding="utf-8")
    assert "* text=auto eol=lf" in attrs
