#!/usr/bin/env python3
"""Regenerate tests/goldens/* — the ONLY sanctioned way to change goldens.

Run from anywhere:  uv run --project ~/projects/knowbox python ~/projects/knowbox/tests/tools/capture_goldens.py
Then: `git -C ~/projects/knowbox diff tests/goldens/` — the diff MUST be reviewed and
committed together with the behavior change that caused it (plan §0.6).
"""
import os
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent.parent

if __name__ == "__main__":
    env = {**os.environ, "KB_GOLDENS_REGENERATE": "1"}
    uv = shutil.which("uv") or "/home/giang/.local/bin/uv"
    sys.exit(subprocess.call([uv, "run", "pytest", "-q", "-k", "golden"],
                             cwd=str(REPO), env=env))
