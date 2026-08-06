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
    let dir = File::open(path)?;
    dir.sync_all()?;
    Ok(())
}

pub(crate) fn create_new_file(path: &Path) -> Result<File, NativeError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "temp".to_string());
    let seq = ATOMIC_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{file_name}.tmp.{seq}"))
}
