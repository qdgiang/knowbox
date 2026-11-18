use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::KbRoot;
use crate::store::{self, WriteLock};

const CACHE_LIMIT: usize = 128 * 1024 * 1024;
const COMPONENT_BUDGET: usize = CACHE_LIMIT / 32;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Cache {
    pub(super) model: String,
    pub(super) notes: BTreeMap<String, StoredVector>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StoredVector {
    pub(super) h: String,
    pub(super) v: Vec<f64>,
}

#[derive(Debug, Error)]
pub(super) enum CacheError {
    #[error("reading embedding cache {path}")]
    Read { path: PathBuf, source: io::Error },
    #[error("no embeddings (.kb-embed.json) — run `knowbox embed` first")]
    Missing,
    #[error("embeddings are for a different model or empty — run `knowbox embed --force`")]
    WrongModelOrEmpty,
    #[error("invalid embedding cache — run `knowbox embed --force`")]
    Invalid,
    #[error("embedding cache exceeds the local storage budget")]
    Budget,
    #[error(transparent)]
    Write(#[from] store::StoreError),
}

pub(super) fn check_component_budget(count: usize, dim: usize) -> Result<(), CacheError> {
    if count
        .checked_mul(dim)
        .is_none_or(|components| components > COMPONENT_BUDGET)
    {
        return Err(CacheError::Budget);
    }
    Ok(())
}

impl Cache {
    pub(super) fn empty(model: &str) -> Self {
        Self {
            model: model.to_owned(),
            notes: BTreeMap::new(),
        }
    }

    pub(super) fn dimension(&self) -> Option<usize> {
        self.notes.values().next().map(|entry| entry.v.len())
    }

    fn validate(&self) -> Result<(), CacheError> {
        if let Some(dim) = self.dimension() {
            if dim == 0
                || self
                    .notes
                    .values()
                    .any(|entry| entry.v.len() != dim || entry.v.iter().any(|n| !n.is_finite()))
            {
                return Err(CacheError::Invalid);
            }
            check_component_budget(self.notes.len(), dim)?;
        }
        Ok(())
    }

    pub(super) fn save(&self, lock: &WriteLock) -> Result<(), CacheError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| CacheError::Invalid)?;
        if bytes.len() > CACHE_LIMIT {
            return Err(CacheError::Budget);
        }
        Ok(store::atomic_write(&path(lock.root()), &bytes)?)
    }
}

fn path(root: &KbRoot) -> PathBuf {
    root.path().join(".kb-embed.json")
}

pub(super) fn read_cache(root: &KbRoot) -> Result<Option<Cache>, CacheError> {
    let path = path(root);
    let read_error = |source| CacheError::Read {
        path: path.clone(),
        source,
    };
    if !store::regular_file(&path).map_err(read_error)? {
        return Ok(None);
    }
    let file = match File::open(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => result.map_err(read_error)?,
    };
    let mut bytes = Vec::new();
    file.take(CACHE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(read_error)?;
    if bytes.len() > CACHE_LIMIT {
        return Err(CacheError::Budget);
    }
    let cache: Cache = serde_json::from_slice(&bytes).map_err(|_| CacheError::Invalid)?;
    cache.validate()?;
    Ok(Some(cache))
}

pub(super) fn required_cache(root: &KbRoot, model: &str) -> Result<Cache, CacheError> {
    let cache = read_cache(root)?.ok_or(CacheError::Missing)?;
    if cache.model != model || cache.notes.is_empty() {
        return Err(CacheError::WrongModelOrEmpty);
    }
    Ok(cache)
}
