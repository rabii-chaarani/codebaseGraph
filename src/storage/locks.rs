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
    companions: Vec<LockedFile>,
}

impl LockedFile {
    /// Verify actual held ownership, including lock inode replacement. Labels
    /// and paths supplied by callers never constitute ownership on their own.
    pub(crate) fn verifies(&self, path: &Path, mode: LockMode) -> Result<bool, NativeError> {
        let Some(file) = &self.file else {
            return Ok(false);
        };
        Ok(self.path == path
            && self.mode == mode
            && file_identity(file)? == path_identity(path)?
            && file_link_count(file)? == 1
            && regular_unaliased(path)?)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn mode(&self) -> LockMode {
        self.mode
    }
    pub(crate) fn is_current(&self) -> Result<bool, NativeError> {
        self.verifies(&self.path, self.mode)
    }
    pub(crate) fn retain(&mut self, companion: LockedFile) {
        self.companions.push(companion);
    }

    pub(crate) fn identity(&self) -> Result<FileIdentity, NativeError> {
        file_identity(
            self.file
                .as_ref()
                .ok_or_else(|| NativeError::InvalidInput("released lock".into()))?,
        )
    }

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
    open_locked_inner(path.as_ref(), mode, false, true, true, false)
}

pub(crate) fn try_open_locked(
    path: impl AsRef<Path>,
    mode: LockMode,
) -> Result<Option<LockedFile>, NativeError> {
    try_open_locked_inner(path.as_ref(), mode, true, true)
}

pub(crate) fn try_open_locked_in_existing_parent(
    path: impl AsRef<Path>,
    mode: LockMode,
) -> Result<Option<LockedFile>, NativeError> {
    try_open_locked_inner(path.as_ref(), mode, false, true)
}

/// Pure observation: never creates a missing parent or file.
pub(crate) fn try_open_existing_locked(
    path: &Path,
    mode: LockMode,
) -> Result<Option<LockedFile>, NativeError> {
    try_open_locked_inner(path, mode, false, false)
}

pub(crate) fn open_existing_shared_coordination(path: &Path) -> Result<LockedFile, NativeError> {
    open_locked_inner(path, LockMode::Shared, false, false, false, true)
}
pub(crate) fn open_existing_exclusive(path: &Path) -> Result<LockedFile, NativeError> {
    open_locked_inner(path, LockMode::Exclusive, false, false, false, false)
}

/// Existing readable shared coordination requires no write permission. Create
/// normally only when absent, never recreate a detached parent. A competing
/// creator may publish a read-only file before our create/open; retry that
/// existing file read-only instead of imposing unrelated write permission.
pub(crate) fn open_shared_coordination(path: &Path) -> Result<LockedFile, NativeError> {
    match open_locked_inner(path, LockMode::Shared, false, false, false, true) {
        Ok(lease) => Ok(lease),
        Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            match open_locked_inner(path, LockMode::Shared, false, false, true, false) {
                Ok(lease) => Ok(lease),
                Err(create_error) => {
                    open_locked_inner(path, LockMode::Shared, false, false, false, true)
                        .or(Err(create_error))
                }
            }
        }
        Err(error) => Err(error),
    }
}

fn try_open_locked_inner(
    path: &Path,
    mode: LockMode,
    create_parent: bool,
    create_file: bool,
) -> Result<Option<LockedFile>, NativeError> {
    match open_locked_inner(path, mode, true, create_parent, create_file, false) {
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
    create_parent: bool,
    create_file: bool,
    read_only: bool,
) -> Result<LockedFile, NativeError> {
    if let Some(parent) = path.parent() {
        if create_parent {
            fs::create_dir_all(parent)?;
        }
        let parent_metadata = fs::symlink_metadata(parent)?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(NativeError::InvalidInput(format!(
                "lock parent must be a real directory: {}",
                parent.display()
            )));
        }
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent_identity = path_identity(parent)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if create_file && error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if metadata.is_some_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file()) {
        return Err(NativeError::InvalidInput(format!(
            "lock path must be a real file: {}",
            path.display()
        )));
    }
    let file = OpenOptions::new()
        .create(create_file)
        .read(true)
        .write(!read_only)
        .truncate(false)
        .open(path)?;
    if file_link_count(&file)? != 1 || file_identity(&file)? != path_identity(path)? {
        return Err(NativeError::InvalidInput(format!(
            "lock_identity_ambiguous: {}",
            path.display()
        )));
    }
    #[cfg(test)]
    BEFORE_LOCK_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook();
        }
    });
    match (mode, non_blocking) {
        (LockMode::Shared, false) => FileExt::lock_shared(&file)?,
        (LockMode::Exclusive, false) => FileExt::lock_exclusive(&file)?,
        (LockMode::Shared, true) => FileExt::try_lock_shared(&file)?,
        (LockMode::Exclusive, true) => FileExt::try_lock_exclusive(&file)?,
    }
    if parent_identity != path_identity(parent)?
        || file_identity(&file)? != path_identity(path)?
        || file_link_count(&file)? != 1
        || !regular_unaliased(path)?
    {
        return Err(NativeError::InvalidInput(format!(
            "lock_identity_changed: {}",
            path.display()
        )));
    }
    Ok(LockedFile {
        path: path.to_path_buf(),
        file: Some(file),
        mode,
        companions: Vec::new(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileIdentity {
    volume: u64,
    index: u64,
}

pub(crate) fn path_identity(path: &Path) -> Result<FileIdentity, NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "symlink_identity: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileIdentity {
            volume: metadata.dev(),
            index: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        file_identity(&file)
    }
    #[cfg(not(any(unix, windows)))]
    Err(NativeError::InvalidInput(
        "physical_identity_unavailable".into(),
    ))
}

pub(crate) fn file_identity(file: &File) -> Result<FileIdentity, NativeError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(FileIdentity {
            volume: metadata.dev(),
            index: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(FileIdentity {
            volume: u64::from(info.dwVolumeSerialNumber),
            index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        })
    }
    #[cfg(not(any(unix, windows)))]
    Err(NativeError::InvalidInput(
        "physical_identity_unavailable".into(),
    ))
}

pub(crate) fn file_link_count(file: &File) -> Result<u64, NativeError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(file.metadata()?.nlink())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(u64::from(info.nNumberOfLinks))
    }
    #[cfg(not(any(unix, windows)))]
    Err(NativeError::InvalidInput(
        "physical_link_count_unavailable".into(),
    ))
}

fn regular_unaliased(path: &Path) -> Result<bool, NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    let file = File::open(path)?;
    Ok(file_identity(&file)? == path_identity(path)? && file_link_count(&file)? == 1)
}

#[cfg(test)]
thread_local! {
    static BEFORE_LOCK_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn before_lock_test_hook(hook: impl FnOnce() + 'static) {
    BEFORE_LOCK_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}
