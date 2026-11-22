mod detail;

use std::borrow::Cow;
use std::collections::BTreeMap;
#[cfg(test)]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::config::KbRoot;
use crate::store::{EntryKind, Inventory, directory, read_regular};

pub(crate) use detail::{read_detail, read_detail_path};

const MAX_LIST_DEPTH: usize = 128;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(crate) enum NoteValue {
    Scalar(String),
    List(Vec<NoteValue>),
}

/// A Markdown body and its scalar/list frontmatter.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Note {
    pub(crate) body: String,
    #[serde(rename = "frontmatter")]
    pub(crate) meta: BTreeMap<String, NoteValue>,
}

impl Note {
    /// Returns a metadata scalar, but not a list rendered as a string.
    pub(crate) fn scalar(&self, key: &str) -> Option<&str> {
        match self.meta.get(key) {
            Some(NoteValue::Scalar(value)) => Some(value),
            Some(NoteValue::List(_)) | None => None,
        }
    }

    /// Returns the index's display form (Python `str`), or `default` when absent.
    pub(crate) fn text<'a>(&'a self, key: &str, default: &'a str) -> Cow<'a, str> {
        match self.meta.get(key) {
            Some(NoteValue::Scalar(value)) => Cow::Borrowed(value),
            Some(NoteValue::List(values)) => Cow::Owned(list_repr(values)),
            None => Cow::Borrowed(default),
        }
    }

    /// Returns all string items, recursively flattening inline/nested lists.
    pub(crate) fn items(&self, key: &str) -> Vec<&str> {
        let mut result = Vec::new();
        if let Some(value) = self.meta.get(key) {
            collect_items(value, &mut result);
        }
        result
    }

    /// The shared text projection used by indexing, embeddings, and search.
    pub(crate) fn search_text_parts(&self) -> [Cow<'_, str>; 5] {
        [
            self.text("title", ""),
            self.text("summary", ""),
            self.search_items("tags"),
            self.search_items("aliases"),
            self.search_items("triggers"),
        ]
    }

    fn search_items(&self, key: &str) -> Cow<'_, str> {
        match self.meta.get(key) {
            Some(NoteValue::Scalar(value)) => Cow::Borrowed(value),
            Some(NoteValue::List(values)) => Cow::Owned(
                values
                    .iter()
                    .map(|value| match value {
                        NoteValue::Scalar(text) => Cow::Borrowed(text.as_str()),
                        NoteValue::List(items) => Cow::Owned(list_repr(items)),
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            None => Cow::Borrowed(""),
        }
    }
}

fn collect_items<'a>(value: &'a NoteValue, result: &mut Vec<&'a str>) {
    match value {
        NoteValue::Scalar(value) => result.push(value),
        NoteValue::List(values) => {
            for value in values {
                collect_items(value, result);
            }
        }
    }
}

fn list_repr(values: &[NoteValue]) -> String {
    let mut result = String::new();
    push_list_repr(&mut result, values);
    result
}

fn push_list_repr(out: &mut String, values: &[NoteValue]) {
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        match value {
            NoteValue::Scalar(text) => push_string_repr(out, text),
            NoteValue::List(items) => push_list_repr(out, items),
        }
    }
    out.push(']');
}

fn push_string_repr(out: &mut String, text: &str) {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    out.push(quote);
    for ch in text.chars() {
        match ch {
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\\' => out.push_str("\\\\"),
            ch if ch == quote => {
                out.push('\\');
                out.push(ch);
            }
            ch if ch.is_control() || (ch != ' ' && ch.is_whitespace()) => {
                let code = u32::from(ch);
                if code <= 0xff {
                    out.push_str(&format!("\\x{code:02x}"));
                } else {
                    out.push_str(&format!("\\u{code:04x}"));
                }
            }
            ch => out.push(ch),
        }
    }
    out.push(quote);
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum ParseError {
    #[error("invalid UTF-8")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    #[error("unterminated frontmatter (no closing '---')")]
    Malformed,
    #[error("frontmatter list nesting exceeds {MAX_LIST_DEPTH}")]
    ListDepth,
}

/// Convert CRLF and old-Mac CR line endings to LF without otherwise changing text.
pub(crate) fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }

    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            chars.next_if_eq(&'\n');
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    Cow::Owned(normalized)
}

