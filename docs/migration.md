# Switch from Python Knowbox to Rust

The two binaries use the same Markdown, journal, and derived-file paths. No
notes or Git history need conversion. Keep the Python executable available
until the Rust binary has passed native tests on the OS where it will run.

## Rehearse on a disposable copy

1. Copy your KB to a local test directory and set `KNOWBOX_ROOT` explicitly.
   Leave the live KB alone. Remove the copy's production remote, or replace it
   with a disposable local bare repository before running any Git-backed writer.
   Use separate copies for concurrent comparisons. Do not point Python and Rust
   at the same root at the same time, even for search: both can rebuild it.
2. Run `knowbox version`, `knowbox lint`, and representative `search` and `show`
   commands. Use the same embedding configuration if you compare hybrid
   results, or unset `KNOWBOX_EMBED_URL` to compare offline.
3. Run a write, then check the resulting Markdown bytes, `log.md`, `git status`,
   and the local commit. Run `sweep` if testing a remote. Confirm that `touch`
   alone does not commit and that an intentional push failure leaves a local
   commit for retry.
4. Stop Rust commands and switch the copy back to Python. Confirm that Python
   can read the changed note, see the journal, and rebuild search. Run this
   rehearsal on each platform you plan to use.

## Behavior differences to account for

- A present but blank `KNOWBOX_ROOT`, linked managed paths, and nonempty init
  targets are refused. Use a regular directory on a local filesystem.
- Git writers refuse existing staged changes and unfinished Git operations.
  They use the KB's own Git worktree and configured remote upstream. Push
  failure warns after a successful local commit. An unavailable or failed
  requested Git initialization is an error, not silent success.
- `new` refuses ID exhaustion at `kb-9999` rather than issuing a five-digit ID.
  An editor edits a private body draft, not a partially published note.
- Malformed embedding batches do not overwrite the cache. Remote embedding
  endpoints require HTTPS; redirects are refused. Older derived SQLite
  schemas may rebuild on first search.
- Cron, janitor, and heartbeat checking are not part of this port. `sweep` can
  still be run manually. Keep any existing scheduled jobs stopped during the
  switch, and migrate scheduling separately.

## Roll back safely

Stop Rust writers and searches before switching the executable. Inspect the
journal, working tree, staged changes, and local commits first. A failed command
can have published a note before a later index or Git failure; preserve that
work and inspect the named failure phase rather than resetting Git. With the
data reconciled, start only Python and rebuild derived files if needed. Do not
remove or replace the writer lock while any command could be running.
