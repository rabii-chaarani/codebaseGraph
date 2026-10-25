use crate::error::NativeError;
use fs2::{lock_contended_error, FileExt};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

/// Exact OS error compatibility for the declared Rust1.82 minimum.
pub(crate) fn is_directory_not_empty(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ENOTEMPTY)
    }
    #[cfg(windows)]
    {
        error.raw_os_error() == Some(145)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = error;
        false
    }
}
pub(crate) fn directory_not_empty_error() -> std::io::Error {
    #[cfg(unix)]
    {
        std::io::Error::from_raw_os_error(libc::ENOTEMPTY)
    }
    #[cfg(windows)]
    {
        std::io::Error::from_raw_os_error(145)
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::io::Error::new(std::io::ErrorKind::Other, "directory not empty")
    }
}

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

// Fixed creation-generation header for persisted root cohort bindings. Native
// (device,inode) pairs can be recycled once all descriptors to a removed empty
// lock close; those numeric pairs alone do not identify a file's creation.
pub(crate) const COORDINATION_HEADER_BYTES: usize = 72;
const COORDINATION_HEADER_MAGIC: &[u8; 8] = b"CGCOOR02";
pub(crate) fn encode_coordination_incarnation(
    incarnation: [u8; 32],
) -> [u8; COORDINATION_HEADER_BYTES] {
    use sha2::{Digest, Sha256};
    let mut bytes = [0; COORDINATION_HEADER_BYTES];
    bytes[..8].copy_from_slice(COORDINATION_HEADER_MAGIC);
    bytes[8..40].copy_from_slice(&incarnation);
    let checksum = Sha256::digest(&bytes[..40]);
    bytes[40..].copy_from_slice(&checksum);
    bytes
}
fn decode_coordination_incarnation(bytes: &[u8]) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    if bytes.len() != COORDINATION_HEADER_BYTES
        || &bytes[..8] != COORDINATION_HEADER_MAGIC
        || Sha256::digest(&bytes[..40]).as_slice() != &bytes[40..]
    {
        return None;
    }
    bytes[8..40].try_into().ok()
}
impl LockedFile {
    /// Called only by the genuine fresh-root creator, or a future explicit
    /// controlled-migration operation, while owning actual EXCLUSIVE exclusion.
    /// Mere file emptiness, ordinary use and observational checks never call it.
    pub(crate) fn initialize_creation_incarnation(
        &self,
        incarnation: [u8; 32],
    ) -> Result<(), NativeError> {
        use std::io::{Seek, SeekFrom, Write};
        if self.mode != LockMode::Exclusive || !self.is_current()? {
            return Err(NativeError::InvalidInput(
                "coordination_creation_not_exclusive".into(),
            ));
        }
        let mut file = self
            .file
            .as_ref()
            .ok_or_else(|| NativeError::InvalidInput("released lock".into()))?;
        if file.metadata()?.len() != 0 {
            return Err(NativeError::InvalidInput(
                "coordination_creation_not_empty".into(),
            ));
        }
        let bytes = encode_coordination_incarnation(incarnation);
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        if !self.is_current()? {
            return Err(NativeError::InvalidInput(
                "coordination_creation_changed".into(),
            ));
        }
        Ok(())
    }

