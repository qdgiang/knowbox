use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::{redirect_error, redirected, symlink_metadata};

#[derive(Debug, Default)]
pub(crate) struct InventoryWarnings(BTreeMap<PathBuf, String>);

#[derive(serde::Serialize)]
pub(crate) struct InventoryWarning {
    path: String,
    message: String,
}

impl InventoryWarnings {
    pub(crate) fn report(self, err: &mut impl Write) -> io::Result<()> {
        for message in self.0.values() {
            writeln!(err, "knowbox: warning: {message}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum Inventory<'a> {
    Strict,
    Read(&'a mut InventoryWarnings),
}

#[derive(Debug)]
pub(crate) enum EntryKind {
    MarkdownFile,
    Visible,
}

impl EntryKind {
    fn includes(&self, name: &OsStr) -> bool {
        let name = name.as_encoded_bytes();
        match self {
            Self::MarkdownFile => name.ends_with(b".md"),
            Self::Visible => !name.starts_with(b"."),
        }
    }
}

impl Inventory<'_> {
    pub(crate) fn take_warnings(&mut self) -> impl Iterator<Item = (PathBuf, String)> {
        match self {
            Self::Strict => BTreeMap::new(),
            Self::Read(warnings) => std::mem::take(&mut warnings.0),
        }
        .into_iter()
    }

    pub(crate) fn take_json_warnings(&mut self) -> Vec<InventoryWarning> {
        self.take_warnings()
            .map(|(path, message)| InventoryWarning {
                path: path.to_string_lossy().into_owned(),
                message,
            })
            .collect()
    }

    fn irregular(&mut self, path: &Path, error: io::Error) -> io::Result<()> {
        match self {
            Self::Strict => Err(error),
            Self::Read(warnings) => {
                warnings
                    .0
                    .entry(path.into())
                    .or_insert_with(|| format!("{error} (skipped)"));
                Ok(())
            }
        }
    }

    fn metadata(&mut self, path: &Path) -> io::Result<Option<Metadata>> {
        let meta = symlink_metadata(path)?;
        if meta.as_ref().is_some_and(redirected) {
            self.irregular(path, redirect_error(path))?;
            return Ok(None);
        }
        Ok(meta)
    }

    pub(crate) fn directory(&mut self, path: &Path) -> io::Result<bool> {
        match self.metadata(path)? {
            None => Ok(false),
            Some(meta) if meta.is_dir() => Ok(true),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a directory: {}", path.display()),
            )),
        }
    }

    fn is_regular(&mut self, path: &Path, meta: &Metadata) -> io::Result<bool> {
        if meta.is_file() {
            return Ok(true);
        }
        if !meta.is_dir() || matches!(self, Self::Strict) {
            self.irregular(
                path,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("not a regular file: {path:?}"),
                ),
            )?;
        }
        Ok(false)
    }

    pub(super) fn file_metadata(&mut self, path: &Path) -> io::Result<Option<Metadata>> {
        if let Some(meta) = self.metadata(path)?
            && self.is_regular(path, &meta)?
        {
            return Ok(Some(meta));
        }
        Ok(None)
    }

    pub(crate) fn file(&mut self, path: &Path) -> io::Result<bool> {
        Ok(self.file_metadata(path)?.is_some())
    }

    pub(crate) fn names(&mut self, dir: &Path, kind: EntryKind) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if !kind.includes(&name) {
                continue;
            }
            let path = entry.path();
            let Some(meta) = self.metadata(&path)? else {
                continue;
            };
            if matches!(kind, EntryKind::MarkdownFile) && !self.is_regular(&path, &meta)? {
                continue;
            }
            match name.into_string() {
                Ok(name) => names.push(name),
                Err(_) => self.irregular(
                    &path,
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("non-UTF-8 filename {path:?}"),
                    ),
                )?,
            }
        }
        names.sort_unstable();
        Ok(names)
    }
}
