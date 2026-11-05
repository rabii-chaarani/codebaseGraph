mod build;
mod io;
mod model;
mod read;

use std::path::{Path, PathBuf};

pub(crate) use build::{build, SearchIndexBuildRequest};
pub(crate) use model::RankedDocument;
pub(crate) use read::{search, validate};

pub(crate) fn sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}", database_path.display(), suffix))
}

#[cfg(test)]
mod tests;
