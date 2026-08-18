use crate::error::NativeError;
use fs2::{lock_contended_error, FileExt};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug)]
pub(crate) struct LockedFile {
    path: PathBuf,
    file: Option<File>,
    mode: LockMode,
}

impl LockedFile {
    pub(crate) fn relock_shared(mut self) -> Result<Self, NativeError> {
        let file = self.file.as_ref().ok_or_else(|| {
            NativeError::InvalidInput(format!(
                "lock {} has already been released",
                self.path.display()
            ))
        })?;
        FileExt::lock_shared(file)?;
        self.mode = LockMode::Shared;
        Ok(self)
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            let _ = FileExt::unlock(&file);
        }
    }
}

pub(crate) type WriterLease = LockedFile;
pub(crate) type StateLease = LockedFile;
pub(crate) type RunLease = LockedFile;
pub(crate) type RefreshLease = LockedFile;
pub(crate) type WorkerLease = LockedFile;
pub(crate) type CoordinatorLease = LockedFile;

pub(crate) fn open_locked(
    path: impl AsRef<Path>,
    mode: LockMode,
) -> Result<LockedFile, NativeError> {
    open_locked_inner(path.as_ref(), mode, false)
}

pub(crate) fn try_open_locked(
    path: impl AsRef<Path>,
    mode: LockMode,
) -> Result<Option<LockedFile>, NativeError> {
    match open_locked_inner(path.as_ref(), mode, true) {
        Ok(locked) => Ok(Some(locked)),
        Err(NativeError::Io(error)) if is_lock_contended(&error) => Ok(None),
        Err(other) => Err(other),
    }
}

fn is_lock_contended(error: &std::io::Error) -> bool {
    let expected = lock_contended_error();
    error.kind() == std::io::ErrorKind::WouldBlock
        || (error.raw_os_error().is_some() && error.raw_os_error() == expected.raw_os_error())
}

fn open_locked_inner(
    path: &Path,
    mode: LockMode,
    non_blocking: bool,
) -> Result<LockedFile, NativeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let parent_metadata = fs::symlink_metadata(parent)?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(NativeError::InvalidInput(format!(
                "lock parent must be a real directory: {}",
                parent.display()
            )));
        }
    }
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(NativeError::InvalidInput(format!(
                "lock path must be a real file: {}",
                path.display()
            )));
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match (mode, non_blocking) {
        (LockMode::Shared, false) => FileExt::lock_shared(&file)?,
        (LockMode::Exclusive, false) => FileExt::lock_exclusive(&file)?,
        (LockMode::Shared, true) => FileExt::try_lock_shared(&file)?,
        (LockMode::Exclusive, true) => FileExt::try_lock_exclusive(&file)?,
    }
    Ok(LockedFile {
        path: path.to_path_buf(),
        file: Some(file),
        mode,
    })
}
