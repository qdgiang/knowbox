use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::notes::sha1_hex;
use crate::store::{self, WriteLock};

use super::{publish_log, read_log};

const PENDING: &str = ".kb-pending-write.json";

#[derive(Debug, thiserror::Error)]
#[error("operation is staged; retry the same request to finish recovery")]
pub(crate) struct PendingWrite(#[source] anyhow::Error);

#[derive(Debug, thiserror::Error)]
#[error("{path} changed since it was opened (current SHA-1 {current_sha1})")]
pub(crate) struct ChangedNote {
    pub(crate) path: String,
    pub(crate) current_sha1: String,
    pub(crate) current_markdown: String,
}

impl ChangedNote {
    pub(crate) fn new(path: String, bytes: &[u8]) -> Self {
        Self {
            path,
            current_sha1: sha1_hex(bytes),
            current_markdown: String::from_utf8_lossy(bytes).into_owned(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FileWrite<'a> {
    pub(crate) path: &'a Path,
    pub(crate) before: Option<&'a [u8]>,
    pub(crate) after: &'a [u8],
}

#[derive(Serialize, Deserialize)]
struct Pending {
    version: u8,
    log_len: usize,
    log_hash: String,
    suffix: String,
    files: Vec<Replacement>,
}

#[derive(Serialize, Deserialize)]
struct Replacement {
    area: Area,
    name: String,
    before: Option<String>,
    after: String,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Area {
    Notes,
    Raw,
}

impl Area {
    fn name(self) -> &'static str {
        match self {
            Self::Notes => "notes",
            Self::Raw => "raw",
        }
    }
}

impl Replacement {
    fn stage(lock: &WriteLock, write: &FileWrite<'_>) -> Result<Self> {
        let root = lock.root().path();
        let area = if write.path.parent() == Some(root.join("notes").as_path()) {
            Area::Notes
        } else {
            ensure!(
                write.path.parent() == Some(root.join("raw").as_path()),
                "journaled writes must target a file in notes/ or raw/"
            );
            Area::Raw
        };
        let name = write
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("reading filename for {:?}", write.path))?;
        Ok(Self {
            area,
            name: name.to_owned(),
            before: write.before.map(sha1_hex),
            after: std::str::from_utf8(write.after)?.to_owned(),
        })
    }

    fn valid(&self) -> bool {
        self.name.ends_with(".md")
            && matches!(Path::new(&self.name).components().next(), Some(Component::Normal(name)) if name == self.name.as_str())
    }
}

pub(crate) fn stage(lock: &WriteLock, log: &[u8], files: &[FileWrite<'_>]) -> Result<()> {
    stage_record(lock, log, files)?;
    Ok(())
}

fn stage_record(lock: &WriteLock, log: &[u8], files: &[FileWrite<'_>]) -> Result<Pending> {
    let path = lock.root().path().join(PENDING);
    ensure!(
        !store::regular_file(&path)?,
        "unfinished journaled write at {}",
        path.display()
    );
    let original = read_log(lock.root())?;
    let suffix = log
        .strip_prefix(original.as_bytes())
        .context("log.md changed while preparing a journaled write")?;
    let pending = Pending {
        version: 1,
        log_len: original.len(),
        log_hash: sha1_hex(original.as_bytes()),
        suffix: std::str::from_utf8(suffix)?.to_owned(),
        files: files
            .iter()
            .map(|write| Replacement::stage(lock, write))
            .collect::<Result<_>>()?,
    };
    store::atomic_write(&path, &serde_json::to_vec(&pending)?)?;
    Ok(pending)
}

pub(crate) fn apply(lock: &WriteLock, log: &[u8], files: &[FileWrite<'_>]) -> Result<()> {
    let pending = stage_record(lock, log, files)?;
    let ready = validate(lock, &pending, Phase::Fresh).map_err(|error| abort(lock, error))?;
    publish(lock, ready).map_err(PendingWrite)?;
    Ok(())
}

pub(crate) fn finish(lock: &WriteLock, log: &[u8]) -> Result<()> {
    publish_log(lock, log)?;
    clear(lock)
}

pub(crate) fn abort(lock: &WriteLock, error: anyhow::Error) -> anyhow::Error {
    match clear(lock) {
        Ok(()) => error,
        Err(cleanup) => error.context(format!(
            "could not discard pending journaled write: {cleanup:#}"
        )),
    }
}

fn clear(lock: &WriteLock) -> Result<()> {
    let path = lock.root().path().join(PENDING);
    fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))
}

pub(crate) fn create_without_overwrite(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("new note has no parent")?;
    let mut temp = NamedTempFile::new_in(dir)
        .with_context(|| format!("creating note beside {}", path.display()))?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .context("writing private note file")?;
    temp.into_temp_path()
        .persist_noclobber(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing new note {} without overwriting", path.display()))?;
    Ok(())
}

pub(crate) fn recover(lock: &WriteLock) -> Result<()> {
    let path = lock.root().path().join(PENDING);
    if store::regular_file(&path)? {
        let resumed = (|| {
            let pending: Pending = serde_json::from_slice(&store::read_regular(&path)?)
                .with_context(|| format!("reading journaled write {}", path.display()))?;
            let ready = validate(lock, &pending, Phase::Recover)?;
            publish(lock, ready)
                .with_context(|| format!("recovering journaled write from {}", path.display()))
        })();
        resumed.map_err(PendingWrite)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Phase {
    Fresh,
    Recover,
}

struct Publication<'a> {
    remaining: Vec<(PathBuf, &'a Replacement)>,
    log: String,
}

fn validate<'a>(
    lock: &WriteLock,
    pending: &'a Pending,
    phase: Phase,
) -> Result<Option<Publication<'a>>> {
    ensure!(
        pending.version == 1,
        "unsupported pending write version {}",
        pending.version
    );
    ensure!(!pending.suffix.is_empty(), "empty pending write");
    let mut log = read_log(lock.root())?;
    let prefix = log
        .get(..pending.log_len)
        .context("log.md shortened during an unfinished write")?;
    ensure!(
        sha1_hex(prefix.as_bytes()) == pending.log_hash,
        "log.md changed during an unfinished write"
    );
    if log.get(pending.log_len..) == Some(pending.suffix.as_str()) {
        return Ok(None);
    }
    ensure!(
        log.len() == pending.log_len,
        "log.md has unrelated entries after an unfinished write"
    );

    let mut destinations = BTreeSet::new();
    let mut remaining = Vec::new();
    for file in &pending.files {
        ensure!(
            file.valid() && destinations.insert((file.area, &file.name)),
            "invalid pending destination {:?}",
            file.name
        );
        let dir = lock.root().path().join(file.area.name());
        if !store::directory(&dir)? {
            ensure!(
                file.area == Area::Raw && file.before.is_none(),
                "missing pending destination directory {}",
                dir.display()
            );
        }
        let path = dir.join(&file.name);
        let current = if store::regular_file(&path)? {
            Some(
                store::read_regular(&path)
                    .with_context(|| format!("reading {}", path.display()))?,
            )
        } else {
            None
        };
        if current.as_deref() == Some(file.after.as_bytes()) {
            continue;
        }
        if current.as_deref().map(sha1_hex) != file.before {
            if matches!(phase, Phase::Fresh)
                && file.area == Area::Notes
                && let Some(bytes) = current
            {
                let relative = format!("notes/{}", file.name);
                return Err(ChangedNote::new(relative, &bytes).into());
            }
            anyhow::bail!("{} changed during an unfinished write", path.display());
        }
        remaining.push((path, file));
    }
    log.push_str(&pending.suffix);
    Ok(Some(Publication { remaining, log }))
}

fn publish(lock: &WriteLock, publication: Option<Publication<'_>>) -> Result<()> {
    let Some(Publication { remaining, log }) = publication else {
        return clear(lock);
    };
    for (path, file) in remaining {
        if file.before.is_none() {
            if file.area == Area::Raw {
                let dir = lock.root().path().join("raw");
                if !store::directory(&dir)? {
                    fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
                }
            }
            create_without_overwrite(&path, file.after.as_bytes())?;
        } else {
            store::atomic_write(&path, file.after.as_bytes())?;
        }
    }
    finish(lock, log.as_bytes())
}
