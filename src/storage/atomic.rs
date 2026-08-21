use crate::error::NativeError;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static ATOMIC_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AtomicWriteFailure {
    None,
    AfterFileSync,
    AfterRename,
}

pub(crate) fn write_json_atomically<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), NativeError> {
    write_json_atomically_with_fault(path, value, AtomicWriteFailure::None)
}

pub(crate) fn write_json_atomically_with_fault<T: Serialize>(
    path: &Path,
    value: &T,
    fault: AtomicWriteFailure,
) -> Result<(), NativeError> {
    let parent = path.parent().ok_or_else(|| {
        NativeError::InvalidInput(format!("path {} has no parent", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let temp_path = temp_path_for(path);
    let payload = serde_json::to_vec_pretty(value)?;
    let mut file = create_new_file(&temp_path)?;
    file.write_all(&payload)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    if fault == AtomicWriteFailure::AfterFileSync {
        let _ = fs::remove_file(&temp_path);
        return Err(NativeError::Io(std::io::Error::other(
            "injected atomic write failure after file sync",
        )));
    }
    fs::rename(&temp_path, path)?;
    sync_dir(parent)?;
    if fault == AtomicWriteFailure::AfterRename {
        let _ = fs::remove_file(&temp_path);
        return Err(NativeError::Io(std::io::Error::other(
            "injected atomic write failure after rename",
        )));
    }
    Ok(())
}

pub(crate) fn sync_dir(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(NativeError::InvalidInput(format!(
            "path {} is not a directory",
            path.display()
        )));
    }

    #[cfg(not(windows))]
    {
        let dir = File::open(path)?;
        dir.sync_all()?;
    }

    // Windows intentionally stops after metadata validation: Rust's portable
    // file API cannot flush a directory handle there. The file itself is
    // synced before rename, so a successful commit must not become a false
    // write failure during the unsupported directory-sync step.

    Ok(())
}

pub(crate) fn create_new_file(path: &Path) -> Result<File, NativeError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

fn temp_path_for(path: &Path) -> PathBuf {
    let seq = ATOMIC_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    temp_path_for_writer(path, std::process::id(), seq)
}

fn temp_path_for_writer(path: &Path, process_id: u32, seq: u64) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "temp".to_string());
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{file_name}.tmp.{process_id}.{seq}"))
}

#[cfg(test)]
mod tests {
    use super::temp_path_for_writer;
    use std::path::Path;

    #[test]
    fn atomic_temp_paths_are_scoped_to_the_writer_process() {
        let target = Path::new("storage/worker.json");

        assert_ne!(
            temp_path_for_writer(target, 41, 1),
            temp_path_for_writer(target, 42, 1)
        );
    }
}
