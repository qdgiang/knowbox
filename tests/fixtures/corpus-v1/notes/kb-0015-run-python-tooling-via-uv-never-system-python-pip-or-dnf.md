---
id: kb-0015
title: "Run Python tooling via uv — never system python, pip, or dnf; no npx/npm either"
type: howto
domain: python
tags: [uv, python, pip, fonttools, npx, npm]
aliases: [pip, python package, fonttools, "sudo dnf install python3", npx, "npm install -g"]
triggers:
  - "needing a Python tool or package (fonttools, matplotlib, any pypi package) on this box"
  - "about to suggest `sudo dnf install python3-*` or `pip install`"
  - "about to suggest `npx <tool>` or `npm install -g`"
status: verified
confidence: high
created: 2026-08-03
updated: 2026-08-22
supersedes: []
related: [kb-0012, kb-0002]
source: "session: personal-resume font instancing, 2026-08-03; extended 2026-08-16 env-audit"
summary: "Run ALL Python tooling via uv (uvx / uv run --with / uv venv) — never system pip, never dnf python3-* packages, never /usr/bin/python3 itself; no npx/npm globals either."
---
On this box, every Python tool or package runs through `uv` (`~/.local/bin/uv`, v0.12+). Never install Python packages via `sudo dnf install python3-*` and never use system `pip install` — system python must not be modified (see [[kb-0012]]). Never run code with `/usr/bin/python3` itself either, not even one-off scripts — it is OS infrastructure (dnf, cockpit, firewalld, selinux bindings). `dnf` is still the rule for non-Python software; Python packages are the uv exception.

**Patterns:**
- Ephemeral one-off tool / script: `uvx <tool>` or `uv run --with <pkg> python script.py`. uv builds an isolated env under `~/.cache/uv`; nothing touches system python.
- Project deps: `uv venv --python 3.12` then `uv pip install <pkg>` (or `uv add` / `uv sync` from a pyproject).
- Pin a Python version: `uv run --python 3.12 ...` / `uv venv --python 3.12`.

**Evidence (2026-08-03, personal-resume):** needed `fonttools` to instance variable TTFs into static weights for a XeLaTeX CV. Correct call: `uv run --with fonttools python instance_fonts.py`. The earlier reflex to suggest `sudo dnf install python3-fonttools` was corrected by the user — "use uv, not system python."

**Gotcha in fonttools 4.63:** the instancer moved — import from `fontTools.varLib.instancer` (`from fontTools.varLib.instancer import instantiateVariableFont`), not the old top-level `fontTools.instancer` (which now raises `ModuleNotFoundError`). `uv run --with fonttools python -c "..."` is the fast way to confirm an import path.

**Node/JS and other dev tooling (added 2026-08-16):** the "nothing into the machine's main env" rule generalizes beyond Python. Never `npx` — it silently downloads into `~/.npm` caches; never `npm install -g` (no nodejs/npm rpm is installed here at all). If a Node or other dev tool is needed: prefer a dnf package (see [[kb-0002]]); if none exists, ask the user before installing anything. Env audit 2026-08-16 confirmed system python was pip-clean (no pip module, no user site-packages, all site-packages rpm-owned); the stale 76MB `~/.npm/_cacache` from past npx use was purged.

## Related
- [[kb-0012]] — the kb CLI itself runs under uv-managed python 3.12 (PEP 723); same "never system python" rule, scoped there to the kb shebang. This note generalizes it to all Python tooling.
- [[kb-0002]] — install-only-via-dnf for non-Python software; npx/npm-globals and dropped binaries violate it.
