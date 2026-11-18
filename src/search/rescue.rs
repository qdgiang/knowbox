use std::collections::HashSet;
use std::fs;
use std::io::Write;

use anyhow::Context;
use jiff::civil::DateTime;

use crate::config::KbRoot;
use crate::index::ref_names;
use crate::notes::NoteEntry;
use crate::store::{self, WriteLock};

// Test candidates from the vocabulary instead of allocating O(alphabet * len²)
// edit strings. Python restricts inserted/substituted letters, not deletions or swaps.
fn one_edit(word: &str, candidate: &str) -> bool {
    let prefix: usize = word
        .chars()
        .zip(candidate.chars())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum();
    let (left, right) = (&word[prefix..], &candidate[prefix..]);
    let (mut x, mut y) = (left.chars(), right.chars());
    let (a, b) = (x.next(), y.next());
    if a.is_some() && x.as_str() == right {
        return true;
    }
    if b.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && (left == y.as_str() || a.is_some() && x.as_str() == y.as_str())
    {
        return true;
    }
    a.is_some() && b.is_some() && x.next() == b && y.next() == a && x.as_str() == y.as_str()
}

pub(super) fn typo(terms: &[String], notes: &[NoteEntry]) -> (Vec<String>, Vec<String>) {
    let mut vocab = HashSet::new();
    for entry in notes {
        let Ok(note) = &entry.parsed else { continue };
        let parts = note.search_text_parts();
        for text in parts.iter().map(|s| s.as_ref()).chain([note.body.as_str()]) {
            for token in super::tokens(&text.to_lowercase()) {
                vocab.insert(token.to_owned());
                vocab.extend(
                    token
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .filter(|p| !p.is_empty() && *p != token)
                        .map(str::to_owned),
                );
            }
        }
    }
    let mut fixed = terms.to_vec();
    let mut fixes = Vec::new();
    for t in &mut fixed {
        if t.chars().count() < 4 || vocab.contains(t.as_str()) {
            continue;
        }
        let mut hits = vocab.iter().filter(|v| one_edit(t, v));
        if let Some(only) = hits.next()
            && hits.next().is_none()
        {
            fixes.push(format!("'{t}'→'{only}'"));
            t.clone_from(only);
        }
    }
    (fixed, fixes)
}

pub(super) fn refs(root: &KbRoot, terms: &[String]) -> anyhow::Result<Vec<String>> {
    let terms: Vec<_> = terms.iter().filter(|t| t.chars().count() >= 3).collect();
    let mut hits = Vec::new();
    if terms.is_empty() {
        return Ok(hits);
    }
    let matches = |s: &str| {
        let lower = s.to_lowercase();
        terms.iter().any(|t| lower.contains(t.as_str()))
    };
    for name in ref_names(root)? {
        if !name.ends_with(".md") {
            continue;
        }
        let path = root.path().join("refs").join(&name);
        if !store::regular_file(&path)? {
            continue;
        }
        let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let text = crate::notes::normalize_newlines(&text);
        let headings: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("# ").or_else(|| line.strip_prefix("## ")))
            .map(str::trim)
            .filter(|h| matches(h))
            .take(3)
            .collect();
        if !headings.is_empty() {
            hits.push(format!("refs/{name}: {}", headings.join(" / ")));
        } else if matches(&name) || matches(&text.chars().take(2048).collect::<String>()) {
            hits.push(format!("refs/{name}: (filename or lead section)"));
        }
    }
    Ok(hits)
}

pub(super) fn log_miss(root: &KbRoot, query: &str, now: DateTime) -> anyhow::Result<()> {
    let lock = WriteLock::acquire(root)?;
    let path = lock.root().path().join(".search-misses.log");
    // Replacing instead of appending in place also protects notes aliased by hard links.
    let mut bytes = if store::regular_file(&path)? {
        fs::read(&path)?
    } else {
        Vec::new()
    };
    let query = query.replace(['\r', '\n', '\t'], " ");
    writeln!(bytes, "{now:.0}\t{query}")?;
    store::atomic_write(&path, &bytes)
        .with_context(|| format!("recording search miss in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_match_the_asymmetric_legacy_alphabet() {
        for candidate in ["grav", "graΔvy", "grow", "grwa"] {
            let word = match candidate {
                "grav" | "graΔvy" => "graΔv",
                _ => "graw",
            };
            assert!(one_edit(word, candidate));
        }
        assert!(!one_edit("mist", "miΔt"));
        assert!(!one_edit("mist", "mostt"));
        assert!(!one_edit("mist", "mist"));
    }
}
