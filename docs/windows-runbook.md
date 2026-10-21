# Windows runbook — knowbox CLI + a fresh, notes-only KB

Goal: the knowbox CLI on a Windows machine with its **own fresh empty KB**.
The server's private KB repo **never lands on
this machine** — that separation is the whole reason this repo exists.
Everything below is PowerShell unless noted.

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
knowbox init                 # skeleton at %USERPROFILE%\knowbox
```

- Local disk only, **never OneDrive** (SQLite locking breaks).
- Custom location: `knowbox init D:\knowbox` then
  `setx KNOWBOX_ROOT D:\knowbox` (new terminal).
- `init` creates `notes/ refs/ raw/`, a SCHEMA stub, LF-pinned
  `.gitattributes`, a git repo with repo-local identity, no remote.

## 5. Verify

```powershell
knowbox lint                 # 0 errors (NT: never-present heartbeat auto-exempt)
knowbox new --type fact --domain windows --title "first note" --no-edit --body "…"
knowbox search first
knowbox reindex              # "no changes"
git -C $HOME\knowbox log --oneline  # init + first-note commits, LOCAL ONLY
```

## 6. Optional: private backup remote

Point `origin` at a SEPARATE private repo (e.g. `<you>/knowbox-windows`) —
this activates the CLI's auto-push:

```powershell
gh repo create <you>/knowbox-windows --private
git -C $HOME\knowbox remote add origin https://github.com/<you>/knowbox-windows.git
git -C $HOME\knowbox push -u origin main
```

**NEVER add the server's KB repo as a remote** — not even read-only.
The server notes must not be fetchable from this machine.

## 7. Optional: semantic search (embed/vsearch/fusion)

Point the CLI at your AI provider's embeddings endpoint and set its token:

```powershell
setx KNOWBOX_EMBED_URL <endpoint>   # e.g. https://<provider>/v1/embeddings
setx KNOWBOX_API_KEY <token>        # new terminal
knowbox embed
```

## 8. Optional: Claude Code harness files

Hand-place (nothing syncs them here):

- `%USERPROFILE%\.claude\skills\knowbox\SKILL.md` — copy it from the machine
  where you maintain your Claude config.
- `%USERPROFILE%\.claude\CLAUDE.md` — your main version, minus any `@import`
  lines for files that only exist there.
- An adapted `AGENTS.md` with server-only rules scoped out (package-manager
  rules, crontab, janitor, containers).

No crontab/janitor here: the server keeps the single-janitor role; on native
Windows the missing heartbeat is auto-exempt, no env var needed.
