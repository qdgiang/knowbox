# Knowbox

Knowbox is a local Markdown knowledge base CLI. This Rust port targets native
Linux, Windows, and macOS. It bundles SQLite and FTS5 and does not require Python,
`uv`, a POSIX shell, or a running service. The tool repository contains synthetic
test fixtures, not your notes.

Markdown under `notes/` and `refs/` is the source of truth. `.kb.db` and
`index.md` are derived. The Rust port keeps the note frontmatter dialect used by
the Python CLI rather than interpreting existing frontmatter as full YAML.

## Download and try

The Rust CLI release line starts at `1.0`. The CLI and Homebrew formula use
`major.minor`, with a matching Git tag such as `v1.1`. Advance the minor
version for a new build in the same major series. Cargo requires a third
component, so `Cargo.toml` uses `major.minor.0`.

Get the archive for your platform and `SHA256SUMS` from the Knowbox releases.
The package names identify the Linux x64 GNU, macOS Apple Silicon, macOS Intel,
and Windows x64 MSVC targets. Linux packages are built and smoke-tested on
Ubuntu 24.04 with glibc. On macOS and Windows, the CI smoke tests cover the
version of the OS on the named release runner. [Knowbox releases](https://github.com/qdgiang/knowbox/releases)

Compare the archive's hash to its line in `SHA256SUMS` before extracting it.
Use `sha256sum` on Linux, `shasum -a 256` on macOS or `Get-FileHash -Algorithm
SHA256` in PowerShell. Run `knowbox version` from the extracted binary to check
the version. Git is needed for a Git-backed KB.

### Build from source

Build with `cargo build --locked` from this directory; the pinned toolchain
comes from `rust-toolchain.toml`. Building bundled SQLite requires the platform's
C toolchain (the MSVC tools on Windows). The binary is in `target/debug/`
(`knowbox.exe` on Windows).

On Windows, install Rust with [rustup](https://rustup.rs/) and Visual Studio
Build Tools with **Desktop development with C++**. With Git for Windows on
`Path`, run `cargo test --workspace --locked` before installing your build.
Copy `target\debug\knowbox.exe` into a folder on your user `Path` and run
`knowbox version` from a fresh PowerShell window to check which CLI is active.

Try the port on a new folder or a **disposable copy**. Use separate roots for
concurrent comparisons with Python because Python search can publish an index.

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
Interactively, `new` opens a private draft with the frontmatter and body. Edit
the title, tags, summary, and text in that draft. Leave `id`, `created`, and
`updated` as `pending` for Knowbox to fill when it saves. Session drafts need a
`project`. If validation or publication fails before the note is saved, the
error gives the path to the private draft for recovery. On Windows, set
`EDITOR` to a native executable name or path. On Unix, `EDITOR` also accepts
quoted arguments.

## Working with Git

Omit `--no-git` on `init` to create a Git-backed KB. Git is required only for
Git-backed operations. Knowbox commits its managed data paths and pushes to
the current branch's configured upstream; `init` does not add a remote. A
failed push leaves the local commit in place, and `sweep` can retry it. `touch`
leaves changes uncommitted for `sweep` or the next event commit.

Journal entries carry a local timestamp with UTC offset, client, host, and an
optional session. `--client` overrides `KNOWBOX_CLIENT`. The default is
`sweep` for that command and `cli` for other invocations. Set `KNOWBOX_HOST`
or `KNOWBOX_SESSION` to supply those fields explicitly. Without an explicit
session, Knowbox checks `CODEX_THREAD_ID`, `CODEX_SESSION_ID`,
`CLAUDE_SESSION_ID`, and `OPENCODE_SESSION_ID`, in that order. Git commit
trailers identify the client, host, and session of the invocation that
committed the changes. The journal preserves the origin of a deferred
`touch`. `knowbox supersede OLD --with NEW --reason REASON` records its
reason in the journal and commit. Existing date-only journal entries are
still accepted.

Knowbox ignores inherited `GIT_CONFIG*` overrides for its Git children. Git's
normal on-disk config still applies. A write also refuses environment variables
that redirect the Git directory, index or worktree.

Resolve Git conflicts and finish or unstage your own staged changes before
running a writer. Knowbox refuses to absorb somebody else's staged work or
use a surrounding repository. A failure after note publication can leave
recoverable work on disk; inspect the named phase, journal, and `git status`
before retrying. Per-file atomic replacement is not a multi-file transaction.

## Index and maintenance

Search uses a temporary snapshot when the saved index is stale. It reports this
on stderr and returns results from current notes. Use `knowbox reindex` to refresh
the saved index. Search misses append to the local `.search-misses.log` when
available. New TSV rows have timestamp, query, client, and session columns.
Older rows have just timestamp and query. Decode each row separately and skip
blank separators and invalid UTF-8. An interrupted append can leave a
truncated query. Later records start on their own lines. Inventory reads skip
linked entries and non-UTF-8 filenames with a warning, which `lint` includes in its summary.
Writers that rebuild the index require a valid inventory. The KB root and
`notes/` must be real directories.

`reindex` and `sweep` journal observed out-of-CLI note changes with client
`manual`. The last journaled fingerprints survive rebuilds of the search
index and recognize a Git checkout that only normalizes line endings. On
upgrade, the audit starts from the previous index manifest when available,
or from the current note bytes. The next writer can recover an interrupted
journaled write if the affected files still match. `sweep` repairs a stale index
in KBs created with `--no-git`. A note created after the journal began needs
a create entry. The existing `created-vs-log` waiver still applies.

`knowbox lint` checks `.kb-heartbeat` after 8 days and `.kb-sweep-heartbeat`
after 2 days. External maintenance jobs create and refresh the heartbeat files.
On a replica without those jobs, set `KNOWBOX_NO_HEARTBEAT=1`. Windows omits
checks for absent files and checks the age of any heartbeat that exists.

## Local usage

`knowbox usage [--days N]` reports explicit note opens, notes shown by search
or graph expansion, and the outcome of each search. Monthly JSONL under `.kb-usage/`
records the time, client, session, note IDs, and search layer. Successful
query text is omitted. Miss text stays in the separate local miss log.
`knowbox usage --rollup` and `sweep` fold complete records into local SQLite
and expire raw months 90 days after their last day once covered. The report
includes complete events received since the last rollup. If a raw month reaches
expiry and its checkpoint cannot verify the recorded prefix, Knowbox retains the
file and reports a coverage gap. Usage, the audit seed, and pending write
recovery files stay local. Existing KBs can apply the
[schema and ignore patch](docs/plan/schema-changes.md).

## Structured reads

Add `--json` to `search`, `show`, `expand`, `graph`, `list`, `recent`,
`refs`, `raw-backlog`, `stale`, `lint`, `history`, `timeline`, or `status`.
Search JSON includes the result layer, per-hit match kind and available ranks.
Use `--local` to skip the embedding provider and `--no-fix` to skip typo
correction. Use `--no-write` on any read to omit usage and miss logging.
An indexed search in that mode reports `stale_index` with exit status 1
when it needs `knowbox reindex`. Direct ID lookups can still read notes.

## Local Explorer

The Explorer website ships from the separate `knowbox-server` project.
Download or build its static bundle, then run
`knowbox serve --assets /path/to/web --read-only` and open the full URL printed
in your terminal. With a desktop browser, pass `--open` to launch it directly.
`knowbox serve` without an asset directory serves the JSON data API.

If Knowbox is running on another machine, forward the port from the machine
with your browser. For the default port, run
`ssh -N -L 7878:127.0.0.1:7878 user@server`, then open the URL printed by
Knowbox on the server. If you choose another port with `--port`, use that port
on both sides of the SSH forward.

The Explorer listens on loopback and uses a new access token on each start.
It reads the same knowledge base as the CLI and skips usage and miss logging.
Search stays local unless you enable semantic search for the query.

Use `knowbox serve --assets /path/to/web` to enable the Editor and Review actions.
The Editor checks the file hash before saving
and keeps both versions visible if the note changed. Inbox ingest, split,
links and verdict actions are journaled with the notes they change.

## Editing and review from the CLI

```sh
knowbox edit kb-0042 --set 'summary=Revised routing summary'
knowbox edit kb-0042 --base-sha1 HASH --body-stdin < new-body.md
knowbox lint --file draft.md --against kb-0042
knowbox link kb-0042 kb-0043
knowbox verdict kb-0042 kb-0043 --verdict complementary --reason "The notes explain adjacent steps" --link
```

`knowbox edit ID` opens a private copy from a terminal. Supply `--source-stdin`
to edit the complete Markdown file from stdin. Use `show ID --json` to get
the raw-byte `base_sha1`. Repeat `--related ID` with `knowbox new` to create
both directions of each link.

For a save you may need to retry, set `--request-id KEY` on `new`, `edit`,
`link` or `verdict`. Repeat the same payload and key after an interruption.
The journal records the result so a retry can finish indexing or committing
without allocating a second note.

## Agent context and Explorer insights

```sh
knowbox context --project knowbox --host fedora-server --explain
knowbox map --json
knowbox metrics --json
```

The context command emits at most 9,000 characters in either text mode.
The default map uses local embeddings, with an authored-link layout when
usable vectors are absent. Add `--project-misses` to send up to 16 recent
missed queries to the configured embedding provider.

`sweep` records the first measurement of each local day in
`.kb-metrics.jsonl`. The measurements stay on the machine. The Explorer
shows recorded days and gaps. Fenced code is outside the prose word budget,
and lint warns when a Mermaid fence exceeds 15 lines.

## Optional semantic search

Configure `KNOWBOX_EMBED_URL` and `KNOWBOX_API_KEY`, then run `knowbox embed`.
`KNOWBOX_EMBED_MODEL` defaults to `text-embedding-v4`. Remote endpoints must
use HTTPS; numeric loopback endpoints (`127.0.0.1`, `[::1]`) can use HTTP.
For HTTPS providers, `HTTPS_PROXY` can point to an HTTP or HTTPS CONNECT proxy.
Set `NO_PROXY` to comma-separated bypasses such as `localhost,*.example.net`
or `*`. Lowercase `https_proxy` and `no_proxy` also work. Numeric loopback HTTP
uses a direct connection. Provider TLS certificates are verified and redirects
are disabled. Rerun `knowbox embed` after edits to refresh `.kb-embed.json`.

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
