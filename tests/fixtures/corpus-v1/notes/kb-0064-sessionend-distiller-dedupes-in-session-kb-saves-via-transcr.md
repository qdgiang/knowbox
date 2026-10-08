---
id: kb-0064
title: "SessionEnd distiller dedupes in-session KB saves via transcript extraction, not git log"
type: decision
domain: tooling
tags: []
aliases: []
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0026]
source: "session: 2026-08-19 session-distill dedupe wiring"
summary: "Distiller dedupe: the session's own KB writes are jq-extracted from its transcript and injected as already-ingested scope; post-save tail stays in scope. git-log attribution rejected (concurrent sessions)."
---
The SessionEnd distiller re-proposed knowledge the session had already saved to the KB, filling ~/kb/raw with duplicates — but skipping distill entirely on save would lose knowledge produced between the last save and session end. Fix (wired 2026-08-19 in ~/.claude/hooks/session-distill-run.sh): mechanically extract the session's OWN KB writes from its transcript and inject the list into the distiller prompt as already-ingested scope, keeping post-save knowledge in scope.

Extraction is a jq pass over the transcript JSONL (`.message.content[] | select(.type=="tool_use")`): Bash commands matching `kb (new|touch)` or `sed -i…kb/notes/`, plus Edit/Write tool calls with file_path under /kb/notes/. Output is truncated to 20 entries and injected after the write-gate paragraph: "Their content is ALREADY ingested — never re-propose it… Distill only durable knowledge NOT written to the KB this session — including everything learned AFTER the last kb write."

Rejected alternative: `git log --since=<session-start>` attribution on ~/kb. On this box multiple sessions run concurrently on different projects, so any commit by another session inside the time window would contaminate the list and wrongly exclude topics still in scope. Transcript-based attribution is per-session exact by construction — the dilemma's edge case is not rare, it is the normal operating mode.

Cost/benefit: the injected list adds ~50–350 input tokens (<1% of a distiller run, whose transcript dominates input) and saves output tokens plus all downstream drain work on duplicate proposals. The extraction runs shell-side (jq), zero LLM cost.

Related facts: the distiller's 0-3 proposal cap is prompt guidance only — the worker appends any output containing "PROPOSAL", nothing is mechanically enforced. The write gate inside the prompt covers pre-existing notes via ~/kb/index.md; this mechanism covers the second duplicate source (writes made during the session itself), which the index cannot contain yet. Unreviewed proposals are tracked by the Monday lint-cron raw-backlog counter and ~/.claude/loop.md check 6.
