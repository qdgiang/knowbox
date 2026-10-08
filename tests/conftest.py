"""Shared harness for knowbox CLI tests (hardening plan P1; knowbox split kb-0142).

Design notes:
- Tests invoke the REAL entry point as a subprocess (`uv run --script
  src/knowbox/cli.py`) with KNOWBOX_ROOT pointing at a sandbox. Never import
  the CLI module (it freezes KB at import time).
- Production safety: kb_run() refuses to target /home/giang/kb. Every sandbox
  is built under pytest's tmp dirs.
- HOME is deliberately NOT faked: uv discovers its managed CPython 3.12 under
  the real HOME, and _embed_token() short-circuits on ANTHROPIC_AUTH_TOKEN
  before ever reading ~/.claude/settings.json. Isolation comes from
  KNOWBOX_ROOT + the blackholed KNOWBOX_EMBED_URL, not from HOME.
  (Fake-HOME fallback tests live in test_resolution.py and fake HOME
  explicitly, pinning UV_PYTHON_INSTALL_DIR.)
- Goldens: compare against tests/goldens/*; regenerate ONLY via
  tests/tools/capture_goldens.py (KB_GOLDENS_REGENERATE=1) in a phase that
  intentionally changes behavior — the golden diff must be in that commit.
"""
import json
import os
import re
import shlex
import shutil
import stat
import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent          # knowbox repo root
PROD_KB = Path("/home/giang/kb").resolve()
CORPUS_V1 = REPO / "tests" / "fixtures" / "corpus-v1"
GOLDENS = REPO / "tests" / "goldens"
CLI_SRC = REPO / "src" / "knowbox" / "cli.py"          # the version under test
DOTFILES_JANITOR = Path("/home/giang/dotfiles/scripts/kb-janitor.sh")
UV = shutil.which("uv") or "/home/giang/.local/bin/uv"
REGEN = os.environ.get("KB_GOLDENS_REGENERATE") == "1"

GIT_ENV_EXTRA = {
    "GIT_AUTHOR_NAME": "kb-tests", "GIT_COMMITTER_NAME": "kb-tests",
    "GIT_AUTHOR_EMAIL": "kb-tests@localhost",
    "GIT_COMMITTER_EMAIL": "kb-tests@localhost",
}


# --------------------------------------------------------------------------
# runner
# --------------------------------------------------------------------------

def _cli_env(root):
    env = {**os.environ, **GIT_ENV_EXTRA,
           "KNOWBOX_ROOT": str(root),
           "ANTHROPIC_AUTH_TOKEN": "test-token",     # short-circuits settings.json read
           "KNOWBOX_EMBED_URL": "http://127.0.0.1:9/embeddings"}  # blackhole
    env.pop("KB_ROOT", None)       # deprecated fallbacks never leak in
    env.pop("KB_EMBED_URL", None)
    return env


def kb_run(*args, root, env_extra=None, expect_rc=None, timeout=180):
    """Run the CLI with KNOWBOX_ROOT=root. Returns CompletedProcess."""
    root = Path(root).resolve()
    assert root != PROD_KB and not str(root).startswith(str(PROD_KB) + os.sep), \
        f"refusing to run knowbox against production {PROD_KB} (root={root})"
    env = _cli_env(root)
    for k, v in (env_extra or {}).items():
        if v is None:
            env.pop(k, None)
        else:
            env[k] = v
    r = subprocess.run(
        [UV, "run", "--quiet", "--python", "3.12", "--script",
         str(CLI_SRC), *[str(a) for a in args]],
        capture_output=True, text=True, env=env,
        cwd=str(root) if root.is_dir() else str(root.parent),
        stdin=subprocess.DEVNULL, timeout=timeout)
    if expect_rc is not None:
        assert r.returncode == expect_rc, (
            f"knowbox {' '.join(str(a) for a in args)} → rc={r.returncode} "
            f"(want {expect_rc})\nstdout:\n{r.stdout}\nstderr:\n{r.stderr}")
    return r


