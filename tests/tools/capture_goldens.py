#!/usr/bin/env python3
"""Regenerate tests/goldens/* — the ONLY sanctioned way to change goldens.

Run from the repo root:  uv run python tests/tools/capture_goldens.py
Then: `git diff tests/goldens/` — the diff MUST be reviewed and committed
together with the behavior change that caused it (plan §0.6).
"""
import os
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent.parent

if __name__ == "__main__":
    env = {**os.environ, "KB_GOLDENS_REGENERATE": "1"}
    uv = shutil.which("uv")
    if uv is None:
        sys.exit("uv not found on PATH — install uv first (https://docs.astral.sh/uv/)")
    sys.exit(subprocess.call([uv, "run", "pytest", "-q", "-k", "golden"],
                             cwd=str(REPO), env=env))
