use std::env;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Search and maintain a local Markdown knowledge base")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create an empty knowledge base.
    Init {
        /// Do not initialize a Git repository.
        #[arg(long)]
        no_git: bool,
        /// Target directory (defaults to ~/knowbox).
        dir: Option<PathBuf>,
    },
    /// Rebuild index.md and the SQLite full-text index.
    Reindex,
    /// Search notes, falling back to typo correction and reference headings.
    Search {
        /// Also show author-written links near the top results.
        #[arg(long)]
        expand: bool,
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
    },
    /// Print one Markdown note, even if its frontmatter is malformed.
    Show { id_or_slug: String },
    /// Browse the author-written links and backlinks of one note.
    Expand {
        id_or_slug: String,
        #[arg(long, default_value = "1", value_parser = parse_depth)]
        depth: crate::browse::Depth,
    },
    /// List notes, optionally filtered by domain or type.
    List {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long = "type")]
        kind: Option<String>,
    },
    /// Print the newest session notes.
    Recent {
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long)]
        here: bool,
        #[arg(long, value_name = "PROJECT")]
        project: Option<String>,
    },
    /// List reference documents, their headings, and their map notes.
    Refs { filter: Vec<String> },
    /// Report how long notes have gone without updates or verification.
    Stale {
        #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u32).range(1..))]
        days: u32,
    },
    /// Rank notes by the frequency of content edits in Git.
    Churn {
        #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u32).range(1..))]
        days: u32,
    },
    /// Create a note and publish its journal entry and index.
    New(Box<crate::changes::NewArgs>),
    /// Preview the next available note ID.
    NextId,
    /// Stamp a note's updated date; leaves the change for sweep or the next commit.
    Touch { id_or_slug: String },
    /// Record verification of one or more notes.
    Verify {
        #[arg(required = true)]
        ids: Vec<String>,
        #[arg(long, default_value = "re-verified")]
        evidence: String,
    },
    /// Retire OLD and link it and its successor to each other.
    Supersede {
        old: String,
        #[arg(long, value_name = "NEW")]
        with: String,
    },
    /// Commit pending knowledge-base changes and retry configured pushes.
    Sweep,
    /// Check notes and the generated index for structural problems.
    Lint,
    /// List unreviewed entries under raw/.
    RawBacklog,
    /// Mark a raw entry as ingested or rejected.
    RawMark {
        file: String,
        number: usize,
        #[arg(
            long,
            conflicts_with = "rejected",
            required_unless_present = "rejected"
        )]
        note: Option<String>,
        #[arg(long)]
        rejected: Option<String>,
    },
    /// Refresh the note embeddings using the configured provider.
    Embed {
        #[arg(long)]
        force: bool,
    },
    /// Search the stored embeddings with a provider-generated query vector.
    Vsearch {
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
    },
    /// Report similar live-note pairs as review candidates, not verdicts.
    Conflicts {
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..))]
        top: u32,
        #[arg(long, default_value_t = 0.70, allow_negative_numbers = true)]
        min_cos: f64,
        #[arg(long)]
        json: bool,
    },
    /// Print the installed version.
    Version,
}

fn parse_depth(value: &str) -> Result<crate::browse::Depth, &'static str> {
    match value {
        "1" => Ok(crate::browse::Depth::One),
        "2" => Ok(crate::browse::Depth::Two),
        _ => Err("depth must be 1 or 2"),
    }
}

pub(crate) fn parse() -> Result<Command, clap::Error> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "knowbox".into());
    let mut rest = args.peekable();
    if rest.peek().is_none() {
        return Cli::try_parse_from([program, "--help".into()]).map(|cli| cli.command);
    }
    Cli::try_parse_from(std::iter::once(program).chain(rest)).map(|cli| cli.command)
}
