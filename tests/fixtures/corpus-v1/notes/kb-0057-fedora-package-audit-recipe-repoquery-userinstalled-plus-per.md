---
id: kb-0057
title: "Fedora package-audit recipe: repoquery --userinstalled plus per-file rpm -qf scans expose redundant and non-rpm-owned packages on a pip-clean box"
type: howto
domain: fedora
tags: [dnf,repoquery,rpm,audit,cleanup]
aliases: [package audit, stray packages, unowned files, repoquery userinstalled]
triggers:
  - "auditing fedora-server for redundant or stray packages"
  - "finding non-rpm-owned files under /usr/local or site-packages"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0002, kb-0003]
source: "raw/session-distill-2026-08-16.md"
summary: "Recipe to audit a Fedora box for redundant packages and non-rpm strays: repoquery --userinstalled, --whorequires leaf-proof, per-file rpm -qf walk, sudo -n precondition."
---
Recipe for auditing whether a Fedora system env holds redundant or non-rpm-owned packages (built during the 2026-08-16 fedora-server env audit):

1. **Explicitly installed** — `dnf repoquery --userinstalled --qf '%{name}' 'python3*'` separates deliberately installed packages from pulled-in deps; on fedora-server it surfaced only python3-file-magic, python3-lib389, python3-psutil out of ~80 installed python packages.
2. **Prove a leaf** — `dnf repoquery --installed --whorequires <pkg> --qf '%{name}'` (or `rpm -q --whatrequires <pkg>`); an empty result plus no references in `~/.local/bin` scripts or `~/kb` marks a genuine redundant candidate (this is how psutil was flagged).
3. **Detect non-rpm strays** — walk a directory per-file: `find <dir> -maxdepth 1 | tail -n +3 | while read f; do rpm -qf "$f" >/dev/null 2>&1 || echo "UNOWNED: $f"; done`. `rpm -qf /usr/local/bin/kubectl` → "not owned by any package" exposed a static-binary violation of the dnf-only rule.
4. **Preconditions** — check `sudo -n true` before planning root removals (passwordless sudo is absent on this box, so the user must run removals). Only remove rpm packages whose reverse-dependency chain is empty: most site-packages content is OS infrastructure (dnf, cockpit, firewalld, selinux) that must stay.

## Related
- [[kb-0002]] — the dnf-only install rule this audit enforces; unowned binaries like kubectl are its violations
- [[kb-0003]] — fedora-server, the machine this recipe was built and verified on
