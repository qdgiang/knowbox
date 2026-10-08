# Gas Town / persistent agent workforce — research verdict

Date: 2026-08-09 · Method: deep-research workflow (103 agents, 21 sources,
100 claims extracted, 25 adversarially verified → 23 confirmed, 2 refuted) ·
Decision: **REJECTED — do not revisit** (user decision 2026-08-09).
Companion docs: claude-harness-future-work.md §A.5, KB note kb-0026.

## Verdict

A Gas Town-style persistent agent workforce is not worth it on fedora-server
(headless i5-9300H, 4c/8t, already running Docker + Hermes + daily work).
Cheaper path to "agents working while away" if ever wanted: **cloud Routines**
(Anthropic-managed, zero local footprint, subscription-bounded, lid-closed OK)
+ 1–2 batched on-demand local workers in tmux/Docker with a hard token budget.
No Bors-style merge queue unless a reliable test suite exists as the gate.

## What Gas Town is

Steve Yegge's multi-agent orchestrator for Claude Code (launched Jan 1, 2026;
v1.0.0 ~April 2026; ~17.5k stars; actively maintained). Roles: Mayor
(orchestrator, never codes), ephemeral Polecat workers, per-rig Witness
supervisors, per-rig Refinery running a Bors-style merge queue (batch MRs →
verification gates → bisect failures → merge). Agent identities/work state
persist in git (Beads); crash recovery via `gt prime`. `gt up` boots an
always-on stack: Dolt DB, daemon, Deacon, Mayor, Witnesses, Refineries.
Design point: 20–30 parallel agents ("4–10 become chaotic" per its own README).

## Why rejected (verified findings)

- **Scale mismatch:** design point 20–30 agents vs. realistic ceiling of 1–2
  on this hardware (inference from process-count evidence; no published
  per-agent CPU/RAM benchmark exists — the one specific footprint claim was
  refuted 0-3 in verification). One trial left 141 orphaned claude processes
  devouring RAM on a 32GB MacBook Air.
- **Cost:** measured ~$100/hr in tokens (~10× a normal session, DoltHub n=1);
  Yegge: "expensive as hell… a cash guzzler", needed 2+ Claude accounts to
  outrun spend caps; sustained use = thousands of dollars/month. Anthropic has
  since blocked third-party use of Claude Code subscriptions, so Yegge's
  multi-Max-plan arbitrage is dead anyway.
- **Failure modes (first-person):** queue autonomously merged a PR that failed
  integration tests → colleague had to `reset --hard` + force-push to recover
  (DoltHub); 4/4 PRs produced in that run were closed by the maintainer as not
  good enough; bugs fixed 2–3× with fixes lost (Yegge's own words); poor
  operator visibility (Hartcher).
- **Maturity:** "17 days, 75k lines, 2000 commits", "100% vibe coded",
  four-times-repeated "Do not use Gas Town" warning at launch; v1.0 softened
  it, but hands-on maturity evidence remains thin.

## The viable alternative (vendor-documented, verified)

- **Cloud Routines** (research preview): saved prompts on Anthropic-managed
  infra, scheduled (min 1 h) / API / GitHub-event triggers, run with the
  laptop closed, draw subscription usage + per-account daily run cap.
- **Desktop scheduled tasks:** local machine, min 1 min, no open session.
- **/loop + cron:** session-scoped, fire only while a session runs & idle, no
  catch-up for missed fires.

## Caveats

Most negative hands-on evidence dates to Jan 2026 (tool 2–3 weeks old);
v1.0.x may have improved (a March 2026 DoltHub follow-up was reportedly more
positive, unverified). $100/hr is n=1 on a hard repo. Non-Claude alternatives
(herdr, OpenClaw, GitHub Actions agents, Conductor) produced no claims that
survived adversarial verification.

## Sources

- https://steve-yegge.medium.com/welcome-to-gas-town-4f25ee16dd04 (primary)
- https://github.com/gastownhall/gastown · https://gastown.dev/docs/INSTALLING
- https://www.dolthub.com/blog/2026-01-15-a-day-in-gas-town/ (hands-on, $100/hr, bad merge)
- https://maggieappleton.com/gastown
- https://simonhartcher.com/posts/2026-01-19-my-thoughts-on-gas-town-after-10000-hours-of-claude-code/
- https://tenzinwangdhen.com/posts/gastown-good-bad-ugly/ (141 orphaned processes)
- https://code.claude.com/docs/en/routines · https://code.claude.com/docs/en/scheduled-tasks
