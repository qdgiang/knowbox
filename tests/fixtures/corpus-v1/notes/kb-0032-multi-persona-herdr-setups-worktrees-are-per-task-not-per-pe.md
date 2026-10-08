---
id: kb-0032
title: "Multi-persona herdr setups: worktrees are per-task, not per-persona; sync deferred to merge time"
type: howto
domain: tooling
tags: [herdr, git-worktree, multi-agent, orchestration]
aliases: [worktree staleness, updating worktrees across tabs, worktree per tab]
triggers:
  - "worrying about stale worktrees when running multiple herdr agents/personas"
  - "deciding how many git worktrees a multi-persona setup needs"
status: active
confidence: medium
created: 2026-08-09
updated: 2026-08-09
supersedes: []
related: [kb-0008, kb-0031]
source: "session 2026-08-09 (reasoning over refs/herdr-guide.md §11 + deep-research workflow; pattern-level, not doc-verified)"
summary: "Herdr multi-persona hygiene: worktree per TASK not per persona (reviewer/orchestrator share the worker's), create→task→PR→remove lifecycle, branch sync deferred to merge time, pick non-overlapping tasks."
---
"Doesn't every tab's worktree need re-syncing after each task?" — no, if the setup follows three rules:

1. **Personas ≠ worktrees.** Only parallel *workers* get their own worktree. The reviewer attaches to the worker's worktree (it must see exactly the diff produced); the orchestrator only prompts other agents and touches no code. "Update all tabs" reduces to "update N ongoing worker branches."
2. **Worktrees are task-scoped, not long-lived.** PR-factory lifecycle: `herdr worktree create --branch feat/x --base origin/main` → agent does the one task → PR reviewed + merged → `herdr worktree remove --workspace w3`. Nothing goes stale because nothing lives long enough to. Next task = fresh worktree off current main. Per-task tedium is one create; plugins automate even that (`herdr-plugin-gh-workflow`, `herd-manager`, `herdr-worktree-lifecycle`).
3. **Sync is deferred to merge time, not per task.** A worktree is just a branch checkout — being behind main costs nothing until merge: `git rebase origin/main` right before the PR. If a whole-fleet sync is ever wanted it's a script over `git worktree list`, but the real fix is organizational: pick **non-overlapping tasks** and keep them short so branches live hours, not days.

Why staleness is survivable: agents botch big merge conflicts anyway ([[kb-0008]] §11: "often ends up dropping a lot of code") — the design goal is to structure work so merges are trivial or nonexistent, not to keep everything continuously synced.

## Related
- [[kb-0008]] — herdr guide map: §11B worktree commands, §11D PR factories, parallelism hygiene
- [[kb-0031]] — orchestrator landscape; its "one worktree per agent" line means per parallel worker, refined here
