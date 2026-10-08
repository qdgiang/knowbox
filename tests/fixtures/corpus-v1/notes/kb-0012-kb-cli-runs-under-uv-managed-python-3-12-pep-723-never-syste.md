---
id: kb-0012
title: "kb CLI runs under uv-managed Python 3.12 (PEP 723) — never system python"
type: decision
domain: kb
tags: [uv,python,kb-cli]
aliases: [why does kb use uv, kb shebang, uv run --script]
triggers:
  - "modifying the kb CLI shebang or dependencies"
  - "kb fails with python errors"
  - "adding a python dependency to kb"
status: verified
confidence: high
created: 2026-08-02
updated: 2026-08-11
supersedes: []
related: [kb-0001, kb-0002, kb-0015]
source: "user directive 2026-08-02: use uv, never modify system python"
summary: "bin/kb is a PEP 723 script: shebang 'uv run --quiet --python 3.12 --script', zero deps. Resolves to ~/.local/share/uv/python. User rule: system python must never be modified."
---
`~/kb/bin/kb` executes via uv, not system python, by explicit user directive (2026-08-02): "switch to using uv instead of system python. I do not want you to modify anything related to system python."

## Mechanism

PEP 723 inline metadata — no venv, no package install, the script is self-describing:

```python
#!/usr/bin/env -S uv run --quiet --python 3.12 --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
```

`--python 3.12` resolves to the uv-managed build at `~/.local/share/uv/python/cpython-3.12-linux-x86_64-gnu/` (system python here is 3.14 only, so it can never be selected). Startup overhead measured ~40ms — negligible.

## Rules that follow

- **Never modify system python** — the general rule (no dnf python packages, no pip, nothing under /usr/lib/python*) is canonical in [[kb-0015]]; kb obeys it via the shebang above.
- If kb ever needs a dependency: add it to the `dependencies` list in the PEP 723 block — uv creates and caches an isolated script environment automatically. Still nothing touches system python.
- uv itself was already installed at `~/.local/bin/uv` (0.12.1); had it been missing, the install route would be `sudo dnf install uv` per [[kb-0002]].

## Related
- [[kb-0002]] — the install rule uv respects (it was pre-installed; no action needed)
- [[kb-0001]] — KB design (stdlib-only tooling principle; uv adds zero dependencies)
