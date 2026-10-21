"""Concurrency + atomicity (hardening plan P7, audit finding T3).

Pre-P7 the flock covered only `knowbox new`'s id+write window: derived rebuilds,
log appends and git commits ran unlocked, touch/verify/supersede/reindex had
no lock at all, and build_db/build_index/embed-store used FIXED `X.tmp` names
that two concurrent rebuilds would remove-then-share. Now every write command
is one critical section (KNOWBOX_LOCK_TIMEOUT bounds the wait, then a loud exit),
tmp names come from mkstemp, and `knowbox embed` locks only its final merge+write
— never the HTTP phase.

Coverage note: the in-process re-entrancy guard inside _locked() (nested
with-blocks ride the outer lock) is exercised only implicitly — no CLI
command nests locks today; it exists so future refactors can't self-deadlock
(two fds on the same file flock-contend even within one process).
"""
import json
import re
import shutil
import subprocess
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from conftest import (MINI_NOTES, build_sandbox, git_dir_run, kb_popen,
                      kb_run, note_path)

FLOCK = shutil.which("flock")


def _tmp_residue(root):
    return sorted(p.name for p in Path(root).iterdir()
                  if p.name.endswith(".tmp"))


# --------------------------------------------------------------------------
# lock timeout: loud exit, note untouched
# --------------------------------------------------------------------------

