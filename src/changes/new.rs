use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use tempfile::TempDir;

use super::{Draft, Event, FileWrite, next_id, note_journal, rewrite, single_line, transaction};
use crate::browse::project_from_cwd;
use crate::config::KbRoot;
use crate::journal::Stamp;
use crate::notes;
use crate::store::{self, WriteLock};

const TEMPLATE: &str = "FIRST SENTENCE = the load-bearing fact.\n\nDetails, commands, evidence.\n\n## Related\n- [[kb-xxxx]] — why\n";
const PENDING: &str = "pending";
const DRAFT_FILE: &str = "note.md";
const NOTE_TYPES: [&str; 7] = [
    "fact",
    "gotcha",
    "howto",
    "decision",
    "reference",
    "project",
    "session",
];
const STATUSES: [&str; 4] = ["draft", "active", "verified", "superseded"];
const CONFIDENCES: [&str; 3] = ["high", "medium", "low"];

#[derive(Debug, clap::Args)]
pub(crate) struct NewArgs {
    #[arg(long = "type", value_parser = clap::builder::PossibleValuesParser::new(NOTE_TYPES))]
    kind: String,
    #[arg(long)]
    domain: String,
    #[arg(long)]
    title: String,
    #[arg(long, default_value = "")]
    tags: String,
    #[arg(long, default_value = "")]
    aliases: String,
    #[arg(long, default_value = "active", value_parser = clap::builder::PossibleValuesParser::new(STATUSES))]
    status: String,
    #[arg(long, default_value = "medium", value_parser = clap::builder::PossibleValuesParser::new(CONFIDENCES))]
    confidence: String,
    #[arg(long, default_value = "")]
    source: String,
    #[arg(long, default_value = "TODO: what question does this answer?")]
    summary: String,
    #[arg(long, default_value = "")]
    triggers: String,
    #[arg(long)]
    project: Option<String>,
    #[arg(long, conflicts_with = "body_stdin")]
    body: Option<String>,
    #[arg(long)]
    body_stdin: bool,
    #[arg(long)]
    no_edit: bool,
    /// Link the new note and an existing ID in both directions. Repeat for additional IDs.
    #[arg(long, value_delimiter = ',', value_name = "ID")]
    pub(crate) related: Vec<String>,
    /// Reuse this key when retrying creation after an interrupted save.
    #[arg(long, value_name = "KEY")]
    pub(crate) request_id: Option<String>,
}

pub(crate) fn prepare_new(root: &KbRoot, mut args: NewArgs, err: &mut impl Write) -> Result<Draft> {
    if args.kind == "session" && args.project.as_deref().is_none_or(str::is_empty) {
        args.project = Some(project_from_cwd()?);
    }
    for (field, value) in [
        ("domain", args.domain.as_str()),
        ("title", &args.title),
        ("tags", &args.tags),
        ("aliases", &args.aliases),
        ("summary", &args.summary),
        ("source", &args.source),
        ("triggers", &args.triggers),
        ("project", args.project.as_deref().unwrap_or("")),
    ] {
        single_line(field, value)?;
    }
    for (name, value) in [
        ("domain", args.domain.as_str()),
        ("project", args.project.as_deref().unwrap_or("machine")),
    ] {
        ensure!(
            !value.is_empty() && value.trim() == value && !value.starts_with(['[', '\'', '"']),
            "unsupported or empty {name} scalar"
        );
    }
    ensure!(!args.title.trim().is_empty(), "title must not be blank");
    validate_list("tags", &args.tags)?;
    validate_list("aliases", &args.aliases)?;
    let stdin = io::stdin();
    let body = if let Some(body) = args.body.take() {
        body
    } else if args.body_stdin {
        let mut body = String::new();
        stdin
            .lock()
            .read_to_string(&mut body)
            .context("reading UTF-8 body from stdin")?;
        body
    } else if stdin.is_terminal() && !args.no_edit {
        return edit_note(root, &render_draft(&args, TEMPLATE), err);
    } else {
        TEMPLATE.to_owned()
    };
    let text = render_draft(&args, &body);
    Ok(Draft {
        title: args.title,
        text,
        edited: None,
    })
}

