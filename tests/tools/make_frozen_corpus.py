#!/usr/bin/env python3
"""Generate tests/fixtures/corpus-v1/ — a SYNTHETIC frozen corpus.

The frozen corpus exists so golden tests (search ranking, index build, lint
report) have a fixed, non-trivial corpus to pin against. It is generated, not
hand-written: every note is generic public dev knowledge, deterministic dates,
no personal content. Never freeze a live KB into fixtures.

Usage: uv run python tests/tools/make_frozen_corpus.py
Then regenerate goldens: uv run python tests/tools/capture_goldens.py
"""
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "fixtures" / "corpus-v1"

# ---------------------------------------------------------------------------
# content notes: (id-suffix, domain, type, slug, title, [body points])
# First point = first sentence = the claim. Bodies stay generic on purpose.
# ---------------------------------------------------------------------------
NOTES = [
    (1, "sqlite", "fact", "wal-mode-journal",
     "SQLite WAL mode keeps readers unblocked during writes",
     ["WAL mode appends to a -wal file so readers never block behind a writer.",
      "Checkpointing folds the wal back into the main db; it runs automatically every 1000 pages.",
      "Readers see the last checkpointed state, so long reads can stall checkpointing."]),
    (2, "sqlite", "gotcha", "fts5-bm25-ranking",
     "FTS5 bm25() returns negative scores — more negative is better",
     ["In SQLite FTS5, bm25() scores are negative and SMALLER (more negative) means a better match.",
      "ORDER BY bm25(tbl) ascending gives best-first ranking; forgetting this inverts results.",
      "Column weights via bm25(tbl, 10, 1) boost title-like columns over body columns."]),
    (3, "sqlite", "howto", "fts5-external-content",
     "FTS5 external-content tables must be rebuilt after bulk changes",
     ["An external-content FTS5 table does not track the source table; rebuild it after bulk edits.",
      "INSERT INTO ft(ft) VALUES('rebuild') is the canonical rebuild.",
      "Contentless delete tables trade a small index for not storing the text twice."]),
    (4, "python", "fact", "venv-before-install",
     "Create the virtualenv before installing anything into it",
     ["A venv only isolates packages installed after its creation.",
      "uv venv then uv pip install keeps the system interpreter untouched.",
      "Activating a venv only rewrites PATH; installs go by which pip runs."]),
    (5, "python", "gotcha", "mutable-default-argument",
     "Python default arguments are evaluated once at def time",
     ["def f(x=[]) shares one list across all calls — the classic mutable-default trap.",
      "Use None as the sentinel and create the list inside the function body.",
      "The same trap applies to dict and set defaults, and to datetime.now() defaults."]),
    (6, "python", "howto", "pytest-fixture-scopes",
     "pytest fixture scope decides how often setup runs",
     ["pytest fixture scope=function reruns setup per test; scope=session runs it once per run.",
      "Session-scoped fixtures must stay read-only or tests leak state into each other.",
      "tmp_path_factory is the session-scoped counterpart of tmp_path."]),
    (7, "python", "fact", "gil-release-c-extensions",
     "C extensions can release the GIL; pure Python cannot",
     ["NumPy-style C extensions release the GIL around heavy loops, so threads help there.",
      "Pure-Python threads never run bytecode in parallel — use processes for CPU work.",
      "The GIL protects refcounts; free-threaded builds replace that with biased locking."]),
    (8, "git", "howto", "rebase-onto",
     "git rebase --onto moves a branch stack without replaying everything",
     ["git rebase --onto NEW OLD BRANCH replays commits after OLD onto NEW.",
      "This is the fix when a feature branch was cut from another feature branch.",
      "The old branch tip is untouched; only BRANCH moves."]),
    (9, "git", "gotcha", "force-with-lease",
     "git push --force-with-lease refuses to overwrite unseen commits",
     ["--force-with-lease fails the push if the remote ref moved since your last fetch.",
      "Plain --force silently clobbers a teammate's push; lease makes the clobber explicit.",
      "It still overwrites — just only the commits you have actually seen."]),
    (10, "git", "fact", "detached-head-is-fine",
     "A detached HEAD is a normal state, not data loss",
     ["Detached HEAD means HEAD points at a commit, not a branch — commits are safe.",
      "Create a branch before leaving the commit and nothing is lost.",
      "git reflog finds orphaned commits for ~90 days by default."]),
    (11, "git", "howto", "bisect-run",
     "git bisect run automates the search for a breaking commit",
     ["git bisect run <script> binary-searches history with the script's exit code as verdict.",
      "Exit 0 = good, 1 = bad, 125 = skip (untestable commit like a broken build).",
      "Start with git bisect start, mark known bad and good endpoints first."]),
    (12, "linux", "gotcha", "logind-lid-close-sleep",
     "logind suspends on lid close unless HandleLidSwitch is changed",
     ["systemd-logind suspends a laptop on lid close by default via HandleLidSwitch=suspend.",
      "Set HandleLidSwitch=ignore in logind.conf for headless lid-closed operation.",
      "Docked and on-external-power variants are separate keys: HandleLidSwitchExternalPower."]),
    (13, "linux", "fact", "usr-local-bin-path-order",
     "/usr/local/bin precedes /usr/bin in the default PATH",
     ["The default PATH puts /usr/local/bin before /usr/bin, so local installs shadow system ones.",
      "which -a shows every match in resolution order.",
      "Package managers own /usr/bin; /usr/local is the admin's namespace — no package writes there."]),
    (14, "linux", "howto", "journalctl-persistent",
     "journalctl is volatile until Storage=persistent is set",
     ["The systemd journal lives in /run (RAM) unless Storage=persistent in journald.conf.",
      "With persistence, logs land in /var/log/journal and survive reboots.",
      "journalctl --disk-usage and --vacuum-time=2weeks manage the footprint."]),
    (15, "linux", "gotcha", "sudo-tty-tickets",
     "sudo tty_tickets confines the sudo grace period to one terminal",
     ["With tty_tickets (default), sudo's timestamp is per-tty — a second terminal re-prompts.",
      "Disabling tty_tickets makes the grace period global across sessions.",
      "timestamp_timeout=0 makes sudo prompt every time."]),
    (16, "linux", "fact", "consoleblank-kernel-param",
     "consoleblank blanks the virtual console after N seconds",
     ["The consoleblank kernel parameter blanks text consoles after the given seconds; 0 disables.",
      "It only affects virtual consoles, not graphical sessions.",
      "setterm -blank N does the same from userspace for the current console."]),
    (17, "docker", "gotcha", "docker-group-root-equivalent",
     "The docker group is root-equivalent on the host",
     ["Docker group membership is root-equivalent: -v /:/host gives a container the whole filesystem.",
      "There is no permission boundary between the docker socket and host root.",
      "Rootless docker or sudo-only access avoids the silent escalation path."]),
    (18, "docker", "howto", "docker-layer-cache",
     "Docker layer cache invalidates at the first changed instruction",
     ["Every instruction after the first changed one rebuilds — order Dockerfile steps by change frequency.",
      "COPY package files before source so dependency installs stay cached.",
      "BuildKit cache mounts (RUN --mount=type=cache) survive invalidation for package downloads."]),
    (19, "docker", "fact", "compose-project-name",
     "Compose prefixes every resource with the project name",
     ["docker compose namespaces containers, networks, and volumes as <project>_<name>.",
      "The project name defaults to the directory name; -p or COMPOSE_PROJECT_NAME overrides.",
      "Two checkouts of one repo collide unless the project name differs."]),
    (20, "docker", "gotcha", "latest-tag-moves",
     "The latest tag is a moving pointer, not a version",
     ["image:latest is just the default tag — it moves whenever anyone pushes it.",
      "Pin by digest (@sha256:…) for reproducible deploys.",
      "docker pull is not automatic: a running container keeps its original image forever."]),
    (21, "nginx", "howto", "nginx-reverse-proxy-routing",
     "nginx location blocks route by longest prefix match",
     ["nginx picks the longest matching location prefix for reverse proxy routing.",
      "proxy_pass with a trailing slash rewrites the URI prefix; without, it passes unchanged.",
      "Exact match (= /path) beats prefix; regex locations beat prefixes when declared with ~."]),
    (22, "nginx", "fact", "nginx-proxy-headers",
     "Reverse proxies must forward Host and X-Forwarded-* headers",
     ["proxy_set_header Host $host keeps virtual hosting working behind nginx.",
      "X-Forwarded-For/Proto let the app see the real client and scheme.",
      "Without X-Forwarded-Proto, apps behind TLS terminators generate http:// redirects."]),
    (23, "search", "fact", "bm25-saturation",
     "BM25 saturates term frequency — repetition stops helping",
     ["BM25's k1 parameter saturates term frequency: the 10th repetition adds almost nothing.",
      "Document-length normalization (b) keeps long documents from dominating.",
      "IDF means rare query terms dominate ranking over common ones."]),
    (24, "search", "howto", "reciprocal-rank-fusion",
     "Reciprocal rank fusion merges rankings with 1/(k+rank)",
     ["RRF scores each document as the sum of 1/(k+rank) across rankers, k=60 canonical.",
      "It needs no score calibration — only rank order — which is why it suits hybrid search.",
      "Fixed k beats adaptive weighting; adaptive RRF was refuted in benchmarks."]),
    (25, "search", "gotcha", "tokenizer-compound-tokens",
     "A tokenizer that glues paths makes path-part queries miss",
     ["If the tokenizer indexes ~/.claude/settings.json as one token, querying settings.json finds nothing.",
      "A second, splitting tokenizer (default unicode61) indexes the parts; query both and merge.",
      "This is the dual-tokenizer pattern: exact compounds AND their parts are searchable."]),
    (26, "testing", "fact", "golden-master-testing",
     "Golden tests pin outputs byte-for-byte and regenerate explicitly",
     ["Golden-master tests compare output to a committed expected file, byte for byte.",
      "Regeneration must be an explicit, reviewable step — never auto-update on failure.",
      "Pin ids and order, not scores or timestamps; those are environment-fragile."]),
    (27, "testing", "howto", "sandbox-fixtures",
     "Build throwaway sandboxes for CLI tests, never touch production data",
     ["CLI tests should build a synthetic sandbox per run and point the tool at it.",
      "A hard guard that refuses to run against the real data directory is cheap insurance.",
      "Deterministic fixture content makes goldens reproducible across machines."]),
    (28, "testing", "gotcha", "session-scoped-mutation",
     "Mutating a session-scoped fixture leaks state between tests",
     ["A session-scoped fixture shared by tests must be treated as read-only.",
      "If a test must mutate, take a copy or drop to function scope.",
      "Order-dependent passes are the symptom; pytest -p no:randomly helps diagnose."]),
    (29, "docs", "howto", "section-map-notes",
     "Large reference docs get a section-map companion note",
     ["Keep large docs whole and give each a companion note mapping its sections.",
      "Agents grep the map, then read one section with an offset — never the whole file.",
      "A doc without a map is flagged so the gap is visible."]),
    (30, "docs", "fact", "index-first-routing",
     "A small always-readable index beats searching blind",
     ["A one-line-per-entry index read first routes better than any search over the corpus.",
      "The index is generated from metadata, never hand-maintained — hand-maintained indexes drift.",
      "The summary line is the routing hint; write it as the question the entry answers."]),
    (31, "docs", "gotcha", "summary-line-length",
     "Index summary lines over ~220 characters get truncated by readers",
     ["Keep the one-line index summary under ~220 characters; longer lines stop routing well.",
      "The summary is what future readers scan — spend the words on discriminating keywords.",
      "A TODO placeholder in the summary is worse than a short one."]),
    (32, "project", "project", "widget-factory-project",
     "widget-factory project: a synthetic example project for fixtures",
     ["The widget-factory project is the example project this fixture corpus uses.",
      "Its notes exist so project-scoped queries have deterministic hits.",
      "Nothing about it refers to any real repository."]),
    (33, "kb", "howto", "staged-proposal-promotion",
     "Staged proposals are promoted to notes by review, not by accident",
     ["A staging area collects distilled proposals; promotion to a note is an explicit review act.",
      "Unreviewed entries stay listed in a backlog command until marked.",
      "Marking writes a state marker line under the entry heading; the area is append-only."]),
    (34, "kb", "fact", "supersede-dont-delete",
     "Supersede wrong facts instead of deleting them",
     ["Replacing a note marks the old one superseded; it leaves search but stays addressable.",
      "Deletion destroys provenance; supersession keeps the correction auditable.",
      "A banner on the old note points at its replacement."]),
    (35, "kb", "gotcha", "write-time-gate",
     "Search before creating — most new facts belong in an existing note",
     ["Gate every new note with a search first; updating beats duplicating.",
      "Two notes making one claim drift into contradiction within weeks.",
      "The gate is cheap: one search call before the write."]),
    (36, "linux", "howto", "dnf-install-policy",
     "Prefer the distribution package manager for system installs",
     ["A dnf package install policy keeps the system reproducible: repo packages first.",
      "Static binaries into /usr/local bypass updates and provenance checks.",
      "When no package exists, a containerized deployment beats a host install."]),
    (37, "terminal", "fact", "tmux-detach-session",
     "tmux detach keeps the session alive server-side",
     ["tmux detach (prefix d) leaves the session running; tmux attach resumes it.",
      "Sessions survive logout because the server process is detached from the tty.",
      "tmux ls lists live sessions; attach -t picks one by name."]),
    (38, "terminal", "gotcha", "tmux-clipboard-osc52",
     "tmux clipboard needs OSC52 or set-clipboard for remote copies",
     ["Copying inside a remote tmux needs OSC52 pass-through or set-clipboard on.",
      "Without it, mouse copy lands in the terminal's buffer, not the remote clipboard.",
      "Some terminals cap OSC52 payload size; large selections silently truncate."]),
    (39, "claude-code", "howto", "settings-json-locations",
     "Claude Code reads settings from ~/.claude/settings.json then project files",
     ["Claude Code settings resolve user-level (~/.claude/settings.json) before project-level.",
      "Project .claude/settings.json is shared; settings.local.json is personal and git-ignored.",
      "Precedence: local project > shared project > user."]),
    (40, "claude-code", "fact", "hooks-are-deterministic",
     "Hooks fire on tool events deterministically, unlike instructions",
     ["A hook configured in settings.json runs on every matching tool call — no model discretion.",
      "Use hooks for invariants (always run tests after edits), prompts for guidance.",
      "Hook output shown to the model steers the next action; keep it short."]),
    (41, "http", "fact", "http-cache-headers",
     "Cache-Control beats Expires when both are present",
     ["Cache-Control: max-age overrides Expires; send both only for ancient clients.",
      "ETag enables cheap revalidation (304) even when content is not cacheable long.",
      "immutable marks fingerprinted assets so browsers never revalidate them."]),
    (42, "yaml", "gotcha", "yaml-norway-problem",
     "YAML 1.1 parses bare no as boolean false — the Norway problem",
     ["In YAML 1.1, country: NO parses as false — quote two-letter codes.",
      "yes/no/on/off are all booleans unquoted; YAML 1.2 dropped them but parsers lag.",
      "Octal-looking values like 0755 also surprise: quote or use 0o755 explicitly."]),
    (43, "regex", "howto", "regex-catastrophic-backtracking",
     "Nested quantifiers cause catastrophic regex backtracking",
     ["(a+)+ style nested quantifiers explode exponentially on near-miss input.",
      "Possessive quantifiers or atomic groups cut the search tree.",
      "RE2-style engines trade backreferences for linear-time guarantees."]),
    (44, "ssh", "howto", "ssh-config-host-blocks",
     "ssh config Host blocks apply first-match-wins per option",
     ["In ssh config, the FIRST obtained value for each option wins — put specific Host blocks first.",
      "Host * at the bottom carries defaults; a specific block above overrides per key.",
      "Include directives splice other files in place."]),
    (45, "ssh", "fact", "ssh-agent-forwarding-risk",
     "Agent forwarding exposes your keys to the remote host's root",
     ["With ForwardAgent, the remote root can use your agent for any key it holds.",
      "ProxyJump gives the same convenience without exposing the agent socket.",
      "Per-key confirm extensions make each use prompt locally."]),
    (46, "markdown", "gotcha", "markdown-heading-in-code-fence",
     "Headings inside code fences are not headings",
     ["A ## line inside a fenced code block is literal text, not a section heading.",
      "Parsers that ignore fences index phantom sections — fence-awareness matters.",
      "This fixture's refs test pins exactly that behavior."]),
    (47, "python", "gotcha", "pycache-stale-bytecode",
     "Stale __pycache__ rarely bites, but zip-safe eggs and mounts change mtime",
     ["Python invalidates bytecode by source mtime+size; clock skew across mounts defeats it.",
      "Symptom: edits not taking effect — delete __pycache__ and retry.",
      "PYTHONDONTWRITEBYTECODE=1 in containers avoids the layer bloat too."]),
    (48, "git", "howto", "worktree-parallel-branches",
     "git worktree checks out a second branch in a second directory",
     ["git worktree add ../repo-feature feature gives a clean tree without stashing.",
      "Each worktree has its own index and HEAD but shares the object store.",
      "Remove with git worktree remove; prune cleans stale administrative files."]),
    (49, "linux", "fact", "oom-score-adj",
     "oom_score_adj biases which process the OOM killer picks",
     ["oom_score_adj (-1000..1000) shifts a process's OOM-kill priority; -1000 is immune.",
      "The killer sums badness heuristics; the adjustment is a multiplier offset.",
      "systemd units set it via OOMScoreAdjust= in the service file."]),
    (50, "docker", "howto", "docker-multi-stage-builds",
     "Multi-stage builds keep build tools out of the runtime image",
     ["A multi-stage Dockerfile compiles in a builder stage and COPYs artifacts into a slim final stage.",
      "The final image carries no compiler, no package cache, no source.",
      "Name stages (AS builder) so COPY --from=builder stays readable."]),
]

