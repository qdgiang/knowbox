use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use jiff::civil::Date;
use tempfile::NamedTempFile;

use super::{Draft, Event, append_entry, next_id, single_line};
use crate::browse::project_from_cwd;
use crate::config::KbRoot;
use crate::notes;
use crate::store::{self, WriteLock};

const TEMPLATE: &str = "FIRST SENTENCE = the load-bearing fact.\n\nDetails, commands, evidence.\n\n## Related\n- [[kb-xxxx]] — why\n";

#[derive(Debug, clap::Args)]
pub(crate) struct NewArgs {
    #[arg(long = "type", value_parser = ["fact", "gotcha", "howto", "decision", "reference", "project", "session"])]
    kind: String,
    #[arg(long)]
    domain: String,
    #[arg(long)]
    title: String,
    #[arg(long, default_value = "")]
    tags: String,
    #[arg(long, default_value = "")]
    aliases: String,
    #[arg(long, default_value = "active", value_parser = ["draft", "active", "verified", "superseded"])]
    status: String,
    #[arg(long, default_value = "medium", value_parser = ["high", "medium", "low"])]
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
}

pub(crate) fn prepare_new(mut args: NewArgs, err: &mut impl Write) -> Result<Draft> {
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
        edit_body(&KbRoot::resolve()?, err)?
    } else {
        TEMPLATE.to_owned()
    };
    Ok(Draft { args, body })
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

fn edit_body(root: &KbRoot, err: &mut impl Write) -> Result<String> {
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
    let body = dir.path().join("body.md");
    fs::File::create_new(&body)?.write_all(TEMPLATE.as_bytes())?;
    let editor = env::var_os("EDITOR")
        .unwrap_or_else(|| if cfg!(windows) { "notepad.exe" } else { "vi" }.into());
    let mut command = editor_command(&editor)?;
    writeln!(
        err,
        "knowbox: editing private draft; no note ID has been allocated"
    )?;
    let status = command
        .arg(&body)
        .status()
        .with_context(|| format!("launching editor {editor:?}"))?;
    ensure!(
        status.success(),
        "editor exited with {status}; no note was created"
    );
    ensure!(
        store::regular_file(&body)?,
        "editor removed draft {}",
        body.display()
    );
    fs::read_to_string(&body).with_context(|| format!("reading edited draft {}", body.display()))
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

fn slug(title: &str) -> String {
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

pub(crate) fn create(lock: &WriteLock, draft: Draft, today: Date) -> Result<Event> {
    let dir = lock.root().path().join("notes");
    ensure!(
        store::directory(&dir)?,
        "missing notes directory {}",
        dir.display()
    );
    let id = next_id(lock.root())?;
    let Draft { args, body } = draft;
    let path = dir.join(format!("{id}-{}.md", slug(&args.title)));
    let mut note = format!(
        "---\nid: {id}\ntitle: \"{}\"\ntype: {}\ndomain: {}\n",
        args.title,
        args.kind,
        args.domain.to_lowercase()
    );
    if args.kind == "session"
        && let Some(project) = args.project
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
    note.push_str(&format!("status: {}\nconfidence: {}\ncreated: {today}\nupdated: {today}\nsupersedes: []\nrelated: []\nsource: \"{}\"\nsummary: \"{}\"\n---\n", args.status, args.confidence, args.source, args.summary));
    note.push_str(notes::normalize_newlines(&body).trim_end_matches('\n'));
    note.push('\n');
    let mut temp = NamedTempFile::new_in(&dir)
        .with_context(|| format!("creating note beside {}", path.display()))?;
    temp.write_all(note.as_bytes())
        .and_then(|()| temp.as_file().sync_all())
        .context("writing private note file")?;
    temp.into_temp_path()
        .persist_noclobber(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing new note {} without overwriting", path.display()))?;
    append_entry(lock, today, "create", &id, &args.title)
        .with_context(|| format!("{} was created but not journaled", path.display()))?;
    Ok(Event {
        subject: format!("knowbox: create {id} — {}", args.title),
        stdout: path.display().to_string(),
    })
}
