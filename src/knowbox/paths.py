"""knowbox paths: KB root resolution and schema constants.

Owns the 8 root globals (KB, NOTES, REFS, RAW, DB, INDEX, LOG, EMBED) —
mutable via _use_kb (import-time + cmd_init re-root). Consumers MUST
late-bind (paths.NOTES inside a function), never snapshot at module level.
"""
import os
import re
import sys


def _env(suffix):
    """KNOWBOX_<suffix> only (KB_* fallback dropped at 2.0). Returns "" when
    unset."""
    return os.environ.get(f"KNOWBOX_{suffix}", "").strip()


KB = _env("ROOT")
if KB:
    # explicit override (tests, sandboxes, worktrees). NEVER fall back
    # silently — a bogus KNOWBOX_ROOT must fail loudly, not operate on ~/knowbox.
    if not os.path.isdir(KB):
        print(f"knowbox: KNOWBOX_ROOT={KB!r} is not a directory — refusing to "
              f"fall back to ~/knowbox (fix KNOWBOX_ROOT or unset it)",
              file=sys.stderr)
        sys.exit(2)
    KB = os.path.realpath(KB)
    _KB_FROM_ENV = True
else:
    KB = os.path.realpath(os.path.expanduser("~/knowbox"))
    _KB_FROM_ENV = False


def _use_kb(root):
    global KB, NOTES, REFS, RAW, DB, INDEX, LOG, EMBED
    KB = root
    NOTES = os.path.join(KB, "notes")
    REFS = os.path.join(KB, "refs")
    RAW = os.path.join(KB, "raw")
    DB = os.path.join(KB, ".kb.db")
    INDEX = os.path.join(KB, "index.md")
    LOG = os.path.join(KB, "log.md")
    EMBED = os.path.join(KB, ".kb-embed.json")


_use_kb(KB)


def _looks_like_kb(root):
    return os.path.isdir(root) and os.path.isdir(os.path.join(root, "notes"))


ENUMS = {
    "type": {"fact", "gotcha", "howto", "decision", "reference", "project",
             "session"},
    "status": {"draft", "active", "verified", "superseded"},
    "confidence": {"high", "medium", "low"},
}
REQUIRED = ["id", "title", "type", "domain", "status", "confidence",
            "created", "updated", "summary"]
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ID_RE = re.compile(r"^kb-\d{4}$")
# janitor dead-man's switch: weekly cron touches .kb-heartbeat (kb-0014).
# Weekly schedule + 1 day grace.
HEARTBEAT_MAX_AGE_DAYS = 8