# Extra content notes to reach 74 (ids 51-74): short generic facts.
EXTRA = [
    (51, "python", "fact", "walrus-operator-scope", "The walrus operator binds in comprehensions without leaking the loop var",
     ["The walrus operator (:=) assigns inside expressions, including comprehension filters.",
      "In comprehensions the walrus target binds in the enclosing scope by design."]),
    (52, "sqlite", "gotcha", "sqlite-busy-timeout", "SQLITE_BUSY needs a busy_timeout, not retries",
     ["A locked SQLite database raises SQLITE_BUSY immediately unless busy_timeout is set.",
      "Set PRAGMA busy_timeout=5000 or pass timeout= to connect; retry loops reinvent it badly.",
      "WAL mode plus busy_timeout covers most single-writer contention."]),
    (53, "linux", "fact", "inotify-limits", "inotify watch limits break file watchers on big trees",
     ["fs.inotify.max_user_watches caps watched paths; big repos or monorepos hit it.",
      "Symptoms are silent non-reloading dev servers, not errors.",
      "Raise via sysctl; watch tools dedupe poorly by default."]),
    (54, "git", "fact", "git-rerere", "git rerere records conflict resolutions and replays them",
     ["git rerere remembers how you resolved a conflict and replays it on identical conflicts.",
      "Long-lived parallel branches benefit most; enable with rerere.enabled=true.",
      "The cache lives in .git/rr-cache and is local only."]),
    (55, "testing", "fact", "deterministic-dates-in-fixtures", "Fixture dates must be absolute and fixed",
     ["Relative dates in fixtures (yesterday, last week) rot the fixture as time passes.",
      "Absolute ISO dates keep goldens stable forever.",
      "Age-based logic under test gets its dates injected, never from the clock."]),
    (56, "http", "gotcha", "http-429-retry-after", "429 responses carry Retry-After — honor it",
     ["HTTP 429 means slow down; Retry-After says for how long, in seconds or a date.",
      "Retrying without backoff on 429 turns throttling into an outage.",
      "Exponential backoff with jitter handles the absence of the header."]),
    (57, "yaml", "fact", "yaml-anchors-merge", "YAML anchors and merge keys deduplicate mappings",
     ["YAML anchors (&x) and aliases (*x) reuse subtrees; <<: *x merges mappings.",
      "Merge keys compose base configs; later keys override merged ones.",
      "Aliases are references at parse time — no runtime identity is kept."]),
    (58, "regex", "fact", "regex-word-boundary", "\\b matches between word and non-word characters",
     ["The regex \\b is zero-width: it matches at word/non-word transitions.",
      r"\bcat\b misses category but matches 'a cat'; unicode word chars vary by engine.",
      "In URLs and paths, hyphens are boundaries — \\b often surprises there."]),
    (59, "ssh", "gotcha", "ssh-known-hosts-changed", "CHANGED host key means rebuild or MITM — verify out of band",
     ["A REMOTE HOST IDENTIFICATION HAS CHANGED warning means the key differs from known_hosts.",
      "After a legitimate OS reinstall, remove the stale line; otherwise investigate.",
      "ssh-keygen -R host removes the entry without hand-editing the file."]),
    (60, "sqlite", "fact", "sqlite-deep-dive-oversized", "SQLite deep-dive (deliberately oversized fixture note)",
     ["OVERSIZED"]),
    (61, "git", "fact", "git-internals-oversized", "Git internals deep-dive (deliberately oversized fixture note)",
     ["OVERSIZED"]),
    (62, "linux", "fact", "linux-boot-oversized", "Linux boot process deep-dive (deliberately oversized fixture note)",
     ["OVERSIZED"]),
    (63, "kb", "fact", "verified-stamp-meaning", "A verified stamp records when a claim was last confirmed",
     ["A verified date stamps when a note's claim was last checked against reality.",
      "Edits after the stamp mean the content changed since confirmation.",
      "Re-verification refreshes the stamp with a one-line evidence record."]),
    (64, "kb", "fact", "conflict-precedence", "Conflicting notes resolve by status, then recency, then provenance",
     ["Between conflicting notes, verified beats active beats draft.",
      "Within a status, the newer verification wins; then the stronger source.",
      "Material conflicts escalate to a human with both ids cited."]),
    (65, "python", "fact", "verified-no-date-one", "Fixture: verified status without a verified date (lint error)",
     ["VERIFIEDNODATE"]),
    (66, "git", "fact", "verified-no-date-two", "Fixture: second verified-without-date note (lint error)",
     ["VERIFIEDNODATE"]),
    (67, "docker", "fact", "no-triggers-one", "Fixture: note without triggers (lint warn)",
     ["NOTRIGGERS"]),
    (68, "linux", "fact", "no-triggers-two", "Fixture: second note without triggers (lint warn)",
     ["NOTRIGGERS"]),
    (69, "kb", "fact", "long-summary-note", "Fixture: note whose summary exceeds the index-line budget",
     ["LONGSUMMARY"]),
    (70, "search", "fact", "typo-correction-unique-only", "Typo correction fires only on a unique edit-distance-1 candidate",
     ["A conservative typo layer rewrites a query token only when exactly one vocabulary word is one edit away.",
      "Ambiguous corrections are left alone for an honest miss.",
      "The correction is announced in the output so hits are read critically."]),
    (71, "http", "howto", "idempotency-keys", "Idempotency keys make retried POSTs safe",
     ["An idempotency key lets a server deduplicate retried POST requests.",
      "Clients generate a UUID per logical operation and resend it on retry.",
      "Servers store the first response and replay it for the same key."]),
    (72, "markdown", "fact", "markdown-table-escaping", "Pipes inside markdown table cells need escaping",
     ["A literal pipe in a table cell must be \\| or the column count breaks.",
      "Code spans inside tables still need the pipe escaped.",
      "HTML tables are the escape hatch for genuinely tabular prose."]),
    (73, "terminal", "fact", "terminal-truecolor", "Truecolor terminals advertise via COLORTERM=truecolor",
     ["COLORTERM=truecolor or 24bit signals 24-bit color support to TUI apps.",
      "Falling back to 256-color palettes mangles themes designed for truecolor.",
      "tmux needs terminal-overrides for RGB to pass through."]),
    (74, "project", "session", "widget-factory-session-note", "widget-factory session note: example session-log entry",
     ["Session notes record what a work session changed and what is next.",
      "This fixture entry exists so type:session appears in the frozen corpus.",
      "It refers to the synthetic widget-factory project only."]),
]

