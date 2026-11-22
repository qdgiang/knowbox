use std::env;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version = crate::VERSION, about = "Search and maintain a local Markdown knowledge base")]
pub(crate) struct Cli {
    /// Attribute this invocation to a named client.
    #[arg(long, global = true, value_name = "CLIENT")]
    pub(crate) client: Option<String>,
    /// Disable knowledge-base writes and telemetry for this read.
    #[arg(long, global = true)]
    pub(crate) no_write: bool,
    #[command(subcommand)]
    pub(crate) command: Command,
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
        /// Skip the embedding provider.
        #[arg(long)]
        local: bool,
        /// Skip typo correction.
        #[arg(long)]
        no_fix: bool,
        /// Print structured JSON.
        #[arg(long)]
        json: bool,
        #[arg(required = true, value_name = "QUERY")]
        query: Vec<String>,
    },
    /// Print one Markdown note, even if its frontmatter is malformed.
    Show {
        id_or_slug: String,
        /// Interpret the argument as the exact notes/ path from a result.
        #[arg(long)]
        path: bool,
        /// Print the note as structured JSON.
        #[arg(long)]
        json: bool,
    },
    /// Browse the author-written links and backlinks of one note.
    Expand {
        id_or_slug: String,
        #[arg(long, default_value = "1", value_parser = parse_depth)]
        depth: crate::browse::Depth,
        #[arg(long)]
        json: bool,
    },
    /// Show all notes and their authored graph edges.
    Graph {
        #[arg(long)]
        json: bool,
    },
    /// Report the activity and Git history of one note.
    History {
        id_or_slug: String,
        #[arg(long)]
        json: bool,
    },
    /// Show knowledge-base events in journal order.
    Timeline {
        #[arg(long)]
        json: bool,
    },
    /// Show Git head, branch, and working tree state.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Launch the local Knowbox Explorer.
    Serve(crate::serve::Options),
    /// List notes, optionally filtered by domain or type.
    List {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long = "type")]
        kind: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print the newest session notes.
    Recent {
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long)]
        here: bool,
        #[arg(long, value_name = "PROJECT")]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List reference documents, their headings, and their map notes.
    Refs {
        filter: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Report how long notes have gone without updates or verification.
    Stale {
        #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u32).range(1..))]
        days: u32,
        #[arg(long)]
        json: bool,
    },
    /// Rank notes by the frequency of content edits in Git.
    Churn {
        #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u32).range(1..))]
        days: u32,
    },
    /// Create a note and publish its journal entry and index.
    New(Box<crate::changes::NewArgs>),
    /// Edit a note from stdin, frontmatter assignments, or the configured editor.
    Edit(crate::changes::mutation::EditArgs),
    /// Link two notes in both directions.
    Link {
        a: String,
        b: String,
        #[arg(long, value_name = "KEY")]
        request_id: Option<String>,
    },
    /// Record a conflict review verdict.
    Verdict {
        a: String,
        b: String,
        #[arg(long, value_enum)]
        verdict: crate::maintain::raw::Verdict,
        #[arg(long)]
        reason: String,
        /// Link both notes as part of a complementary verdict.
        #[arg(long)]
        link: bool,
        /// Retain this ID and supersede the other note as part of the verdict.
        #[arg(long, value_name = "ID", conflicts_with = "link")]
        winner: Option<String>,
        #[arg(long, value_name = "KEY")]
        request_id: Option<String>,
    },
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
        /// Record why the successor replaces the old note.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Audit note edits, refresh stale indexes, and commit pending Git changes.
    Sweep,
    /// Check notes and the generated index for structural problems.
    Lint {
        /// Lint a draft file. Use - to read it from stdin.
        #[arg(long, conflicts_with = "stdin", group = "draft_input")]
        file: Option<PathBuf>,
        /// Read a draft from stdin.
        #[arg(long, group = "draft_input")]
        stdin: bool,
        /// Evaluate the draft as a replacement for this existing note.
        #[arg(long, requires = "draft_input")]
        against: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Report local note opens, exposures, and search outcomes.
    Usage {
        /// Limit the report to the last N local calendar days, including today.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        days: Option<u32>,
        /// Roll up completed local records and expire covered old logs.
        #[arg(long)]
        rollup: bool,
    },
    /// List unreviewed entries under raw/.
    RawBacklog {
        #[arg(long)]
        json: bool,
    },
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
    /// Show a budgeted brief for a project and host.
    Context {
        #[arg(long)]
        project: String,
        #[arg(long)]
        host: String,
        #[arg(long)]
        explain: bool,
        #[arg(long)]
        json: bool,
    },
    /// Map notes using the local embeddings or the authored link graph.
    Map {
        #[arg(long)]
        json: bool,
        /// Send recent missed queries to the configured embedding provider for placement.
        #[arg(long)]
        project_misses: bool,
    },
    /// Read daily health measurements.
    Metrics {
        #[arg(long)]
        json: bool,
    },
    /// Print the installed version.
    Version,
}

impl Command {
    pub(crate) fn writes(&self) -> bool {
        match self {
            Self::Init { .. }
            | Self::Reindex
            | Self::New(_)
            | Self::Edit(_)
            | Self::Link { .. }
            | Self::Verdict { .. }
            | Self::Touch { .. }
            | Self::Verify { .. }
            | Self::Supersede { .. }
            | Self::Sweep
            | Self::RawMark { .. }
            | Self::Embed { .. } => true,
            Self::Usage { rollup, .. } => *rollup,
            Self::Search { .. }
            | Self::Show { .. }
            | Self::Expand { .. }
            | Self::Graph { .. }
            | Self::History { .. }
            | Self::Timeline { .. }
            | Self::Status { .. }
            | Self::Serve(_)
            | Self::List { .. }
            | Self::Recent { .. }
            | Self::Refs { .. }
            | Self::Stale { .. }
            | Self::Churn { .. }
            | Self::NextId
            | Self::Lint { .. }
            | Self::RawBacklog { .. }
            | Self::Vsearch { .. }
            | Self::Conflicts { .. }
            | Self::Context { .. }
            | Self::Map { .. }
            | Self::Metrics { .. }
            | Self::Version => false,
        }
    }
}

fn parse_depth(value: &str) -> Result<crate::browse::Depth, &'static str> {
    match value {
        "1" => Ok(crate::browse::Depth::One),
        "2" => Ok(crate::browse::Depth::Two),
        _ => Err("depth must be 1 or 2"),
    }
}

