"""Protocol + docs alignment (hardening plan P11).

- Index flag: superseded entries render `⚠SUPERSEDED→[[successor]]` when a
  live note lists them in supersedes:, bare `⚠SUPERSEDED` otherwise — plus
  the no-successor lint WARN (dead ends stay visible).
- Doc-drift canaries: the machine's knowbox SKILL.md and AGENTS.md must teach
  the `knowbox supersede OLD --with NEW` recipe (both are chezmoi-managed;
  canaries read the APPLIED targets and skip when absent on other boxes). The
  old SKILL.md taught a divergent manual recipe — the exact drift P11 exists
  to close. Rename canary (kb-0142): no bare `kb <cmd>` invocations remain.
"""
from pathlib import Path

import pytest

from conftest import build_sandbox, kb_run, note_path


# --------------------------------------------------------------------------
# index successor flag + no-successor WARN
# --------------------------------------------------------------------------

def test_index_flag_points_at_successor(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    kb_run("reindex", root=root, expect_rc=0)
    idx = (root / "index.md").read_text()
    line = next(l for l in idx.splitlines() if "kb-9004" in l)
    assert "⚠SUPERSEDED→[[kb-9005]]" in line, line


def test_index_flag_bare_and_lint_warns_when_no_successor(tmp_path):
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=False)
    # strip kb-9005's supersedes link → kb-9004 becomes a dead end
    p5 = note_path(root, "kb-9005")
    p5.write_text(p5.read_text().replace("supersedes: [kb-9004]",
                                         "supersedes: []"))
    kb_run("reindex", root=root, expect_rc=0)
    idx = (root / "index.md").read_text()
    line = next(l for l in idx.splitlines() if "kb-9004" in l)
    assert "⚠SUPERSEDED" in line and "→[[" not in line, line
    r = kb_run("lint", root=root)
    out = r.stdout + r.stderr
    hits = [l for l in out.splitlines() if "no successor" in l]
    assert hits and hits[0].startswith("WARN") and "kb-9004" in hits[0], hits


def test_successor_link_survives_supersede_command(tmp_path):
    # CLI-created supersession must render the routed flag immediately
    root = build_sandbox(tmp_path / "kb", corpus="mini", git=True)
    r = kb_run("new", "--type", "fact", "--domain", "test",
               "--title", "successor note", "--body", "s\n",
               root=root, expect_rc=0)
    new_id = r.stdout.strip().split("/")[-1][:7]
    kb_run("supersede", "kb-9003", "--with", new_id, root=root, expect_rc=0)
    idx = (root / "index.md").read_text()
    line = next(l for l in idx.splitlines() if "kb-9003" in l)
    assert f"⚠SUPERSEDED→[[{new_id}]]" in line, line


# --------------------------------------------------------------------------
# doc-drift canaries (read the real machine's applied chezmoi targets)
# --------------------------------------------------------------------------

SKILL = Path.home() / ".claude" / "skills" / "knowbox" / "SKILL.md"
AGENTS = Path.home() / "AGENTS.md"


def _bare_kb_invocations(text):
    """`kb <word>` tokens that invoke the retired CLI — note ids (kb-NNNN),
    repo paths (~/kb, kb/), and frontmatter values never match."""
    import re
    cmds = ("search|expand|show|list|new|recent|touch|verify|supersede|embed|"
            "vsearch|reindex|lint|stale|churn|refs|conflicts|next-id|init")
    return re.findall(rf"(?<![\w/.-])kb (?:{cmds})\b", text)


@pytest.mark.skipif(not SKILL.exists(), reason="knowbox SKILL.md not on this box")
def test_skill_teaches_cli_supersede():
    assert "knowbox supersede OLD --with NEW" in SKILL.read_text(), \
        "SKILL.md drifted back to the manual supersede recipe (P11)"


@pytest.mark.skipif(not SKILL.exists(), reason="knowbox SKILL.md not on this box")
def test_skill_teaches_conflicts_judge_pass():
    text = SKILL.read_text()
    assert "knowbox conflicts --json" in text and "> verdict:" in text, \
        "SKILL.md lost the conflicts judge-pass procedure (P12/D5)"


@pytest.mark.skip(reason="kb-0142 Phase B5 gate — flips green when the "
                         "dotfiles rename pass lands knowbox wording in "
                         "AGENTS.md")
def test_agents_md_names_cli_supersede():
    assert "knowbox supersede OLD --with NEW" in AGENTS.read_text(), \
        "AGENTS.md write-protocol line doesn't name the CLI command (P11)"


@pytest.mark.skip(reason="kb-0142 Phase B5 gate — flips green when the "
                         "dotfiles rename pass lands knowbox wording in "
                         "SKILL.md/AGENTS.md")
def test_no_bare_kb_invocations_remain():
    # kb-0142 rename canary: bare `kb <cmd>` = the retired command name
    found = {}
    for name, path in (("SKILL.md", SKILL), ("AGENTS.md", AGENTS)):
        if path.exists():
            hits = _bare_kb_invocations(path.read_text())
            if hits:
                found[name] = hits
    assert not found, f"bare `kb` invocations survived the rename: {found}"
