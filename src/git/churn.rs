use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use anyhow::{Context, Result, ensure};

use super::{discover, history::read_cmd, id_prefix, run};
use crate::store::Inventory;
use crate::{config::KbRoot, notes};

#[derive(Default)]
struct History<'a> {
    rows: BTreeMap<&'a str, (usize, BTreeSet<&'a str>)>,
    commits: usize,
}

impl<'a> History<'a> {
    fn count(&mut self, day: &'a str, edited: &mut BTreeSet<&'a str>) {
        if !edited.is_empty() {
            self.commits += 1;
        }
        for id in std::mem::take(edited) {
            let (count, days) = self.rows.entry(id).or_default();
            *count += 1;
            days.insert(day);
        }
    }
}

fn history(patch: &str) -> History<'_> {
    let mut counts = History::default();
    let mut day = "";
    let mut edited = BTreeSet::new();
    let mut old = "";
    let mut file = "";
    let mut in_hunk = false;
    for line in patch.lines() {
        if let Some(date) = line.strip_prefix('\u{1}') {
            counts.count(day, &mut edited);
            day = date;
            file = "";
            in_hunk = false;
        } else if line.starts_with("diff --git ") {
            old = "";
            file = "";
            in_hunk = false;
        } else if line.starts_with("@@ ") {
            in_hunk = true;
        } else if !in_hunk && let Some(path) = line.strip_prefix("--- ") {
            old = path
                .trim_end_matches('\t')
                .strip_prefix("a/")
                .unwrap_or(path);
        } else if !in_hunk && let Some(path) = line.strip_prefix("+++ ") {
            let path = path.trim_end_matches('\t');
            file = if path == "/dev/null" {
                old
            } else {
                path.strip_prefix("b/").unwrap_or(path)
            };
        } else if in_hunk
            && !file.is_empty()
            && let Some(content) = line.strip_prefix(['+', '-'])
            && !content.starts_with("updated: ")
            && !content.starts_with("verified: ")
        {
            let id = file
                .match_indices("kb-")
                .find_map(|(pos, _)| id_prefix(&file[pos..]));
            edited.insert(id.unwrap_or(file));
        }
    }
    counts.count(day, &mut edited);
    counts
}

pub(crate) fn churn(
    root: &KbRoot,
    days: u32,
    policy: &mut Inventory<'_>,
    out: &mut impl Write,
) -> Result<()> {
    ensure!(days >= 1, "knowbox churn: --days must be >= 1");
    let repo = discover(root)?.context("knowbox churn: no Git repository at KB root")?;
    let since = format!("--since={days} days ago");
    let data = run(
        read_cmd(&repo).args([
            "-c",
            "core.quotePath=false",
            "log",
            &since,
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-M",
            "--pretty=format:%x01%cs",
            "--",
            "notes/",
        ]),
        "log",
    )?
    .checked()?;
    let patch = String::from_utf8_lossy(&data);
    let counts = history(&patch);
    if counts.rows.is_empty() {
        writeln!(
            out,
            "knowbox churn: no note content edits in the last {days} days (date-only updated:/verified: bumps skipped)"
        )?;
        return Ok(());
    }
    let titles: BTreeMap<_, _> = notes::snapshot_with(root, policy)?
        .into_iter()
        .filter_map(|e| e.parsed.ok())
        .filter_map(|n| {
            Some((
                n.scalar("id")?.to_owned(),
                n.text("title", "").chars().take(56).collect::<String>(),
            ))
        })
        .collect();
    let mut rows: Vec<_> = counts.rows.into_iter().collect();
    rows.sort_unstable_by_key(|(id, (n, _))| (Reverse(*n), *id));
    let n_rows = rows.len();
    let mut hot = 0;
    writeln!(
        out,
        "knowbox churn — content edits per note id, last {days} days (HIGH-CHURN at >=5; date-only bumps skipped, renames aggregated)"
    )?;
    writeln!(out, "{:>7} {:>5}  note", "commits", "days")?;
    for (key, (n, dates)) in rows {
        let label = titles.get(key).map(String::as_str).unwrap_or_else(|| {
            if id_prefix(key) == Some(key) {
                "(note deleted or renamed away)"
            } else {
                "(not a top-level note)"
            }
        });
        let mark = if n >= 5 {
            hot += 1;
            "  HIGH-CHURN"
        } else {
            ""
        };
        writeln!(out, "{n:>7} {:>5}  {key}  {label}{mark}", dates.len())?;
    }
    writeln!(
        out,
        "\n{n_rows} note id(s) edited across {} commit(s); high-churn: {hot}",
        counts.commits
    )?;
    if hot > 0 {
        writeln!(
            out,
            "high-churn notes keep changing: consider splitting them, or moving the stable core into refs/ with a section map."
        )?;
    }
    Ok(())
}
