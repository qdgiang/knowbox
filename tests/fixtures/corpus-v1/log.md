## [2026-08-02] create | kb-0001 | SQLite WAL mode keeps readers unblocked during writes
## [2026-08-03] create | kb-0002 | FTS5 bm25() returns negative scores — more negative is better
## [2026-08-04] create | kb-0003 | FTS5 external-content tables must be rebuilt after bulk changes
## [2026-08-05] create | kb-0004 | Create the virtualenv before installing anything into it
## [2026-08-06] create | kb-0005 | Python default arguments are evaluated once at def time
## [2026-08-07] create | kb-0006 | pytest fixture scope decides how often setup runs
## [2026-08-08] create | kb-0007 | C extensions can release the GIL; pure Python cannot
## [2026-08-09] create | kb-0008 | git rebase --onto moves a branch stack without replaying everything
## [2026-08-10] create | kb-0009 | git push --force-with-lease refuses to overwrite unseen commits
## [2026-08-11] create | kb-0010 | A detached HEAD is a normal state, not data loss
## [2026-08-12] create | kb-0011 | git bisect run automates the search for a breaking commit
## [2026-08-13] create | kb-0012 | logind suspends on lid close unless HandleLidSwitch is changed
## [2026-08-14] create | kb-0013 | /usr/local/bin precedes /usr/bin in the default PATH
## [2026-08-15] create | kb-0014 | journalctl is volatile until Storage=persistent is set
## [2026-08-16] create | kb-0015 | sudo tty_tickets confines the sudo grace period to one terminal
## [2026-08-17] create | kb-0016 | consoleblank blanks the virtual console after N seconds
## [2026-08-18] create | kb-0017 | The docker group is root-equivalent on the host
## [2026-08-19] create | kb-0018 | Docker layer cache invalidates at the first changed instruction
## [2026-08-20] create | kb-0019 | Compose prefixes every resource with the project name
## [2026-08-01] create | kb-0020 | The latest tag is a moving pointer, not a version
## [2026-08-02] create | kb-0021 | nginx location blocks route by longest prefix match
## [2026-08-03] create | kb-0022 | Reverse proxies must forward Host and X-Forwarded-* headers
## [2026-08-04] create | kb-0023 | BM25 saturates term frequency — repetition stops helping
## [2026-08-05] create | kb-0024 | Reciprocal rank fusion merges rankings with 1/(k+rank)
## [2026-08-06] create | kb-0025 | A tokenizer that glues paths makes path-part queries miss
## [2026-08-07] create | kb-0026 | Golden tests pin outputs byte-for-byte and regenerate explicitly
## [2026-08-08] create | kb-0027 | Build throwaway sandboxes for CLI tests, never touch production data
## [2026-08-09] create | kb-0028 | Mutating a session-scoped fixture leaks state between tests
## [2026-08-10] create | kb-0029 | Large reference docs get a section-map companion note
## [2026-08-11] create | kb-0030 | A small always-readable index beats searching blind
## [2026-08-12] create | kb-0031 | Index summary lines over ~220 characters get truncated by readers
## [2026-08-13] create | kb-0032 | widget-factory project: a synthetic example project for fixtures
## [2026-08-14] create | kb-0033 | Staged proposals are promoted to notes by review, not by accident
## [2026-08-15] create | kb-0034 | Supersede wrong facts instead of deleting them
## [2026-08-16] create | kb-0035 | Search before creating — most new facts belong in an existing note
## [2026-08-17] create | kb-0036 | Prefer the distribution package manager for system installs
## [2026-08-18] create | kb-0037 | tmux detach keeps the session alive server-side
## [2026-08-19] create | kb-0038 | tmux clipboard needs OSC52 or set-clipboard for remote copies
## [2026-08-20] create | kb-0039 | Claude Code reads settings from ~/.claude/settings.json then project files
## [2026-08-01] create | kb-0040 | Hooks fire on tool events deterministically, unlike instructions
## [2026-08-02] create | kb-0041 | Cache-Control beats Expires when both are present
## [2026-08-03] create | kb-0042 | YAML 1.1 parses bare no as boolean false — the Norway problem
## [2026-08-04] create | kb-0043 | Nested quantifiers cause catastrophic regex backtracking
## [2026-08-05] create | kb-0044 | ssh config Host blocks apply first-match-wins per option
## [2026-08-06] create | kb-0045 | Agent forwarding exposes your keys to the remote host's root
## [2026-08-07] create | kb-0046 | Headings inside code fences are not headings
## [2026-08-08] create | kb-0047 | Stale __pycache__ rarely bites, but zip-safe eggs and mounts change mtime
## [2026-08-09] create | kb-0048 | git worktree checks out a second branch in a second directory
## [2026-08-10] create | kb-0049 | oom_score_adj biases which process the OOM killer picks
## [2026-08-11] create | kb-0050 | Multi-stage builds keep build tools out of the runtime image
## [2026-08-12] create | kb-0051 | The walrus operator binds in comprehensions without leaking the loop var
## [2026-08-13] create | kb-0052 | SQLITE_BUSY needs a busy_timeout, not retries
## [2026-08-14] create | kb-0053 | inotify watch limits break file watchers on big trees
## [2026-08-15] create | kb-0054 | git rerere records conflict resolutions and replays them
## [2026-08-16] create | kb-0055 | Fixture dates must be absolute and fixed
## [2026-08-17] create | kb-0056 | 429 responses carry Retry-After — honor it
## [2026-08-18] create | kb-0057 | YAML anchors and merge keys deduplicate mappings
## [2026-08-19] create | kb-0058 | \b matches between word and non-word characters
## [2026-08-20] create | kb-0059 | CHANGED host key means rebuild or MITM — verify out of band
## [2026-08-01] create | kb-0060 | SQLite deep-dive (deliberately oversized fixture note)
## [2026-08-02] create | kb-0061 | Git internals deep-dive (deliberately oversized fixture note)
## [2026-08-03] create | kb-0062 | Linux boot process deep-dive (deliberately oversized fixture note)
## [2026-08-04] create | kb-0063 | A verified stamp records when a claim was last confirmed
## [2026-08-05] create | kb-0064 | Conflicting notes resolve by status, then recency, then provenance
## [2026-08-06] create | kb-0065 | Fixture: verified status without a verified date (lint error)
## [2026-08-07] create | kb-0066 | Fixture: second verified-without-date note (lint error)
## [2026-08-08] create | kb-0067 | Fixture: note without triggers (lint warn)
## [2026-08-09] create | kb-0068 | Fixture: second note without triggers (lint warn)
## [2026-08-10] create | kb-0069 | Fixture: note whose summary exceeds the index-line budget
## [2026-08-11] create | kb-0070 | Typo correction fires only on a unique edit-distance-1 candidate
## [2026-08-12] create | kb-0071 | Idempotency keys make retried POSTs safe
## [2026-08-13] create | kb-0072 | Pipes inside markdown table cells need escaping
## [2026-08-14] create | kb-0073 | Truecolor terminals advertise via COLORTERM=truecolor
## [2026-08-15] create | kb-0074 | widget-factory session note: example session-log entry
## [2026-08-16] create | kb-0075 | sqlite internals guide: section map
## [2026-08-17] create | kb-0076 | pytest cookbook: section map
## [2026-08-18] create | kb-0077 | docker networking notes: section map
## [2026-08-19] create | kb-0078 | git workflows guide: section map
## [2026-08-20] create | kb-0079 | http caching survey: section map
## [2026-08-01] create | kb-0080 | linux permissions cheatsheet: section map
## [2026-08-02] create | kb-0081 | regex field guide: section map
## [2026-08-03] create | kb-0082 | markdown renderer notes: section map
## [2026-08-04] create | kb-0083 | search ranking survey: section map
## [2026-08-05] create | kb-0084 | terminal emulation notes: section map
## [2026-08-06] create | kb-0085 | yaml pitfalls guide: section map
## [2026-08-07] create | kb-0086 | ssh config guide: section map
