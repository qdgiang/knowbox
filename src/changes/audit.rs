//! The upgrade seed is local and immutable. Every subsequent recognized change
//! travels with its human journal entry, including changes published by another host.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::notes::{self, Note, NoteEntry};
use crate::store::{self, WriteLock};

const SEED: &str = ".kb-audit-seed.json";
const PREFIX: &str = "<!-- knowbox-audit-v1 ";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Fingerprint {
    pub(crate) id: Option<String>,
    pub(crate) sha1: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) lf_sha1: Option<String>,
}

impl Fingerprint {
    fn new(path: &str, note: Option<&Note>, sha1: String, lf_sha1: Option<String>) -> Self {
        let id = note
            .and_then(|note| note.scalar("id"))
            .filter(|id| notes::is_id(id))
            .or_else(|| path.strip_prefix("notes/").and_then(notes::id_prefix))
            .map(str::to_owned);
        Self { id, sha1, lf_sha1 }
    }

    pub(crate) fn from_bytes(path: &str, bytes: &[u8]) -> Self {
        // Malformed notes still have a byte identity and can carry an ID in the filename.
        Self::new(
            path,
            notes::parse(bytes).as_ref().ok(),
            notes::sha1_hex(bytes),
            notes::lf_sha1(bytes),
        )
    }

