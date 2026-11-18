use std::env;
use std::fs::{self, File, Metadata, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::{NamedTempFile, TempPath};
use thiserror::Error;

use crate::config::KbRoot;

pub(crate) const LOCK_NAME: &str = ".kb.lock";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Error)]
pub(crate) enum StoreError {
    #[error("{action} {path}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    #[error("writer lock held by another writer at {path} (waited {timeout:?})")]
    Contended { path: PathBuf, timeout: Duration },
    #[error("publishing temporary file {temp} as {dest}")]
    Publish {
        temp: PathBuf,
        dest: PathBuf,
        source: io::Error,
    },
}

/// An open, exclusively locked handle. Dropping it releases the lock, not the directory entry.
#[derive(Debug)]
pub(crate) struct WriteLock {
    root: KbRoot,
    handle: File,
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        // A Unix child can keep an inherited descriptor alive until exec.
        if let Err(error) = self.handle.unlock() {
            // Closing the handle is the fallback; reporting is best-effort in Drop.
            drop(writeln!(
                io::stderr(),
                "knowbox: releasing writer lock for {}: {error}",
                self.root.path().display()
            ));
        }
    }
}

impl WriteLock {
    pub(crate) fn acquire(root: &KbRoot) -> Result<Self, StoreError> {
        let path = root.path().join(LOCK_NAME);
        regular_file(&path).map_err(|source| StoreError::Io {
            action: "checking lock file",
            path: path.clone(),
            source,
        })?;
        let handle = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| StoreError::Io {
                action: "opening lock file",
                path: path.clone(),
                source,
            })?;
        Self::lock(root.clone(), handle, path, lock_timeout())
    }

    // Never adopt an existing lock during init: even a lock-only directory is nonempty.
    pub(crate) fn reserve(root: KbRoot) -> Result<Self, StoreError> {
        let path = root.path().join(LOCK_NAME);
        let handle = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| StoreError::Io {
                action: "creating init reservation",
                path: path.clone(),
                source,
            })?;
        Self::lock(root, handle, path, lock_timeout())
    }

    fn lock(
        root: KbRoot,
        handle: File,
        path: PathBuf,
        timeout: Duration,
    ) -> Result<Self, StoreError> {
        let started = Instant::now();
        loop {
            match handle.try_lock() {
                Ok(()) => return Ok(Self { root, handle }),
                Err(TryLockError::Error(source)) => {
                    return Err(StoreError::Io {
                        action: "locking",
                        path,
                        source,
                    });
                }
                Err(TryLockError::WouldBlock) => {
                    let remaining = timeout.saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        return Err(StoreError::Contended { path, timeout });
                    }
                    thread::sleep(Duration::from_millis(50).min(remaining));
                }
            }
        }
    }

    pub(crate) fn root(&self) -> &KbRoot {
        &self.root
    }
}

fn lock_timeout() -> Duration {
    match env::var("KNOWBOX_LOCK_TIMEOUT") {
        Ok(value) => parse_timeout(&value),
        Err(_) => DEFAULT_TIMEOUT,
    }
}

fn parse_timeout(value: &str) -> Duration {
    match value.trim().parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds > 0.0 => {
            Duration::from_secs_f64(seconds.min(3600.0))
        }
        _ => DEFAULT_TIMEOUT,
    }
}

// lstat, not stat. On Windows a junction (and some other redirects) is a
// reparse point even when the standard FileType doesn't classify it as a symlink.
fn metadata(path: &Path) -> io::Result<Option<Metadata>> {
    // Strip trailing separators and `/.`: on Unix either would force lstat to follow the leaf.
    let clean: PathBuf = path.components().collect();
    let meta = match fs::symlink_metadata(clean) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    #[cfg(not(windows))]
    let redirected = meta.is_symlink();
    if redirected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing symbolic link or reparse point {}", path.display()),
        ));
    }
    Ok(Some(meta))
}

fn file_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    let meta = metadata(path)?;
    if let Some(meta) = &meta
        && !meta.is_file()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a regular file: {}", path.display()),
        ));
    }
    Ok(meta)
}

pub(crate) fn regular_file(path: &Path) -> io::Result<bool> {
    Ok(file_metadata(path)?.is_some())
}

pub(crate) fn directory(path: &Path) -> io::Result<bool> {
    match metadata(path)? {
        None => Ok(false),
        Some(meta) if meta.is_dir() => Ok(true),
        Some(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a directory: {}", path.display()),
        )),
    }
}

fn parent_dir(path: &Path) -> io::Result<&Path> {
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a file name",
        ));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !directory(parent)? {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("parent directory {} does not exist", parent.display()),
        ));
    }
    Ok(parent)
}

pub(crate) fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let create_error = |source| StoreError::Io {
        action: "creating temporary file beside",
        path: dest.into(),
        source,
    };
    let parent = parent_dir(dest).map_err(create_error)?;
    let mut temp = NamedTempFile::new_in(parent).map_err(create_error)?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|source| StoreError::Io {
            action: "writing temporary file",
            path: temp.path().into(),
            source,
        })?;
    // into_temp_path closes the file handle before persist, including on Windows.
    replace(temp.into_temp_path(), dest)
}

