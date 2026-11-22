use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::changes::audit::{Fingerprint, Transition, same_content};
use crate::git::history::{Blobs, Commit, History, Image, is_note};
use crate::{journal, notes};

use super::journal_rows::Occurrence;

#[derive(Clone, Debug, Serialize)]
pub(super) struct NoteImage {
    pub(super) path: String,
    pub(super) created: Option<String>,
    pub(super) fingerprint: Fingerprint,
}

impl NoteImage {
    pub(super) fn new(path: &str, bytes: &[u8]) -> Self {
        let created = notes::parse(bytes)
            .as_ref()
            .ok()
            .and_then(|note| note.scalar("created"))
            .filter(|date| journal::local_day(date) == Some(*date))
            .map(str::to_owned);
        Self {
            created,
            path: path.to_owned(),
            fingerprint: Fingerprint::from_bytes(path, bytes),
        }
    }

    pub(super) fn id(&self) -> Option<&str> {
        self.fingerprint.id.as_deref()
    }
}

#[derive(Debug)]
pub(super) struct Diff {
    pub(super) revision: usize,
    change: usize,
    pub(super) before: Option<NoteImage>,
    pub(super) after: Option<NoteImage>,
}

impl Diff {
    pub(super) fn images(&self) -> impl Iterator<Item = &NoteImage> {
        self.before.iter().chain(&self.after)
    }

