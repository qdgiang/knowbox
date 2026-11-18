# Knowbox

Knowbox is a local Markdown knowledge base CLI. This Rust port targets native
Linux, Windows, and macOS. It bundles SQLite and FTS5 and does not require Python,
`uv`, a POSIX shell, or a running service. The tool repository contains synthetic
test fixtures, not your notes.

Markdown under `notes/` and `refs/` is the source of truth. `.kb.db` and
`index.md` are derived. The Rust port keeps the note frontmatter dialect used by
the Python CLI rather than interpreting existing frontmatter as full YAML.

## Build and try

Build with `cargo build --locked` from this directory; the pinned toolchain
comes from `rust-toolchain.toml`. Building bundled SQLite requires the platform's
C toolchain (the MSVC tools on Windows). The binary is in `target/debug/`
(`knowbox.exe` on Windows).

On Windows, install Rust with [rustup](https://rustup.rs/) and Visual Studio
Build Tools with **Desktop development with C++**. With Git for Windows on
`Path`, run `cargo test --workspace --locked` before installing your build.
Copy `target\debug\knowbox.exe` into a folder on your user `Path` and run
`knowbox version` from a fresh PowerShell window to check which CLI is active.

Try the port on a new folder or a **disposable copy**, not on your live KB.
Do not run both implementations against the same root concurrently, including
searches that can rebuild the index.

```sh
cargo run --locked -- init --no-git ./demo-kb
export KNOWBOX_ROOT=./demo-kb
cargo run --locked -- new --type fact --domain demo --title 'First finding' --body 'A searchable observation.'
cargo run --locked -- search 'searchable observation'
cargo run --locked -- show kb-0001
```

In PowerShell, set `$env:KNOWBOX_ROOT = './demo-kb'` instead of `export`.
Without `KNOWBOX_ROOT`, the default is `~/knowbox` (the user's home directory
on Windows too). An invalid explicit root is an error, not a fallback. Store the
KB on a local filesystem that supports file locking and atomic replacement.

For note authoring, `new --body-stdin` takes UTF-8 Markdown from a pipe.
Interactively, `new` opens a private draft before issuing an ID. On Windows,
set `EDITOR` to a native executable name or path; use a wrapper executable if
you need editor arguments. On Unix, `EDITOR` also accepts quoted arguments.

## Working with Git

Omit `--no-git` on `init` to create a Git-backed KB. Git is required only for
Git-backed operations. Knowbox commits its managed data paths and pushes to
the current branch's configured upstream; `init` does not add a remote. A
failed push leaves the local commit in place, and `sweep` can retry it. `touch`
leaves changes uncommitted for `sweep` or the next event commit.

Resolve Git conflicts and finish or unstage your own staged changes before
running a writer. Knowbox refuses to absorb somebody else's staged work or
use a surrounding repository. A failure after note publication can leave
recoverable work on disk; inspect the named phase, journal, and `git status`
before retrying. Per-file atomic replacement is not a multi-file transaction.

## Optional semantic search

Configure `KNOWBOX_EMBED_URL` and `KNOWBOX_API_KEY`, then run `knowbox embed`.
`KNOWBOX_EMBED_MODEL` defaults to `text-embedding-v4`. Remote endpoints must
use HTTPS; numeric loopback endpoints (`127.0.0.1`, `[::1]`) can use HTTP.
Knowbox connects directly without a proxy or following redirects. The embedding
store `.kb-embed.json` is rebuildable. After editing notes, rerun `knowbox embed`
to refresh vectors computed from earlier text.

`embed` sends note text to the provider. `vsearch` and degraded `search`
can send the query; exact lexical and successful typo-corrected searches
stay offline. `KNOWBOX_NO_FUSION=1` disables partial-match reranking, **not**
semantic rescue after a full miss. Unset `KNOWBOX_EMBED_URL` to keep all
searches offline. `KNOWBOX_NO_RECENCY=1` disables age weighting during fusion.
`conflicts --json` reads the local vectors and returns review candidates, not
verdicts, without contacting the provider.

## Verification

`scripts/gate.sh full` runs formatting, Clippy, tests, and dependency policy
checks. CI runs the Rust checks on native Linux, Windows, and macOS runners.
The tests use disposable roots and local Git remotes. The synthetic `corpus-v1`
pins generated index bytes, ordered note IDs, and lint findings against the
Python v4.1.1 baseline. Ranked-ID goldens do not depend on SQLite snippets or
floating-point scores.

The port is based on [Python revision `1da7ffdd`](https://github.com/qdgiang/knowbox-python/tree/1da7ffdd70b10d53295199ac8777612c8d90d49b).
[Migration and rollback](docs/migration.md) and the
[agent workflow](skills/knowbox/SKILL.md) describe the operational contract.
External cron, janitor, and their heartbeat checks are deferred.