// The note dialect has no escape for a quote within a quoted CSV item. Accept
// ordinary and quoted (possibly comma-containing) items; never silently merge two tags.
fn validate_list(field: &str, raw: &str) -> Result<()> {
    let mut rest = raw.trim();
    while !rest.is_empty() {
        let tail = if let Some(quote) = rest.chars().next().filter(|ch| matches!(*ch, '\'' | '"')) {
            rest[1..]
                .split_once(quote)
                .with_context(|| format!("unclosed quote in {field}"))?
                .1
                .trim_start()
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            let item = rest[..end].trim();
            ensure!(
                !item.is_empty() && !item.contains(['[', ']', '\'', '"']),
                "unsupported list item in {field}"
            );
            &rest[end..]
        };
        if tail.is_empty() {
            break;
        }
        rest = tail
            .strip_prefix(',')
            .with_context(|| format!("expected comma in {field}"))?
            .trim_start();
        ensure!(!rest.is_empty(), "trailing comma in {field}");
    }
    Ok(())
}

pub(crate) fn validate_frontmatter(text: &str) -> Result<()> {
    let mut keys = BTreeSet::new();
    let mut list_field = None;
    for line in rewrite::field_lines(text)? {
        single_line("frontmatter", line)?;
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (field, value) = if let Some((key, raw)) = notes::key_line(line) {
            ensure!(keys.insert(key), "duplicate frontmatter field {key}");
            list_field = raw.trim().is_empty().then_some(key);
            (key, raw.trim())
        } else {
            let item = trimmed
                .strip_prefix('-')
                .filter(|rest| rest.starts_with(char::is_whitespace));
            let (Some(key), Some(raw)) = (list_field, item) else {
                anyhow::bail!("expected a frontmatter field or indented list item");
            };
            ensure!(
                trimmed != line && !raw.trim().is_empty(),
                "invalid block-list item in {key}"
            );
            (key, raw.trim())
        };
        if let Some(inner) = value.strip_prefix('[') {
            let list = inner
                .strip_suffix(']')
                .with_context(|| format!("unclosed inline list in {field}"))?;
            validate_list(field, list)?;
        } else if let Some(quote) = value.chars().next().filter(|ch| matches!(ch, '\'' | '"')) {
            ensure!(
                value.len() > 1 && value.ends_with(quote),
                "unclosed quote in {field}"
            );
        }
    }
    Ok(())
}

pub(super) fn edited_title(text: &str) -> Result<String> {
    validate_frontmatter(text)?;
    let note = notes::parse(text.as_bytes())?;
    for key in ["id", "created", "updated"] {
        ensure!(
            note.scalar(key) == Some(PENDING),
            "leave {key} as {PENDING}; knowbox fills it on save"
        );
    }
    for (field, allowed) in [
        ("type", NOTE_TYPES.as_slice()),
        ("status", STATUSES.as_slice()),
        ("confidence", CONFIDENCES.as_slice()),
    ] {
        ensure!(
            note.scalar(field)
                .is_some_and(|value| allowed.contains(&value)),
            "invalid {field} (expected {})",
            allowed.join(", ")
        );
    }
    ensure!(
        note.scalar("domain")
            .is_some_and(|name| !name.trim().is_empty()),
        "domain must be a nonempty scalar"
    );
    ensure!(note.scalar("summary").is_some(), "summary must be a scalar");
    if note.scalar("type") == Some("session") {
        ensure!(
            note.scalar("project")
                .is_some_and(|project| !project.trim().is_empty()),
            "session notes require a nonempty project field"
        );
    }
    let title = note
        .scalar("title")
        .filter(|title| !title.trim().is_empty())
        .context("title must be a nonempty scalar")?;
    Ok(title.to_owned())
}

