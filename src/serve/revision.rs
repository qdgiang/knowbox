use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use jiff::Zoned;
use sha1::{Digest, Sha1};

use crate::config::KbRoot;
use crate::store::{self, EntryKind, Inventory};
use crate::{git, maintain};

#[derive(Clone, Copy)]
pub(super) enum View {
    Home,
    Search,
    Note,
    Graph,
    Timeline,
    Refs,
    Review,
    Agent,
    Insights,
}

impl View {
    pub(super) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "home" => Self::Home,
            "search" => Self::Search,
            "note" => Self::Note,
            "graph" => Self::Graph,
            "timeline" => Self::Timeline,
            "refs" => Self::Refs,
            "review" => Self::Review,
            "agent" => Self::Agent,
            "insights" => Self::Insights,
            _ => return None,
        })
    }

    fn directories(self) -> &'static [&'static str] {
        match self {
            Self::Home | Self::Review | Self::Insights => &["notes", "raw", "refs"],
            Self::Note | Self::Graph | Self::Timeline => &["notes", "raw"],
            Self::Search | Self::Refs => &["notes", "refs"],
            Self::Agent => &["notes"],
        }
    }

    fn files(self) -> &'static [&'static str] {
        match self {
            Self::Home | Self::Insights => &[
                "log.md",
                "index.md",
                ".kb.db",
                "lint-waivers.md",
                ".search-misses.log",
                ".kb-embed.json",
                ".kb-heartbeat",
                ".kb-sweep-heartbeat",
                maintain::metrics::FILE,
            ],
            Self::Search => &[
                "index.md",
                ".kb.db",
                ".kb.db-wal",
                ".kb.db-shm",
                ".kb.db-journal",
                ".kb-embed.json",
            ],
            Self::Review => &[
                "log.md",
                "index.md",
                ".kb.db",
                "lint-waivers.md",
                ".search-misses.log",
                ".kb-embed.json",
                ".kb-heartbeat",
                ".kb-sweep-heartbeat",
            ],
            Self::Note | Self::Timeline => &["log.md"],
            Self::Graph => &[".kb-embed.json"],
            Self::Refs => &[],
            Self::Agent => &["index.md", "log.md"],
        }
    }
}

pub(super) fn read(root: &KbRoot, view: View, inventory: &mut Inventory<'_>) -> Result<String> {
    let mut digest = Sha1::new();
    digest.update(Zoned::now().date().to_string());
    let status = match git::status(root) {
        Ok(status) => serde_json::to_vec(&status)?,
        Err(error) => error.to_string().into_bytes(),
    };
    digest.update(status);
    let heartbeats = serde_json::to_value(maintain::heartbeats(root, SystemTime::now()))?;
    if let Some(heartbeats) = heartbeats.as_array() {
        for heartbeat in heartbeats {
            digest.update(heartbeat["state"].to_string());
        }
    }
    for dir in view.directories() {
        let path = root.path().join(dir);
        digest.update(dir.as_bytes());
        if inventory.directory(&path)? {
            for name in inventory.names(&path, EntryKind::MarkdownFile)? {
                hash_path(&mut digest, &path.join(&name), &name, inventory)?;
            }
        } else {
            digest.update(b"missing");
        }
    }
    for name in view.files() {
        hash_path(&mut digest, &root.path().join(name), name, inventory)?;
    }
    if matches!(
        view,
        View::Home | View::Note | View::Review | View::Agent | View::Insights
    ) {
        let dir = root.path().join(".kb-usage");
        if inventory.directory(&dir)? {
            let mut files: Vec<_> = fs::read_dir(&dir)?.collect::<io::Result<_>>()?;
            files.sort_by_key(fs::DirEntry::file_name);
            for file in files {
                let name = file.file_name();
                let Some(name) = name.to_str() else { continue };
                if name == "rollup.sqlite" || name.ends_with(".jsonl") {
                    hash_path(&mut digest, &file.path(), name, inventory)?;
                }
            }
        }
    }
    if matches!(
        view,
        View::Graph | View::Review | View::Home | View::Search | View::Insights
    ) {
        digest.update(
            std::env::var_os("KNOWBOX_EMBED_MODEL")
                .unwrap_or_default()
                .as_encoded_bytes(),
        );
    }
    let mut revision = String::with_capacity(40);
    for byte in digest.finalize() {
        write!(revision, "{byte:02x}")?;
    }
    Ok(revision)
}

fn hash_path(
    digest: &mut Sha1,
    path: &Path,
    label: &str,
    inventory: &mut Inventory<'_>,
) -> io::Result<()> {
    digest.update(label.len().to_be_bytes());
    digest.update(label.as_bytes());
    if !inventory.file(path)? {
        digest.update(b"missing");
        return Ok(());
    }
    let mut file = match store::open_read(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            digest.update(b"missing");
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let meta = file.metadata()?;
        // A write followed by restored mtime still changes ctime. Coarse timestamps
        // fall back to hashing content, as do platforms without a change time.
        if meta.ctime_nsec() != 0 {
            digest.update(b"metadata");
            for field in [meta.dev(), meta.ino(), meta.len()] {
                digest.update(field.to_be_bytes());
            }
            for field in [
                meta.mtime(),
                meta.mtime_nsec(),
                meta.ctime(),
                meta.ctime_nsec(),
            ] {
                digest.update(field.to_be_bytes());
            }
            return Ok(());
        }
    }
    digest.update(b"content");
    let mut content = Sha1::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        content.update(&buffer[..read]);
    }
    digest.update(content.finalize());
    Ok(())
}
