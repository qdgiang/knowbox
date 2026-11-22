use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::journal::test_stamp;
use crate::store::WriteLock;

use super::*;

#[test]
fn prose_budget_skips_fenced_code_and_counts_mermaid_source_lines() {
    let body = "Two words.\n\n```rust\ncode tokens can be numerous\n```\n\nAfter code.\n\n> ~~~mermaid\n> flowchart LR\n>   A --> B\n> ~~~\n\nLast word.";
    let mut diagrams = Vec::new();
    assert_eq!(prose_and_diagrams(body, |lines| diagrams.push(lines)), 6);
    assert_eq!(diagrams, [2]);
    assert_eq!(
        prose_and_diagrams("one\n\n```mermaid\na\nb", |lines| assert_eq!(lines, 2)),
        1
    );
}

fn sandbox() -> Result<(tempfile::TempDir, KbRoot)> {
    let dir = tempfile::tempdir()?;
    fs::create_dir(dir.path().join("raw"))?;
    fs::create_dir(dir.path().join("notes"))?;
    let root = KbRoot::open(dir.path())?;
    Ok((dir, root))
}

#[test]
fn backlog_preserves_numbering_and_recognizes_only_eligible_markers() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let path = root.path().join("raw/staging.md");
    let text = "\u{feff}> rejected: orphan\r\n## Cpö\r\n> state: ingested kb-0001\r\n## PAIR: record\r\n> rejected: inside exempt record\r\n## Legacy\r> rejected: embedded\r## Late marker\u{2028}\u{85}> state: rejected late\u{2029}## Untouched";
    fs::write(&path, text)?;
    let mut output = Vec::new();
    raw_backlog(&root, &mut Inventory::Strict)?.render(&mut output)?;
    assert_eq!(
        String::from_utf8(output)?,
        "raw backlog: 2 unreviewed of 4 entries\n  staging.md #3: Late marker\n  staging.md #4: Untouched\n\nmark after ingest: knowbox raw-mark <file> <N> --note kb-XXXX | --rejected \"reason\"\n"
    );
    let mut warnings = Vec::new();
    raw::warnings(&root, &mut Inventory::Strict, |_, _, message| {
        warnings.push(message)
    })?;
    assert_eq!(
        warnings,
        [
            "raw/staging.md: 1 orphan legacy marker in the preamble (above the first `## ` heading) — they resolve no entry but skew the janitor's backlog count; attach each to its entry's block",
            "raw/staging.md: 2 unreviewed entries — ingest via the knowbox skill, then `knowbox raw-mark <file> <N> --note kb-XXXX | --rejected \"reason\"`",
        ]
    );
    assert_eq!(fs::read(path)?, text.as_bytes());
    Ok(())
}

#[test]
fn raw_verdicts_require_a_conflict_document_and_two_distinct_ids() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let marker = "> verdict: complementary\r\nThe claims cover different cases.\r\n";
    for (file, title) in [
        ("ordinary.md", "kb-0001 kb-0002"),
        ("conflicts-bad.md", "kb-0001 foo kb-0002"),
        ("conflicts-repeat.md", "kb-0001 kb-0001"),
        ("conflicts-2026-11-18.md", "kb-0001 kb-0002 (cos 0.8)"),
    ] {
        fs::write(
            root.path().join("raw").join(file),
            format!("## pair: {title}\r\n{marker}"),
        )?;
    }
    let evidence = serde_json::to_value(raw_evidence(&root, &mut Inventory::Strict)?)?;
    assert_eq!(
        evidence,
        serde_json::json!([{
            "kind": "verdict",
            "file": "conflicts-2026-11-18.md",
            "line": 1,
            "ids": ["kb-0001", "kb-0002"],
            "verdict": "complementary",
            "rationale": "The claims cover different cases.",
            "date": "2026-11-18"
        }])
    );
    Ok(())
}

