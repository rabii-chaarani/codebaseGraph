use crate::error::NativeError;
use std::path::{Path, PathBuf};

pub(super) fn remove_existing_database(path: &str) -> Result<(), NativeError> {
    let path = Path::new(path);
    for sidecar in database_sidecar_paths(path) {
        remove_path_if_exists(&sidecar)?;
    }
    remove_path_if_exists(path)
}

fn database_sidecar_paths(path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for suffix in ["wal", "tmp", "lock"] {
        paths.push(PathBuf::from(format!(
            "{}.{suffix}",
            path.to_string_lossy()
        )));
    }
    paths
}

fn remove_path_if_exists(path: &Path) -> Result<(), NativeError> {
    if !path.exists() {
        return Ok(());
    }
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| {
        NativeError::Database(format!(
            "failed to remove existing database {}: {error}",
            path.display()
        ))
    })
}