    fn valid(&self) -> bool {
        let valid_hash = |hash: &str| {
            hash.len() == 40
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        self.id.as_deref().is_none_or(notes::is_id)
            && valid_hash(&self.sha1)
            && self.lf_sha1.as_deref().is_none_or(valid_hash)
    }

    fn content_hash(&self) -> &str {
        self.lf_sha1.as_deref().unwrap_or(&self.sha1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Transition {
    pub(crate) path: String,
    pub(crate) before: Option<Fingerprint>,
    pub(crate) after: Option<Fingerprint>,
}

fn note_path(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("reading note filename {path:?}"))?;
    ensure!(name.ends_with(".md"), "expected Markdown note at {path:?}");
    Ok(format!("notes/{name}"))
}

fn valid_path(path: &str) -> bool {
    path.strip_prefix("notes/")
        .is_some_and(|name| name.ends_with(".md") && !name.contains(['/', '\0']))
}

impl Transition {
    pub(crate) fn between(
        path: &Path,
        before: Option<&[u8]>,
        after: Option<&[u8]>,
    ) -> Result<Self> {
        ensure!(
            before.is_some() || after.is_some(),
            "missing both note images for {path:?}"
        );
        let path = note_path(path)?;
        Ok(Self {
            before: before.map(|bytes| Fingerprint::from_bytes(&path, bytes)),
            after: after.map(|bytes| Fingerprint::from_bytes(&path, bytes)),
            path,
        })
    }

    pub(crate) fn id(&self) -> &str {
        self.after
            .as_ref()
            .and_then(|state| state.id.as_deref())
            .or_else(|| self.before.as_ref().and_then(|state| state.id.as_deref()))
            .unwrap_or("—")
    }

    pub(crate) fn description(&self) -> String {
        let verb = match (&self.before, &self.after) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            _ => "edited",
        };
        // Debug quoting keeps arbitrary filenames inside a single journal heading.
        format!("{verb} {:?}", self.path)
    }

    pub(crate) fn write_line(&self, out: &mut impl Write) -> Result<()> {
        let json = serde_json::to_string(self)?.replace("--", r"\u002d\u002d");
        writeln!(out, "{PREFIX}{json} -->")?;
        Ok(())
    }

    fn valid(&self) -> bool {
        valid_path(&self.path)
            && (self.before.is_some() || self.after.is_some())
            && self
                .before
                .iter()
                .chain(&self.after)
                .all(Fingerprint::valid)
    }
}

type States = BTreeMap<String, Fingerprint>;

pub(crate) fn same_content(before: Option<&Fingerprint>, after: Option<&Fingerprint>) -> bool {
    // Git's LF normalization can change raw bytes between machines.
    before.map(|state| &state.sha1) == after.map(|state| &state.sha1)
        || before.map(Fingerprint::content_hash) == after.map(Fingerprint::content_hash)
}

pub(crate) fn decode(line: &str) -> Result<Option<Transition>> {
    let Some(payload) = line.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let json = payload
        .strip_suffix(" -->")
        .context("unterminated audit comment")?;
    let row: Transition = serde_json::from_str(json)?;
    ensure!(row.valid(), "invalid journal fingerprint");
    Ok(Some(row))
}

#[derive(Serialize, Deserialize)]
struct Seed {
    version: u8,
    started: String,
    notes: States,
}

#[derive(Debug)]
pub(crate) struct Baseline {
    notes: States,
}

fn observed(notes: &[NoteEntry]) -> States {
    notes
        .iter()
        .map(|entry| {
            (
                entry.relative.clone(),
                Fingerprint::new(
                    &entry.relative,
                    entry.parsed.as_ref().ok(),
                    entry.sha1.clone(),
                    entry.lf_sha1.clone(),
                ),
            )
        })
        .collect()
}

impl Baseline {
    pub(crate) fn load(lock: &WriteLock) -> Result<Self> {
        let root = lock.root();
        let journal = super::read_log(root)?;
        let path = root.path().join(SEED);
        let existing = store::regular_file(&path)
            .with_context(|| format!("checking audit seed {}", path.display()))?;
        let seed = if existing {
            let bytes = fs::read(&path)
                .with_context(|| format!("reading audit seed {}", path.display()))?;
            let seed: Seed = serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing audit seed {}", path.display()))?;
            ensure!(
                seed.version == 1,
                "unsupported audit seed version {}",
                seed.version
            );
            ensure!(
                seed.notes
                    .iter()
                    .all(|(p, state)| valid_path(p) && state.valid()),
                "invalid fingerprints in audit seed {}",
                path.display()
            );
            seed
        } else {
            let current = observed(&notes::snapshot(root)?);
            let notes = match crate::index::previous_hashes(root)? {
                None => current,
                Some(stored) => stored
                    .into_iter()
                    .map(|(path, hash)| {
                        let state = current
                            .get(&path)
                            .filter(|entry| entry.sha1 == hash || entry.content_hash() == hash)
                            .cloned()
                            .unwrap_or_else(|| Fingerprint::new(&path, None, hash, None));
                        (path, state)
                    })
                    .collect(),
            };
            Seed {
                version: 1,
                started: jiff::Timestamp::now().to_string(),
                notes,
            }
        };
        let new_seed = (!existing).then(|| serde_json::to_vec(&seed)).transpose()?;
        let mut baseline = Self { notes: seed.notes };
        baseline.replay(&journal)?;
        if let Some(bytes) = new_seed {
            store::atomic_write(&path, &bytes)?;
        }
        Ok(baseline)
    }

    fn replay(&mut self, journal: &str) -> Result<()> {
        // A freshly cloned machine's local seed can already contain the tip of
        // the entire synced journal. Its first record for each path takes precedence.
        let mut seen = BTreeSet::new();
        let mut under_heading = false;
        for (line_no, line) in notes::normalize_newlines(journal).lines().enumerate() {
            if !line.starts_with(PREFIX) {
                under_heading = crate::journal::split_heading(line).is_some_and(|(date, _)| {
                    date.contains('T') && crate::journal::local_day(date).is_some()
                });
                continue;
            }
            if !under_heading {
                continue;
            }
            let Some(transition) = decode(line).with_context(|| {
                format!("reading audit metadata at log.md line {}", line_no + 1)
            })?
            else {
                continue;
            };
            if !seen.insert(transition.path.clone()) {
                ensure!(
                    same_content(self.notes.get(&transition.path), transition.before.as_ref()),
                    "conflicting audit predecessor for {:?} at log.md line {}",
                    transition.path,
                    line_no + 1
                );
            }
            match transition.after {
                Some(after) => {
                    self.notes.insert(transition.path, after);
                }
                None => {
                    self.notes.remove(&transition.path);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn drift(&self, entries: &[NoteEntry]) -> Vec<Transition> {
        let mut current = observed(entries);
        let mut transitions = Vec::new();
        for (path, before) in &self.notes {
            let after = current.remove(path);
            if !same_content(Some(before), after.as_ref()) {
                transitions.push(Transition {
                    path: path.clone(),
                    before: Some(before.clone()),
                    after,
                });
            }
        }
        transitions.extend(current.into_iter().map(|(path, after)| Transition {
            path,
            before: None,
            after: Some(after),
        }));
        transitions.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        transitions
    }

    pub(crate) fn preimage(&self, path: &Path, bytes: Option<&[u8]>) -> Result<Option<Transition>> {
        let path = note_path(path)?;
        let before = self.notes.get(&path);
        let after = bytes.map(|bytes| Fingerprint::from_bytes(&path, bytes));
        Ok((!same_content(before, after.as_ref())).then(|| Transition {
            before: before.cloned(),
            after,
            path,
        }))
    }
}

#[cfg(test)]
mod tests;
