"""Crontab regression test (audit finding H1/T1).

The Monday janitor line died because cronie truncates a command at the first
BARE % (the printf format string had 7 unescaped %s). Every % in every command
field of the crontab source of truth must be backslash-escaped — or better,
the logic lives in a script file with zero % in the crontab (the P2 fix).
"""
from pathlib import Path

import pytest

CRONTAB = Path.home() / "dotfiles" / "scripts" / "crontab.txt"


@pytest.mark.skipif(not CRONTAB.exists(),
                    reason="crontab source of truth not on this box")
def test_all_crontab_percents_escaped():
    # T1 flipped green in P2 (2026-09-06): the janitor logic moved into
    # bin/kb-janitor.sh, so the crontab command fields contain zero %.
    bad = []
    for line in CRONTAB.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = line.split(None, 5)   # 5 schedule fields + command
        if len(parts) < 6:
            continue
        cmd = parts[5]
        for i, ch in enumerate(cmd):
            if ch == "%" and (i == 0 or cmd[i - 1] != "\\"):
                bad.append(" ".join(parts[:5]))
                break
    assert not bad, f"bare % in crontab command field(s): {bad}"


def test_crontab_knowbox_paths_canary():
    # kb-0142 split canary: the janitor line must run the DOTFILES script with
    # the knowbox binary; nothing may still point at ~/kb/bin (the data repo
    # stops carrying the CLI in Phase C).
    text = CRONTAB.read_text()
    active = [l for l in text.splitlines()
              if l.strip() and not l.lstrip().startswith("#")]
    janitor = [l for l in active if "knowbox-janitor" in l]
    embed = [l for l in active if " embed" in l]
    assert janitor, "no janitor line in crontab"
    assert any("dotfiles/scripts/knowbox-janitor.sh" in l for l in janitor), janitor
    assert all("/kb/bin/" not in l for l in active), \
        f"crontab still references ~/kb/bin: {active}"
    if embed:
        assert any(".local/bin/knowbox" in l for l in embed), embed
