use std::collections::{BTreeMap, BTreeSet};

use crate::journal::{local_day, split_heading};
use crate::notes::id_prefix;

use super::{is_line_break, iso_date, repr};

pub(super) const MISSING_CREATE: &str =
    "no create entry in log.md (created after the journal started — hand-made note?)";

#[derive(Debug, Default)]
pub(super) struct History<'a> {
    first: Option<&'a str>,
    creates: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> History<'a> {
    pub(super) fn parse(text: &'a str) -> Self {
        let mut history = Self::default();
        for line in text.split(['\r', '\n']) {
            let Some((date, rest)) =
                split_heading(line).and_then(|(stamp, rest)| Some((local_day(stamp)?, rest)))
            else {
                continue;
            };
            history.first.get_or_insert(date);
            if let Some(id) = rest.strip_prefix(" create | ").and_then(id_prefix) {
                history.creates.entry(id).or_default().push(date);
            }
        }
        history
    }

    pub(super) fn warnings(&self, id: &str, created: &str, display_date: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(dates) = self.creates.get(id) {
            if dates.len() > 1 {
                warnings.push(format!("log.md has {} create entries for {id} (historical id recycling — see the P9 gotcha note)", dates.len()));
            }
            if !dates.contains(&created) {
                let unique = dates
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ");
                warnings.push(format!(
                    "created: {display_date} matches no log.md create date for {id} ({unique})"
                ));
            }
        }
        warnings
    }

    pub(super) fn missing_create(&self, id: &str, created: &str) -> bool {
        iso_date(created)
            && !self.creates.contains_key(id)
            && self.first.is_some_and(|first| created > first)
    }
}

const CATEGORY: &str = "created-vs-log";

#[derive(Debug, Default)]
pub(super) struct Waivers<'a> {
    unused: BTreeSet<&'a str>,
    matched: BTreeSet<&'a str>,
    malformed: Vec<String>,
}

impl<'a> Waivers<'a> {
    pub(super) fn parse(text: &'a str) -> Self {
        let mut waivers = Self::default();
        for line in text.split(is_line_break).map(str::trim) {
            let Some(id) = id_prefix(line) else { continue };
            let rest = &line[id.len()..];
            if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
                continue;
            }
            let mut parts = rest.split_whitespace();
            let valid_category = |cat: &&str| {
                cat.bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
            };
            match (
                parts.next().filter(valid_category),
                parts.next().filter(|date| iso_date(date)),
                parts.next(),
            ) {
                (Some(CATEGORY), Some(_), Some(_)) => {
                    waivers.unused.insert(id);
                }
                (Some(cat), Some(_), Some(_)) => waivers.malformed.push(format!(
                    "unknown category {} in line '{line}' (waivable: {CATEGORY})",
                    repr(cat)
                )),
                _ => waivers.malformed.push(format!(
                    "line '{line}' is not `kb-XXXX <category> <date> <reason>`"
                )),
            }
        }
        waivers
    }

    pub(super) fn applies(&mut self, id: &str) -> bool {
        if let Some(waiver) = self.unused.take(id) {
            self.matched.insert(waiver);
        }
        self.matched.contains(id)
    }

    pub(super) fn append_unused_warnings(self, warnings: &mut Vec<String>) {
        warnings.extend(
            self.malformed
                .into_iter()
                .map(|warning| format!("lint-waivers.md: {warning}")),
        );
        warnings.extend(self.unused.into_iter().map(|id| format!(
            "lint-waivers.md: stale waiver {id} {CATEGORY} — suppressed nothing this run; remove the line"
        )));
    }
}