#[test]
fn raw_mark_inserts_before_body_and_revalidates_resolved_state() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let file = "staging Δ # %.md";
    let path = root.path().join("raw").join(file);
    let lock = WriteLock::acquire(&root)?;
    fs::write(&path, "\u{feff}## Café\r\n\r\nold body\n## No newline")?;
    let stamp = test_stamp();
    let first = raw_mark(&lock, file, 1, Mark::Ingested("kb-0147"), &stamp)?;
    assert_eq!(first.subject, "knowbox: raw-mark staging Δ # %.md #1");
    assert_eq!(
        first.stdout,
        "marked staging Δ # %.md #1: Café -> state: ingested kb-0147 (2026-11-18)"
    );
    let already = raw_mark(&lock, file, 1, Mark::Rejected("changed"), &stamp).unwrap_err();
    assert_eq!(
        already.to_string(),
        "raw-mark: staging Δ # %.md #1 (\"Café\") is already marked"
    );
    raw_mark(&lock, file, 2, Mark::Rejected("déjà couvert"), &stamp)?;
    assert_eq!(fs::read(path)?, "\u{feff}## Café\r\n> state: ingested kb-0147 (2026-11-18)\r\n\r\nold body\n## No newline\n> state: rejected déjà couvert (2026-11-18)".as_bytes());
    Ok(())
}

#[test]
fn raw_mark_rejects_bad_selectors_and_marker_injection() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let lock = WriteLock::acquire(&root)?;
    let stamp = test_stamp();
    let path = root.path().join("raw/input.md");
    fs::write(&path, "## Idea")?;
    for file in [
        "../input.md",
        "..\\input.md",
        "N:input.md",
        "input.MD",
        "\ninput.md",
    ] {
        let error = raw_mark(&lock, file, 1, Mark::Ingested("kb-0123"), &stamp).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("raw-mark: bad raw filename {file:?} (basename of a .md file inside raw/)")
        );
    }
    for reason in [
        "",
        "reason\n## new",
        "reason\u{2028}## new",
        "reason\0hidden",
    ] {
        let error = raw_mark(&lock, "input.md", 1, Mark::Rejected(reason), &stamp).unwrap_err();
        assert_eq!(
            error.to_string(),
            "raw-mark: rejection reason must be nonempty and on one line"
        );
    }
    let no_entry = raw_mark(&lock, "input.md", 0, Mark::Ingested("kb-0123"), &stamp).unwrap_err();
    assert_eq!(
        no_entry.to_string(),
        "raw-mark: input.md has 1 entries, no #0"
    );
    assert_eq!(fs::read(path)?, b"## Idea");
    Ok(())
}

#[test]
fn unreadable_raw_content_is_not_reported_as_an_empty_clean_backlog() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let path = root.path().join("raw/corrupt.md");
    fs::write(&path, b"## Invalid UTF-8 \xff")?;
    let error = raw_backlog(&root, &mut Inventory::Strict).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("reading {} as UTF-8", path.display())
    );
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::InvalidData)
    );
    Ok(())
}

#[test]
fn lint_counts_malformed_notes_and_reports_dead_related_ids() -> Result<()> {
    let (_dir, root) = sandbox()?;
    fs::write(
        root.path().join("notes/kb-0001-probe.md"),
        "---\nid: kb-0001\ntitle: Probe\ntype: fact\ndomain: synthetic\nstatus: verified\nconfidence: high\ncreated: 2026-11-18\nupdated: 2026-11-17\nverified: Invalid\nsummary: Probe\nsource: fixture\ntriggers: probe\nrelated: [kb-0003]\n---\n[[kb-0002]]\n",
    )?;
    fs::write(
        root.path().join("notes/kb-0003-truncated.md"),
        "---\nid: kb-0003\n",
    )?;
    let entries = notes::snapshot(&root)?;
    let mut findings = Findings::default();
    let count = findings.check_notes(&entries, &History::default(), &mut Waivers::default());
    assert_eq!(count, 1);
    let report = findings.report(count);
    let messages = |severity| {
        report
            .findings
            .iter()
            .filter(move |finding| finding.severity == severity)
            .map(|finding| finding.message.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        messages(Severity::Error),
        [
            "notes/kb-0001-probe.md: updated (2026-11-17) < created (2026-11-18)",
            "notes/kb-0001-probe.md: verified not ISO date: 'Invalid'",
            "notes/kb-0001-probe.md: related points at missing kb-0003",
            "notes/kb-0003-truncated.md: unterminated frontmatter (excluded from index/search)",
            "1 of 2 notes/*.md failed to parse — excluded from index/search (see errors above)",
        ]
    );
    assert_eq!(
        messages(Severity::Warning),
        ["notes/kb-0001-probe.md: body links [[kb-0002]] but it is not in related:"]
    );
    Ok(())
}

#[test]
fn lint_matches_legacy_frozen_report_exactly() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus-v1");
    for item in fs::read_dir(fixture.join("notes"))? {
        let item = item?;
        fs::copy(
            item.path(),
            root.path().join("notes").join(item.file_name()),
        )?;
    }
    fs::copy(fixture.join("log.md"), root.path().join("log.md"))?;
    let lock = WriteLock::acquire(&root)?;
    index::rebuild(&lock)?;
    drop(lock);
    let mut out = Vec::new();
    let report = lint(&root, HeartbeatCheck::Disabled, &mut Inventory::Strict)?;
    report.render(&mut out)?;
    let golden: BTreeMap<&str, Vec<&str>> =
        serde_json::from_str(include_str!("../../tests/goldens/lint_report.json"))?;
    let groups = |key| {
        golden
            .get(key)
            .map(|lines| lines.join("\n"))
            .context("missing lint golden group")
    };
    let expected = format!(
        "{}\n{}\n\n{}\n",
        groups("warnings")?,
        groups("errors")?,
        groups("summary")?
    );
    assert!(report.has_errors());
    assert_eq!(String::from_utf8(out)?, expected);
    Ok(())
}

