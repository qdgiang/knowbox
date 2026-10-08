---
id: kb-0055
title: "kb CLI writes (new/touch/reindex) auto-commit AND auto-push when a remote exists — the AGENTS.md commit exception also covers pushes"
type: gotcha
domain: kb
tags: [kb-cli,git,auto-commit,auto-push]
aliases: [kb reindex pushes, kb auto-push, why is git status clean after reindex]
triggers:
  - "hand-editing notes in ~/kb then running kb reindex"
  - "surprised by a clean git status or unexpected commits in ~/kb"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0012, kb-0014]
source: "raw/session-distill-2026-08-10.md"
summary: "kb new/touch/reindex commit note edits themselves and push to the KB remote — the AGENTS.md kb-CLI commit exception covers pushes too; check git log, not status, to see what shipped."
---
The kb CLI's write paths — `kb new`, `kb touch`, `kb reindex` — auto-commit all note edits AND auto-push them when a git remote exists; no user git action is needed, and edits are public on GitHub immediately. The documented kb-CLI commit exception in AGENTS.md ("never commit or push unless explicitly asked — the kb CLI's auto-commits are the documented exception") therefore covers pushes too, not just commits.

Observed 2026-08-09 while removing openclaw from KB notes: after hand-editing notes and running `kb reindex`, `git -C ~/kb status` came back unexpectedly empty. Investigation showed the tool itself had regenerated `index.md` + FTS, committed the edits (commit `8024a16`), and pushed to the remote — working tree clean, edits already shipped.

Know this before hand-editing notes:

- Assume any `kb new`/`touch`/`reindex` ships the whole working tree to the remote. Don't stage experiments in note files planning to discard them later.
- After reindexing, check `git -C ~/kb log -1` to see what the tool already shipped — a clean status means it already went out, not that nothing changed.
- Undo is still git (revert), but since the push already happened, revert with a new commit rather than rewriting pushed history.
- This is the behavior behind SCHEMA.md's "Git is the undo button: the CLI auto-commits."

## Related
- [[kb-0012]] — the kb CLI itself (PEP 723 script under uv) whose write-path behavior this describes
- [[kb-0014]] — the weekly janitor cron also invokes kb commands; its runs go through the same auto-commit/push path
