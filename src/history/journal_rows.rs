use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::rc::Rc;

use anyhow::{Context, Result};

use crate::changes::audit::{self, Transition};
use crate::config::KbRoot;
use crate::git::history::{Blobs, History};
use crate::journal::{self, Heading};
use crate::{notes, store::Inventory};

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) heading: Heading,
    pub(super) line: usize,
    pub(super) transitions: Vec<Transition>,
    key: String,
}

#[derive(Clone, Debug)]
pub(super) struct Occurrence {
    pub(super) entry: Entry,
    pub(super) origins: BTreeSet<(usize, usize)>,
    pub(super) uncertain: bool,
    pub(super) current: bool,
}

pub(super) fn read(
    root: &KbRoot,
    policy: &mut Inventory<'_>,
    git: Option<&History<'_>>,
    blobs: &mut Blobs,
    diagnostics: &mut Vec<String>,
) -> Result<Vec<Occurrence>> {
    let path = root.path().join("log.md");
    let entries = if policy.file(&path)? {
        let journal = fs::read_to_string(&path).context("reading log.md")?;
        parse(&journal, "log.md", diagnostics)
    } else {
        Vec::new()
    };
    let Some(git) = git else {
        return Ok(attach(entries, &[], None, false));
    };

    let mut remains = BTreeMap::new();
    for revision in &git.revisions {
        for parent in &revision.commit.parents {
            *remains.entry(parent.as_str()).or_insert(0_usize) += 1;
        }
    }
    let mut snapshots: BTreeMap<&str, Rc<Vec<Occurrence>>> = BTreeMap::new();
    let mut archive = BTreeMap::new();
    let mut head = Rc::new(Vec::new());
    let mut head_known = true;
    for (index, revision) in git.revisions.iter().enumerate() {
        let parents: Vec<_> = revision
            .commit
            .parents
            .iter()
            .map(|parent| snapshots.get(parent.as_str()).cloned())
            .collect();
        let first = parents.first().and_then(Option::as_ref);
        let before = revision
            .changes
            .iter()
            .filter_map(|change| change.before.as_ref())
            .find(|image| image.path == "log.md");
        let after = revision
            .changes
            .iter()
            .filter_map(|change| change.after.as_ref())
            .find(|image| image.path == "log.md");
        let changed = before.is_some() || after.is_some();
        let unknown_parent =
            parents.iter().any(Option::is_none) || (parents.is_empty() && git.shallow);
        // When the first parent is outside the retained window, its before-image
        // can rule out introductions even though the publisher may be missing.
        let fallback = if first.is_none() && unknown_parent {
            let entries = read_image(before.map(|image| image.oid.as_str()), blobs, diagnostics)?;
            Some(Rc::new(attach(entries, &[], None, true)))
        } else {
            None
        };
        let previous = first.or(fallback.as_ref());
        let known = changed || first.is_some() || !unknown_parent;
        head = if changed {
            let entries = read_image(after.map(|image| image.oid.as_str()), blobs, diagnostics)?;
            let parents: Vec<_> = previous
                .into_iter()
                .chain(parents.iter().skip(1).filter_map(Option::as_ref))
                .map(|parent| parent.as_slice())
                .collect();
            Rc::new(attach(entries, &parents, Some(index), unknown_parent))
        } else if let Some(previous) = previous {
            if parents.len() <= 1 {
                Rc::clone(previous)
            } else {
                let entries = previous.iter().map(|row| row.entry.clone()).collect();
                let parents: Vec<_> = std::iter::once(previous)
                    .chain(parents.iter().skip(1).filter_map(Option::as_ref))
                    .map(|parent| parent.as_slice())
                    .collect();
                Rc::new(attach(entries, &parents, Some(index), unknown_parent))
            }
        } else {
            Rc::new(Vec::new())
        };
        if changed || parents.len() > 1 {
            for row in head.iter() {
                let origin = (index, row.entry.line);
                if row.origins.contains(&origin) {
                    archive.insert(origin, row.clone());
                }
            }
        }
        head_known = known;
        if known {
            snapshots.insert(&revision.commit.oid, Rc::clone(&head));
        }
        for parent in &revision.commit.parents {
            if let Some(left) = remains.get_mut(parent.as_str()) {
                *left -= 1;
                if *left == 0 {
                    snapshots.remove(parent.as_str());
                }
            }
        }
    }

    let inherited = align(entries.iter(), &head);
    let retained: BTreeSet<_> = inherited
        .iter()
        .filter_map(|matched| match matched {
            Match::Occurrence(index) => Some(*index),
            Match::Absent | Match::Uncertain => None,
        })
        .collect();
    let mut rows = attach(entries, &[&head], None, !head_known);
    for (index, previous) in head.iter().enumerate() {
        if !retained.contains(&index)
            && !rows
                .iter()
                .any(|row| row.entry.key == previous.entry.key && row.uncertain)
        {
            let mut previous = previous.clone();
            previous.current = false;
            rows.push(previous);
        }
    }
    let represented: BTreeSet<_> = rows
        .iter()
        .flat_map(|row| row.origins.iter().copied())
        .collect();
    for (origin, mut row) in archive {
        if !represented.contains(&origin) {
            row.current = false;
            rows.push(row);
        }
    }
    Ok(rows)
}

