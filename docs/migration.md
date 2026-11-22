# Switch from Python Knowbox to Rust

The two binaries use the same Markdown, journal, and derived-file paths. No
notes or Git history need conversion. Keep the Python executable available
until the Rust binary has passed native tests on the OS where it will run.
The Rust CLI starts its own `major.minor` release line at `1.0`. Use the release
archive and checksum for your platform. [Download and verification](../README.md#download-and-try)

## Rehearse on a disposable copy

1. Copy your KB to a local test directory and set `KNOWBOX_ROOT` explicitly.
   Leave the live KB alone. Remove the copy's production remote, or replace it
   with a disposable local bare repository before running any Git-backed writer.
   Use separate copies for concurrent comparisons because Python search can
   rebuild the shared index.
2. Run `knowbox version`, `knowbox lint`, and representative `search` and `show`
   commands. Use the same embedding configuration if you compare hybrid
   results, or unset `KNOWBOX_EMBED_URL` to compare offline.
3. Run a write, then check the resulting Markdown bytes, `log.md`, `git status`,
   and the local commit. Run `sweep` if testing a remote. Confirm that `touch`
   alone does not commit and that an intentional push failure leaves a local
   commit for retry.
4. Stop Rust commands and switch the copy back to Python. Confirm that Python
   can read the changed note and rebuild search. Rust's new timestamped
   journal entries require a reader that understands the added metadata. Run
   this rehearsal on each platform you plan to use.

## Behavior differences to account for

- Use a regular directory on a local filesystem for the KB root and `notes/`.
  Read commands skip linked entries and non-UTF-8 filenames with a warning.
  Choose an empty directory for `init` and an explicit, valid `KNOWBOX_ROOT`
  when testing a copy.
- Git writers refuse existing staged changes and unfinished Git operations.
  They use the KB's own Git worktree and configured remote upstream. Push
  failure warns after a successful local commit. An unavailable or failed
  requested Git initialization is an error, not silent success.
- `new` refuses ID exhaustion at `kb-9999` rather than issuing a five-digit ID.
  The editor opens a private draft containing the frontmatter and body. Knowbox
  assigns the ID and dates after the editor exits successfully.
- Malformed embedding batches do not overwrite the cache. Remote embedding
  endpoints use HTTPS and accept a CONNECT proxy via `HTTPS_PROXY` and
  `NO_PROXY`. Redirects are refused.
- A stale search uses a private index and prints a notice. Run `knowbox reindex`
  to publish an updated `index.md` and SQLite database.
- Review the [schema v1 patch](plan/schema-changes.md) for the data repository.
  It documents timestamped events and adds ignores for usage, the local
  audit seed, and pending write recovery. Keep the existing miss-log ignore.
  Existing journal dates are accepted without conversion.
- Existing schedules can keep refreshing `.kb-heartbeat` and `.kb-sweep-heartbeat`.
  Lint checks their age and the existing `N errors, M warnings` summary still
  works for a janitor script. Set `KNOWBOX_NO_HEARTBEAT=1` for a disposable copy
  or replica without scheduled maintenance. Switch schedules separately from
  the migration rehearsal.

## Roll back safely

Stop Rust writers and searches before switching the executable. Inspect the
journal, working tree, staged changes, and local commits first. A failed command
can have published a note before a later index or Git failure; preserve that
work and inspect the named failure phase rather than resetting Git. With the
data reconciled, start only Python and rebuild derived files if needed. Do not
remove or replace the writer lock while any command could be running.