    pub(super) fn action(&self) -> &'static str {
        match (&self.before, &self.after) {
            (None, _) => "add",
            (_, None) => "delete",
            (Some(before), Some(after)) if before.path != after.path => "rename",
            _ => "edit",
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct Patch {
    pub(super) commit: Commit,
    before: Option<NoteImage>,
    after: Option<NoteImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) text: Option<String>,
}

pub(super) fn collect(git: &History<'_>, blobs: &mut Blobs, id: Option<&str>) -> Result<Vec<Diff>> {
    let mut evidence = Vec::new();
    let mut cached = BTreeMap::<(String, String), NoteImage>::new();
    let mut image = |image: &Image| -> Result<Option<NoteImage>> {
        if !is_note(&image.path) {
            return Ok(None);
        }
        let key = (image.path.clone(), image.oid.clone());
        if let Some(note) = cached.get(&key) {
            return Ok(Some(note.clone()));
        }
        let bytes = blobs
            .get(&image.oid)
            .with_context(|| format!("reading Git blob for {}", image.path))?;
        let note = NoteImage::new(&image.path, &bytes);
        cached.insert(key, note.clone());
        Ok(Some(note))
    };
    for (revision, entry) in git.revisions.iter().enumerate() {
        for (change, entry) in entry
            .changes
            .iter()
            .enumerate()
            .filter(|(_, change)| change.note())
        {
            let diff = Diff {
                revision,
                change,
                before: entry.before.as_ref().map(&mut image).transpose()?.flatten(),
                after: entry.after.as_ref().map(&mut image).transpose()?.flatten(),
            };
            if id.is_none_or(|id| diff.images().any(|image| image.id() == Some(id))) {
                evidence.push(diff);
            }
        }
    }
    Ok(evidence)
}

pub(super) fn patches(
    git: &History<'_>,
    diffs: &[Diff],
    selection: super::Selection<'_>,
) -> Result<Vec<Patch>> {
    diffs
        .iter()
        .map(|diff| {
            let revision = &git.revisions[diff.revision];
            Ok(Patch {
                commit: revision.commit.clone(),
                before: diff.before.clone(),
                after: diff.after.clone(),
                text: match selection {
                    super::Selection::Note(_) => {
                        Some(git.patch(&revision.commit, &revision.changes[diff.change])?)
                    }
                    super::Selection::Timeline => None,
                },
            })
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Confidence {
    Unpublished,
    EarlierContent,
    PublishedTogether,
}

pub(super) fn associate(
    git: &History<'_>,
    journal: &[Occurrence],
    diffs: &[Diff],
) -> Vec<BTreeSet<usize>> {
    let mut paths = BTreeMap::<&str, Vec<(usize, &Transition)>>::new();
    for (index, row) in journal.iter().enumerate() {
        for transition in &row.entry.transitions {
            paths
                .entry(&transition.path)
                .or_default()
                .push((index, transition));
        }
    }
    for rows in paths.values_mut() {
        rows.sort_by_key(|(index, _)| {
            let row = &journal[*index];
            row.origins
                .first()
                .copied()
                .unwrap_or((usize::MAX, row.entry.line))
        });
    }
    let mut associations =
        vec![BTreeMap::<&str, BTreeMap<usize, Confidence>>::new(); journal.len()];
    let revisions: BTreeMap<_, _> = git
        .revisions
        .iter()
        .enumerate()
        .map(|(index, revision)| (revision.commit.oid.as_str(), index))
        .collect();
    let mut ancestors = BTreeMap::new();

    for (number, diff) in diffs.iter().enumerate() {
        let diff_paths: BTreeSet<_> = diff.images().map(|image| image.path.as_str()).collect();
        for path in diff_paths {
            let Some(rows) = paths.get(path) else {
                continue;
            };
            let before = endpoint(diff.before.as_ref(), path);
            let after = endpoint(diff.after.as_ref(), path);
            if same_content(before, after) {
                continue;
            }
            let mut chains = Vec::new();
            for (start, &(index, transition)) in rows.iter().enumerate() {
                if !matches(before, transition.before.as_ref()) {
                    continue;
                }
                let row = &journal[index];
                if row.uncertain || row.origins.len() > 1 {
                    continue;
                }
                let publication = row.origins.first().map(|&(revision, _)| revision);
                let confidence = match publication {
                    None => Confidence::Unpublished,
                    Some(published) if published == diff.revision => Confidence::PublishedTogether,
                    Some(published)
                        if published > diff.revision
                            && *ancestors.entry((diff.revision, published)).or_insert_with(
                                || ancestor(git, &revisions, diff.revision, published),
                            ) =>
                    {
                        Confidence::EarlierContent
                    }
                    Some(_) => continue,
                };
                let mut state = before;
                for (offset, &(row_index, transition)) in rows[start..].iter().enumerate() {
                    if !matches(state, transition.before.as_ref()) {
                        break;
                    }
                    let row = &journal[row_index];
                    if row.uncertain
                        || row.origins.len() > 1
                        || row.origins.first().map(|&(revision, _)| revision) != publication
                    {
                        break;
                    }
                    state = transition.after.as_ref();
                    if matches(state, after) {
                        chains.push((confidence, start..=start + offset));
                        if confidence != Confidence::PublishedTogether {
                            break;
                        }
                    }
                    if state.is_none() {
                        break;
                    }
                }
            }
            if let Some((confidence, range)) = winning_chain(&chains) {
                for &(linked, transition) in &rows[range] {
                    if !same_content(transition.before.as_ref(), transition.after.as_ref()) {
                        associations[linked]
                            .entry(path)
                            .or_default()
                            .insert(number, confidence);
                    }
                }
            }
        }
    }

    associations
        .into_iter()
        .map(|paths| {
            paths
                .into_values()
                .filter_map(|candidates| {
                    let strongest = candidates.values().copied().max();
                    let mut matches = candidates
                        .into_iter()
                        .filter(|(_, confidence)| Some(*confidence) == strongest);
                    match (matches.next(), matches.next()) {
                        (Some((patch, _)), None) => Some(patch),
                        _ => None,
                    }
                })
                .collect()
        })
        .collect()
}

fn winning_chain(
    chains: &[(Confidence, RangeInclusive<usize>)],
) -> Option<(Confidence, RangeInclusive<usize>)> {
    let strongest = chains.iter().map(|(confidence, _)| *confidence).max()?;
    let mut matches = chains
        .iter()
        .filter(|(confidence, _)| *confidence == strongest)
        .map(|(_, range)| range);
    let start = matches.clone().map(RangeInclusive::start).min()?;
    let end = matches.clone().map(RangeInclusive::end).max()?;
    // Multiple strongest chains justify one patch only if one spans the rest.
    matches
        .any(|range| range.start() == start && range.end() == end)
        .then_some((strongest, *start..=*end))
}

fn endpoint<'a>(image: Option<&'a NoteImage>, path: &str) -> Option<&'a Fingerprint> {
    image
        .filter(|image| image.path == path)
        .map(|image| &image.fingerprint)
}

fn matches(left: Option<&Fingerprint>, right: Option<&Fingerprint>) -> bool {
    same_content(left, right)
        && left
            .and_then(|f| f.id.as_deref())
            .zip(right.and_then(|f| f.id.as_deref()))
            .is_none_or(|(left, right)| left == right)
}

fn ancestor(
    git: &History<'_>,
    indexes: &BTreeMap<&str, usize>,
    from: usize,
    descendant: usize,
) -> bool {
    let mut stack = vec![descendant];
    let mut seen = BTreeSet::new();
    while let Some(index) = stack.pop() {
        if index == from {
            return true;
        }
        if index > from && seen.insert(index) {
            stack.extend(
                git.revisions[index]
                    .commit
                    .parents
                    .iter()
                    .filter_map(|parent| indexes.get(parent.as_str()).copied()),
            );
        }
    }
    false
}