/// Parse the deliberately minimal frontmatter dialect used by the Python tool.
pub(crate) fn parse(bytes: &[u8]) -> Result<Note, ParseError> {
    let text = normalize_newlines(std::str::from_utf8(bytes)?);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);

    if !text.starts_with("---") {
        return Ok(Note {
            body: text.to_owned(),
            meta: BTreeMap::new(),
        });
    }

    let Some(end) = text[3..].find("\n---").map(|offset| offset + 3) else {
        return Err(ParseError::Malformed);
    };
    let frontmatter = text[3..end].trim_matches('\n');
    let body = text[end + 4..].trim_start_matches('\n');
    let mut meta = BTreeMap::new();
    let mut current_key = None;

    for line in frontmatter.split('\n') {
        let indented = line.trim_start_matches(python_whitespace);
        if indented.is_empty() || indented.starts_with('#') {
            continue;
        }
        if let Some((key, raw)) = key_line(line) {
            current_key = Some(key);
            let raw = raw.trim_matches(python_whitespace);
            meta.insert(
                key.to_owned(),
                if raw.is_empty() {
                    NoteValue::List(Vec::new())
                } else {
                    scalar_value(raw, 0)?
                },
            );
            continue;
        }
        if line.len() != indented.len()
            && let Some(item) = indented.strip_prefix('-')
            && item.starts_with(python_whitespace)
            && let Some(existing) = current_key.and_then(|key| meta.get_mut(key))
        {
            let value = scalar_value(item.trim_matches(python_whitespace), 0)?;
            match existing {
                NoteValue::List(items) => items.push(value),
                NoteValue::Scalar(previous) => {
                    let items = if previous.is_empty() {
                        vec![value]
                    } else {
                        vec![NoteValue::Scalar(std::mem::take(previous)), value]
                    };
                    *existing = NoteValue::List(items);
                }
            }
        }
    }

    Ok(Note {
        body: body.to_owned(),
        meta,
    })
}

pub(crate) fn key_line(line: &str) -> Option<(&str, &str)> {
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'_')
    {
        return None;
    }
    Some((key, &line[colon + 1..]))
}

fn python_whitespace(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')
}

fn scalar_value(raw: &str, depth: usize) -> Result<NoteValue, ParseError> {
    if let Some(inner) = raw
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        if depth == MAX_LIST_DEPTH {
            return Err(ParseError::ListDepth);
        }
        return split_csv(inner.trim_matches(python_whitespace))
            .into_iter()
            .map(|part| scalar_value(part, depth + 1))
            .collect::<Result<_, _>>()
            .map(NoteValue::List);
    }
    if (raw.starts_with('"') && raw.ends_with('"'))
        || (raw.starts_with('\'') && raw.ends_with('\''))
    {
        return Ok(NoteValue::Scalar(
            raw.get(1..raw.len() - 1).unwrap_or("").to_owned(),
        ));
    }
    Ok(NoteValue::Scalar(raw.to_owned()))
}

fn split_csv(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quote = None;
    for (index, ch) in text.char_indices() {
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if ch == ',' {
            result.push(text[start..index].trim_matches(python_whitespace));
            start = index + ch.len_utf8();
        }
    }
    let last = text[start..].trim_matches(python_whitespace);
    if !last.is_empty() {
        result.push(last);
    }
    result
}

#[derive(Debug, Error)]
pub(crate) enum NoteError {
    #[error("inspecting {}", .path.display())]
    Inspect { path: PathBuf, source: io::Error },
    #[error("reading note {}", .path.display())]
    Read { path: PathBuf, source: io::Error },
}

#[derive(Debug)]
pub(crate) struct NoteEntry {
    pub(crate) relative: String,
    pub(crate) size: usize,
    pub(crate) sha1: String,
    pub(crate) lf_sha1: Option<String>,
    pub(crate) parsed: Result<Note, ParseError>,
}

/// Read the direct `.md` files in `notes/`, preserving parse failures for lint.
pub(crate) fn snapshot(root: &KbRoot) -> Result<Vec<NoteEntry>, NoteError> {
    snapshot_with(root, &mut Inventory::Strict)
}

pub(crate) fn snapshot_with(
    root: &KbRoot,
    policy: &mut Inventory<'_>,
) -> Result<Vec<NoteEntry>, NoteError> {
    let notes = root.path().join("notes");
    let mut entries = Vec::new();
    for name in inventory(&notes, policy)? {
        if let Some(bytes) = read_with(&notes.join(&name), policy)? {
            entries.push(NoteEntry {
                relative: format!("notes/{name}"),
                size: bytes.len(),
                sha1: sha1_hex(&bytes),
                lf_sha1: lf_sha1(&bytes),
                parsed: parse(&bytes),
            });
        }
    }
    Ok(entries)
}

