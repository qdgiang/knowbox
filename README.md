# knowbox

Personal/agent knowledge-base CLI — the tool half of the `~/kb` setup.
Single-module, **stdlib-only** Python; runs under uv-managed Python 3.12+.
Split out of the data repo (`qdgiang/kb`) on 2026-10: this repo carries **no
notes** — installing it never fetches KB data.

Markdown notes with YAML frontmatter are canonical; SQLite FTS5 index,
`index.md` and embeddings are derived and rebuildable. Writes auto-commit
(+ auto-push when the data repo has a remote).

## Install (any machine)

```sh
# prerequisite: uv (https://docs.astral.sh/uv/) — on Windows also Git for Windows
uv tool install git+https://github.com/qdgiang/knowbox
knowbox version
```

Update (canonical — re-fetches git HEAD; `uv tool upgrade` is unreliable for
git sources):

```sh
uv tool install --reinstall git+https://github.com/qdgiang/knowbox
```

## Bootstrap a KB

```sh
knowbox init            # skeleton at ~/kb (notes/ refs/ raw/, git-initialized)
knowbox init D:\kb      # custom location; then: export KNOWBOX_ROOT=D:\kb
```

Root resolution: `KNOWBOX_ROOT` if set (must exist — loud fail otherwise);
else `~/kb`, validated at startup (must contain `notes/`; `knowbox init` is
exempt). There is no script-relative inference — the installed CLI lives in
the uv tools venv, not in your KB.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `KNOWBOX_ROOT` | `~/kb` | KB data root |
| `KNOWBOX_EMBED_URL` | `https://api.ai-box.vn/v1/embeddings` | embeddings endpoint |
| `KNOWBOX_EMBED_MODEL` | `text-embedding-v4` | embedding model |
| `ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY` | — | embed token (fallback: `env.ANTHROPIC_AUTH_TOKEN` in `~/.claude/settings.json`) |
| `KNOWBOX_LOCK_TIMEOUT` | built-in | write-lock wait |
| `KNOWBOX_NO_FUSION` / `KNOWBOX_NO_RECENCY` | — | search kill-switches |
| `KNOWBOX_NO_HEARTBEAT` | — | exempt replica machines from the janitor-heartbeat lint |
| `EDITOR` | — | `knowbox new` interactive body edit |

**Deprecated:** legacy `KB_*` spellings (`KB_ROOT`, `KB_EMBED_URL`, …) are
still honored as fallbacks for one release and will be dropped in 2.0.
`KNOWBOX_*` always wins when both are set.

## Commands

`knowbox --help` for the full list. Core protocol: `search` (hybrid BM25 +
gated semantic fusion, typo/semantic/refs rescue), `expand`, `show`, `list`,
`recent`, `new`, `touch`, `verify`, `supersede`, `embed`, `vsearch`,
`reindex`, `lint`, `stale`, `churn`, `refs`, `conflicts`, `next-id`, `init`,
`version`.

## Development

```sh
git clone https://github.com/qdgiang/knowbox && cd knowbox
uv run pytest -q                       # full suite (sandboxed; never touches a live KB)
uv run --script src/knowbox/cli.py --help   # script mode (no install)
```

Tests invoke the CLI as a subprocess against synthetic sandbox KBs
(`tests/conftest.py`); a hard guard blocks the production `~/kb`.
Goldens live in `tests/goldens/` — regenerate only via
`tests/tools/capture_goldens.py` with a reviewed diff.

## Schema contract

The note frontmatter contract (fields, enums, templates) is documented in the
**data repo's** `SCHEMA.md`, while the parser/enforcement lives here. Changing
the contract requires a coordinated knowbox release. `tests/fixtures/corpus-v1`
is the frozen cross-repo compatibility fixture — the suite pins knowbox
against it forever.

## Windows

`uv tool install` generates `knowbox.exe` in `%USERPROFILE%\.local\bin`
(`uv tool update-shell` puts it on PATH). Locking uses `msvcrt`, files are
written LF, stdio is UTF-8. Keep the KB on a local disk — never OneDrive
(SQLite locking). The janitor heartbeat is auto-exempt on NT.

Full setup walkthrough (Git, uv, install, `knowbox init`, fresh-KB rules,
optional backup remote/embed/harness files): [docs/windows-runbook.md](docs/windows-runbook.md).
