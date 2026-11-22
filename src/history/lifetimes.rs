use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::changes::audit::{Fingerprint, same_content};
use crate::config::KbRoot;
use crate::git::history::History;
use crate::store::Inventory;
use crate::{journal, notes};

use super::diffs::{Diff, NoteImage};
use super::journal_rows::Occurrence;

#[derive(Debug, Serialize)]
pub(super) struct Lifetime {
    pub(super) created: Option<String>,
    paths: BTreeSet<String>,
    current: bool,
}

#[derive(Default)]
pub(super) struct Lifetimes {
    pub(super) rows: Vec<Lifetime>,
    image_matches: BTreeMap<(String, String), BTreeSet<usize>>,
    pub(super) patches: Vec<BTreeSet<usize>>,
    pub(super) journal: Vec<Option<usize>>,
}

type Active = BTreeMap<String, (usize, Fingerprint, Option<String>)>;

pub(super) fn read(
    root: &KbRoot,
    policy: &mut Inventory<'_>,
    id: &str,
    git: Option<&History<'_>>,
    diffs: &[Diff],
    journal: &[Occurrence],
    links: &[BTreeSet<usize>],
) -> Result<Lifetimes> {
    let current = working_note(root, policy, id)?;
    let mut result = Lifetimes::default();
    let mut active = Active::new();
    if let Some(git) = git {
        let mut revisions = BTreeMap::<&str, Active>::new();
        let mut patches = diffs.iter().peekable();
        for (index, revision) in git.revisions.iter().enumerate() {
            let parents: Vec<_> = revision
                .commit
                .parents
                .iter()
                .filter_map(|parent| revisions.get(parent.as_str()))
                .collect();
            active = revision
                .commit
                .parents
                .first()
                .and_then(|parent| revisions.get(parent.as_str()))
                .cloned()
                .unwrap_or_default();
            while let Some(diff) = patches.next_if(|diff| diff.revision == index) {
                result.change(id, diff, &mut active, &parents);
            }
            revisions.insert(&revision.commit.oid, active.clone());
        }
    }
    if let Some(image) = current {
        let exact = active
            .get(&image.path)
            .filter(|(_, _, created)| compatible(created, &image.created));
        let renamed: BTreeSet<_> = active
            .values()
            .filter(|(_, old, created)| {
                compatible(created, &image.created)
                    && same_content(Some(old), Some(&image.fingerprint))
            })
            .map(|(number, _, _)| *number)
            .collect();
        let number = exact
            .map(|(number, _, _)| *number)
            .or_else(|| one(&renamed))
            .unwrap_or_else(|| result.add(image.created.clone()));
        result.record(number, &image);
        result.rows[number].current = true;
    }

    result.journal = vec![None; journal.len()];
    let mut create_groups = BTreeSet::new();
    for (index, item) in journal.iter().enumerate() {
        let heading = &item.entry.heading;
        if heading.action != "create" || heading.id.as_deref() != Some(id) {
            continue;
        }
        let day = journal::local_day(&heading.time);
        let exact = result.journal_group(id, item, &links[index]);
        let dated: BTreeSet<_> = result
            .rows
            .iter()
            .enumerate()
            .filter(|(number, row)| {
                day == row.created.as_deref() && !create_groups.contains(number)
            })
            .map(|(number, _)| number)
            .collect();
        let number = exact
            .or_else(|| one(&dated))
            .unwrap_or_else(|| result.add(day.map(str::to_owned)));
        create_groups.insert(number);
        result.journal[index] = Some(number);
    }
    for (index, item) in journal.iter().enumerate() {
        if result.journal[index].is_none() {
            result.journal[index] = result
                .journal_group(id, item, &links[index])
                .or_else(|| result.by_date(journal::local_day(&item.entry.heading.time)));
        }
    }
    Ok(result)
}

fn working_note(root: &KbRoot, policy: &mut Inventory<'_>, id: &str) -> Result<Option<NoteImage>> {
    if let Some(path) = notes::find_with(root, id, policy)?
        && let Some(bytes) = notes::read_with(&path, policy)?
    {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid current note path")?;
        let note = NoteImage::new(&format!("notes/{name}"), &bytes);
        if note.id() == Some(id) {
            return Ok(Some(note));
        }
    }
    // A rename can remove the ID prefix while leaving the frontmatter ID.
    let notes_dir = root.path().join("notes");
    for name in policy.names(&notes_dir, crate::store::EntryKind::MarkdownFile)? {
        let path = notes_dir.join(&name);
        if let Some(bytes) = notes::read_with(&path, policy)? {
            let image = NoteImage::new(&format!("notes/{name}"), &bytes);
            if image.id() == Some(id) {
                return Ok(Some(image));
            }
        }
    }
    Ok(None)
}