pub(crate) fn read_with(
    path: &Path,
    policy: &mut Inventory<'_>,
) -> Result<Option<Vec<u8>>, NoteError> {
    if !policy.file(path).map_err(|source| NoteError::Inspect {
        path: path.into(),
        source,
    })? {
        return Ok(None);
    }
    read_regular(path)
        .map(Some)
        .map_err(|source| NoteError::Read {
            path: path.into(),
            source,
        })
}

pub(crate) fn validate_inventory(root: &KbRoot) -> Result<(), NoteError> {
    inventory(&root.path().join("notes"), &mut Inventory::Strict)?;
    Ok(())
}

pub(crate) fn sha1_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha1::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}

pub(crate) fn lf_sha1(bytes: &[u8]) -> Option<String> {
    if !bytes.windows(2).any(|pair| pair == b"\r\n") || bytes.contains(&0) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    Some(sha1_hex(text.replace("\r\n", "\n").as_bytes()))
}

fn inventory(notes: &Path, policy: &mut Inventory<'_>) -> Result<Vec<String>, NoteError> {
    let inspect = |source| NoteError::Inspect {
        path: notes.into(),
        source,
    };
    if !directory(notes).map_err(inspect)? {
        return Ok(Vec::new());
    }
    policy
        .names(notes, EntryKind::MarkdownFile)
        .map_err(inspect)
}