REFS_DOCS = [
    "sqlite-internals-guide.md", "pytest-cookbook.md", "docker-networking-notes.md",
    "git-workflows-guide.md", "http-caching-survey.md", "linux-permissions-cheatsheet.md",
    "regex-field-guide.md", "markdown-renderer-notes.md", "search-ranking-survey.md",
    "terminal-emulation-notes.md", "yaml-pitfalls-guide.md", "ssh-config-guide.md",
]

SECTIONS = ["Overview", "Mechanics", "Pitfalls", "Recipes", "References"]

OVERSIZE_SENTENCE = (" This sentence exists to push the fixture note past the "
                     "oversized-note lint budget while staying deterministic "
                     "and content-free.")


def fm(nid, title, typ, domain, created, summary, *, updated=None,
       status="active", verified="__ABSENT__", triggers=True, tags=()):
    updated = updated or created
    lines = ["---", f"id: {nid}", f'title: "{title}"', f"type: {typ}",
             f"domain: {domain}",
             f"tags: [{', '.join(tags)}]", "aliases: []"]
    if triggers:
        lines.append("triggers:")
        lines.append(f'  - "when working on {domain} topics in the fixture corpus"')
    lines += [f"status: {status}", "confidence: medium",
              f"created: {created}", f"updated: {updated}"]
    if verified != "__ABSENT__":
        lines.append(f"verified: {verified}")
    lines += ["supersedes: []", "related: []", 'source: "synthetic fixture"',
              f'summary: "{summary}"', "---", ""]
    return "\n".join(lines)


