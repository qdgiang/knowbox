---
id: kb-0039
title: "Default git branch is main, never master"
type: decision
domain: preferences
tags: [git, default-branch, repository]
aliases: [default branch, master vs main, git init branch]
triggers:
  - "initializing a new git repository"
  - "running git init without -b main"
  - "renaming an existing master branch to main"
status: verified
confidence: high
created: 2026-08-12
updated: 2026-08-19
verified: 2026-08-12
supersedes: []
related: [kb-0002, kb-0028, kb-0007]
source: "session — explicit /kb instruction 2026-08-12"
summary: "Standing user rule: default git branch is 'main', never 'master'. Enforced via init.defaultBranch=main in ~/.gitconfig (chezmoi dot_gitconfig); 'git init -b main' also works."
---
The default branch name for every git repository on this machine is `main`, never `master`. This is a standing user preference set explicitly 2026-08-12; it applies to all new repositories and to any existing repo whose default branch is still `master`.

**How to apply:**
1. New repo — a bare `git init` now defaults to `main` (config-enforced 2026-08-12, see below). `git init -b main` is equivalent and still preferred: harmless here and portable to boxes without the config.
2. Existing repo still on `master` — `git branch -m master main` renames locally; then `git push -u origin main`, and update the remote default branch in the forge (GitHub/GitLab) before deleting the old `master` ref remotely.
3. Cloned/forked repos — leave their existing default branch as-is unless you are aligning a personal fork.

**Enforcement (set 2026-08-12):** `init.defaultBranch = main` now lives in `~/.gitconfig`, rendered from the chezmoi source `~/dotfiles/dot_gitconfig` ([[kb-0028]]). A fresh `git init` in an empty dir creates `main` (verified: `git var GIT_DEFAULT_BRANCH` → `main`). Before this was set, the config was unset and `git var GIT_DEFAULT_BRANCH` returned `master` under git 2.55.0 — keep the `git init -b main` form in muscle memory anyway: it is harmless here and works on any box regardless of config state.

**Why `master` can still happen despite `init.defaultBranch=main`:** the config only controls the local `git init` branch name — on an EMPTY GitHub repo the default branch is silently set by whichever branch is pushed first. Pushing `master` first makes GitHub's default `master`, with no warning; this is how qdgiang/personal-resume ended up master-default on 2026-08-12. Fix a wrong remote default: `git branch -m master main && git push -u origin main && gh repo edit --default-branch main && git push origin --delete master`.

## Related
- [[kb-0002]] — analogous standing user rule (dnf-only); same preferences domain
- [[kb-0028]] — dotfiles/chezmoi is where init.defaultBranch=main would live if enforced via config
- [[kb-0007]] — e2e-data-ai-project git workflow (identity caveat; orthogonal to branch naming)
