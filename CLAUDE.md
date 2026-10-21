# CLAUDE.md — knowbox CLI repo

Personal/agent knowledge-base CLI. Stdlib-only Python, uv-managed, tests via
`uv run pytest`. The KB it operates on is governed by a `SCHEMA.md` in the KB
root — the frontmatter contract there is what this CLI enforces; schema
changes need a version bump + release here.

## Release flow

1. Bump `version` in `pyproject.toml` AND `__version__` in
   `src/knowbox/__init__.py`, then `uv lock`.
2. `uv run pytest` — must be green.
3. Commit + push; install/update via
   `uv tool install --reinstall git+https://github.com/qdgiang/knowbox`.

## Conventions

- Env vars are `KNOWBOX_`-prefixed only (legacy `KB_*` fallback dropped at
  2.0); read them via `paths._env()`.
- Frontmatter rewrites must go through the guarded `_rewrite_frontmatter`
  (notes.py) — never regex whole files.
- `cmd_show` stays RAW (malformed notes must remain readable); any parsed
  additions on top must be defensive (stderr, MalformedNote-tolerant).