fn read_image(
    oid: Option<&str>,
    blobs: &mut Blobs,
    diagnostics: &mut Vec<String>,
) -> Result<Vec<Entry>> {
    let Some(oid) = oid else {
        return Ok(Vec::new());
    };
    let bytes = blobs
        .get(oid)
        .with_context(|| format!("reading log.md Git blob {oid}"))?;
    let text =
        std::str::from_utf8(&bytes).with_context(|| format!("decoding log.md Git blob {oid}"))?;
    Ok(parse(text, &format!("log.md blob {oid}"), diagnostics))
}

fn parse(text: &str, source: &str, diagnostics: &mut Vec<String>) -> Vec<Entry> {
    let normalized = notes::normalize_newlines(text);
    let mut rows = Vec::new();
    let mut attached = None::<usize>;
    for (line, text) in normalized
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
    {
        if let Some(heading) = journal::heading(text) {
            let timestamped = heading.time.contains('T');
            rows.push(Entry {
                heading,
                line: line + 1,
                transitions: Vec::new(),
                key: text.to_owned(),
            });
            attached = timestamped.then_some(rows.len() - 1);
            continue;
        }
        if let Some(index) = attached {
            match audit::decode(text) {
                Ok(Some(transition)) => {
                    rows[index].transitions.push(transition);
                    rows[index].key.push('\n');
                    rows[index].key.push_str(text);
                }
                Ok(None) => attached = None,
                Err(error) => {
                    diagnostics.push(format!("{source} line {}: {error}", line + 1));
                    attached = None;
                }
            }
        }
    }
    rows
}

#[derive(Clone, Copy)]
enum Match {
    Absent,
    Occurrence(usize),
    Uncertain,
}

fn align<'a>(
    entries: impl ExactSizeIterator<Item = &'a Entry> + Clone,
    parent: &[Occurrence],
) -> Vec<Match> {
    let common = entries
        .clone()
        .zip(parent)
        .take_while(|(entry, old)| entry.key == old.entry.key)
        .count();
    // The journal writer appends, so a preserved prefix identifies occurrences,
    // including identical headings appended more than once.
    if common == parent.len() || common == entries.len() {
        return (0..entries.len())
            .map(|index| {
                if index < common {
                    Match::Occurrence(index)
                } else {
                    Match::Absent
                }
            })
            .collect();
    }
    let mut before = BTreeMap::<&str, Vec<usize>>::new();
    for (index, row) in parent.iter().enumerate() {
        before.entry(&row.entry.key).or_default().push(index);
    }
    let mut counts = BTreeMap::new();
    for row in entries.clone() {
        *counts.entry(row.key.as_str()).or_insert(0_usize) += 1;
    }
    entries
        .map(
            |entry| match before.get(entry.key.as_str()).map(Vec::as_slice) {
                None => Match::Absent,
                Some([index]) if counts.get(entry.key.as_str()) == Some(&1) => {
                    Match::Occurrence(*index)
                }
                Some(_) => Match::Uncertain,
            },
        )
        .collect()
}

fn attach(
    entries: Vec<Entry>,
    parents: &[&[Occurrence]],
    commit: Option<usize>,
    unknown_parent: bool,
) -> Vec<Occurrence> {
    let matches: Vec<_> = parents
        .iter()
        .map(|parent| align(entries.iter(), parent))
        .collect();
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut origins = BTreeSet::new();
            let mut uncertain = false;
            let mut inherited = false;
            for (parent, matched) in parents.iter().zip(&matches) {
                match matched[index] {
                    Match::Absent => {}
                    Match::Uncertain => uncertain = true,
                    Match::Occurrence(previous) => {
                        let old = &parent[previous];
                        inherited = true;
                        origins.extend(&old.origins);
                        uncertain |= old.uncertain;
                    }
                }
            }
            if !inherited {
                uncertain |= unknown_parent;
                if let Some(commit) = commit.filter(|_| !uncertain) {
                    origins.insert((commit, entry.line));
                }
            }
            Occurrence {
                entry,
                origins,
                uncertain,
                current: true,
            }
        })
        .collect()
}
