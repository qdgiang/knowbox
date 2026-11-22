use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::VERSION;
use crate::config::{KbRoot, SUBDIRS};
use crate::journal::Stamp;
use crate::store::{LOCK_NAME, WriteLock, directory};

const SCHEMA: &str = "# SCHEMA — note conventions

Schema version: 2

Frontmatter (YAML-ish, one key per line):
  id: kb-NNNN (issued by `knowbox new`; never reuse)
  title: one line
  type: fact|gotcha|howto|decision|reference|project|session
  domain: subject area
  status: draft|active|verified|superseded
  confidence: high|medium|low
  created / updated: YYYY-MM-DD
  summary: one line — this is the index routing text
Optional: tags, aliases, related ([[id]] links), supersedes, source,
triggers, verified.

Body: free markdown. Large docs go whole into refs/ with a type: reference
section-map note here. Full contract: https://github.com/qdgiang/knowbox
Fenced code does not count toward the body word budget. Mermaid fences
support diagrams. Keep each diagram to 15 lines of source.

raw/ (staging, append-only): `## ` headings delimit entries (bodies use
### and deeper ONLY). Reviewed entries carry a marker as the FIRST line
under the heading: `> state: ingested kb-XXXX (date)` / `> state: rejected
<reason>` — `knowbox raw-backlog` lists unmarked entries, `knowbox raw-mark
<file> <N> --note kb-XXXX | --rejected \"reason\"` stamps them. Never delete
entries; legacy `> ingested:`/`> rejected:` lines still resolve an entry.

`log.md` is append-only. New entries carry an offset-bearing timestamp,
client, host and optional session. Older date-only entries remain valid.
Use `knowbox new` for notes created after the journal began. `knowbox reindex`
and `knowbox sweep` record detected out-of-CLI note edits. Usage and search
miss logs stay on this machine.
";

const GITATTRIBUTES: &str = "* text=auto eol=lf\n";
const GITIGNORE: &str = "# knowbox data repo — derived files (regenerated from notes/ by the CLI)
.kb.db
.kb.db-wal
.kb.db-shm
.kb-embed.json
.kb.lock
/.kb-audit-seed.json
/.kb-pending-write.json
/.kb-usage/
/.kb-metrics.jsonl
/.search-misses.log
.lint-cron.log
.embed-cron.log
.sweep-cron.log
.kb-heartbeat
.kb-sweep-heartbeat
__pycache__/
";

/// Initialize canonical files and keep the writer lock held for the caller's derived build.
pub(crate) fn prepare(path: &Path, stamp: &Stamp<'_>) -> Result<WriteLock> {
    ensure!(!path.as_os_str().is_empty(), "empty init target");
    if !directory(path).with_context(|| format!("checking init target {}", path.display()))? {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating parent of {}", path.display()))?;
        }
        fs::create_dir(path).with_context(|| format!("creating init target {}", path.display()))?;
    }
    let read_entries =
        || fs::read_dir(path).with_context(|| format!("listing init target {}", path.display()));
    let first = read_entries()?
        .next()
        .transpose()
        .with_context(|| format!("checking init target {}", path.display()))?;
    ensure!(
        first.is_none(),
        "{} is not empty — refusing to init",
        path.display()
    );
    let guard = WriteLock::reserve(KbRoot::open(path)?)?;
    // If a concurrent writer won a create before our reservation, retain all its files and refuse.
    let mut saw_lock = false;
    for entry in read_entries()? {
        let entry =
            entry.with_context(|| format!("checking init reservation in {}", path.display()))?;
        ensure!(
            entry.file_name() == LOCK_NAME,
            "unexpected entry in reserved directory {}: {}",
            path.display(),
            entry.path().display()
        );
        saw_lock = true;
    }
    ensure!(
        saw_lock,
        "init reservation disappeared from {}",
        path.display()
    );

    let path = guard.root().path();
    for name in SUBDIRS {
        let dir = path.join(name);
        fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
        create_file(&dir.join(".gitkeep"), b"")?;
    }
    create_file(&path.join("SCHEMA.md"), SCHEMA.as_bytes())?;
    create_file(&path.join(".gitignore"), GITIGNORE.as_bytes())?;
    create_file(&path.join(".gitattributes"), GITATTRIBUTES.as_bytes())?;
    let log = format!(
        "{}\n",
        stamp.heading("init", "—", &format!("KB skeleton created by {VERSION}"))?
    );
    create_file(&path.join("log.md"), log.as_bytes())?;
    Ok(guard)
}

fn create_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file =
        File::create_new(path).with_context(|| format!("creating {}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::test_stamp;

    #[test]
    fn init_keeps_lock_and_creates_lf_log_and_skeleton() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("nested/kb");
        let guard = prepare(&root, &test_stamp())?;
        let other = File::options()
            .read(true)
            .write(true)
            .open(root.join(".kb.lock"))?;
        assert!(matches!(
            other.try_lock(),
            Err(fs::TryLockError::WouldBlock)
        ));
        for name in ["notes", "refs", "raw"] {
            assert_eq!(fs::read(root.join(name).join(".gitkeep"))?, b"");
        }
        for name in ["SCHEMA.md", ".gitignore", ".gitattributes", "log.md"] {
            assert!(!fs::read(root.join(name))?.contains(&b'\r'));
        }
        let version = env!("CARGO_PKG_VERSION")
            .strip_suffix(".0")
            .context("Cargo package version must be major.minor.0")?;
        assert_eq!(
            fs::read_to_string(root.join("log.md"))?,
            format!(
                "## [2026-11-18T09:10:11+07:00] init | — | client=test | host=test-host | session=test-session | KB skeleton created by {version}\n"
            )
        );
        drop(guard);
        other.try_lock()?;
        Ok(())
    }

    #[test]
    fn lock_only_directory_is_not_empty() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join(".kb.lock"), b"foreign")?;
        let error = prepare(dir.path(), &test_stamp()).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("{} is not empty — refusing to init", dir.path().display())
        );
        assert_eq!(fs::read(dir.path().join(".kb.lock"))?, b"foreign");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn target_link_is_not_followed_even_with_trailing_dot() -> Result<()> {
        use std::io;
        let dir = tempfile::tempdir()?;
        let actual = dir.path().join("actual");
        fs::create_dir(&actual)?;
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&actual, &link)?;
        let error = prepare(&link.join("."), &test_stamp()).unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(io::ErrorKind::InvalidInput)
        );
        assert_eq!(fs::read_dir(actual)?.count(), 0);
        Ok(())
    }
}