pub(crate) fn parse() -> Result<Cli, clap::Error> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "knowbox".into());
    let mut rest = args.peekable();
    if rest.peek().is_none() {
        return Cli::try_parse_from([program, "--help".into()]);
    }
    Cli::try_parse_from(std::iter::once(program).chain(rest))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn bundled_skill_documents_each_command_and_the_note_write_rule() -> anyhow::Result<()> {
        let skill = include_str!("../skills/knowbox/SKILL.md");
        let (_, reference) = skill
            .split_once("\n## CLI reference\n")
            .ok_or_else(|| anyhow::anyhow!("missing CLI reference in the bundled skill"))?;
        let documented = reference
            .lines()
            .filter_map(|line| line.strip_prefix("- `knowbox "))
            .filter_map(|line| line.split([' ', '`']).next())
            .collect::<BTreeSet<_>>();
        let cli = Cli::command();
        let commands = cli
            .get_subcommands()
            .map(clap::Command::get_name)
            .collect::<BTreeSet<_>>();
        assert_eq!(documented, commands);
        assert!(skill.contains("`knowbox supersede OLD --with NEW`"));
        assert!(skill.contains("Agents must make every note change through the `knowbox` CLI."));
        assert!(skill.contains("`knowbox conflicts --json`"));
        assert!(skill.contains("`> verdict:`"));
        Ok(())
    }
}