#[derive(Debug, Error)]
pub(crate) enum LookupError {
    #[error(transparent)]
    Inventory(#[from] NoteError),
    #[error("ambiguous slug {selector:?} matches: {} — use the full kb-id", .hits.join(", "))]
    Ambiguous { selector: String, hits: Vec<String> },
}

/// Resolve a selector using only sorted, direct regular files under `notes/`.
pub(crate) fn find(root: &KbRoot, selector: &str) -> Result<Option<PathBuf>, LookupError> {
    find_with(root, selector, &mut Inventory::Strict)
}

pub(crate) fn find_for_write(
    root: &KbRoot,
    selector: &str,
) -> Result<Option<PathBuf>, LookupError> {
    let dir = root.path().join("notes");
    if let Some(name) = selector.strip_prefix("notes/") {
        return Ok(inventory(&dir, &mut Inventory::Strict)?
            .into_iter()
            .find(|candidate| candidate == name)
            .map(|name| dir.join(name)));
    }
    if is_id(selector) {
        let hits = inventory(&dir, &mut Inventory::Strict)?
            .into_iter()
            .filter(|name| {
                name.strip_prefix(selector)
                    .is_some_and(|tail| tail == ".md" || tail.starts_with('-'))
            })
            .collect();
        return unique_hit(&dir, selector, hits);
    }
    find(root, selector)
}

pub(crate) fn find_with(
    root: &KbRoot,
    selector: &str,
    policy: &mut Inventory<'_>,
) -> Result<Option<PathBuf>, LookupError> {
    if !valid_selector(selector) {
        return Ok(None);
    }
    let notes = root.path().join("notes");
    let names = inventory(&notes, policy)?;
    if is_id(selector)
        && let Some(name) = names.iter().find(|name| {
            name.strip_prefix(selector)
                .is_some_and(|rest| rest == ".md" || rest.starts_with('-'))
        })
    {
        return Ok(Some(notes.join(name)));
    }

    let basename = if selector.ends_with(".md") {
        Cow::Borrowed(selector)
    } else {
        Cow::Owned(format!("{selector}.md"))
    };
    // Exact basenames follow the volume's case rules; IDs and substrings do not.
    let candidate = notes.join(basename.as_ref());
    if policy
        .file(&candidate)
        .map_err(|source| NoteError::Inspect {
            path: candidate.clone(),
            source,
        })?
    {
        return Ok(Some(candidate));
    }

    let hits = names
        .into_iter()
        .filter(|name| name.contains(selector))
        .collect();
    unique_hit(&notes, selector, hits)
}

fn unique_hit(
    notes: &Path,
    selector: &str,
    hits: Vec<String>,
) -> Result<Option<PathBuf>, LookupError> {
    match hits.as_slice() {
        [] => Ok(None),
        [name] => Ok(Some(notes.join(name))),
        _ => Err(LookupError::Ambiguous {
            selector: selector.to_owned(),
            hits,
        }),
    }
}

fn valid_selector(selector: &str) -> bool {
    !selector.is_empty()
        && selector != "."
        && !selector.contains("..")
        && selector
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(crate) fn is_id(value: &str) -> bool {
    id_prefix(value) == Some(value)
}

pub(crate) fn id_prefix(value: &str) -> Option<&str> {
    value.get(..7).filter(|prefix| {
        prefix
            .strip_prefix("kb-")
            .is_some_and(|digits| digits.bytes().all(|b| b.is_ascii_digit()))
    })
}

pub(crate) fn body_links(body: &str) -> impl Iterator<Item = &str> {
    body.match_indices("[[kb-").filter_map(move |(start, _)| {
        let target = body.get(start + 2..)?;
        let id = id_prefix(target)?;
        target[id.len()..].starts_with("]]").then_some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> anyhow::Result<(tempfile::TempDir, KbRoot)> {
        let dir = tempfile::tempdir()?;
        fs::create_dir(dir.path().join("notes"))?;
        let root = KbRoot::open(dir.path())?;
        Ok((dir, root))
    }

    #[test]
    fn parses_scalars_lists_and_body_without_normalizing_yaml() -> anyhow::Result<()> {
        let note = parse(
            b"\xef\xbb\xbf---\r\ntitle: 'A title'\r\ntags: [one, \"two, too\"]\r\naliases:\r\n  - first\r\n  - second\r\nnumber: 01\r\n---\r\nbody\r\nline\r\n",
        )?;
        assert_eq!(note.scalar("title"), Some("A title"));
        assert_eq!(note.scalar("tags"), None);
        assert_eq!(note.items("tags"), vec!["one", "two, too"]);
        assert_eq!(note.items("aliases"), vec!["first", "second"]);
        assert_eq!(note.text("tags", "missing"), "['one', 'two, too']");
        assert_eq!(note.text("number", "missing"), "01");
        assert_eq!(note.text("unlisted", "missing"), "missing");
        assert_eq!(note.body, "body\nline\n");
        Ok(())
    }

    #[test]
    fn delimiter_prefixes_and_body_only_notes_match_legacy() -> anyhow::Result<()> {
        let note = parse(b"---title: value\r---suffix\r\rstart\r")?;
        assert_eq!(note.scalar("title"), Some("value"));
        assert_eq!(note.body, "suffix\n\nstart\n");
        let note = parse(b"\xef\xbb\xbftitle: body\r\n---\r\n")?;
        assert_eq!(note.scalar("title"), None);
        assert_eq!(note.body, "title: body\n---\n");
        Ok(())
    }

    #[test]
    fn block_items_extend_previous_key_and_duplicates_reset_it() -> anyhow::Result<()> {
        let note = parse(b"---\ntags: discarded\ntags: ''\n  # ignore\nnot-a-key: ignored\n- no indent\n  -\tkept\nsummary: first # literal\n  - second\ntitle: '\n---\n")?;
        assert_eq!(note.items("tags"), ["kept"]);
        assert_eq!(note.items("summary"), ["first # literal", "second"]);
        assert_eq!(note.scalar("title"), Some(""));
        Ok(())
    }

    #[test]
    fn search_projection_stringifies_nested_items_but_items_flattens_them() -> anyhow::Result<()> {
        let note = parse(
            b"---\ntitle: [a, b]\ntags: [[one], 'two, too',, ]\naliases: synonym\ntriggers:\n---\n",
        )?;
        assert_eq!(note.items("tags"), ["one", "two, too", ""]);
        assert_eq!(
            note.search_text_parts().map(Cow::into_owned),
            ["['a', 'b']", "", "['one'] two, too ", "synonym", ""]
        );
        Ok(())
    }

    #[test]
    fn excessive_list_nesting_is_reported_without_unbounded_recursion() {
        let text = format!("---\ntags: {}x{}\n---", "[".repeat(129), "]".repeat(129));
        assert_eq!(parse(text.as_bytes()), Err(ParseError::ListDepth));
    }

    #[test]
    fn malformed_frontmatter_is_carried_as_a_specific_error() {
        assert_eq!(
            parse(b"---\ntitle: unfinished\n"),
            Err(ParseError::Malformed)
        );
        assert_eq!(
            ParseError::Malformed.to_string(),
            "unterminated frontmatter (no closing '---')"
        );
    }

    #[test]
    fn invalid_utf8_is_distinct_from_unterminated_frontmatter() {
        assert!(
            matches!(parse(b"---\n\xff"), Err(ParseError::InvalidUtf8(error)) if error.valid_up_to() == 4)
        );
    }

    #[test]
    fn newline_normalization_keeps_bom_and_borrowed_lf() {
        assert!(matches!(
            normalize_newlines("\u{feff}a\nb"),
            Cow::Borrowed("\u{feff}a\nb")
        ));
        assert_eq!(normalize_newlines("\u{feff}a\r\nb\rc"), "\u{feff}a\nb\nc");
    }

    #[test]
    fn lookup_selects_id_then_basename_then_unique_substring() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let notes = root.path().join("notes");
        for name in [
            "topic.md",
            "kb-0001.md",
            "kb-0001-topic.md",
            "kb-0002-topic.md",
            "not-a-note.txt",
        ] {
            fs::write(notes.join(name), b"")?;
        }
        fs::create_dir(notes.join("inner"))?;
        fs::write(notes.join("inner/inside.md"), b"")?;

        assert_eq!(
            find(&root, "kb-0001")?,
            Some(notes.join("kb-0001-topic.md"))
        );
        assert_eq!(find(&root, "kb-0001.md")?, Some(notes.join("kb-0001.md")));
        assert_eq!(find(&root, "topic")?, Some(notes.join("topic.md")));
        assert_eq!(find(&root, "0002")?, Some(notes.join("kb-0002-topic.md")));
        for selector in [
            "inside",
            "not-a-note",
            "../topic",
            "notes/topic.md",
            "notes\\topic.md",
            ".",
            "",
        ] {
            assert_eq!(find(&root, selector)?, None, "selector: {selector:?}");
        }
        Ok(())
    }

    #[test]
    fn ambiguous_hits_are_sorted_and_include_noncanonical_names() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        fs::write(root.path().join("notes/z-topic.md"), b"")?;
        fs::write(root.path().join("notes/a-topic.md"), b"")?;
        match find(&root, "topic") {
            Err(error @ LookupError::Ambiguous { .. }) => {
                assert_eq!(
                    error.to_string(),
                    "ambiguous slug \"topic\" matches: a-topic.md, z-topic.md — use the full kb-id"
                );
            }
            result => panic!("unexpected lookup result: {result:?}"),
        }
        Ok(())
    }

    #[test]
    fn exact_basename_but_not_substring_follows_filesystem_case_rules() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let path = root.path().join("notes/Case.md");
        fs::write(&path, b"")?;
        let lowercase = path.with_file_name("case.md");
        let expected = lowercase.try_exists()?.then_some(lowercase);
        assert_eq!(find(&root, "case")?, expected);
        assert_eq!(find(&root, "cas")?, None);
        Ok(())
    }

    #[test]
    fn snapshot_keeps_fingerprints_for_malformed_notes_in_path_order() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        fs::write(root.path().join("notes/z-readme.md"), b"abc")?;
        fs::write(root.path().join("notes/a-unterminated.md"), b"---\r\n")?;
        let entries = snapshot(&root)?;
        let [malformed, body_only] = entries.as_slice() else {
            panic!("unexpected snapshot entries: {entries:?}")
        };
        assert_eq!(malformed.relative, "notes/a-unterminated.md");
        assert_eq!(malformed.size, 5);
        assert_eq!(malformed.sha1, "02d09b1d042166463a291807a5525258379ad02a");
        assert_eq!(malformed.parsed, Err(ParseError::Malformed));
        assert_eq!(body_only.relative, "notes/z-readme.md");
        assert_eq!(body_only.size, 3);
        assert_eq!(body_only.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            body_only.parsed.as_ref().map(|note| note.body.as_str()),
            Ok("abc")
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn lookup_and_snapshot_reject_linked_inventory_entries() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let target = root.path().join("outside.md");
        let linked = root.path().join("notes/kb-0001-link.md");
        fs::write(&target, b"outside notes")?;
        std::os::unix::fs::symlink(&target, &linked)?;
        for selector in ["kb-0001", "kb-0001-link.md", "link"] {
            assert!(
                matches!(find(&root, selector), Err(LookupError::Inventory(NoteError::Inspect {source, ..})) if source.kind() == io::ErrorKind::InvalidInput && source.to_string().contains("refusing symbolic link or reparse point"))
            );
        }
        assert!(
            matches!(snapshot(&root), Err(NoteError::Inspect {source, ..}) if source.kind() == io::ErrorKind::InvalidInput && source.to_string().contains("refusing symbolic link or reparse point"))
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn notes_directory_cannot_be_replaced_with_a_symlink() -> anyhow::Result<()> {
        let (_dir, root) = fixture()?;
        let notes = root.path().join("notes");
        let replaced = root.path().join("other");
        fs::rename(&notes, &replaced)?;
        std::os::unix::fs::symlink(&replaced, &notes)?;
        assert!(matches!(snapshot(&root), Err(NoteError::Inspect {path, ..}) if path == notes));
        assert!(
            matches!(find(&root, "link"), Err(LookupError::Inventory(NoteError::Inspect {path, ..})) if path == notes)
        );
        Ok(())
    }
}
