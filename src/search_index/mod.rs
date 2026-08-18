mod build;
mod io;
mod model;
mod read;

use crate::error::{MemoryBudgetExceeded, NativeError};
use std::path::{Path, PathBuf};

pub(crate) use build::{build, SearchIndexBuildRequest};
pub(crate) use model::RankedDocument;
pub(crate) use read::{search, validate};

pub(crate) fn sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}", database_path.display(), suffix))
}

fn memory_budget_error(phase: &str, limit: usize, accounted: usize) -> NativeError {
    NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
        phase,
        limit as u64,
        accounted as u64,
        0,
    ))
}

#[cfg(test)]
mod tests;
