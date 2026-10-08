# Windows runbook — knowbox CLI + a fresh, notes-only KB

Goal: the knowbox CLI on a Windows machine with its **own fresh empty KB**.
The server KB (`qdgiang/kb`, private notes) **never lands on this machine** —
that separation is the whole reason this repo exists (server-side note
kb-0144). Everything below is PowerShell unless noted.

## 1. Git

```powershell
winget install --id Git.Git -e
```

Open a NEW terminal. First `git`/uv access to the private repo triggers
Git Credential Manager browser auth (sign in as the GitHub account with
access to `qdgiang/knowbox`).

## 2. uv

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
uv tool update-shell
```

New terminal again (PATH refresh).

## 3. Install the CLI

```powershell
uv tool install git+https://github.com/qdgiang/knowbox
uv tool list                 # knowbox v1.x.x
knowbox version
where.exe knowbox            # %USERPROFILE%\.local\bin\knowbox.exe
```

Update later with:

```powershell
uv tool install --reinstall git+https://github.com/qdgiang/knowbox
```

## 4. Create the fresh KB

```powershell
knowbox init                 # skeleton at %USERPROFILE%\kb
```

- Local disk only, **never OneDrive** (SQLite locking breaks).
- Custom location: `knowbox init D:\kb` then
  `setx KNOWBOX_ROOT D:\kb` (new terminal).
- `init` creates `notes/ refs/ raw/`, a SCHEMA stub, LF-pinned
  `.gitattributes`, a git repo with repo-local identity, no remote.

## 5. Verify

```powershell
knowbox lint                 # 0 errors (NT: never-present heartbeat auto-exempt)
knowbox new --type fact --domain windows --title "first note" --no-edit --body "…"
knowbox search first
knowbox reindex              # "no changes"
git -C $HOME\kb log --oneline  # init + first-note commits, LOCAL ONLY
```

## 6. Optional: private backup remote

Point `origin` at a SEPARATE private repo (e.g. `qdgiang/kb-windows`) —
this activates the CLI's auto-push:

```powershell
gh repo create qdgiang/kb-windows --private
git -C $HOME\kb remote add origin https://github.com/qdgiang/kb-windows.git
git -C $HOME\kb push -u origin main
```

**NEVER add `qdgiang/kb` as a remote** — not even read-only. The server notes
must not be fetchable from this machine.

## 7. Optional: semantic search (embed/vsearch/fusion)

Uses the same ai-box endpoint as the server; auth is the token from
`%USERPROFILE%\.claude\settings.json` (`env.ANTHROPIC_AUTH_TOKEN`) — free if
Claude Code is installed — or:

```powershell
setx ANTHROPIC_AUTH_TOKEN <token>   # new terminal
knowbox embed
```

## 8. Optional: Claude Code harness files

Hand-place (chezmoi doesn't run here):

- `%USERPROFILE%\.claude\skills\knowbox\SKILL.md` — from the server dotfiles
  (`dot_claude/skills/knowbox/SKILL.md`).
- `%USERPROFILE%\.claude\CLAUDE.md` — server version, minus the `@~/AGENTS.md`
  import line.
- An adapted `AGENTS.md` with server-only rules scoped out (dnf, dotfiles,
  crontab, janitor, containers).

No crontab/janitor here: the Fedora server keeps the single-janitor role
(kb-0014); on native Windows the missing heartbeat is auto-exempt, no env var
needed.