@pytest.mark.skipif(not FLOCK, reason="util-linux flock not available")
def test_lock_timeout_exits_loud_and_writes_nothing(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    p = note_path(root, "kb-9001")
    before = p.read_bytes()
    holder = subprocess.Popen(
        [FLOCK, "-x", str(root / ".kb.lock"), "-c", "sleep 30"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(0.5)                      # let the holder acquire
    try:
        t0 = time.monotonic()
        r = kb_run("touch", "kb-9001", root=root,
                   env_extra={"KNOWBOX_LOCK_TIMEOUT": "2"})
        dt = time.monotonic() - t0
        assert r.returncode != 0, "must fail loudly, not wait forever"
        out = r.stdout + r.stderr
        assert "lock held" in out and "lsof" in out, out
        assert 1.5 <= dt < 10, f"KNOWBOX_LOCK_TIMEOUT=2 not honored: waited {dt:.1f}s"
        assert p.read_bytes() == before, "timed-out write touched the note"
    finally:
        holder.kill()
        holder.wait(timeout=10)


# --------------------------------------------------------------------------
# N=8 concurrent `knowbox new`: unique ids, consistent log/index/remote
# --------------------------------------------------------------------------

def test_concurrent_kb_new_race(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True, remote=True)
    bare = root.parent / "remotes" / "origin.git"
    before = {q.name for q in (root / "notes").glob("kb-*.md")}

    def one(i):
        return kb_run("new", "--type", "fact", "--domain", "test",
                      "--title", f"race note {i}", "--body", f"body {i}\n",
                      root=root)

    with ThreadPoolExecutor(max_workers=8) as ex:
        rs = list(ex.map(one, range(8)))
    assert all(r.returncode == 0 for r in rs), \
        [(r.stdout, r.stderr) for r in rs if r.returncode != 0]

    # 8 unique paths printed, 8 unique ids on disk, no duplicate allocation
    paths = [r.stdout.strip() for r in rs]
    assert len(set(paths)) == 8, paths
    after = {q.name for q in (root / "notes").glob("kb-*.md")}
    assert len(after - before) == 8
    ids = [re.match(r"(kb-\d{4})", n).group(1) for n in sorted(after)]
    assert len(set(ids)) == len(ids), f"duplicate ids allocated: {ids}"

    # log.md gains exactly 8 create entries (the mini fixture seeds 12 of
    # its own — P9), one per title
    log = (root / "log.md").read_text()
    assert log.count("] create | ") == 8 + len(MINI_NOTES)
    for i in range(8):
        assert log.count(f"race note {i}") == 1

    # index rebuilt consistently: every new id routed
    idx = (root / "index.md").read_text()
    for path in paths:
        pid = re.match(r"(kb-\d{4})", Path(path).name).group(1)
        assert pid in idx

    # every commit+push reached the fake remote (serialized under the lock)
    remote_log = git_dir_run(bare, "log", "--oneline")
    assert remote_log.count("knowbox: create") == 8, remote_log

    # no fixed-name or mkstemp tmp residue anywhere in the KB root
    assert _tmp_residue(root) == []


# --------------------------------------------------------------------------
# mixed writer race: new + touch + verify + supersede + reindex concurrently
# --------------------------------------------------------------------------

def test_mixed_concurrent_writers_stay_consistent(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    kb_run("reindex", root=root, expect_rc=0)       # baseline index + commit
    lint_before = kb_run("lint", root=root)
    errs_before = sorted(l for l in
                         (lint_before.stdout + lint_before.stderr).splitlines()
                         if l.startswith("ERROR"))

    ops = [
        ("new", "--type", "fact", "--domain", "test",
         "--title", "mixed a", "--body", "a\n"),
        ("touch", "kb-9001"),
        ("reindex",),
        ("touch", "kb-9002"),
        ("new", "--type", "fact", "--domain", "test",
         "--title", "mixed b", "--body", "b\n"),
        ("verify", "kb-9003", "--evidence", "race probe"),
        ("reindex",),
        ("supersede", "kb-9001", "--with", "kb-9002"),
    ]
    with ThreadPoolExecutor(max_workers=len(ops)) as ex:
        rs = list(ex.map(lambda o: kb_run(*o, root=root), ops))
    assert all(r.returncode == 0 for r in rs), \
        [(o, r.stdout, r.stderr) for o, r in zip(ops, rs) if r.returncode != 0]

    # T10 interaction: every op runs under the P7 lock and each writer op
    # commits before releasing it, so both racing reindexes see a clean tree
    # + byte-identical index → deterministic no-ops ("no changes", no commit).
    # Commits: fixture init + baseline reindex + 6 writer ops = 8.
    assert all("no changes" in rs[i].stdout for i in (2, 6)), \
        [(i, rs[i].stdout) for i in (2, 6)]
    assert git_dir_run(root / ".git", "rev-list", "--count",
                       "HEAD").strip() == "8"

    # lint ERROR set unchanged by the race (no new corruption classes)
    lint_after = kb_run("lint", root=root)
    errs_after = sorted(l for l in
                        (lint_after.stdout + lint_after.stderr).splitlines()
                        if l.startswith("ERROR"))
    assert errs_after == errs_before, (errs_before, errs_after)

    # final state coherent: supersede landed, new notes routed, no residue
    old = note_path(root, "kb-9001").read_text()
    assert "status: superseded" in old and "Superseded by [[kb-9002]]" in old
    idx = (root / "index.md").read_text()
    assert "mixed a" in idx and "mixed b" in idx
    assert _tmp_residue(root) == []


# --------------------------------------------------------------------------
# kb embed: flock never held across HTTP
# --------------------------------------------------------------------------

class _SlowEmbedHandler(BaseHTTPRequestHandler):
    DELAY = 2.5
    inflight = 0
    counter_lock = threading.Lock()

    def do_POST(self):
        with type(self).counter_lock:
            type(self).inflight += 1
        try:
            payload = json.loads(
                self.rfile.read(int(self.headers["Content-Length"])))
            n = len(payload["input"])
            time.sleep(self.DELAY)
            resp = json.dumps({
                "data": [{"index": i, "embedding": [0.01 * i, 0.2, 0.3]}
                         for i in range(n)]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(resp)))
            self.end_headers()
            self.wfile.write(resp)
        finally:
            with type(self).counter_lock:
                type(self).inflight -= 1

    def log_message(self, *a):
        pass


def test_embed_lock_not_held_across_http(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    _SlowEmbedHandler.inflight = 0
    srv = ThreadingHTTPServer(("127.0.0.1", 0), _SlowEmbedHandler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    port = srv.server_address[1]
    proc = kb_popen("embed", root=root, env_extra={
        "KNOWBOX_EMBED_URL": f"http://127.0.0.1:{port}/embeddings"})
    try:
        deadline = time.monotonic() + 20
        while _SlowEmbedHandler.inflight == 0 and time.monotonic() < deadline:
            time.sleep(0.05)
        assert _SlowEmbedHandler.inflight > 0, \
            "knowbox embed never reached the fake server"
        # embed is mid-HTTP (warmup or batch, 2.5 s per call): the flock must
        # be FREE — a touch with a 2 s lock timeout has to sail through.
        r = kb_run("touch", "kb-9002", root=root,
                   env_extra={"KNOWBOX_LOCK_TIMEOUT": "2"})
        assert r.returncode == 0, (
            "touch blocked while embed was in its network phase — flock must "
            "not be held across HTTP: " + r.stdout + r.stderr)
        out, err = proc.communicate(timeout=90)
        assert proc.returncode == 0, err
        store = json.loads((root / ".kb-embed.json").read_text())
        assert store["model"] == "text-embedding-v4"
        # 12 mini notes − kb-9004 (superseded) — merge wrote them all despite
        # the concurrent touch landing mid-run
        assert len(store["notes"]) == 11
        assert _tmp_residue(root) == []
    finally:
        srv.shutdown()
        srv.server_close()
        if proc.poll() is None:
            proc.kill()
            proc.communicate(timeout=10)