fn compatible(left: &Option<String>, right: &Option<String>) -> bool {
    left.as_deref()
        .zip(right.as_deref())
        .is_none_or(|(left, right)| left == right)
}

fn one(values: &BTreeSet<usize>) -> Option<usize> {
    (values.len() == 1)
        .then(|| values.first().copied())
        .flatten()
}

impl Lifetimes {
    fn add(&mut self, created: Option<String>) -> usize {
        let number = self.rows.len();
        self.rows.push(Lifetime {
            created,
            paths: BTreeSet::new(),
            current: false,
        });
        number
    }

    fn record(&mut self, number: usize, image: &NoteImage) {
        self.rows[number].paths.insert(image.path.clone());
        if self.rows[number].created.is_none() {
            self.rows[number].created.clone_from(&image.created);
        }
        for hash in hashes(&image.fingerprint) {
            self.image_matches
                .entry((image.path.clone(), hash.to_owned()))
                .or_default()
                .insert(number);
        }
    }

    fn change(&mut self, id: &str, diff: &Diff, active: &mut Active, parents: &[&Active]) {
        let before = diff.before.as_ref().filter(|image| image.id() == Some(id));
        let after = diff.after.as_ref().filter(|image| image.id() == Some(id));
        let mut groups = BTreeSet::new();
        let prior = before.map(|image| {
            let number = active
                .remove(&image.path)
                .map(|(number, _, _)| number)
                .unwrap_or_else(|| self.add(image.created.clone()));
            self.record(number, image);
            groups.insert(number);
            number
        });
        if let Some(image) = after {
            let incoming: BTreeSet<_> = parents
                .iter()
                .filter_map(|parent| {
                    let (number, fingerprint, created) = parent.get(&image.path)?;
                    (same_content(Some(fingerprint), Some(&image.fingerprint))
                        && compatible(created, &image.created))
                    .then_some(*number)
                })
                .collect();
            let number = prior
                .filter(|_| {
                    before.is_some_and(|before| compatible(&before.created, &image.created))
                })
                .or_else(|| one(&incoming))
                .unwrap_or_else(|| self.add(image.created.clone()));
            self.record(number, image);
            groups.insert(number);
            active.insert(
                image.path.clone(),
                (number, image.fingerprint.clone(), image.created.clone()),
            );
        }
        self.patches.push(groups);
    }

    fn journal_group(
        &self,
        id: &str,
        item: &Occurrence,
        patches: &BTreeSet<usize>,
    ) -> Option<usize> {
        let from_patches: BTreeSet<_> = patches
            .iter()
            .flat_map(|&patch| &self.patches[patch])
            .copied()
            .collect();
        if let Some(number) = one(&from_patches) {
            return Some(number);
        }
        let mut matched = BTreeSet::new();
        for transition in &item.entry.transitions {
            for state in transition
                .before
                .iter()
                .chain(&transition.after)
                .filter(|state| state.id.as_deref() == Some(id))
            {
                for hash in hashes(state) {
                    if let Some(found) = self
                        .image_matches
                        .get(&(transition.path.clone(), hash.to_owned()))
                    {
                        matched.extend(found);
                    }
                }
            }
        }
        one(&matched)
    }

    pub(super) fn by_date(&self, date: Option<&str>) -> Option<usize> {
        if self.rows.len() == 1 {
            return Some(0);
        }
        let date = date?;
        if self.rows.iter().any(|row| row.created.is_none()) {
            return None;
        }
        let mut best = None;
        let mut latest = "";
        for (number, row) in self.rows.iter().enumerate() {
            let created = row.created.as_deref()?;
            if created <= date && created >= latest {
                best = (created > latest).then_some(number);
                latest = created;
            }
        }
        best
    }
}

fn hashes(state: &Fingerprint) -> impl Iterator<Item = &str> {
    std::iter::once(state.sha1.as_str()).chain(state.lf_sha1.as_deref())
}
