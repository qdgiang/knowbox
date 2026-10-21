"""knowbox sysutil: platform shims, atomic file IO, the writer lock.

Named sysutil (not platform): cli.py's PEP 723 script mode puts the package
dir on sys.path and imports helpers as top-level modules — a `platform.py`
would shadow the stdlib platform module process-wide (stdlib lazily imports
it via importlib.metadata, used by cli._version()).
"""
import os
import sys
import tempfile
import time
from contextlib import contextmanager

import paths as _paths

# Windows portability: fcntl is Unix-only — a
# top-level import killed EVERY command (even readers) on native Windows.
# msvcrt.locking is the Win32 analog; byte-range locks there are mandatory,
# not advisory, but only kb touches .kb.lock, so the semantics are equivalent.
IS_NT = os.name == "nt"
if IS_NT:
    import msvcrt
else:
    import fcntl

# On native Windows, piped stdout/stderr falls back to the
# locale codec (cp1252) and the first em-dash raises UnicodeEncodeError —
# fatal for agent harnesses, which ALWAYS pipe. Force UTF-8 on all three
# streams; no-op elsewhere (Python on Linux/macOS is already UTF-8).
if IS_NT:
    for _s in (sys.stdin, sys.stdout, sys.stderr):
        try:
            _s.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):
            pass  # already detached / replaced — nothing sensible to do


def _replace_retry(src, dst, attempts=5, delay=0.1):
    """os.replace with a short retry. POSIX replace-over-an-open-
    file always succeeds (readers keep the old inode); on Windows it raises
    PermissionError while ANY process holds dst open without FILE_SHARE_DELETE
    — a concurrent kb reader mid-parse, an editor, an AV scan. Reader windows
    are milliseconds, so retrying almost always lands; the last error raises.
    Retries only on nt — a POSIX PermissionError is a real permission problem
    and retrying would just delay the traceback."""
    for i in range(attempts):
        try:
            os.replace(src, dst)
            return
        except PermissionError:
            if not IS_NT or i == attempts - 1:
                raise
            time.sleep(delay)


def _atomic_write_bytes(path, data):
    """Write bytes via tempfile.mkstemp(same dir) + os.replace: readers never
    see a half-written note, and a failed write never truncates the original."""
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path) or ".",
                               prefix=".kb-write-")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        os.chmod(tmp, os.stat(path).st_mode & 0o7777)  # mkstemp makes 0600
        _replace_retry(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def _tmp_in_kb(prefix):
    """Uniquely-named temp file in the KB root (T3/P7). The old FIXED `X.tmp`
    names raced: two concurrent rebuilds removed-then-built into the SAME
    path, corrupting both. mkstemp names never collide."""
    fd, tmp = tempfile.mkstemp(dir=_paths.KB, prefix=prefix, suffix=".tmp")
    os.close(fd)
    return tmp


def _swap_into_place(tmp, dest):
    """chmod (mkstemp makes 0600) + atomic replace; clean the tmp on failure."""
    try:
        os.chmod(tmp, 0o644)
        _replace_retry(tmp, dest)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


LOCK_TIMEOUT_DEFAULT = 120.0
_LOCK_DEPTH = 0


def _lock_acquire(lf):
    """Non-blocking exclusive lock on the open lock-file handle; raises
    OSError on contention on BOTH platforms (the retry loop in _locked
    depends on that). msvcrt locks a 1-byte range at the current position —
    seek(0) first so acquire/release always address the same range."""
    if IS_NT:
        lf.seek(0)
        msvcrt.locking(lf.fileno(), msvcrt.LK_NBLCK, 1)
    else:
        fcntl.flock(lf, fcntl.LOCK_EX | fcntl.LOCK_NB)


def _lock_release(lf):
    if IS_NT:
        lf.seek(0)
        msvcrt.locking(lf.fileno(), msvcrt.LK_UNLCK, 1)
    else:
        fcntl.flock(lf, fcntl.LOCK_UN)


@contextmanager
def _locked():
    """Exclusive lock for EVERY write command (T3/P7): note writes, derived
    rebuilds, log append and git commit are one critical section, so parallel
    agent sessions serialize instead of interleaving index/log/commits.

    Waits up to KB_LOCK_TIMEOUT seconds (default 120; unparsable or ≤0 values
    fall back to the default) with non-blocking flock retries, then exits
    loudly — a hung writer can never wedge a live session silently. Re-entrant
    within one process: a nested call rides the outer lock (two separate fds
    on the same file would flock-contend even in-process). Readers
    (search/show/list/lint/stale/churn) never lock.

    Documented trade-off: git_commit (incl. push, 60 s timeout) runs INSIDE
    the lock — worst case a concurrent writer waits ~60 s during a network
    hang. Chosen because serialized commit+push keeps log/index/commits
    consistent, and pushes here are ~1 s. `knowbox embed` is the exception: it
    locks only its final merge+write, never the HTTP phase.
    """
    global _LOCK_DEPTH
    if _LOCK_DEPTH:
        _LOCK_DEPTH += 1
        try:
            yield
        finally:
            _LOCK_DEPTH -= 1
        return
    try:
        timeout = float(_paths._env("LOCK_TIMEOUT"))
    except ValueError:
        timeout = LOCK_TIMEOUT_DEFAULT
    if timeout <= 0:
        timeout = LOCK_TIMEOUT_DEFAULT
    lock_path = os.path.join(_paths.KB, ".kb.lock")
    lf = open(lock_path, "a")
    deadline = time.monotonic() + timeout
    while True:
        try:
            _lock_acquire(lf)
            break
        except OSError:
            if time.monotonic() >= deadline:
                lf.close()
                # lsof doesn't exist on Windows
                probe = (f"handle.exe {lock_path}" if IS_NT
                         else f"lsof {lock_path}")
                sys.exit(f"knowbox: lock held >{timeout:g}s by another knowbox process — "
                         f"check: {probe}")
            time.sleep(0.05)
    _LOCK_DEPTH = 1
    try:
        yield
    finally:
        _LOCK_DEPTH = 0
        _lock_release(lf)
        lf.close()