fn edit_note(root: &KbRoot, initial: &str, err: &mut impl Write) -> Result<Draft> {
    let (text, dir) = editor_draft(
        root,
        initial.as_bytes(),
        "knowbox: editing a private note draft. The ID and dates are assigned on save.",
        err,
    )?;
    match edited_title(&text) {
        Ok(title) => Ok(Draft {
            title,
            text,
            edited: Some(dir),
        }),
        Err(error) => Err(keep_draft(dir, error.context("invalid edited note draft"))),
    }
}

pub(super) fn editor_draft(
    root: &KbRoot,
    initial: &[u8],
    notice: &str,
    err: &mut impl Write,
) -> Result<(String, TempDir)> {
    let base = env::temp_dir()
        .canonicalize()
        .context("resolving temporary directory for editor")?;
    ensure!(
        !base.starts_with(root.path()),
        "editor requires a temporary directory outside the knowledge base"
    );
    let mut builder = tempfile::Builder::new();
    builder.prefix("knowbox-edit-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let dir = builder
        .tempdir_in(base)
        .context("creating private draft directory")?;
    let draft = dir.path().join(DRAFT_FILE);
    fs::File::create_new(&draft)?.write_all(initial)?;
    let editor = env::var_os("EDITOR")
        .unwrap_or_else(|| if cfg!(windows) { "notepad.exe" } else { "vi" }.into());
    let mut command = editor_command(&editor)?;
    writeln!(err, "{notice}")?;
    let status = command
        .arg(&draft)
        .status()
        .with_context(|| format!("launching editor {editor:?}"))?;
    ensure!(
        status.success(),
        "editor exited with {status}; draft was cancelled"
    );
    match store::regular_file(&draft) {
        Ok(true) => {}
        Ok(false) => {
            let path = dir.keep();
            anyhow::bail!("editor removed the note draft; draft directory kept at {path:?}");
        }
        Err(error) => {
            let path = dir.keep();
            return Err(error).with_context(|| format!("draft directory kept at {path:?}"));
        }
    }
    match fs::read_to_string(&draft).context("reading edited draft as UTF-8") {
        Ok(text) => Ok((text, dir)),
        Err(error) => Err(keep_draft(dir, error)),
    }
}

pub(crate) fn keep_draft(dir: TempDir, error: anyhow::Error) -> anyhow::Error {
    let path = dir.keep().join(DRAFT_FILE);
    error.context(format!("edited note draft kept at {path:?}"))
}

impl Draft {
    pub(crate) fn take_source(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    pub(crate) fn on_publication_error(self, error: anyhow::Error) -> anyhow::Error {
        match self.edited {
            Some(dir) => keep_draft(dir, error),
            None => error,
        }
    }
}

fn editor_command(editor: &OsStr) -> Result<Command> {
    ensure!(!editor.is_empty(), "EDITOR is empty");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let path = Path::new(editor);
        if path.is_file() {
            return Ok(Command::new(
                path.canonicalize().context("resolving editor executable")?,
            ));
        }
        let mut words = shlex::bytes::split(editor.as_bytes())
            .context("invalid quoting in EDITOR")?
            .into_iter()
            .map(std::ffi::OsString::from_vec);
        let mut command = Command::new(words.next().context("empty EDITOR command")?);
        command.args(words);
        Ok(command)
    }
    #[cfg(windows)]
    {
        let ext = Path::new(editor).extension().and_then(OsStr::to_str);
        ensure!(
            !ext.is_some_and(|s| s.eq_ignore_ascii_case("bat") || s.eq_ignore_ascii_case("cmd")),
            "EDITOR must be a native executable, not a batch script"
        );
        Ok(Command::new(editor))
    }
}

pub(super) fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(slug.trim_end_matches('-').len().min(60));
    slug
}

