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

/// Fixed facts only. Neither a stamp nor an open handle authorizes retirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixedControlKind {
    File,
    Directory,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixedControlProblem {
    InvalidPath,
    PathTooLong,
    WrongKind,
    Aliased,
    Changed,
    Oversized,
    BufferTooSmall,
    InvalidRecord,
    EpochMismatch,
    EpochExhausted,
    GuardMismatch,
    Busy,
}
#[derive(Debug)]
pub(crate) enum FixedControlError {
    Io(std::io::Error),
    Invalid(FixedControlProblem),
    Resource(super::maintenance_budget::ResourceLimit),
    Grant(super::maintenance_budget::GrantExhausted),
}
impl From<std::io::Error> for FixedControlError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<super::maintenance_budget::GrantExhausted> for FixedControlError {
    fn from(error: super::maintenance_budget::GrantExhausted) -> Self {
        Self::Grant(error)
    }
}

/// The caller reserves this fixed native-path/metadata floor in the SAME pool
/// as its preparation buffers, before constructing the scratch. No PathBuf,
/// CString, wide Vec, formatted path/error or growable collection is created.
pub(crate) struct FixedControlScratch {
    _reservation: super::maintenance_budget::MemoryReservation,
    #[cfg(unix)]
    path: [u8; libc::PATH_MAX as usize],
    #[cfg(windows)]
    path: [u16; 32768],
}
impl FixedControlScratch {
    pub(crate) fn required_bytes() -> usize {
        std::mem::size_of::<Self>() + 4096
    }
    pub(crate) fn new(
        reservation: super::maintenance_budget::MemoryReservation,
    ) -> Result<Self, FixedControlError> {
        if reservation.remaining() < Self::required_bytes() {
            return Err(FixedControlError::Resource(
                super::maintenance_budget::ResourceLimit::CeilingExceeded,
            ));
        }
        Ok(Self {
            _reservation: reservation,
            #[cfg(unix)]
            path: [0; libc::PATH_MAX as usize],
            #[cfg(windows)]
            path: [0; 32768],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FixedFileStamp {
    identity: FileIdentity,
    kind: FixedControlKind,
    len: u64,
    links: u64,
    modified: [i64; 2],
    changed: [i64; 2],
}
impl FixedFileStamp {
    pub(crate) fn identity(&self) -> FileIdentity {
        self.identity
    }
    pub(crate) fn len(&self) -> u64 {
        self.len
    }
    pub(crate) fn capture(file: &File, kind: FixedControlKind) -> Result<Self, FixedControlError> {
        #[cfg(unix)]
        let stamp = {
            use std::os::unix::fs::MetadataExt;
            let m = file.metadata()?;
            let actual = if m.is_file() {
                FixedControlKind::File
            } else if m.is_dir() {
                FixedControlKind::Directory
            } else {
                return Err(FixedControlError::Invalid(FixedControlProblem::WrongKind));
            };
            Self {
                identity: FileIdentity {
                    volume: m.dev(),
                    index: m.ino(),
                },
                kind: actual,
                len: m.len(),
                links: m.nlink(),
                modified: [m.mtime(), m.mtime_nsec()],
                changed: [m.ctime(), m.ctime_nsec()],
            }
        };
        #[cfg(windows)]
        let stamp = {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                FileBasicInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
                GetFileType, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_TYPE_DISK,
            };
            let handle = file.as_raw_handle();
            // SAFETY: File owns a live handle. Both output structs are native
            // POD, correctly aligned/sized, and read only after successful APIs.
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            let mut basic: FILE_BASIC_INFO = unsafe { std::mem::zeroed() };
            if unsafe { GetFileType(handle) } != FILE_TYPE_DISK {
                return Err(FixedControlError::Invalid(FixedControlProblem::WrongKind));
            }
            if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0
                || unsafe {
                    GetFileInformationByHandleEx(
                        handle,
                        FileBasicInfo,
                        (&mut basic as *mut FILE_BASIC_INFO).cast(),
                        std::mem::size_of::<FILE_BASIC_INFO>() as u32,
                    )
                } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(FixedControlError::Invalid(FixedControlProblem::Aliased));
            }
            Self {
                identity: FileIdentity {
                    volume: u64::from(info.dwVolumeSerialNumber),
                    index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
                },
                kind: if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                    FixedControlKind::Directory
                } else {
                    FixedControlKind::File
                },
                len: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
                links: u64::from(info.nNumberOfLinks),
                modified: [basic.LastWriteTime, 0],
                changed: [basic.ChangeTime, 0],
            }
        };
        #[cfg(not(any(unix, windows)))]
        return Err(FixedControlError::Invalid(FixedControlProblem::WrongKind));
        #[cfg(any(unix, windows))]
        {
            if stamp.kind != kind {
                return Err(FixedControlError::Invalid(FixedControlProblem::WrongKind));
            }
            if kind == FixedControlKind::File && stamp.links != 1 {
                return Err(FixedControlError::Invalid(FixedControlProblem::Aliased));
            }
            Ok(stamp)
        }
    }
    pub(crate) fn recheck(&self, file: &File) -> Result<(), FixedControlError> {
        if Self::capture(file, self.kind)? != *self {
            return Err(FixedControlError::Invalid(FixedControlProblem::Changed));
        }
        Ok(())
    }
}

/// Existing-only, no-follow leaf open. The absolute address remains borrowed;
/// no process CWD lookup, file/parent creation, repair or retirement occurs.
pub(crate) fn open_fixed_control(
    path: &Path,
    kind: FixedControlKind,
    write: bool,
    scratch: &mut FixedControlScratch,
) -> Result<File, FixedControlError> {
    if !path.is_absolute() {
        return Err(FixedControlError::Invalid(FixedControlProblem::InvalidPath));
    }
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        let bytes = path.as_os_str().as_encoded_bytes();
        if bytes.contains(&0) {
            return Err(FixedControlError::Invalid(FixedControlProblem::InvalidPath));
        }
        if bytes.len() >= scratch.path.len() {
            return Err(FixedControlError::Invalid(FixedControlProblem::PathTooLong));
        }
        scratch.path[..bytes.len()].copy_from_slice(bytes);
        scratch.path[bytes.len()] = 0;
        let pointer = scratch.path.as_ptr().cast();
        // SAFETY: the caller-reserved array contains one NUL-terminated native
        // path and stays borrowed through these calls; stat is writable native
        // POD. No create flags/mode argument or symlink-following open is used.
        let mut before: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::lstat(pointer, &mut before) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let expected_type = match kind {
            FixedControlKind::File => libc::S_IFREG,
            FixedControlKind::Directory => libc::S_IFDIR,
        };
        if before.st_mode & libc::S_IFMT == libc::S_IFLNK {
            return Err(FixedControlError::Invalid(FixedControlProblem::Aliased));
        }
        if before.st_mode & libc::S_IFMT != expected_type {
            return Err(FixedControlError::Invalid(FixedControlProblem::WrongKind));
        }
        let flags = (if write { libc::O_RDWR } else { libc::O_RDONLY })
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK;
        let fd = unsafe { libc::open(pointer, flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful open returned a new uniquely owned descriptor;
        // File now closes it on every success/error/unwind path.
        let file = unsafe { File::from_raw_fd(fd) };
        let stamp = FixedFileStamp::capture(&file, kind)?;
        let mut after: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::lstat(pointer, &mut after) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if (kind == FixedControlKind::File && after.st_nlink != 1)
            || stamp.identity
                != (FileIdentity {
                    volume: before.st_dev as u64,
                    index: before.st_ino as u64,
                })
            || stamp.identity
                != (FileIdentity {
                    volume: after.st_dev as u64,
                    index: after.st_ino as u64,
                })
            || after.st_mode & libc::S_IFMT != expected_type
        {
            return Err(FixedControlError::Invalid(FixedControlProblem::Changed));
        }
        Ok(file)
    }
    #[cfg(windows)]
    {
        use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
        use windows_sys::Win32::{
            Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
            Storage::FileSystem::{
                CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
                FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
                OPEN_EXISTING,
            },
        };
        let mut len = 0;
        for unit in path.as_os_str().encode_wide() {
            if unit == 0 {
                return Err(FixedControlError::Invalid(FixedControlProblem::InvalidPath));
            }
            if len + 1 >= scratch.path.len() {
                return Err(FixedControlError::Invalid(FixedControlProblem::PathTooLong));
            }
            scratch.path[len] = unit;
            len += 1;
        }
        scratch.path[len] = 0;
        let access = if kind == FixedControlKind::Directory {
            FILE_READ_ATTRIBUTES
        } else {
            GENERIC_READ | if write { GENERIC_WRITE } else { 0 }
        };
        // SAFETY: the fixed aligned UTF-16 array is NUL-terminated and lives
        // through the call. OPEN_EXISTING creates nothing; the returned unique
        // handle is adopted below only after checking INVALID_HANDLE_VALUE.
        let handle = unsafe {
            CreateFileW(
                scratch.path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: this is the successful uniquely owned CreateFileW result.
        let file = unsafe { File::from_raw_handle(handle) };
        FixedFileStamp::capture(&file, kind)?;
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    Err(FixedControlError::Invalid(FixedControlProblem::WrongKind))
}

/// Read at most the caller-reserved capacity, including an EOF check, and
/// reject in-place metadata changes. Address recheck uses a fresh fixed open;
/// retaining an old FD alone is not evidence about its current public name.
pub(crate) fn read_fixed_control(
    file: &mut File,
    stamp: &FixedFileStamp,
    buffer: &mut super::maintenance_budget::ReservedBytes,
) -> Result<usize, FixedControlError> {
    use std::io::{Read, Seek, SeekFrom};
    stamp.recheck(file)?;
    let len = usize::try_from(stamp.len)
        .map_err(|_| FixedControlError::Invalid(FixedControlProblem::Oversized))?;
    if len > buffer.as_slice().len() {
        return Err(FixedControlError::Invalid(FixedControlProblem::Oversized));
    }
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut buffer.as_mut_slice()[..len])?;
    let mut extra = [0; 1];
    if file.read(&mut extra)? != 0 {
        return Err(FixedControlError::Invalid(FixedControlProblem::Changed));
    }
    stamp.recheck(file)?;
    Ok(len)
}

/// Path-free fixed authority; cannot cross threads or survive as Send facts.
/// The handle is closed on every failed acquisition, error and unwind.
pub(crate) struct FixedLockedFile {
    file: File,
    mode: LockMode,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl FixedLockedFile {
    pub(crate) fn try_lock(file: File, mode: LockMode) -> Result<Option<Self>, FixedControlError> {
        let result = match mode {
            LockMode::Shared => FileExt::try_lock_shared(&file),
            LockMode::Exclusive => FileExt::try_lock_exclusive(&file),
        };
        match result {
            Ok(()) => Ok(Some(Self {
                file,
                mode,
                _same_thread: std::marker::PhantomData,
            })),
            Err(error) if is_lock_contended(&error) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    pub(crate) fn file(&self) -> &File {
        &self.file
    }
    pub(crate) fn mode(&self) -> LockMode {
        self.mode
    }
}
impl Drop for FixedLockedFile {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}
impl LockedFile {
    pub(crate) fn matches_fixed_current(
        &self,
        current: &FixedFileStamp,
        mode: LockMode,
    ) -> Result<bool, FixedControlError> {
        let Some(file) = self.file.as_ref() else {
            return Ok(false);
        };
        Ok(self.mode == mode
            && FixedFileStamp::capture(file, FixedControlKind::File)?.identity == current.identity)
    }
}

#[cfg(test)]
mod fixed_control_tests {
    use super::*;
    use crate::storage::maintenance_budget::CleanupMemory;
    use std::io::Write;

    struct Fixture {
        root: PathBuf,
        memory: CleanupMemory,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "fixed-control-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            Self {
                root,
                memory: CleanupMemory::new(1024 * 1024).unwrap(),
            }
        }
        fn scratch(&self) -> FixedControlScratch {
            FixedControlScratch::new(
                self.memory
                    .reserve(FixedControlScratch::required_bytes())
                    .unwrap(),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn dc_fixed_stamp_open_read_and_recheck_are_noncreating_and_bounded() {
        let f = Fixture::new();
        let mut scratch = f.scratch();
        let missing = f.root.join("missing");
        match open_fixed_control(&missing, FixedControlKind::File, true, &mut scratch) {
            Err(FixedControlError::Io(error)) => {
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound)
            }
            other => panic!("expected typed NotFound, got {other:?}"),
        }
        assert!(!missing.exists());
        let path = f.root.join("control");
        fs::write(&path, b"four").unwrap();
        let mut file =
            open_fixed_control(&path, FixedControlKind::File, false, &mut scratch).unwrap();
        let stamp = FixedFileStamp::capture(&file, FixedControlKind::File).unwrap();
        let mut bytes = f.memory.reserve(4).unwrap().bytes(4).unwrap();
        assert_eq!(
            read_fixed_control(&mut file, &stamp, &mut bytes).unwrap(),
            4
        );
        assert_eq!(bytes.as_slice(), b"four");
        let mut too_small = f.memory.reserve(3).unwrap().bytes(3).unwrap();
        assert!(matches!(
            read_fixed_control(&mut file, &stamp, &mut too_small),
            Err(FixedControlError::Invalid(FixedControlProblem::Oversized))
        ));
        assert_eq!(fs::read(&path).unwrap(), b"four");
        assert_eq!(fs::read_dir(&f.root).unwrap().count(), 1);
    }

    #[test]
    fn dc_fixed_stamp_rejects_in_place_change_and_new_public_incarnation() {
        let f = Fixture::new();
        let path = f.root.join("control");
        fs::write(&path, b"before").unwrap();
        let mut scratch = f.scratch();
        let file = open_fixed_control(&path, FixedControlKind::File, false, &mut scratch).unwrap();
        let stamp = FixedFileStamp::capture(&file, FixedControlKind::File).unwrap();
        let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(b"longer-after").unwrap();
        writer.sync_all().unwrap();
        assert!(stamp.recheck(&file).is_err());
        fs::rename(&path, f.root.join("old-control")).unwrap();
        fs::write(&path, b"replacement").unwrap();
        let current =
            open_fixed_control(&path, FixedControlKind::File, false, &mut scratch).unwrap();
        assert_ne!(
            stamp.identity(),
            FixedFileStamp::capture(&current, FixedControlKind::File)
                .unwrap()
                .identity()
        );
        assert!(stamp.recheck(&current).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn dc_fixed_stamp_rejects_symlinks_hardlinks_and_nonregular_controls() {
        let f = Fixture::new();
        let path = f.root.join("control");
        fs::write(&path, b"data").unwrap();
        let alias = f.root.join("alias");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        let mut scratch = f.scratch();
        assert!(matches!(
            open_fixed_control(&alias, FixedControlKind::File, false, &mut scratch),
            Err(FixedControlError::Invalid(FixedControlProblem::Aliased))
        ));
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&path, &alias).unwrap();
        assert!(matches!(
            open_fixed_control(&path, FixedControlKind::File, false, &mut scratch),
            Err(FixedControlError::Invalid(FixedControlProblem::Aliased))
        ));
        assert!(matches!(
            open_fixed_control(&f.root, FixedControlKind::File, false, &mut scratch),
            Err(FixedControlError::Invalid(FixedControlProblem::WrongKind))
        ));
    }

    #[test]
    fn dc_fixed_stamp_same_length_write_with_restored_mtime_stays_changed() {
        let f = Fixture::new();
        let path = f.root.join("control");
        fs::write(&path, b"before").unwrap();
        let mut scratch = f.scratch();
        let file = open_fixed_control(&path, FixedControlKind::File, false, &mut scratch).unwrap();
        let stamp = FixedFileStamp::capture(&file, FixedControlKind::File).unwrap();
        let modified = file.metadata().unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(b"after!").unwrap();
        writer.sync_all().unwrap();
        writer
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let changed = FixedFileStamp::capture(&file, FixedControlKind::File).unwrap();
        assert_eq!(stamp.len, changed.len);
        assert_eq!(stamp.modified, changed.modified);
        assert_ne!(stamp.changed, changed.changed);
        assert!(stamp.recheck(&file).is_err());
    }

    #[test]
    fn dc_fixed_stamp_scratch_uses_shared_reservation_and_releases_it() {
        let memory = CleanupMemory::new(FixedControlScratch::required_bytes() + 128).unwrap();
        let initial = memory.used();
        let scratch = FixedControlScratch::new(
            memory
                .reserve(FixedControlScratch::required_bytes())
                .unwrap(),
        )
        .unwrap();
        assert!(memory
            .reserve(FixedControlScratch::required_bytes())
            .is_err());
        drop(scratch);
        assert_eq!(memory.used(), initial);
        match FixedControlScratch::new(memory.reserve(1).unwrap()) {
            Err(FixedControlError::Resource(limit)) => assert_eq!(
                limit,
                super::super::maintenance_budget::ResourceLimit::CeilingExceeded
            ),
            _ => panic!("expected typed scratch resource limit"),
        }
        assert_eq!(memory.used(), initial);
    }

    #[test]
    fn dc_fixed_stamp_facts_cross_threads_but_actual_authority_cannot() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<FixedFileStamp>();
        send_sync::<FixedControlScratch>();
        trait AmbiguousIfSend<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
        let _ = <FixedLockedFile as AmbiguousIfSend<_>>::check;
        trait AmbiguousIfSync<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfSync<()> for T {}
        impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
        let _ = <FixedLockedFile as AmbiguousIfSync<_>>::check;
    }

    #[test]
    fn dc_fixed_stamp_path_free_lock_release_and_identity_composition() {
        let f = Fixture::new();
        let path = f.root.join("control");
        fs::write(&path, b"").unwrap();
        let mut scratch = f.scratch();
        let file = open_fixed_control(&path, FixedControlKind::File, true, &mut scratch).unwrap();
        let owned = FixedLockedFile::try_lock(file, LockMode::Exclusive)
            .unwrap()
            .unwrap();
        assert_eq!(owned.mode(), LockMode::Exclusive);
        assert!(try_open_existing_locked(&path, LockMode::Exclusive)
            .unwrap()
            .is_none());
        drop(owned);
        let caller = try_open_existing_locked(&path, LockMode::Exclusive)
            .unwrap()
            .unwrap();
        let current =
            open_fixed_control(&path, FixedControlKind::File, false, &mut scratch).unwrap();
        let stamp = FixedFileStamp::capture(&current, FixedControlKind::File).unwrap();
        assert!(caller
            .matches_fixed_current(&stamp, LockMode::Exclusive)
            .unwrap());
        assert!(!caller
            .matches_fixed_current(&stamp, LockMode::Shared)
            .unwrap());
    }
}
