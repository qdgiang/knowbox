use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::store::{LOCK_NAME, directory, regular_file};

pub(crate) const SUBDIRS: [&str; 3] = ["notes", "refs", "raw"];

#[derive(Debug, Error)]
pub(crate) enum RootError {
    #[error("cannot determine an absolute home directory; set KNOWBOX_ROOT explicitly")]
    NoHome,
    #[error("KNOWBOX_ROOT is blank; set a directory or unset the variable")]
    BlankOverride,
    #[error("invalid KNOWBOX_ROOT={path:?}; refusing to fall back to ~/knowbox")]
    InvalidOverride {
        path: PathBuf,
        #[source]
        source: Box<RootError>,
    },
    #[error("checking knowledge base path {path}")]
    Inspect { path: PathBuf, source: io::Error },
    #[error("knowledge base directory {0} does not exist")]
    Missing(PathBuf),
    #[error("{0} is not a knowledge base (no notes/ directory); run `knowbox init` first")]
    NoNotes(PathBuf),
}

#[derive(Debug, Clone)]
pub(crate) struct KbRoot(PathBuf);

impl KbRoot {
    pub(crate) fn resolve() -> Result<Self, RootError> {
        if let Some(path) = env::var_os("KNOWBOX_ROOT") {
            return Self::from_override(&path);
        }
        let root = Self::open(&default_root()?)?;
        let notes = root.path().join("notes");
        if !directory(&notes).map_err(|source| RootError::Inspect {
            path: notes,
            source,
        })? {
            return Err(RootError::NoNotes(root.0));
        }
        Ok(root)
    }

    fn from_override(path: &OsStr) -> Result<Self, RootError> {
        if path.to_str().is_some_and(|s| s.trim().is_empty()) {
            return Err(RootError::BlankOverride);
        }
        Self::open(Path::new(path)).map_err(|source| RootError::InvalidOverride {
            path: path.into(),
            source: Box::new(source),
        })
    }

    pub(crate) fn open(path: &Path) -> Result<Self, RootError> {
        let inspect = |source| RootError::Inspect {
            path: path.into(),
            source,
        };
        if !directory(path).map_err(inspect)? {
            return Err(RootError::Missing(path.into()));
        }
        let path = path.canonicalize().map_err(inspect)?;
        for sub in SUBDIRS {
            let path = path.join(sub);
            directory(&path).map_err(|source| RootError::Inspect { path, source })?;
        }
        let lock = path.join(LOCK_NAME);
        regular_file(&lock).map_err(|source| RootError::Inspect { path: lock, source })?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

pub(crate) fn default_root() -> Result<PathBuf, RootError> {
    env::home_dir()
        .filter(|home| home.is_absolute())
        .map(|home| home.join("knowbox"))
        .ok_or(RootError::NoHome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_override_never_selects_default() {
        for path in ["", " \t\n"] {
            assert!(matches!(
                KbRoot::from_override(OsStr::new(path)),
                Err(RootError::BlankOverride)
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn override_preserves_native_path() -> anyhow::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir()?;
        let native = dir.path().join(OsStr::from_bytes(b" kb\xff "));
        std::fs::create_dir(&native)?;
        assert_eq!(
            KbRoot::from_override(native.as_os_str())?.path(),
            native.canonicalize()?
        );
        Ok(())
    }
}