/// Publish a closed, private same-directory temporary file, never removing `dest` on error.
pub(crate) fn replace(mut temp: TempPath, dest: &Path) -> Result<(), StoreError> {
    let validate = || -> io::Result<()> {
        if parent_dir(dest)?.canonicalize()? != parent_dir(&temp)?.canonicalize()? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "temporary file must be in the destination directory",
            ));
        }
        if !regular_file(&temp)? {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "temporary file is missing",
            ));
        }
        if let Some(old) = file_metadata(dest)? {
            #[cfg(windows)]
            if old.permissions().readonly() {
                // persist clears source attributes on Windows. Never clear read-only on the old dest.
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "destination is read-only",
                ));
            }
            fs::set_permissions(&temp, old.permissions())?;
        }
        Ok(())
    };
    validate().map_err(|source| StoreError::Publish {
        temp: temp.to_path_buf(),
        dest: dest.into(),
        source,
    })?;

    let mut retries = 4;
    loop {
        match temp.persist(dest) {
            Ok(()) => return Ok(()),
            Err(error) if retries > 0 && retryable_replace(&error.error) => {
                temp = error.path;
                retries -= 1;
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(StoreError::Publish {
                    temp: error.path.to_path_buf(),
                    dest: dest.into(),
                    source: error.error,
                });
            }
        }
    }
}

fn retryable_replace(error: &io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn timeout_is_finite_and_bounded() {
        for value in ["NaN", "inf", "-2", "0", "bogus"] {
            assert_eq!(parse_timeout(value), Duration::from_secs(120));
        }
        assert_eq!(parse_timeout(" 0.25 "), Duration::from_millis(250));
        assert_eq!(parse_timeout("1e300"), Duration::from_secs(3600));
    }

    #[test]
    fn lock_serializes_independent_handles_and_does_not_truncate_or_unlink() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = KbRoot::open(dir.path())?;
        let path = root.path().join(".kb.lock");
        fs::write(&path, b"persistent\n")?;
        let guard = WriteLock::acquire(&root)?;
        let other = File::options().read(true).write(true).open(&path)?;
        assert!(matches!(
            WriteLock::lock(root.clone(), other, path.clone(), Duration::ZERO),
            Err(StoreError::Contended {
                timeout: Duration::ZERO,
                ..
            })
        ));
        drop(guard);
        drop(WriteLock::acquire(&root)?);
        assert_eq!(fs::read(path)?, b"persistent\n");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn dropping_guard_unlocks_even_while_a_duplicate_descriptor_is_open() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = KbRoot::open(dir.path())?;
        let guard = WriteLock::acquire(&root)?;
        // A forked child before exec keeps the same open file description alive.
        let duplicate = guard.handle.try_clone()?;
        let contender = File::options()
            .read(true)
            .write(true)
            .open(root.path().join(LOCK_NAME))?;
        assert!(matches!(
            contender.try_lock(),
            Err(TryLockError::WouldBlock)
        ));

        drop(guard);
        contender.try_lock()?;
        drop(duplicate);
        Ok(())
    }

    #[test]
    fn plain_file_and_directory_differ_from_missing() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("file");
        assert!(!regular_file(&path)?);
        assert!(!directory(&path)?);
        File::create_new(&path)?;
        assert!(regular_file(&path)?);
        assert!(directory(dir.path())?);
        assert_eq!(
            directory(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            regular_file(dir.path()).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn writes_reject_dangling_and_existing_links_and_keep_original() -> Result<()> {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("actual");
        let link = dir.path().join("redirect");
        symlink(&target, &link)?;
        assert_eq!(
            regular_file(&link).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        fs::write(&target, b"outside")?;
        assert!(
            matches!(atomic_write(&link, b"no"), Err(StoreError::Publish { source, .. }) if source.kind() == io::ErrorKind::InvalidInput)
        );
        assert_eq!(fs::read(target)?, b"outside");
        assert!(fs::symlink_metadata(link)?.is_symlink());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_mode() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let dest = dir.path().join("index.md");
        atomic_write(&dest, b"old bytes")?;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o640))?;
        atomic_write(&dest, b"new")?;
        assert_eq!(fs::read(&dest)?, b"new");
        assert_eq!(fs::metadata(dest)?.permissions().mode() & 0o777, 0o640);
        Ok(())
    }

    #[test]
    fn foreign_temp_directory_never_replaces_destination() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let other = tempfile::tempdir()?;
        let dest = dir.path().join("index.md");
        fs::write(&dest, b"old")?;
        let temp = NamedTempFile::new_in(other.path())?.into_temp_path();
        let name = temp.to_path_buf();
        assert!(
            matches!(replace(temp, &dest), Err(StoreError::Publish { source, .. }) if source.kind() == io::ErrorKind::InvalidInput)
        );
        assert_eq!(fs::read(dest)?, b"old");
        assert!(!regular_file(&name)?);
        Ok(())
    }
}