def kb_popen(*args, root, env_extra=None):
    """Non-blocking variant of kb_run for concurrency tests (P7): same env
    construction and production-safety guard, returns the Popen object."""
    root = Path(root).resolve()
    assert root != PROD_KB and not str(root).startswith(str(PROD_KB) + os.sep), \
        f"refusing to run knowbox against production {PROD_KB} (root={root})"
    env = _cli_env(root)
    for k, v in (env_extra or {}).items():
        if v is None:
            env.pop(k, None)
        else:
            env[k] = v
    return subprocess.Popen(
        [UV, "run", "--quiet", "--python", "3.12", "--script",
         str(CLI_SRC), *[str(a) for a in args]],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env,
        cwd=str(root) if root.is_dir() else str(root.parent),
        stdin=subprocess.DEVNULL)


def knowbox_shim(dirpath):
    """Write an executable `knowbox` bash shim into dirpath that runs the CLI
    under test — janitor tests point KNOWBOX_BIN at it (the janitor invokes a
    bare binary path, no uv wrapper)."""
    dirpath = Path(dirpath)
    dirpath.mkdir(parents=True, exist_ok=True)
    shim = dirpath / "knowbox"
    shim.write_text(
        "#!/usr/bin/env bash\n"
        f'exec {shlex.quote(UV)} run --quiet --python 3.12 --script '
        f'{shlex.quote(str(CLI_SRC))} "$@"\n')
    shim.chmod(shim.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return shim


def git_run(root, *args):
    return subprocess.run(["git", "-C", str(root), *args],
                          capture_output=True, text=True, check=True,
                          env={**os.environ, **GIT_ENV_EXTRA}).stdout


def git_dir_run(git_dir, *args):
    return subprocess.run(["git", "--git-dir", str(git_dir), *args],
                          capture_output=True, text=True, check=True,
                          env={**os.environ, **GIT_ENV_EXTRA}).stdout


def note_path(root, nid):
    hits = list((Path(root) / "notes").glob(f"{nid}-*"))
    assert len(hits) == 1, f"expected exactly one file for {nid}, got {hits}"
    return hits[0]


def search_ids(stdout):
    """Parse result ids (in rank order) from `knowbox search` stdout."""
    return re.findall(r"^(kb-\d{4})\b", stdout, re.M)
# --------------------------------------------------------------------------
# mini corpus builder (synthetic, deterministic)
# --------------------------------------------------------------------------

def _fm(nid, title, typ="fact", domain="tooling", tags=(), status="active",
        confidence="high", created="2026-08-01", updated="2026-08-01",
        verified=None, supersedes=(), related=(), source="session",
        summary="", project=None, triggers=("when testing this fixture note",)):
    lines = ["---", f"id: {nid}", f'title: "{title}"', f"type: {typ}",
             f"domain: {domain}", f"tags: [{', '.join(tags)}]", "aliases: []"]
    if triggers:
        lines.append("triggers:")
        lines += [f'  - "{t}"' for t in triggers]
    if project:
        lines.append(f"project: {project}")
    lines += [f"status: {status}", f"confidence: {confidence}",
              f"created: {created}", f"updated: {updated}"]
    if verified:
        lines.append(f"verified: {verified}")
    lines += [f"supersedes: [{', '.join(supersedes)}]",
              f"related: [{', '.join(related)}]", f'source: "{source}"',
              f'summary: "{summary or title}, routed."', "---", ""]
    return "\n".join(lines)


MINI_NOTES = [
    # (filename-slug, frontmatter kwargs, body)
    ("kb-9001-grubby-syncs-kernel-cmdline",
     dict(nid="kb-9001", title="grubby syncs the kernel cmdline", tags=("grub",),
          related=("kb-9002",)),
     "grubby --update-kernel=ALL rewrites /etc/kernel/cmdline entries.\n\n"
     "## Related\n- [[kb-9002]] — verified companion fact\n"),
    ("kb-9002-verified-companion-fact",
     dict(nid="kb-9002", title="Verified companion fact about grubby",
          status="verified", verified="2026-08-05", related=("kb-9001",)),
     "The companion fact, verified on 2026-08-05.\n"),
    ("kb-9003-draft-idea",
     dict(nid="kb-9003", title="Draft idea about widget migration",
          status="draft"),
     "An unfinished thought.\n"),
    ("kb-9004-old-widget-configuration",
     dict(nid="kb-9004", title="Old widget configuration (retired)",
          status="superseded", related=("kb-9005",)),
     "> **Superseded by [[kb-9005]] — kept for history, excluded from search.**\n\n"
     "The old widget configuration.\n"),
    ("kb-9005-new-widget-configuration",
     dict(nid="kb-9005", title="New widget configuration",
          supersedes=("kb-9004",), related=("kb-9004",)),
     "The new widget configuration replaces the old one.\n"),
    ("kb-9006-session-log",
     dict(nid="kb-9006", title="2026-08-01 session log", typ="session",
          domain="sessions", project="machine"),
     "## Summary\nA session happened.\n"),
    ("kb-9007-reference-section-map",
     dict(nid="kb-9007", title="Mini guide section map", typ="reference"),
     "## Section map\n\n## 1. Setup — how to do things.\n"),
    ("kb-9008-body-contains-fm-like-lines",
     dict(nid="kb-9008", title="Note whose body contains frontmatter-like lines",
          related=("kb-9001",)),
     "Example block that must never be rewritten by CLI regexes:\n\n"
     "```\nupdated: 2020-01-01\nstatus: draft\n```\n\n"
     "## Related\n- [[kb-9001]] — the note a buggy regex would hit\n"),
    ("kb-9009-links-first-note",
     dict(nid="kb-9009", title="Note linking the first note",
          related=("kb-9001",)),
     "See [[kb-9001]] for the load-bearing fact.\n"),
    ("kb-9010-empty-source",
     dict(nid="kb-9010", title="Note with an empty source field", source=""),
     "Provenance unknown.\n"),
    ("kb-9011-oversized-note",
     dict(nid="kb-9011", title="Oversized note over the 600 word error budget"),
     "First sentence is the fact.\n\n" + " ".join(f"filler{i}" for i in range(650)) + "\n"),
    ("kb-9012-crlf-bom-note",
     dict(nid="kb-9012", title="Note with CRLF line endings and a BOM"),
     "Body of the CRLF note.\n"),
]

# opt-in extra notes for specific regression tests
EXTRA_NOTES = {
    # T5 (fixed in P4): no `updated:` in frontmatter → kb touch must REFUSE
    # (non-zero exit, file byte-identical); the old whole-file regex rewrote
    # the BODY line instead.
    "no_updated": ("kb-9101-no-updated-field",
                   "---\nid: kb-9101\ntitle: \"Note missing the updated field\"\n"
                   "type: fact\ndomain: tooling\ntags: []\naliases: []\n"
                   "status: active\nconfidence: high\ncreated: 2026-08-01\n"
                   "supersedes: []\nrelated: []\nsource: \"session\"\n"
                   "summary: \"Regression fixture for T5 body-rewrite bug.\"\n---\n"
                   "Body with a frontmatter-like line inside a code block:\n\n"
                   "```\nupdated: 2020-01-01\n```\n", None),
    # T7: frontmatter opened but never closed → parse_note raises
    # MalformedNote; the note must be refused by index/FTS/embeds (lint
    # ERRORs name it) yet stay readable via `knowbox show`.
    "unterminated": ("kb-9102-unterminated-frontmatter",
                     "---\nid: kb-9102\ntitle: \"Unterminated frontmatter fixture\"\n"
                     "type: fact\ndomain: tooling\ntags: []\naliases: []\n"
                     "status: active\nconfidence: high\ncreated: 2026-08-01\n"
                     "updated: 2026-08-01\nsupersedes: []\nrelated: []\n"
                     "source: \"session\"\nsummary: \"Malformed on purpose.\"\n"
                     "The closing delimiter never arrives. Token: "
                     "UNTERMINATEDZORK.\n", None),
}


def build_mini(dest: Path, extra_notes=()):
    (dest / "notes").mkdir(parents=True, exist_ok=True)
    (dest / "refs").mkdir(exist_ok=True)
    (dest / "raw").mkdir(exist_ok=True)
    (dest / "SCHEMA.md").write_text("# mini schema stub\n")
    (dest / "refs" / "mini-guide.md").write_text(
        "# Mini Guide\n\n## Setup\nDo things with MotherDuck-style tokens.\n")
    (dest / "raw" / "session-distill-2026-08-01.md").write_text(
        "# Session distill 2026-08-01\n\n## PROPOSAL\nFact one.\n\n"
        "> ingested: kb-9001\n\n## PROPOSAL\nFact two — still unreviewed.\n")
    for slug, kwargs, body in MINI_NOTES:
        text = _fm(**kwargs) + body
        if slug.startswith("kb-9012"):  # CRLF + BOM variant
            text = '\ufeff' + text.replace("\n", "\r\n")
        (dest / "notes" / f"{slug}.md").write_text(text, encoding="utf-8")
    for key in extra_notes:
        slug, text, transform = EXTRA_NOTES[key]
        (dest / "notes" / f"{slug}.md").write_text(text, encoding="utf-8")
    # log.md with machine-consistent create entries (P9): every MINI note's
    # created: (2026-08-01) matches its log date, so the created-vs-log lint
    # checks stay silent on the fixture. next_id's log high-water and the T4
    # flip depend on these entries existing. Extras (kb-91xx) deliberately
    # have NO entries \u2014 their created: equals the first log date, so the
    # pre-log-era exemption covers them.
    with (dest / "log.md").open("w", encoding="utf-8") as f:
        for _slug, kwargs, _body in MINI_NOTES:
            f.write(f"## [{kwargs.get('created', '2026-08-01')}] create | "
                    f"{kwargs['nid']} | {kwargs['title']}\n")


# --------------------------------------------------------------------------
# sandbox builder
# --------------------------------------------------------------------------

def build_sandbox(dest, corpus="mini", git=True, remote=False, extra_notes=()):
    """Create a self-contained KB sandbox. dest should live under pytest tmp."""
    dest = Path(dest)
    assert dest != PROD_KB and not str(dest).startswith(str(PROD_KB) + os.sep)
    # the CLI under test is src/knowbox/cli.py in THIS repo (CLI_SRC) — the
    # sandbox carries data only; the janitor lives in ~/dotfiles (kb-0142) and
    # janitor tests point KNOWBOX_BIN at a knowbox_shim
    dest.mkdir(parents=True, exist_ok=True)
    # seed a fresh heartbeat so lint's dead-man check (P2) stays quiet in
    # every sandbox except the tests that deliberately unset/age it
    (dest / ".kb-heartbeat").touch()
    if corpus == "frozen":
        assert CORPUS_V1.is_dir(), "tests/fixtures/corpus-v1 missing"
        for item in ("notes", "refs"):
            shutil.copytree(CORPUS_V1 / item, dest / item)
        for item in ("index.md", "log.md", "SCHEMA.md"):
            shutil.copy2(CORPUS_V1 / item, dest / item)
        (dest / "raw").mkdir(exist_ok=True)
    elif corpus == "mini":
        build_mini(dest, extra_notes=extra_notes)
    else:
        raise ValueError(f"unknown corpus {corpus!r}")
    if git:
        subprocess.run(["git", "init", "-q", str(dest)], check=True,
                       capture_output=True, env={**os.environ, **GIT_ENV_EXTRA})
        git_run(dest, "add", "-A")
        git_run(dest, "commit", "-q", "-m", "fixture init")
        if remote:
            bare = dest.parent / "remotes" / "origin.git"
            bare.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run(["git", "init", "-q", "--bare", str(bare)],
                           check=True, capture_output=True)
            git_run(dest, "remote", "add", "origin", str(bare))
            git_run(dest, "push", "-q", "-u", "origin", "HEAD")
    return dest


@pytest.fixture(scope="session")
def frozen_sandbox(tmp_path_factory):
    """One frozen-corpus sandbox per session, reindexed. Treat as READ-ONLY
    (exception: search miss-log appends, which no golden depends on)."""
    root = build_sandbox(tmp_path_factory.mktemp("frozen") / "kb",
                         corpus="frozen", git=True)
    kb_run("reindex", root=root, expect_rc=0)
    return root


# --------------------------------------------------------------------------
# golden helpers
# --------------------------------------------------------------------------

def golden_assert(name, content):
    path = GOLDENS / name
    if REGEN:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return
    assert path.exists(), (
        f"golden {name} missing — generate: uv run python tests/tools/capture_goldens.py")
    assert path.read_text(encoding="utf-8") == content, (
        f"golden drift: {name} — if intentional, regenerate with "
        f"tests/tools/capture_goldens.py and commit the diff")


def golden_json_assert(name, obj):
    golden_assert(name, json.dumps(obj, indent=2, ensure_ascii=False) + "\n")
