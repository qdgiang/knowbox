---
id: kb-0068
title: "go-task runs Taskfile sh snippets in its built-in shell — leading '!' pipelines fail; use case statements or explicit sh -c"
type: gotcha
domain: tooling
tags: [go-task, taskfile, shell]
aliases: []
status: active
confidence: medium
created: 2026-08-22
updated: 2026-08-22
supersedes: []
related: [kb-0059]
source: ""
summary: "Taskfile cmd/precondition sh snippets run under go-task's embedded shell, not /bin/sh — POSIX-negation pipelines can fail there; write case statements or wrap in sh -c."
---
go-task (task 3.52.0) executes Taskfile `cmds`/`preconditions` sh snippets
with its built-in shell interpreter, not `/bin/sh` — constructs that work in a
real shell can misbehave there.

Hit 2026-08-22 adding a precondition guard to ~/projects/lean-mds
Taskfile.yml. The snippet

    sh: '! echo ",${COMPOSE_PROFILES:-}," | grep -Eq "prefect.*dagster|dagster.*prefect"'

worked perfectly in a standalone `sh -c` (exit 0 when no match), but as a
Taskfile precondition it always reported "precondition not met", while
`sh: 'true'` passed. Debugging was opaque because go-task **captures
precondition stdout/stderr** — diagnostic echoes never surface.

Fixes that work:
- a `case` statement instead of negation (chosen; also more readable):
  ```yaml
  - sh: |
      case ",${COMPOSE_PROFILES:-}," in
        *prefect*dagster* | *dagster*prefect*) echo reason >&2; exit 1 ;;
      esac
  ```
- wrapping the logic in an explicit `sh -c '...'` command (that runs a real
  shell, where the original one-liner worked).

Debugging tip: add a temporary task whose cmd echoes the variable in question
(preconditions won't show you anything).

## Related
- [[kb-0059]] — other Taskfile/compose teardown traps encoded the same way