fn render_draft(args: &NewArgs, body: &str) -> String {
    let mut note = format!(
        "---\nid: {PENDING}\ntitle: \"{}\"\ntype: {}\ndomain: {}\n",
        args.title,
        args.kind,
        args.domain.to_lowercase()
    );
    if args.kind == "session"
        && let Some(project) = &args.project
    {
        note.push_str(&format!("project: {project}\n"));
    }
    note.push_str(&format!(
        "tags: [{}]\naliases: [{}]\n",
        args.tags, args.aliases
    ));
    let mut triggers = args
        .triggers
        .split('|')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .peekable();
    if triggers.peek().is_some() {
        note.push_str("triggers:\n");
        for t in triggers {
            note.push_str(&format!("  - \"{t}\"\n"));
        }
    }
    note.push_str(&format!("status: {}\nconfidence: {}\ncreated: {PENDING}\nupdated: {PENDING}\nsupersedes: []\nrelated: []\nsource: \"{}\"\nsummary: \"{}\"\n---\n", args.status, args.confidence, args.source, args.summary));
    note.push_str(notes::normalize_newlines(body).trim_end_matches('\n'));
    note.push('\n');
    note
}

pub(crate) fn create(lock: &WriteLock, draft: Draft, stamp: &Stamp<'_>) -> Result<Event> {
    let (id, path, log) = match publish(lock, &draft, stamp) {
        Ok(published) => published,
        Err(error) => return Err(draft.on_publication_error(error)),
    };
    let Draft { title, .. } = draft;
    transaction::finish(lock, &log)
        .with_context(|| format!("{} was created but not journaled", path.display()))?;
    Ok(Event {
        subject: format!("knowbox: create {id} — {title}"),
        stdout: path.display().to_string(),
    })
}

fn publish(
    lock: &WriteLock,
    draft: &Draft,
    stamp: &Stamp<'_>,
) -> Result<(String, PathBuf, Vec<u8>)> {
    let dir = lock.root().path().join("notes");
    ensure!(
        store::directory(&dir)?,
        "missing notes directory {}",
        dir.display()
    );
    let id = next_id(lock.root())?;
    let date = stamp.date().to_string();
    let path = dir.join(format!("{id}-{}.md", slug(&draft.title)));
    let note = rewrite::edit(
        draft.text.as_bytes(),
        &[("id", &id), ("created", &date), ("updated", &date)],
        None,
        None,
    )?;
    let log = note_journal(
        lock,
        stamp,
        "create",
        &id,
        &draft.title,
        &[FileWrite {
            path: &path,
            before: None,
            after: &note,
        }],
    )?;
    transaction::create_without_overwrite(&path, &note)
        .map_err(|error| transaction::abort(lock, error))?;
    Ok((id, path, log))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDITED: &str = "---\n# draft metadata\nid: pending\ntitle: 'Edited: title'\ntype: howto\ndomain: editing\ntags:\n  - one\n  - 'two, three'\nstatus: draft\nconfidence: high\ncreated: pending\nupdated: pending\nsummary: 'Edited summary'\n---\n\nid: body text\n";

    #[test]
    fn edited_draft_accepts_comments_block_lists_and_crlf() -> Result<()> {
        assert_eq!(
            edited_title(&EDITED.replace('\n', "\r\n"))?,
            "Edited: title"
        );
        Ok(())
    }

    #[test]
    fn edited_draft_rejects_ambiguous_or_invalid_metadata() {
        for (original, replacement, expected) in [
            (
                "id: pending",
                "id: kb-0042",
                "leave id as pending; knowbox fills it on save",
            ),
            (
                "updated: pending",
                "updated: 2026-01-01",
                "leave updated as pending; knowbox fills it on save",
            ),
            (
                "type: howto",
                "type: memo",
                "invalid type (expected fact, gotcha, howto, decision, reference, project, session)",
            ),
            (
                "type: howto",
                "type: session",
                "session notes require a nonempty project field",
            ),
            (
                "domain: editing",
                "domain:\n  - editing",
                "domain must be a nonempty scalar",
            ),
            (
                "title: 'Edited: title'",
                "title: a\ntitle: b",
                "duplicate frontmatter field title",
            ),
            (
                "title: 'Edited: title'",
                "title: 'forgot to close",
                "unclosed quote in title",
            ),
            (
                "tags:\n  - one\n  - 'two, three'",
                "tags: [one,]",
                "trailing comma in tags",
            ),
        ] {
            let text = EDITED.replace(original, replacement);
            assert_eq!(edited_title(&text).unwrap_err().to_string(), expected);
        }
    }
}