#[test]
fn heartbeats_expire_strictly_past_their_respective_limits() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(30 * 86_400);
    for (name, days) in [(".kb-heartbeat", 8), (".kb-sweep-heartbeat", 2)] {
        fs::File::create(root.path().join(name))?
            .set_modified(now - Duration::from_secs(days * 86_400))?;
    }
    let mut findings = Findings::default();
    findings.check_heartbeats(&root, now);
    assert!(findings.0.is_empty());
    findings.check_heartbeats(&root, now + Duration::from_secs(1));
    assert_eq!(
        findings
            .0
            .iter()
            .map(|finding| finding.message.as_str())
            .collect::<Vec<_>>(),
        [
            "janitor heartbeat stale (older than 8d) — weekly cron dead? re-run your KB maintenance script",
            "sweep heartbeat stale (older than 2d) — sweep cron dead? check .sweep-cron.log",
        ]
    );
    Ok(())
}

#[test]
fn lint_created_history_uses_first_journal_date_and_waives_only_historical_warnings() -> Result<()>
{
    let history = History::parse(
        "## [2026-09-02T00:05:00+07:00] init | — | client=cli | host=test | session=— | empty\r\n## [2026-09-03] create | kb-0001\n## [2026-09-01] create | kb-0001",
    );
    assert_eq!(
        history.warnings("kb-0001", "2026-09-02", "2026-09-02"),
        [
            "log.md has 2 create entries for kb-0001 (historical id recycling — see the P9 gotcha note)",
            "created: 2026-09-02 matches no log.md create date for kb-0001 (2026-09-01, 2026-09-03)",
        ]
    );
    assert_eq!(
        history.warnings("kb-0002", "2026-09-02", "2026-09-02"),
        Vec::<String>::new()
    );
    assert!(!history.missing_create("kb-0002", "2026-09-02"));
    assert!(history.missing_create("kb-0002", "2026-09-04"));
    let mut waivers = Waivers::parse(
        "kb-0002 high-water 2026-09-03 reason\nkb-0010 created-vs-log 2026-09-03\nkb-0009 created-vs-log 2026-09-03 reason\nkb-0001 created-vs-log 2026-09-03 history",
    );
    assert!(waivers.applies("kb-0001"));
    assert!(waivers.applies("kb-0001"));
    assert!(!waivers.applies("kb-0002"));
    let mut warnings = Vec::new();
    waivers.append_unused_warnings(&mut warnings);
    assert_eq!(
        warnings,
        [
            "lint-waivers.md: unknown category 'high-water' in line 'kb-0002 high-water 2026-09-03 reason' (waivable: created-vs-log)",
            "lint-waivers.md: line 'kb-0010 created-vs-log 2026-09-03' is not `kb-XXXX <category> <date> <reason>`",
            "lint-waivers.md: stale waiver kb-0009 created-vs-log — suppressed nothing this run; remove the line",
        ]
    );
    Ok(())
}