def created_for(n):
    return f"2026-08-{(n % 20) + 1:02d}"


def main():
    if ROOT.exists():
        shutil.rmtree(ROOT)
    (ROOT / "notes").mkdir(parents=True)
    (ROOT / "refs").mkdir()
    log_lines = []
    all_specs = list(NOTES) + list(EXTRA)
    assert len(all_specs) == 74, len(all_specs)
    for n, domain, typ, slug, title, points in all_specs:
        nid = f"kb-{n:04d}"
        created = created_for(n)
        kwargs = {}
        if n in (65, 66):
            kwargs.update(status="verified")           # ERROR: no verified date
        elif n % 7 == 0:
            kwargs.update(status="verified", verified="2026-09-01")
        if n in (67, 68):
            kwargs["triggers"] = False                  # WARN: missing triggers
        if points == ["OVERSIZED"]:
            body = (f"{title}.\n" + OVERSIZE_SENTENCE * 620 + "\n")
        elif points == ["VERIFIEDNODATE"]:
            body = (f"{title}.\n\nThis note is deliberately missing its "
                    f"verified: date to pin the lint error format.\n")
        elif points == ["NOTRIGGERS"]:
            body = (f"{title}.\n\nThis note deliberately omits triggers: to "
                    f"pin the lint warning format.\n")
        elif points == ["LONGSUMMARY"]:
            body = (f"{title}.\n\nThe summary field below is deliberately over "
                    f"the index-line budget to pin the lint warning format.\n")
        else:
            body = "\n".join(["\n".join([points[0], ""]), *points[1:], ""])
        summary = (title if points != ["LONGSUMMARY"] else
                   title + " — " + "padding " * 30).strip()[:400]
        text = fm(nid, title, typ, domain, created, summary, **kwargs) + body
        (ROOT / "notes" / f"{nid}-{slug}.md").write_text(text, encoding="utf-8")
        log_lines.append(f"## [{created}] create | {nid} | {title}")
    # 12 refs docs + 12 map notes (kb-0075..kb-0086)
    for i, doc in enumerate(REFS_DOCS):
        n = 75 + i
        nid = f"kb-{n:04d}"
        title = doc[:-3].replace("-", " ")
        doc_text = (f"# {title.title()}\n\n" +
                    "\n".join(f"## {s}\n\nSynthetic fixture section about "
                              f"{title}: {s.lower()}.\n" for s in SECTIONS))
        (ROOT / "refs" / doc).write_text(doc_text, encoding="utf-8")
        map_title = f"{title}: section map"
        summary = f"Section map for refs/{doc} — synthetic fixture."
        body = (f"Section map for refs/{doc}.\n\n## Section map\n" +
                "\n".join(f"## {j+1}. {s} — synthetic {s.lower()} content."
                          for j, s in enumerate(SECTIONS)) + "\n")
        created = created_for(n)
        text = fm(nid, map_title, "reference", "kb", created, summary) + body
        slug = f"{doc[:-3]}-section-map"
        (ROOT / "notes" / f"{nid}-{slug}.md").write_text(text, encoding="utf-8")
        log_lines.append(f"## [{created}] create | {nid} | {map_title}")
    (ROOT / "log.md").write_text("\n".join(log_lines) + "\n", encoding="utf-8")
    (ROOT / "SCHEMA.md").write_text("# frozen fixture schema stub\n",
                                    encoding="utf-8")
    # index.md is rebuilt by the frozen_sandbox fixture (knowbox reindex);
    # the copy here is a placeholder so the fixture dir is self-describing.
    (ROOT / "index.md").write_text("# placeholder — rebuilt on fixture setup\n",
                                   encoding="utf-8")
    print(f"corpus-v1 regenerated: 86 notes, {len(REFS_DOCS)} refs docs")


if __name__ == "__main__":
    main()
