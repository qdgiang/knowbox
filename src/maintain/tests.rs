use std::collections::BTreeMap;
use std::path::PathBuf;

use jiff::civil::Date;

use crate::store::WriteLock;

use super::*;

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
    raw_backlog(&root, &mut output)?;
    assert_eq!(
        String::from_utf8(output)?,
        "raw backlog: 2 unreviewed of 4 entries\n  staging.md #3: Late marker\n  staging.md #4: Untouched\n\nmark after ingest: knowbox raw-mark <file> <N> --note kb-XXXX | --rejected \"reason\"\n"
    );
    let mut warnings = Vec::new();
    raw::warnings(&raw::inventory(&root)?, &mut warnings);
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
fn raw_mark_inserts_before_body_and_revalidates_resolved_state() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let file = "staging Δ # %.md";
    let path = root.path().join("raw").join(file);
    let lock = WriteLock::acquire(&root)?;
    fs::write(&path, "\u{feff}## Café\r\n\r\nold body\n## No newline")?;
    let today = Date::new(2026, 11, 18)?;
    let first = raw_mark(&lock, file, 1, Mark::Ingested("kb-0147"), today)?;
    assert_eq!(first.subject, "knowbox: raw-mark staging Δ # %.md #1");
    assert_eq!(
        first.stdout,
        "marked staging Δ # %.md #1: Café -> state: ingested kb-0147 (2026-11-18)"
    );
    let already = raw_mark(&lock, file, 1, Mark::Rejected("changed"), today).unwrap_err();
    assert_eq!(
        already.to_string(),
        "raw-mark: staging Δ # %.md #1 (\"Café\") is already marked"
    );
    raw_mark(&lock, file, 2, Mark::Rejected("déjà couvert"), today)?;
    assert_eq!(fs::read(path)?, "\u{feff}## Café\r\n> state: ingested kb-0147 (2026-11-18)\r\n\r\nold body\n## No newline\n> state: rejected déjà couvert (2026-11-18)".as_bytes());
    Ok(())
}

#[test]
fn raw_mark_rejects_bad_selectors_and_marker_injection() -> Result<()> {
    let (_dir, root) = sandbox()?;
    let lock = WriteLock::acquire(&root)?;
    let today = Date::new(2026, 11, 18)?;
    let path = root.path().join("raw/input.md");
    fs::write(&path, "## Idea")?;
    for file in [
        "../input.md",
        "..\\input.md",
        "N:input.md",
        "input.MD",
        "\ninput.md",
    ] {
        let error = raw_mark(&lock, file, 1, Mark::Ingested("kb-0123"), today).unwrap_err();
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
        let error = raw_mark(&lock, "input.md", 1, Mark::Rejected(reason), today).unwrap_err();
        assert_eq!(
            error.to_string(),
            "raw-mark: rejection reason must be nonempty and on one line"
        );
    }
    let no_entry = raw_mark(&lock, "input.md", 0, Mark::Ingested("kb-0123"), today).unwrap_err();
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
    let mut output = Vec::new();
    let error = raw_backlog(&root, &mut output).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("reading {} as UTF-8", path.display())
    );
    assert_eq!(output, b"");
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
    assert_eq!(
        findings.errors,
        [
            "notes/kb-0001-probe.md: updated (2026-11-17) < created (2026-11-18)",
            "notes/kb-0001-probe.md: verified not ISO date: 'Invalid'",
            "notes/kb-0001-probe.md: related points at missing kb-0003",
            "notes/kb-0003-truncated.md: unterminated frontmatter (excluded from index/search)",
            "1 of 2 notes/*.md failed to parse — excluded from index/search (see errors above)",
        ]
    );
    assert_eq!(
        findings.warnings,
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
    let has_errors = lint(&root, &mut out)?;
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
    assert!(has_errors);
    assert_eq!(String::from_utf8(out)?, expected);
    Ok(())
}

#[test]
fn lint_created_history_uses_first_journal_date_and_waives_only_historical_warnings() -> Result<()>
{
    let history = History::parse(
        "## [2026-09-02] init | —\r\n## [2026-09-03] create | kb-0001\n## [2026-09-01] create | kb-0001",
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
    assert_eq!(
        history.warnings("kb-0002", "2026-09-04", "2026-09-04"),
        ["no create entry in log.md (created after the journal started — hand-made note?)"]
    );
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