    /// Read actual held bytes without creating, repairing or using a shared
    /// cursor. Empty/recreated/torn/unknown headers are unverified, not modern.
    pub(crate) fn coordination_incarnation(&self) -> Result<Option<[u8; 32]>, NativeError> {
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| NativeError::InvalidInput("released lock".into()))?;
        read_coordination_incarnation(file)
    }

    /// Windows LockFileEx layers locks rather than atomically converting them.
    /// Fresh creation is durable before this gap and no producer effect has
    /// started; reacquire Shared and revalidate the current stable inode.
    pub(crate) fn share_after_creation(mut self) -> Result<Self, NativeError> {
        if self.mode != LockMode::Exclusive || !self.companions.is_empty() {
            return Err(NativeError::InvalidInput(
                "coordination_creation_mode_mismatch".into(),
            ));
        }
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| NativeError::InvalidInput("released lock".into()))?;
        #[cfg(windows)]
        FileExt::unlock(file)?;
        FileExt::lock_shared(file)?;
        self.mode = LockMode::Shared;
        if !self.is_current()? {
            return Err(NativeError::InvalidInput(
                "coordination_creation_changed".into(),
            ));
        }
        Ok(self)
    }
}
fn read_coordination_header(
    file: &File,
    bytes: &mut [u8; COORDINATION_HEADER_BYTES],
) -> Result<bool, std::io::Error> {
    if file.metadata()?.len() != COORDINATION_HEADER_BYTES as u64 {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(bytes, 0)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut read = 0;
        while read < bytes.len() {
            let count = file.seek_read(&mut bytes[read..], read as u64)?;
            if count == 0 {
                return Ok(false);
            }
            read += count;
        }
    }
    #[cfg(not(any(unix, windows)))]
    return Ok(false);
    Ok(file.metadata()?.len() == COORDINATION_HEADER_BYTES as u64)
}
pub(crate) fn read_coordination_incarnation(file: &File) -> Result<Option<[u8; 32]>, NativeError> {
    if file_link_count(file)? != 1 {
        return Ok(None);
    }
    let mut bytes = [0; COORDINATION_HEADER_BYTES];
    if !read_coordination_header(file, &mut bytes)? {
        return Ok(None);
    }
    Ok(decode_coordination_incarnation(&bytes))
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

pub(crate) fn try_open_existing_shared_coordination(
    path: &Path,
) -> Result<Option<LockedFile>, NativeError> {
    match open_locked_inner(path, LockMode::Shared, true, false, false, true) {
        Ok(lock) => Ok(Some(lock)),
        Err(NativeError::Io(error)) if is_lock_contended(&error) => Ok(None),
        Err(error) => Err(error),
    }
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

pub(crate) fn is_lock_contended(error: &std::io::Error) -> bool {
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
impl FileIdentity {
    /// Fixed persistence facts only; these words confer no ownership.
    pub(crate) fn fixed_words(self) -> (u64, u64) {
        (self.volume, self.index)
    }
    pub(crate) fn from_fixed_words(volume: u64, index: u64) -> Self {
        Self { volume, index }
    }
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
impl std::fmt::Display for FixedControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => std::fmt::Display::fmt(error, f),
            Self::Invalid(problem) => write!(f, "invalid cleanup control: {problem:?}"),
            Self::Resource(limit) => write!(f, "cleanup resource limit: {limit:?}"),
            Self::Grant(limit) => write!(f, "cleanup mutation allowance: {limit:?}"),
        }
    }
}
impl std::error::Error for FixedControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
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
    path: super::maintenance_budget::MemoryBox<FixedControlPath>,
}
#[cfg(unix)]
type FixedControlPath = [u8; libc::PATH_MAX as usize];
#[cfg(windows)]
type FixedControlPath = [u16; 32768];
impl FixedControlScratch {
    pub(crate) fn required_bytes() -> usize {
        std::mem::size_of::<Self>() + std::mem::size_of::<FixedControlPath>() + 4096
    }
    pub(crate) fn new(
        reservation: super::maintenance_budget::MemoryReservation,
    ) -> Result<Self, FixedControlError> {
        if reservation.remaining() < Self::required_bytes() {
            return Err(FixedControlError::Resource(
                super::maintenance_budget::ResourceLimit::CeilingExceeded,
            ));
        }
        let mut path =
            super::maintenance_budget::MemoryBox::<FixedControlPath>::new_uninit_reserved(
                reservation,
            )
            .map_err(FixedControlError::Resource)?;
        // SAFETY: the exact aligned allocation is uniquely owned, and every
        // element of either native character array accepts the all-zero value.
        // Initialize in place so Windows never copies a 64 KiB temporary.
        unsafe { path.as_mut_ptr().write_bytes(0, 1) };
        Ok(Self {
            // SAFETY: every element was initialized above; ownership transfers once.
            path: unsafe { path.assume_init() },
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
        let flags = (if write && kind == FixedControlKind::File {
            libc::O_RDWR
        } else {
            libc::O_RDONLY
        }) | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK;
        let fd = unsafe { libc::open(pointer, flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful open returned a new uniquely owned descriptor;
        // File now closes it on every success/error/unwind path.
        let file = unsafe { File::from_raw_fd(fd) };
        let mut opened: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut opened) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut after: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::lstat(pointer, &mut after) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if kind == FixedControlKind::File && (opened.st_nlink != 1 || after.st_nlink != 1) {
            return Err(FixedControlError::Invalid(FixedControlProblem::Aliased));
        }
        if opened.st_dev != before.st_dev
            || opened.st_ino != before.st_ino
            || opened.st_dev != after.st_dev
            || opened.st_ino != after.st_ino
            || opened.st_mode & libc::S_IFMT != expected_type
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
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_LIST_DIRECTORY, FILE_WRITE_ATTRIBUTES,
                SYNCHRONIZE,
            };
            FILE_READ_ATTRIBUTES
                | FILE_LIST_DIRECTORY
                | SYNCHRONIZE
                | if write {
                    FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY | FILE_WRITE_ATTRIBUTES
                } else {
                    0
                }
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
            Err(error) if is_lock_contended(&error) => {
                #[cfg(test)]
                if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                    eprintln!(
                        "cleanup control lock contended: identity={:?} mode={}",
                        file_identity(&file),
                        if mode == LockMode::Exclusive {
                            "exclusive"
                        } else {
                            "shared"
                        }
                    );
                }
                Ok(None)
            }
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
    /// Read through the ACTUAL lock-owning handle. Windows byte-range locks
    /// would reject reading the same file via a second handle while Exclusive.
    /// Expected bytes were encoded outside authority; no hashing occurs here.
    #[cfg(test)]
    pub(crate) fn matches_fixed_header(
        &self,
        expected: &[u8; COORDINATION_HEADER_BYTES],
    ) -> Result<bool, FixedControlError> {
        let Some(file) = self.file.as_ref() else {
            return Ok(false);
        };
        let stamp = FixedFileStamp::capture(file, FixedControlKind::File)?;
        let mut bytes = [0; COORDINATION_HEADER_BYTES];
        if !read_coordination_header(file, &mut bytes)? {
            return Ok(false);
        }
        stamp.recheck(file)?;
        Ok(bytes == *expected)
    }

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

    #[test]
    fn dc_control_scratch_uses_small_worker_frame_and_retains_reserved_path_storage() {
        // Exercise the shipping worker's stack limit, including the 64 KiB
        // native Windows path buffer, without constructing it on that stack.
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                assert!(std::mem::size_of::<FixedControlScratch>() <= 64);
                let memory = CleanupMemory::new(128 * 1024).unwrap();
                let before = memory.used();
                let charge = FixedControlScratch::required_bytes();
                let mut scratch =
                    FixedControlScratch::new(memory.reserve(charge).unwrap()).unwrap();
                assert_eq!(memory.used(), before + charge);
                assert!(scratch.path.iter().all(|unit| *unit == 0));
                let last = scratch.path.len() - 1;
                scratch.path[last] = 1;
                assert_eq!(scratch.path[last], 1);
                drop(scratch);
                assert_eq!(memory.used(), before);
                assert!(FixedControlScratch::new(memory.reserve(charge - 1).unwrap()).is_err());
                assert_eq!(memory.used(), before);
            })
            .unwrap()
            .join()
            .unwrap();
    }

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
    fn dc_coordination_nonce_initializes_only_exclusive_empty_creation_and_is_immutable() {
        let f = Fixture::new();
        let path = f.root.join("nonce-lock");
        let shared = open_locked(&path, LockMode::Shared).unwrap();
        assert!(shared.initialize_creation_incarnation([1; 32]).is_err());
        assert!(fs::read(&path).unwrap().is_empty());
        drop(shared);
        let exclusive = open_locked(&path, LockMode::Exclusive).unwrap();
        exclusive.initialize_creation_incarnation([1; 32]).unwrap();
        let mut before = [0; COORDINATION_HEADER_BYTES];
        assert!(read_coordination_header(exclusive.file.as_ref().unwrap(), &mut before).unwrap());
        assert_eq!(exclusive.coordination_incarnation().unwrap(), Some([1; 32]));
        assert!(exclusive.initialize_creation_incarnation([2; 32]).is_err());
        let mut after = [0; COORDINATION_HEADER_BYTES];
        assert!(read_coordination_header(exclusive.file.as_ref().unwrap(), &mut after).unwrap());
        assert_eq!(after, before);
        let shared = exclusive.share_after_creation().unwrap();
        assert!(try_open_existing_locked(&path, LockMode::Shared)
            .unwrap()
            .is_some());
        assert_eq!(shared.coordination_incarnation().unwrap(), Some([1; 32]));
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

/// Minted only by this module's successful mkfifoat event.
#[cfg(unix)]
pub(crate) struct CreatedContainmentFifo {
    file: File,
}
#[cfg(unix)]
impl CreatedContainmentFifo {
    pub(crate) fn file(&self) -> &File {
        &self.file
    }
}

/// Minted only after the fresh FIFO hardlink and both namespace flushes.
#[cfg(unix)]
pub(crate) struct CreatedFifoPin {
    file: File,
}
#[cfg(unix)]
impl CreatedFifoPin {
    pub(crate) fn identity(&self) -> Result<FileIdentity, NativeError> {
        file_identity(&self.file)
    }
}

/// Actual directory anchor for fixed-name creation. Relative native operations
/// address this retained object even if a public ancestor/name is substituted.
/// Opening/preparing the anchor belongs outside a cleanup execution permit.
pub(crate) struct RootedDirectory {
    directory: File,
    identity: FileIdentity,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl RootedDirectory {
    /// Nonlocking pin of the same actual directory incarnation. Native-use
    /// leases retain this handle without exporting namespace mutation authority.
    pub(crate) fn pin_handle(&self) -> std::io::Result<File> {
        self.directory.try_clone()
    }
    pub(crate) fn open(path: &Path, expected: FileIdentity) -> std::io::Result<Self> {
        Self::open_root(path, expected, true)
    }
    pub(crate) fn open_readonly_root(path: &Path, expected: FileIdentity) -> std::io::Result<Self> {
        Self::open_root(path, expected, false)
    }
    fn open_root(path: &Path, expected: FileIdentity, write: bool) -> std::io::Result<Self> {
        #[cfg(not(windows))]
        let _ = write;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
                FILE_WRITE_ATTRIBUTES, SYNCHRONIZE,
            };
            options
                .access_mode(
                    FILE_LIST_DIRECTORY
                        | FILE_READ_ATTRIBUTES
                        | SYNCHRONIZE
                        | if write {
                            FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY | FILE_WRITE_ATTRIBUTES
                        } else {
                            0
                        },
                )
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let directory = options.open(path)?;
        let metadata = directory.metadata()?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
        }
        let identity = file_identity(&directory).map_err(native_io)?;
        if identity != expected {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            directory,
            identity,
            _same_thread: std::marker::PhantomData,
        })
    }
    pub(crate) fn identity(&self) -> FileIdentity {
        self.identity
    }
    pub(crate) fn create_new(&self, name: &[u8]) -> std::io::Result<File> {
        self.child(name, true)
    }
    pub(crate) fn open_existing(&self, name: &[u8]) -> std::io::Result<File> {
        self.child(name, false)
    }
    fn child(&self, name: &[u8], create: bool) -> std::io::Result<File> {
        self.relative_handle(name, create, false, false, true)
    }
    pub(crate) fn open_readonly_file(&self, name: &[u8]) -> std::io::Result<File> {
        self.relative_handle(name, false, false, false, false)
    }
    /// Query the named inode without requesting data access. Windows permits
    /// this while its exclusive creator still denies read/write/delete sharing.
    pub(crate) fn open_metadata_file(&self, name: &[u8]) -> std::io::Result<File> {
        self.relative_handle_access(name, false, false, false, false, true)
    }
    pub(crate) fn open_directory_readonly(&self, name: &[u8]) -> std::io::Result<Self> {
        let directory = self.relative_handle(name, false, true, false, false)?;
        let identity = file_identity(&directory).map_err(native_io)?;
        Ok(Self {
            directory,
            identity,
            _same_thread: std::marker::PhantomData,
        })
    }
    pub(crate) fn create_directory(&self, name: &[u8]) -> std::io::Result<Self> {
        self.child_directory(name, true)
    }
    pub(crate) fn open_directory(&self, name: &[u8]) -> std::io::Result<Self> {
        self.child_directory(name, false)
    }
    fn child_directory(&self, name: &[u8], create: bool) -> std::io::Result<Self> {
        let directory = self.relative_handle(name, create, true, false, true)?;
        let identity = file_identity(&directory).map_err(native_io)?;
        Ok(Self {
            directory,
            identity,
            _same_thread: std::marker::PhantomData,
        })
    }
    fn relative_handle(
        &self,
        name: &[u8],
        create: bool,
        directory: bool,
        delete: bool,
        write: bool,
    ) -> std::io::Result<File> {
        self.relative_handle_access(name, create, directory, delete, write, false)
    }
    fn relative_handle_access(
        &self,
        name: &[u8],
        create: bool,
        directory: bool,
        delete: bool,
        write: bool,
        metadata_only: bool,
    ) -> std::io::Result<File> {
        if name.is_empty()
            || name.len() >= 256
            || name == b"."
            || name == b".."
            || name
                .iter()
                .any(|byte| !byte.is_ascii() || matches!(*byte, b'/' | b'\\' | b':' | 0))
        {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        #[cfg(unix)]
        let file = {
            let _ = metadata_only;
            use std::os::fd::{AsRawFd, FromRawFd};
            let mut native = [0u8; 256];
            native[..name.len()].copy_from_slice(name);
            if create && directory {
                // SAFETY: retained actual parent FD and one validated basename.
                if unsafe {
                    libc::mkdirat(self.directory.as_raw_fd(), native.as_ptr().cast(), 0o700)
                } != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
            }
            let _ = delete;
            let flags = if directory {
                libc::O_RDONLY | libc::O_DIRECTORY
            } else if write {
                libc::O_RDWR
            } else {
                libc::O_RDONLY
            } | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK
                | if create && !directory {
                    libc::O_CREAT | libc::O_EXCL
                } else {
                    0
                };
            // SAFETY: actual directory FD is retained; native name is one
            // validated NUL-terminated basename with no separators/traversal.
            let fd = unsafe {
                libc::openat(
                    self.directory.as_raw_fd(),
                    native.as_ptr().cast(),
                    flags,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: successful openat returned a uniquely owned descriptor.
            unsafe { File::from_raw_fd(fd) }
        };
        #[cfg(windows)]
        let file = {
            use std::os::windows::io::{AsRawHandle, FromRawHandle};
            use windows_sys::{
                Wdk::{
                    Foundation::OBJECT_ATTRIBUTES,
                    Storage::FileSystem::{
                        NtCreateFile, FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE,
                        FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
                        FILE_WRITE_THROUGH,
                    },
                },
                Win32::{
                    Foundation::{
                        RtlNtStatusToDosError, GENERIC_READ, GENERIC_WRITE, OBJ_CASE_INSENSITIVE,
                        UNICODE_STRING,
                    },
                    Storage::FileSystem::{
                        DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_NORMAL,
                        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_EA, FILE_SHARE_DELETE,
                        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_EA,
                        SYNCHRONIZE,
                    },
                    System::IO::IO_STATUS_BLOCK,
                },
            };
            let mut native = [0u16; 256];
            for (out, byte) in native.iter_mut().zip(name) {
                *out = u16::from(*byte);
            }
            let mut unicode = UNICODE_STRING {
                Length: (name.len() * 2) as u16,
                MaximumLength: (name.len() * 2) as u16,
                Buffer: native.as_mut_ptr(),
            };
            let attributes = OBJECT_ATTRIBUTES {
                Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
                RootDirectory: self.directory.as_raw_handle(),
                ObjectName: &mut unicode,
                Attributes: OBJ_CASE_INSENSITIVE,
                SecurityDescriptor: std::ptr::null_mut(),
                SecurityQualityOfService: std::ptr::null_mut(),
            };
            let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            let mut handle = std::ptr::null_mut();
            // SAFETY: all pointers refer to fixed initialized stack records;
            // RootDirectory is the live actual anchor, not a joined pathname.
            let result = unsafe {
                NtCreateFile(
                    &mut handle,
                    (if directory {
                        FILE_LIST_DIRECTORY
                            | FILE_READ_ATTRIBUTES
                            | FILE_READ_EA
                            | if write {
                                FILE_ADD_FILE
                                    | FILE_ADD_SUBDIRECTORY
                                    | FILE_WRITE_ATTRIBUTES
                                    | FILE_WRITE_EA
                            } else {
                                0
                            }
                    } else if metadata_only {
                        FILE_READ_ATTRIBUTES
                    } else {
                        GENERIC_READ | if write { GENERIC_WRITE } else { 0 }
                    }) | SYNCHRONIZE
                        | if delete { DELETE } else { 0 },
                    &attributes,
                    &mut status,
                    std::ptr::null(),
                    FILE_ATTRIBUTE_NORMAL,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    if create { FILE_CREATE } else { FILE_OPEN },
                    (if directory {
                        FILE_DIRECTORY_FILE
                    } else {
                        FILE_NON_DIRECTORY_FILE
                    }) | FILE_OPEN_REPARSE_POINT
                        | FILE_SYNCHRONOUS_IO_NONALERT
                        | FILE_WRITE_THROUGH,
                    std::ptr::null(),
                    0,
                )
            };
            if result < 0 {
                return Err(std::io::Error::from_raw_os_error(unsafe {
                    RtlNtStatusToDosError(result)
                } as i32));
            }
            // SAFETY: successful NtCreateFile returned one owned file handle.
            unsafe { File::from_raw_handle(handle) }
        };
        #[cfg(not(any(unix, windows)))]
        return Err(std::io::ErrorKind::Unsupported.into());
        let metadata = file.metadata()?;
        if (directory && !metadata.is_dir())
            || (!directory
                && (!metadata.is_file() || file_link_count(&file).map_err(native_io)? != 1))
        {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if file.metadata()?.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
        }
        Ok(file)
    }
    pub(crate) fn sync_namespace(&self) -> std::io::Result<()> {
        #[cfg(not(windows))]
        {
            self.directory.sync_all()
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle};
            use windows_sys::{
                Wdk::Storage::FileSystem::NtFlushBuffersFileEx,
                Win32::{
                    Foundation::{RtlNtStatusToDosError, GENERIC_WRITE, INVALID_HANDLE_VALUE},
                    Storage::FileSystem::{
                        FileIdType, OpenFileById, FILE_FLAG_BACKUP_SEMANTICS,
                        FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0,
                        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
                    },
                    System::IO::IO_STATUS_BLOCK,
                },
            };
            // Flush requires write access even when the directory was pinned
            // for observation. Reopen the exact object, never its pathname.
            let id = FILE_ID_DESCRIPTOR {
                dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
                Type: FileIdType,
                Anonymous: FILE_ID_DESCRIPTOR_0 {
                    FileId: self.identity.index as i64,
                },
            };
            // SAFETY: the live pin prevents ID reuse; its own volume supplies
            // the hint. The descriptor is fixed-size and fully initialized.
            let handle = unsafe {
                OpenFileById(
                    self.directory.as_raw_handle(),
                    &id,
                    GENERIC_WRITE | FILE_READ_ATTRIBUTES,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    std::ptr::null(),
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                let error = std::io::Error::last_os_error();
                #[cfg(test)]
                eprintln!("namespace reopen failed: {error:?}");
                return Err(error);
            }
            // SAFETY: successful OpenFileById returned one owned handle.
            let writable = unsafe { File::from_raw_handle(handle) };
            if file_identity(&writable).map_err(native_io)? != self.identity {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
            let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            // SAFETY: live actual directory handle and initialized output. A
            // denied/unsupported filesystem flush fails closed for attestation.
            let result = unsafe {
                NtFlushBuffersFileEx(
                    writable.as_raw_handle(),
                    0,
                    std::ptr::null(),
                    0,
                    &mut status,
                )
            };
            if result < 0 {
                #[cfg(test)]
                eprintln!("namespace flush failed: {result:#x}");
                Err(std::io::Error::from_raw_os_error(
                    unsafe { RtlNtStatusToDosError(result) } as i32,
                ))
            } else {
                Ok(())
            }
        }
    }
}
fn native_io(error: NativeError) -> std::io::Error {
    match error {
        NativeError::Io(error) => error,
        _ => std::io::ErrorKind::InvalidInput.into(),
    }
}

fn checked_relative_name(name: &[u8]) -> std::io::Result<[u8; 256]> {
    if name.is_empty()
        || name.len() >= 256
        || name == b"."
        || name == b".."
        || name
            .iter()
            .any(|byte| !byte.is_ascii() || matches!(*byte, b'/' | b'\\' | b':' | 0))
    {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut native = [0; 256];
    native[..name.len()].copy_from_slice(name);
    Ok(native)
}
#[cfg(unix)]
fn checked_fifo_name(name: &[u8]) -> std::io::Result<[u8; 256]> {
    if name != b"writers.fifo"
        && !(name.len() == 69
            && &name[..5] == b"fifo-"
            && name[5..]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)))
    {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    checked_relative_name(name)
}
impl RootedDirectory {
    /// Low-level entry operation only. Caller retains actual namespace ownership
    /// and the exact source's creation/origin proof; this helper grants neither.
    pub(crate) fn rename_file_no_replace(
        &self,
        name: &[u8],
        destination: &Self,
        target: &[u8],
        expected: FileIdentity,
    ) -> std::io::Result<()> {
        self.rename_no_replace(name, destination, target, expected, false)
    }
    pub(crate) fn rename_directory_no_replace(
        &self,
        name: &[u8],
        destination: &Self,
        target: &[u8],
        expected: FileIdentity,
    ) -> std::io::Result<()> {
        self.rename_no_replace(name, destination, target, expected, true)
    }
    fn rename_no_replace(
        &self,
        name: &[u8],
        destination: &Self,
        target: &[u8],
        expected: FileIdentity,
        directory: bool,
    ) -> std::io::Result<()> {
        let source_name = checked_relative_name(name)?;
        let target_name = checked_relative_name(target)?;
        let source = self.relative_handle(name, false, directory, true, false)?;
        if file_identity(&source).map_err(native_io)? != expected {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            // SAFETY: both retained directory FDs and fixed NUL basenames live
            // through the native atomic exclusive-destination rename.
            let result = unsafe {
                libc::renameatx_np(
                    self.directory.as_raw_fd(),
                    source_name.as_ptr().cast(),
                    destination.directory.as_raw_fd(),
                    target_name.as_ptr().cast(),
                    libc::RENAME_EXCL,
                )
            };
            #[cfg(target_os = "linux")]
            // SAFETY: same anchored arguments; RENAME_NOREPLACE never overwrites.
            let result = unsafe {
                libc::renameat2(
                    self.directory.as_raw_fd(),
                    source_name.as_ptr().cast(),
                    destination.directory.as_raw_fd(),
                    target_name.as_ptr().cast(),
                    libc::RENAME_NOREPLACE,
                )
            };
            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
            return Err(std::io::ErrorKind::Unsupported.into());
            if result != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::{
                Wdk::Storage::FileSystem::{
                    FileRenameInformation, NtSetInformationFile, FILE_RENAME_INFORMATION,
                },
                Win32::{
                    Foundation::{RtlNtStatusToDosError, HANDLE},
                    System::IO::IO_STATUS_BLOCK,
                },
            };
            #[repr(C)]
            struct Rename {
                flags: u32,
                root: HANDLE,
                length: u32,
                name: [u16; 256],
            }
            const _: () = assert!(
                std::mem::offset_of!(Rename, name)
                    == std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName)
            );
            let _ = (source_name, target_name);
            let mut rename = Rename {
                flags: 0,
                root: destination.directory.as_raw_handle(),
                length: (target.len() * 2) as u32,
                name: [0; 256],
            };
            for (out, byte) in rename.name.iter_mut().zip(target) {
                *out = u16::from(*byte);
            }
            let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            let len = std::mem::offset_of!(Rename, name) + target.len() * 2;
            // SAFETY: actual source handle carries DELETE; target is relative
            // to the retained destination HANDLE. Zero flags means no replace.
            let result = unsafe {
                NtSetInformationFile(
                    source.as_raw_handle(),
                    &mut status,
                    (&rename as *const Rename).cast(),
                    len as u32,
                    FileRenameInformation,
                )
            };
            if result < 0 {
                return Err(std::io::Error::from_raw_os_error(unsafe {
                    RtlNtStatusToDosError(result)
                } as i32));
            }
        }
        #[cfg(not(any(unix, windows)))]
        return Err(std::io::ErrorKind::Unsupported.into());
        let moved = destination.relative_handle(target, false, directory, false, false)?;
        if file_identity(&moved).map_err(native_io)? != expected {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(())
    }
    /// The only hardlinked class admitted by cleanup: a real containment FIFO
    /// and its private creation pin. Regular payloads still require one link.
    #[cfg(unix)]
    pub(crate) fn open_pinned_fifo(
        &self,
        name: &[u8],
        expected: FileIdentity,
        links: u64,
    ) -> std::io::Result<File> {
        use std::os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{FileTypeExt, MetadataExt},
        };
        let native = checked_fifo_name(name)?;
        if !matches!(links, 1 | 2) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        // SAFETY: retained actual parent, one validated basename, nonblocking
        // read prevents waiting for a writer and NOFOLLOW rejects substitution.
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                native.as_ptr().cast(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: successful openat returned this uniquely owned descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        let metadata = file.metadata()?;
        if !metadata.file_type().is_fifo()
            || metadata.nlink() != links
            || file_identity(&file).map_err(native_io)? != expected
        {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(file)
    }
    /// Creates the FIFO itself, so no existing-file observation can mint this
    /// event. The retained reader keeps its inode alive until its pin is made.
    #[cfg(unix)]
    pub(crate) fn create_containment_fifo(&self) -> std::io::Result<CreatedContainmentFifo> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = checked_fifo_name(b"writers.fifo")?;
        // SAFETY: actual parent FD and validated NUL-terminated fixed basename.
        if unsafe { libc::mkfifoat(self.directory.as_raw_fd(), name.as_ptr().cast(), 0o600) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: retained actual parent FD and validated basename; nonblocking.
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr().cast(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: successful openat returned this uniquely owned descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        let id = file_identity(&file).map_err(native_io)?;
        self.open_pinned_fifo(b"writers.fifo", id, 1)?;
        Ok(CreatedContainmentFifo { file })
    }
    #[cfg(unix)]
    pub(crate) fn pin_created_containment_fifo(
        &self,
        created: &CreatedContainmentFifo,
        registry: &Self,
        pin_name: &[u8],
    ) -> std::io::Result<CreatedFifoPin> {
        use std::os::fd::AsRawFd;
        if pin_name == b"writers.fifo" {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let pin = checked_fifo_name(pin_name)?;
        let source = checked_fifo_name(b"writers.fifo")?;
        let id = file_identity(created.file()).map_err(native_io)?;
        self.open_pinned_fifo(b"writers.fifo", id, 1)?;
        // SAFETY: retained source/registry FDs, validated one-component names;
        // zero flags link only the existing entry and never replace a pin.
        if unsafe {
            libc::linkat(
                self.directory.as_raw_fd(),
                source.as_ptr().cast(),
                registry.directory.as_raw_fd(),
                pin.as_ptr().cast(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        self.open_pinned_fifo(b"writers.fifo", id, 2)?;
        let file = registry.open_pinned_fifo(pin_name, id, 2)?;
        self.sync_namespace()?;
        registry.sync_namespace()?;
        Ok(CreatedFifoPin { file })
    }
    /// Caller must retain actual terminal kernel/root-closed authority. This
    /// performs only an exact special-class unlink; it confers no authority.
    #[cfg(unix)]
    pub(crate) fn unlink_pinned_fifo(
        &self,
        name: &[u8],
        expected: FileIdentity,
        links: u64,
    ) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;
        let native = checked_fifo_name(name)?;
        let _pin = self.open_pinned_fifo(name, expected, links)?;
        // SAFETY: actual retained parent FD and validated special FIFO basename.
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), native.as_ptr().cast(), 0) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    pub(crate) fn unlink_file(&self, name: &[u8], expected: FileIdentity) -> std::io::Result<()> {
        self.remove_entry(name, expected, false)
    }
    /// Atomic empty-only removal. Unknown siblings always produce NotEmpty;
    /// no recursive walk, control recreation or forced-delete fallback exists.
    pub(crate) fn remove_empty_directory(
        &self,
        name: &[u8],
        expected: FileIdentity,
    ) -> std::io::Result<()> {
        self.remove_entry(name, expected, true)
    }
    fn remove_entry(
        &self,
        name: &[u8],
        expected: FileIdentity,
        directory: bool,
    ) -> std::io::Result<()> {
        let native = checked_relative_name(name)?;
        let file = self.relative_handle(name, false, directory, true, false)?;
        if file_identity(&file).map_err(native_io)? != expected {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: retained actual parent FD + validated basename; the kernel
            // enforces emptiness for directory removal at this instant.
            let result = unsafe {
                libc::unlinkat(
                    self.directory.as_raw_fd(),
                    native.as_ptr().cast(),
                    if directory { libc::AT_REMOVEDIR } else { 0 },
                )
            };
            if result != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::{
                Wdk::Storage::FileSystem::{
                    FileDispositionInformation, NtSetInformationFile, FILE_DISPOSITION_INFORMATION,
                },
                Win32::{Foundation::RtlNtStatusToDosError, System::IO::IO_STATUS_BLOCK},
            };
            let _ = native;
            let disposition = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
            let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            // SAFETY: DELETE access was acquired on this exact native object;
            // classic disposition retains native sharing/empty-directory rules.
            let result = unsafe {
                NtSetInformationFile(
                    file.as_raw_handle(),
                    &mut status,
                    (&disposition as *const FILE_DISPOSITION_INFORMATION).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFORMATION>() as u32,
                    FileDispositionInformation,
                )
            };
            if result < 0 {
                return Err(std::io::Error::from_raw_os_error(unsafe {
                    RtlNtStatusToDosError(result)
                } as i32));
            }
        }
        #[cfg(not(any(unix, windows)))]
        return Err(std::io::ErrorKind::Unsupported.into());
        drop(file);
        Ok(())
    }
}

#[cfg(test)]
mod rooted_directory_tests {
    use super::*;

    #[test]
    fn metadata_validation_preserves_private_creator_sharing_and_identity() {
        let fixture = Fixture::new();
        let root = fixture.anchor();
        let path = fixture.root.join("private-output");
        let fresh = crate::process_lifecycle::FreshLifecycleFile::create(&path).unwrap();
        #[cfg(windows)]
        {
            let error = root.open_readonly_file(b"private-output").unwrap_err();
            assert_eq!(error.raw_os_error(), Some(32));
        }
        let metadata = root.open_metadata_file(b"private-output").unwrap();
        assert_eq!(
            file_identity(&metadata).unwrap(),
            file_identity(fresh.file()).unwrap()
        );
        assert_eq!(file_link_count(&metadata).unwrap(), 1);
        drop(metadata);
        drop(fresh);
        assert_eq!(
            root.open_readonly_file(b"private-output")
                .unwrap()
                .metadata()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn namespace_flush_accepts_an_existing_readonly_directory_anchor() {
        let fixture = Fixture::new();
        let id = path_identity(&fixture.root).unwrap();
        let root = RootedDirectory::open_readonly_root(&fixture.root, id).unwrap();
        let file = root.create_new(b"durable-entry").unwrap();
        file.sync_all().unwrap();
        root.sync_namespace().unwrap();
        assert_eq!(root.identity(), id);
    }
    struct Fixture {
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "cleanup-rooted-{}",
                crate::storage::layout::managed_generation_id()
            ));
            fs::create_dir(&root).unwrap();
            Self { root }
        }
        fn anchor(&self) -> RootedDirectory {
            RootedDirectory::open(&self.root, path_identity(&self.root).unwrap()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn dc_nonce_promotion_requires_the_exact_existing_creation_tag() {
        let fixture = Fixture::new();
        let root = fixture.anchor();
        let mut file = root.create_new(b"source-state").unwrap();
        std::io::Write::write_all(&mut file, b"unchanged source payload").unwrap();
        let original = [17; 32];
        let promoted = [29; 32];
        assert!(replace_created_file_nonce(&file, &original, &promoted).is_err());
        assert_eq!(created_file_nonce(&file).unwrap(), None);
        set_created_file_nonce(&file, &original).unwrap();
        assert!(replace_created_file_nonce(&file, &[18; 32], &promoted).is_err());
        assert_eq!(created_file_nonce(&file).unwrap(), Some(original));
        replace_created_file_nonce(&file, &original, &promoted).unwrap();
        assert_eq!(created_file_nonce(&file).unwrap(), Some(promoted));
        assert_eq!(
            fs::read(fixture.root.join("source-state")).unwrap(),
            b"unchanged source payload"
        );
        assert!(replace_created_file_nonce(&file, &original, &[31; 32]).is_err());
        assert_eq!(created_file_nonce(&file).unwrap(), Some(promoted));
    }
    #[test]
    fn dc_directory_hints_keep_verified_directories_and_reject_file_aliases() {
        let fixture = Fixture::new();
        let root = fixture.anchor();
        for name in [b"first".as_slice(), b"second"] {
            root.create_directory(name).unwrap();
        }
        for index in 0..64 {
            root.create_new(format!("payload-{index}").as_bytes())
                .unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(fixture.root.join("first"), fixture.root.join("alias")).unwrap();
        let mut scratch = [0; 4096];
        let mut actual = std::collections::BTreeSet::new();
        root.visit_directories(&mut scratch, &mut |name| {
            if root.open_directory_readonly(name).is_ok() {
                actual.insert(name.to_vec());
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            actual,
            std::collections::BTreeSet::from([b"first".to_vec(), b"second".to_vec()])
        );
        // Directory enumeration resets its actual anchor rather than consuming
        // an external stream cursor or retaining a collection between calls.
        let mut repeated = std::collections::BTreeSet::new();
        root.visit_directories(&mut scratch, &mut |name| {
            if root.open_directory_readonly(name).is_ok() {
                repeated.insert(name.to_vec());
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(actual, repeated);
    }
    #[cfg(unix)]
    #[test]
    fn dc_fifo_creation_pin_blocks_inode_aba_and_rejects_aliases() {
        use std::os::unix::fs::MetadataExt;
        let f = Fixture::new();
        let root = f.anchor();
        let registry = root.create_directory(b"registry").unwrap();
        let mut pin = [b'a'; 69];
        pin[..5].copy_from_slice(b"fifo-");
        let created = root.create_containment_fifo().unwrap();
        let id = file_identity(created.file()).unwrap();
        assert!(root.create_containment_fifo().is_err());
        root.pin_created_containment_fifo(&created, &registry, &pin)
            .unwrap();
        assert_eq!(created.file().metadata().unwrap().nlink(), 2);
        assert!(root.open_readonly_file(b"writers.fifo").is_err());
        assert!(registry.open_pinned_fifo(b"payload", id, 2).is_err());
        // A third foreign alias prevents any collector-class unlink.
        fs::hard_link(f.root.join("writers.fifo"), f.root.join("foreign")).unwrap();
        assert!(root.unlink_pinned_fifo(b"writers.fifo", id, 2).is_err());
        fs::remove_file(f.root.join("foreign")).unwrap();
        root.unlink_pinned_fifo(b"writers.fifo", id, 2).unwrap();
        drop(created);
        // Private pin survives public disappearance and keeps the old inode.
        let old = registry.open_pinned_fifo(&pin, id, 1).unwrap();
        let replacement = root.create_containment_fifo().unwrap();
        assert_ne!(file_identity(replacement.file()).unwrap(), id);
        assert!(root.unlink_pinned_fifo(b"writers.fifo", id, 1).is_err());
        assert!(root
            .open_pinned_fifo(
                b"writers.fifo",
                file_identity(replacement.file()).unwrap(),
                1
            )
            .is_ok());
        drop(old);
        registry.unlink_pinned_fifo(&pin, id, 1).unwrap();
        assert!(!f
            .root
            .join("registry")
            .join(std::str::from_utf8(&pin).unwrap())
            .exists());
    }
    #[cfg(unix)]
    #[test]
    fn dc_fifo_creation_pin_never_adopts_pin_collision_or_regular_file() {
        let f = Fixture::new();
        let root = f.anchor();
        let registry = root.create_directory(b"registry").unwrap();
        let mut pin = [b'b'; 69];
        pin[..5].copy_from_slice(b"fifo-");
        let collision = registry.create_new(&pin).unwrap();
        let collision_id = file_identity(&collision).unwrap();
        let created = root.create_containment_fifo().unwrap();
        assert!(root
            .pin_created_containment_fifo(&created, &registry, &pin)
            .is_err());
        assert_eq!(
            file_identity(&registry.open_readonly_file(&pin).unwrap()).unwrap(),
            collision_id
        );
        assert!(registry.open_pinned_fifo(&pin, collision_id, 1).is_err());
        assert!(registry.unlink_pinned_fifo(&pin, collision_id, 1).is_err());
        assert!(root
            .open_pinned_fifo(b"writers.fifo", file_identity(created.file()).unwrap(), 1)
            .is_ok());
    }
    #[test]
    fn dc_rooted_namespace_no_replace_and_exact_empty_removal_preserve_foreign_entries() {
        use std::io::Write;
        let f = Fixture::new();
        let root = f.anchor();
        let temp = root.create_directory(b"temp-owned").unwrap();
        let mut payload = temp.create_new(b"partition.json").unwrap();
        payload.write_all(b"own payload").unwrap();
        payload.sync_all().unwrap();
        let payload_id = file_identity(&payload).unwrap();
        drop(payload);
        let mut foreign = root.create_new(b"canonical").unwrap();
        foreign.write_all(b"foreign canonical").unwrap();
        drop(foreign);
        assert!(root
            .rename_directory_no_replace(b"temp-owned", &root, b"canonical", temp.identity())
            .is_err());
        assert_eq!(
            fs::read(f.root.join("canonical")).unwrap(),
            b"foreign canonical"
        );
        assert!(root
            .remove_empty_directory(b"temp-owned", temp.identity())
            .is_err());
        let sentinel = temp.create_new(b"unknown-sibling").unwrap();
        drop(sentinel);
        temp.unlink_file(b"partition.json", payload_id).unwrap();
        assert!(root
            .remove_empty_directory(b"temp-owned", temp.identity())
            .is_err());
        assert!(f.root.join("temp-owned/unknown-sibling").exists());
        let sentinel = temp.open_existing(b"unknown-sibling").unwrap();
        let sentinel_id = file_identity(&sentinel).unwrap();
        drop(sentinel);
        // Fixture teardown owns this sentinel; production never adopts unknowns.
        temp.unlink_file(b"unknown-sibling", sentinel_id).unwrap();
        drop(temp);
        let empty = root.open_directory(b"temp-owned").unwrap();
        let empty_id = empty.identity();
        drop(empty);
        root.remove_empty_directory(b"temp-owned", empty_id)
            .unwrap();
        assert!(!f.root.join("temp-owned").exists());
        // Also exercise actual directory publication, with only our exact
        // completed payload and every descendant handle closed beforehand.
        let ready = root.create_directory(b"owned-ready").unwrap();
        let mut file = ready.create_new(b"partition.json").unwrap();
        file.write_all(b"completed own leaf").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let ready_id = ready.identity();
        drop(ready);
        root.rename_directory_no_replace(b"owned-ready", &root, b"published-owned", ready_id)
            .unwrap();
        assert_eq!(
            path_identity(&f.root.join("published-owned")).unwrap(),
            ready_id
        );
        assert_eq!(
            fs::read(f.root.join("published-owned/partition.json")).unwrap(),
            b"completed own leaf"
        );
        assert!(!f.root.join("owned-ready").exists());
        assert_eq!(
            fs::read(f.root.join("canonical")).unwrap(),
            b"foreign canonical"
        );
    }
    #[test]
    fn dc_rooted_file_rename_is_no_replace_and_checks_exact_source_identity() {
        use std::io::Write;
        let f = Fixture::new();
        let root = f.anchor();
        let mut source = root.create_new(b"new").unwrap();
        source.write_all(b"new bytes").unwrap();
        let id = file_identity(&source).unwrap();
        drop(source);
        let mut target = root.create_new(b"old").unwrap();
        target.write_all(b"old bytes").unwrap();
        let other_id = file_identity(&target).unwrap();
        drop(target);
        assert!(root
            .rename_file_no_replace(b"new", &root, b"old", id)
            .is_err());
        assert_eq!(fs::read(f.root.join("old")).unwrap(), b"old bytes");
        assert!(root
            .rename_file_no_replace(b"new", &root, b"renamed", other_id)
            .is_err());
        assert!(!f.root.join("renamed").exists());
        root.rename_file_no_replace(b"new", &root, b"renamed", id)
            .unwrap();
        assert_eq!(path_identity(&f.root.join("renamed")).unwrap(), id);
        assert!(root.unlink_file(b"renamed", other_id).is_err());
        assert_eq!(fs::read(f.root.join("renamed")).unwrap(), b"new bytes");
        root.unlink_file(b"renamed", id).unwrap();
    }
    #[test]
    fn dc_rooted_component_validation_is_noncreating_and_accepts_exact_key64() {
        let f = Fixture::new();
        let root = f.anchor();
        for name in [
            b"".as_slice(),
            b".",
            b"..",
            b"../outside",
            b"a/b",
            b"a\\b",
            b"a:b",
            b"a\0b",
        ] {
            assert!(root.create_new(name).is_err());
            assert!(root.create_directory(name).is_err());
        }
        assert_eq!(fs::read_dir(&f.root).unwrap().count(), 0);
        let key = [b'a'; 64];
        drop(root.create_new(&key).unwrap());
        assert!(f.root.join("a".repeat(64)).is_file());
    }
    #[test]
    fn dc_rooted_readonly_payload_access_needs_no_payload_write_permission() {
        let f = Fixture::new();
        let child = f.root.join("canonical");
        fs::create_dir(&child).unwrap();
        let target = child.join("readonly-payload");
        fs::write(&target, b"readable payload").unwrap();
        let original = fs::metadata(&target).unwrap().permissions();
        struct Restore {
            path: PathBuf,
            permissions: fs::Permissions,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.path, self.permissions.clone());
            }
        }
        let _restore = Restore {
            path: target.clone(),
            permissions: original.clone(),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
        }
        #[cfg(windows)]
        {
            let mut permissions = original.clone();
            permissions.set_readonly(true);
            fs::set_permissions(&target, permissions).unwrap();
        }
        let root =
            RootedDirectory::open_readonly_root(&f.root, path_identity(&f.root).unwrap()).unwrap();
        let canonical = root.open_directory_readonly(b"canonical").unwrap();
        let mut file = canonical.open_readonly_file(b"readonly-payload").unwrap();
        use std::io::Read;
        let mut bytes = [0; 16];
        file.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"readable payload");
    }

    #[cfg(unix)]
    #[test]
    fn dc_rooted_operations_remain_on_actual_parent_after_public_name_substitution() {
        let f = Fixture::new();
        let public = f.root.join("public");
        fs::create_dir(&public).unwrap();
        let anchor = RootedDirectory::open(&public, path_identity(&public).unwrap()).unwrap();
        fs::rename(&public, f.root.join("old-owned")).unwrap();
        fs::create_dir(&public).unwrap();
        drop(anchor.create_new(b"created-on-original").unwrap());
        assert!(f.root.join("old-owned/created-on-original").is_file());
        assert!(!public.join("created-on-original").exists());
    }
    #[cfg(any(unix, windows))]
    #[test]
    fn dc_rooted_nonfollowing_open_rejects_symlinks_and_hardlinked_controls() {
        let f = Fixture::new();
        let root = f.anchor();
        let target = f.root.join("target");
        fs::write(&target, b"foreign target").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, f.root.join("alias")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, f.root.join("alias")).unwrap();
        assert!(root.open_existing(b"alias").is_err());
        assert!(root.create_new(b"alias").is_err());
        fs::hard_link(&target, f.root.join("hardlink")).unwrap();
        assert!(root.open_existing(b"hardlink").is_err());
        assert_eq!(fs::read(target).unwrap(), b"foreign target");
    }
}

/// Fixed-buffer native directory streaming. Names are borrowed only for the
/// callback; there is no retained union/catalog, libc DIR or child-path join.
impl RootedDirectory {
    pub(crate) fn visit_entries(
        &self,
        bytes: &mut [u8; 4096],
        visit: &mut impl FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.visit_entries_filtered(bytes, false, visit)
    }
    /// Advisory directory hints avoid opening every regular payload while
    /// looking up a workspace parent. Callers still open and verify directories.
    pub(crate) fn visit_directories(
        &self,
        bytes: &mut [u8; 4096],
        visit: &mut impl FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.visit_entries_filtered(bytes, true, visit)
    }
    fn visit_entries_filtered(
        &self,
        bytes: &mut [u8; 4096],
        directories_only: bool,
        visit: &mut impl FnMut(&[u8]) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            let mut attributes: libc::attrlist = unsafe { std::mem::zeroed() };
            attributes.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
            attributes.commonattr = libc::ATTR_CMN_NAME | libc::ATTR_CMN_RETURNED_ATTRS;
            if directories_only {
                attributes.commonattr |= libc::ATTR_CMN_OBJTYPE;
            }
            // SAFETY: actual directory FD, bounded writable caller scratch and
            // initialized attrlist. No libc DIR allocation or path expansion.
            if unsafe { libc::lseek(self.directory.as_raw_fd(), 0, libc::SEEK_SET) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            loop {
                let count = unsafe {
                    libc::getattrlistbulk(
                        self.directory.as_raw_fd(),
                        (&mut attributes as *mut libc::attrlist).cast(),
                        bytes.as_mut_ptr().cast(),
                        bytes.len(),
                        0,
                    )
                };
                if count < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if count == 0 {
                    return Ok(());
                }
                let mut offset = 0usize;
                for _ in 0..count {
                    if offset + 32 > bytes.len() {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let length =
                        u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
                    if length < 32 || offset + length > bytes.len() {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let relative =
                        i32::from_ne_bytes(bytes[offset + 24..offset + 28].try_into().unwrap());
                    let name_len =
                        u32::from_ne_bytes(bytes[offset + 28..offset + 32].try_into().unwrap())
                            as usize;
                    let start = (offset + 24)
                        .checked_add_signed(relative as isize)
                        .ok_or(std::io::ErrorKind::InvalidData)?;
                    let end = start
                        .checked_add(name_len)
                        .ok_or(std::io::ErrorKind::InvalidData)?;
                    if name_len == 0
                        || start < offset
                        || end > offset + length
                        || bytes[end - 1] != 0
                    {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let name = &bytes[start..end - 1];
                    let returned =
                        u32::from_ne_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
                    let known_non_directory = if directories_only
                        && returned & libc::ATTR_CMN_OBJTYPE != 0
                    {
                        if length < 36 {
                            return Err(std::io::ErrorKind::InvalidData.into());
                        }
                        // Darwin vnode_type_t: VNON=0, VREG=1, VDIR=2. An
                        // unavailable type falls back to the verified open.
                        let kind =
                            u32::from_ne_bytes(bytes[offset + 32..offset + 36].try_into().unwrap());
                        kind != 0 && kind != 2
                    } else {
                        false
                    };
                    if name != b"." && name != b".." && !known_non_directory {
                        visit(name)?;
                    }
                    offset += length;
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::lseek(self.directory.as_raw_fd(), 0, libc::SEEK_SET) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            loop {
                // SAFETY: Linux getdents64 ABI and fixed writable buffer.
                let count = unsafe {
                    libc::syscall(
                        libc::SYS_getdents64,
                        self.directory.as_raw_fd(),
                        bytes.as_mut_ptr(),
                        bytes.len(),
                    )
                };
                if count < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if count == 0 {
                    return Ok(());
                }
                let mut offset = 0usize;
                while offset < count as usize {
                    if offset + 20 > count as usize {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let length =
                        u16::from_ne_bytes(bytes[offset + 16..offset + 18].try_into().unwrap())
                            as usize;
                    if length < 20 || offset + length > count as usize {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let field = &bytes[offset + 19..offset + length];
                    let end = field
                        .iter()
                        .position(|x| *x == 0)
                        .ok_or(std::io::ErrorKind::InvalidData)?;
                    let name = &field[..end];
                    let kind = bytes[offset + 18];
                    if name != b"."
                        && name != b".."
                        && (!directories_only || kind == libc::DT_UNKNOWN || kind == libc::DT_DIR)
                    {
                        visit(name)?;
                    }
                    offset += length;
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = directories_only;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::{
                Wdk::Storage::FileSystem::{FileNamesInformation, NtQueryDirectoryFile},
                Win32::{Foundation::STATUS_NO_MORE_FILES, System::IO::IO_STATUS_BLOCK},
            };
            let mut first = true;
            loop {
                let mut status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
                // SAFETY: synchronous directory handle and fixed writable
                // FILE_NAMES_INFORMATION buffer; request one entry per call.
                let result = unsafe {
                    NtQueryDirectoryFile(
                        self.directory.as_raw_handle(),
                        std::ptr::null_mut(),
                        None,
                        std::ptr::null(),
                        &mut status,
                        bytes.as_mut_ptr().cast(),
                        bytes.len() as u32,
                        FileNamesInformation,
                        true,
                        std::ptr::null(),
                        first,
                    )
                };
                first = false;
                if result == STATUS_NO_MORE_FILES {
                    return Ok(());
                }
                if result < 0 {
                    return Err(std::io::Error::from_raw_os_error(unsafe {
                        windows_sys::Win32::Foundation::RtlNtStatusToDosError(result)
                    } as i32)
                    .into());
                }
                let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
                if length > bytes.len() - 12 || length % 2 != 0 {
                    return Err(std::io::ErrorKind::InvalidData.into());
                }
                // Decode into the disjoint second half of the SAME caller
                // buffer. No per-depth stack/Vec name allocation, and no lossy
                // interpretation of unpaired UTF-16 surrogates.
                if length > 2036 {
                    return Err(std::io::ErrorKind::InvalidData.into());
                }
                let (input, output) = bytes.split_at_mut(2048);
                let mut written = 0usize;
                for scalar in char::decode_utf16(
                    input[12..12 + length]
                        .chunks_exact(2)
                        .map(|unit| u16::from_le_bytes(unit.try_into().unwrap())),
                ) {
                    let scalar = scalar.map_err(|_| std::io::ErrorKind::InvalidData)?;
                    let mut encoded = [0u8; 4];
                    let text = scalar.encode_utf8(&mut encoded).as_bytes();
                    let end = written
                        .checked_add(text.len())
                        .ok_or(std::io::ErrorKind::InvalidData)?;
                    if end > output.len() {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    output[written..end].copy_from_slice(text);
                    written = end;
                }
                let name = &output[..written];
                if name != b"." && name != b".." {
                    visit(name)?;
                }
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        Err(std::io::ErrorKind::Unsupported.into())
    }
    pub(crate) fn contains_only(
        &self,
        allowed: Option<&[u8]>,
        bytes: &mut [u8; 4096],
        grant: &mut super::maintenance_budget::MutationGrant,
    ) -> Result<bool, FixedControlError> {
        let mut seen = false;
        let mut stopped = false;
        let mut exhausted = None;
        let result = self.visit_entries(bytes, &mut |name| {
            if let Err(error) = grant.take_entry() {
                exhausted = Some(error);
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if !seen && allowed == Some(name) {
                seen = true;
                Ok(())
            } else {
                stopped = true;
                Err(std::io::ErrorKind::Interrupted.into())
            }
        });
        if let Some(error) = exhausted {
            return Err(error.into());
        }
        if stopped {
            return Ok(false);
        }
        result?;
        Ok(true)
    }
}

impl FixedLockedFile {
    pub(crate) fn matches_fixed_current(
        &self,
        current: &FixedFileStamp,
        mode: LockMode,
    ) -> Result<bool, FixedControlError> {
        Ok(self.mode == mode
            && FixedFileStamp::capture(&self.file, FixedControlKind::File)?.identity()
                == current.identity())
    }
    pub(crate) fn matches_fixed_header(
        &self,
        expected: &[u8; COORDINATION_HEADER_BYTES],
    ) -> Result<bool, FixedControlError> {
        let stamp = FixedFileStamp::capture(&self.file, FixedControlKind::File)?;
        let mut bytes = [0; COORDINATION_HEADER_BYTES];
        if !read_coordination_header(&self.file, &mut bytes)? {
            return Ok(false);
        }
        stamp.recheck(&self.file)?;
        Ok(bytes == *expected)
    }
}

impl RootedDirectory {
    pub(crate) fn open_fixed(
        path: &Path,
        expected: FileIdentity,
        write: bool,
        scratch: &mut FixedControlScratch,
    ) -> Result<Self, FixedControlError> {
        let directory = open_fixed_control(path, FixedControlKind::Directory, write, scratch)?;
        let identity = FixedFileStamp::capture(&directory, FixedControlKind::Directory)?.identity();
        if identity != expected {
            return Err(FixedControlError::Invalid(FixedControlProblem::Changed));
        }
        Ok(Self {
            directory,
            identity,
            _same_thread: std::marker::PhantomData,
        })
    }
}

impl From<std::io::ErrorKind> for FixedControlError {
    fn from(kind: std::io::ErrorKind) -> Self {
        Self::Io(kind.into())
    }
}

impl FixedFileStamp {
    pub(crate) fn binding_digest(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let (a, b) = self.identity.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
        hash.update(self.len.to_le_bytes());
        hash.update(self.links.to_le_bytes());
        hash.update([match self.kind {
            FixedControlKind::File => 0,
            FixedControlKind::Directory => 1,
        }]);
        for part in self.modified.into_iter().chain(self.changed) {
            hash.update(part.to_le_bytes());
        }
        hash.finalize().into()
    }
}

impl RootedDirectory {
    pub(crate) fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        self.directory.metadata()
    }
}

/// Genuine namespace creation, used by owned workspace factories. Existing
/// coordination names are never adopted merely because their file is empty.
pub(crate) fn create_new_coordination_in(
    root: &RootedDirectory,
    name: &[u8],
    path: &Path,
) -> Result<LockedFile, NativeError> {
    if path.file_name().map(|n| n.as_encoded_bytes()) != Some(name)
        || path.parent().map(path_identity).transpose()? != Some(root.identity())
    {
        return Err(NativeError::InvalidInput(
            "fresh coordination parent mismatch".into(),
        ));
    }
    let file = root.create_new(name)?;
    FileExt::lock_exclusive(&file)?;
    let lock = LockedFile {
        path: path.to_owned(),
        file: Some(file),
        mode: LockMode::Exclusive,
        companions: Vec::new(),
    };
    if !lock.is_current()? {
        return Err(NativeError::InvalidInput(
            "fresh coordination changed".into(),
        ));
    }
    Ok(lock)
}

pub(crate) fn read_fixed_range(file: &File, offset: u64, bytes: &mut [u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(bytes, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < bytes.len() {
            let n = file.seek_read(
                &mut bytes[done..],
                offset
                    .checked_add(done as u64)
                    .ok_or(std::io::ErrorKind::InvalidInput)?,
            )?;
            if n == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            done += n;
        }
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    Err(std::io::ErrorKind::Unsupported.into())
}
pub(crate) fn write_fixed_range(
    file: &File,
    offset: u64,
    bytes: &[u8],
    grant: &super::maintenance_budget::MutationGrant,
) -> Result<(), FixedControlError> {
    let mut done = 0;
    while done < bytes.len() {
        grant.check_time()?;
        let at = offset
            .checked_add(done as u64)
            .ok_or(std::io::ErrorKind::InvalidInput)?;
        #[cfg(unix)]
        let n = {
            use std::os::unix::fs::FileExt;
            file.write_at(&bytes[done..], at)?
        };
        #[cfg(windows)]
        let n = {
            use std::os::windows::fs::FileExt;
            file.seek_write(&bytes[done..], at)?
        };
        #[cfg(not(any(unix, windows)))]
        return Err(std::io::ErrorKind::Unsupported.into());
        if n == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        done += n;
    }
    grant.check_time()?;
    file.sync_all()?;
    Ok(())
}

/// Creation-only physical file nonce. Metadata is outside primary CSV/JSON
/// bytes, survives chmod, and prevents numeric inode ABA from adopting a new
/// untagged file. Unsupported filesystems remain optional/untracked.
pub(crate) fn set_created_file_nonce(file: &File, nonce: &[u8; 32]) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c"com.codebasegraph.origin".as_ptr(),
                nonce.as_ptr().cast(),
                nonce.len(),
                0,
                libc::XATTR_CREATE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        file.sync_all()?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c"user.codebasegraph.origin".as_ptr(),
                nonce.as_ptr().cast(),
                nonce.len(),
                libc::XATTR_CREATE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        file.sync_all()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::{
            Wdk::Storage::FileSystem::NtSetEaFile,
            Win32::{Foundation::RtlNtStatusToDosError, System::IO::IO_STATUS_BLOCK},
        };
        const NAME: &[u8] = b"CODEBASEGRAPH.ORIGIN";
        let mut bytes = [0u8; 64];
        bytes[5] = NAME.len() as u8;
        bytes[6..8].copy_from_slice(&32u16.to_le_bytes());
        bytes[8..8 + NAME.len()].copy_from_slice(NAME);
        bytes[9 + NAME.len()..41 + NAME.len()].copy_from_slice(nonce);
        let mut io: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        let status = unsafe {
            NtSetEaFile(
                file.as_raw_handle(),
                &mut io,
                bytes.as_ptr().cast(),
                (41 + NAME.len()) as u32,
            )
        };
        if status < 0 {
            return Err(std::io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        file.sync_all()?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    Err(std::io::ErrorKind::Unsupported.into())
}
/// Replace only an existing, exactly matched creation nonce while the caller
/// retains its actual source/owner exclusion. This never creates a missing tag.
pub(crate) fn replace_created_file_nonce(
    file: &File,
    expected: &[u8; 32],
    next: &[u8; 32],
) -> std::io::Result<()> {
    if created_file_nonce(file)? != Some(*expected) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c"com.codebasegraph.origin".as_ptr(),
                next.as_ptr().cast(),
                next.len(),
                0,
                libc::XATTR_REPLACE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let result = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c"user.codebasegraph.origin".as_ptr(),
                next.as_ptr().cast(),
                next.len(),
                libc::XATTR_REPLACE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    #[cfg(windows)]
    set_created_file_nonce(file, next)?;
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    return Err(std::io::ErrorKind::Unsupported.into());
    if created_file_nonce(file)? != Some(*next) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    file.sync_all()
}
pub(crate) fn created_file_nonce(file: &File) -> std::io::Result<Option<[u8; 32]>> {
    let mut nonce = [0u8; 32];
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let count = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                c"com.codebasegraph.origin".as_ptr(),
                nonce.as_mut_ptr().cast(),
                nonce.len(),
                0,
                0,
            )
        };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(libc::ENOATTR | libc::ENOTSUP)) {
                return Ok(None);
            }
            return Err(error);
        }
        Ok((count == 32).then_some(nonce))
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let count = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                c"user.codebasegraph.origin".as_ptr(),
                nonce.as_mut_ptr().cast(),
                nonce.len(),
            )
        };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(libc::ENODATA | libc::ENOTSUP)) {
                return Ok(None);
            }
            return Err(error);
        }
        Ok((count == 32).then_some(nonce))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::{
            Wdk::Storage::FileSystem::NtQueryEaFile,
            Win32::{
                Foundation::{
                    RtlNtStatusToDosError, STATUS_EAS_NOT_SUPPORTED, STATUS_NONEXISTENT_EA_ENTRY,
                    STATUS_NO_EAS_ON_FILE,
                },
                System::IO::IO_STATUS_BLOCK,
            },
        };
        const NAME: &[u8] = b"CODEBASEGRAPH.ORIGIN";
        let mut query = [0u8; 32];
        query[4] = NAME.len() as u8;
        query[5..5 + NAME.len()].copy_from_slice(NAME);
        let mut bytes = [0u8; 128];
        let mut io: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        let status = unsafe {
            NtQueryEaFile(
                file.as_raw_handle(),
                &mut io,
                bytes.as_mut_ptr().cast(),
                bytes.len() as u32,
                true,
                query.as_ptr().cast(),
                (6 + NAME.len()) as u32,
                std::ptr::null(),
                true,
            )
        };
        if [
            STATUS_NO_EAS_ON_FILE,
            STATUS_NONEXISTENT_EA_ENTRY,
            STATUS_EAS_NOT_SUPPORTED,
        ]
        .contains(&status)
        {
            return Ok(None);
        }
        if status < 0 {
            return Err(std::io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        let name_len = bytes[5] as usize;
        let value_len = u16::from_le_bytes(bytes[6..8].try_into().unwrap()) as usize;
        if io.Information < 8 + name_len + 1 + value_len
            || name_len != NAME.len()
            || value_len != 32
            || bytes[8..8 + name_len] != *NAME
        {
            return Ok(None);
        }
        nonce.copy_from_slice(&bytes[9 + name_len..41 + name_len]);
        Ok(Some(nonce))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    Ok(None)
}

impl FixedFileStamp {
    pub(crate) fn encode_checkpoint(&self, out: &mut [u8; 80]) {
        out.fill(0);
        out[..8].copy_from_slice(&1u64.to_le_bytes());
        let (a, b) = self.identity.fixed_words();
        out[8..16].copy_from_slice(&a.to_le_bytes());
        out[16..24].copy_from_slice(&b.to_le_bytes());
        out[24] = match self.kind {
            FixedControlKind::File => 0,
            FixedControlKind::Directory => 1,
        };
        out[32..40].copy_from_slice(&self.len.to_le_bytes());
        out[40..48].copy_from_slice(&self.links.to_le_bytes());
        for (index, value) in self.modified.into_iter().chain(self.changed).enumerate() {
            out[48 + index * 8..56 + index * 8].copy_from_slice(&value.to_le_bytes());
        }
    }
    pub(crate) fn decode_checkpoint(input: &[u8; 80]) -> Result<Self, FixedControlError> {
        if u64::from_le_bytes(input[..8].try_into().unwrap()) != 1
            || input[25..32].iter().any(|b| *b != 0)
        {
            return Err(FixedControlProblem::InvalidRecord.into());
        }
        let kind = match input[24] {
            0 => FixedControlKind::File,
            1 => FixedControlKind::Directory,
            _ => return Err(FixedControlProblem::InvalidRecord.into()),
        };
        let links = u64::from_le_bytes(input[40..48].try_into().unwrap());
        if kind == FixedControlKind::File && links != 1 {
            return Err(FixedControlProblem::Aliased.into());
        }
        let time = |offset| i64::from_le_bytes(input[offset..offset + 8].try_into().unwrap());
        Ok(Self {
            identity: FileIdentity::from_fixed_words(
                u64::from_le_bytes(input[8..16].try_into().unwrap()),
                u64::from_le_bytes(input[16..24].try_into().unwrap()),
            ),
            kind,
            len: u64::from_le_bytes(input[32..40].try_into().unwrap()),
            links,
            modified: [time(48), time(56)],
            changed: [time(64), time(72)],
        })
    }
}
impl From<FixedControlProblem> for FixedControlError {
    fn from(problem: FixedControlProblem) -> Self {
        Self::Invalid(problem)
    }
}
