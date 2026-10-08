---
id: kb-0056
title: "Headless KB note creation: EDITOR=true kb new --no-edit skips the interactive editor and leaves a stub to fill by file edit"
type: howto
domain: kb
tags: [kb-cli,headless,editor,agentic-workflow]
aliases: [kb new no editor, EDITOR=true, scripted note creation, headless kb new]
triggers:
  - "creating a KB note from a headless or agentic shell"
  - "kb new stalls waiting for an editor"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0012, kb-0055]
source: "raw/session-distill-2026-08-14.md"
summary: "Agentic note creation: EDITOR=true kb new --type … --no-edit (or --body-stdin) yields a frontmatter-complete stub with no interactive editor; finish via file edit + touch/reindex/lint."
---
To create a KB note from a headless or agentic shell, run `EDITOR=true kb new --type … --domain … --title "…" --no-edit` (or `--body-stdin`) — the stub is created without an interactive editor pass. A plain `kb new` stalls waiting in the user's editor and is unusable from agent sessions; this is the agentic workflow for kb note creation.

Mechanism: after writing frontmatter the CLI shells out to `$EDITOR` for the body pass; pointing it at `true` makes that pass succeed instantly, leaving a frontmatter-complete/empty-body note to be filled afterwards. Verified 2026-08-14 while creating kb-0040: `grep -n "EDITOR\|subprocess\|input(" ~/kb/bin/kb` shows the editor step. kb honors `$EDITOR` including arguments (e.g. `EDITOR='code -w'`), per the kb-0038 hardening line.

Fill-in sequence after the stub exists:

1. Write the body by editing the note file directly (plain markdown under `~/kb/notes/`).
2. `kb touch kb-XXXX` to bump `updated`.
3. `kb reindex` to regenerate `index.md` + FTS (beware: this auto-commits and auto-pushes — see [[kb-0055]]).
4. `kb lint` to confirm the note is structurally clean.

The same trick generalizes to any CLI that shells out to `$EDITOR` for composition: `EDITOR=true` converts the interactive pass into a no-op so scripted flows proceed.

## Related
- [[kb-0012]] — the kb CLI (PEP 723 script under uv) whose editor step this bypasses
