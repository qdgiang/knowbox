---
id: kb-0029
title: "Claude Code pins shell cwd to the session dir — bridge directory renames with a symlink"
type: gotcha
domain: tooling
tags: [claude-code, bash, rename]
aliases: [cwd pinning, shell cwd reset]
status: active
confidence: high
created: 2026-08-08
updated: 2026-08-08
supersedes: []
related: [kb-0028]
source: "session 2026-08-08: ~/server-ops → ~/dotfiles rename executed from inside the session"
summary: "Claude Code resets every Bash-tool shell back to the session's launch dir, so renaming/moving that dir mid-session risks stalling all Bash calls — do mv + symlink bridge, update refs, remove the bridge last."
triggers:
  - "renaming or moving a directory a Claude Code session was launched from"
  - "shell cwd keeps getting reset to the project directory in Claude Code"
---
Claude Code pins every Bash-tool shell's cwd to the session's launch directory: `cd` outside it gets reverted ("Shell cwd was reset to …"), so `mv`-ing that directory mid-session can stall every later Bash call (chdir fails on the vanished path).

Observed 2026-08-08 during the `~/server-ops` → `~/dotfiles` rename:
- `cd ~` and `cd /tmp` were both reverted to the session dir.
- Running the `mv` with the shell sitting INSIDE the moved dir did not kill the shell (Unix cwd follows the inode) — the hazard is the pinned path string used to launch the NEXT shell.

Safe rename procedure (verified in that session):
1. `mv ~/old ~/new && ln -s new ~/old` — the symlink bridge keeps the pinned path resolving.
2. Update all references (configs, crontab, manager sourceDir) and verify, bridge in place.
3. `rm ~/old` only at the end, then run a canary command; if the session shell stalls after removal, restore from a normal terminal: `ln -s new ~/old`.

In the observed rename the pin followed the move (reset messages afterwards showed the new path), but keep the bridge until you have verified.

Related: Claude Code keys transcripts per launch path — sessions from `~/dotfiles` log to `~/.claude/projects/-home-giang-dotfiles/`; the old path's transcripts stay archived under the old-named directory.

## Related
- [[kb-0028]] — the dotfiles repo this rename concerned
