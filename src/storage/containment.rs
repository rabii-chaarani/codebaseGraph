//! Storage-level containment evidence. Never grants authority to persisted PIDs.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ContainmentProof {
    Pending,
    UnixFifo {
        path: PathBuf,
        device: u64,
        inode: u64,
    },
    WindowsJob {
        name: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BuildDescriptorV2 {
    pub version: u32,
    pub build_id: String,
    pub lifecycle_dir: PathBuf,
    pub endpoint: String,
    pub nonce: String,
    pub worker_pid: u32,
    pub guardian_pid: u32,
    pub proof: ContainmentProof,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExitOutcome {
    pub success: bool,
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub high_water_bytes: u64,
    pub budget_reason: Option<String>,
    pub cancelled: bool,
    pub containment_empty: bool,
    #[serde(default)]
    pub supervision_error: Option<String>,
}
impl fmt::Display for ExitOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(reason) = &self.budget_reason {
            return write!(f, "{reason}");
        }
        if let Some(reason) = &self.supervision_error {
            return write!(f, "{reason}");
        }
        if !self.containment_empty {
            return write!(f, "containment_unknown: cleanup could not be proven");
        }
        if self.cancelled {
            return write!(f, "build cancelled");
        }
        write!(
            f,
            "build exited (code {:?}, signal {:?})",
            self.code, self.signal
        )
    }
}

/// Missing registries are inactive. Existing unproven evidence is retained.
/// Retirement is written only while holding an existing guardian lease.
pub(crate) fn probe_registry(root: &Path) -> Result<bool, String> {
    probe_registry_mode(root, LifecycleProbe::Retire)
}

#[derive(Clone, Copy)]
enum LifecycleProbe {
    Retire,
    VerifyRetired,
}

/// Final closure check under the actual workspace startup gate. Every marker
/// must already exist; this path never creates files or reacquires that gate.
pub(crate) fn ensure_retired_registry_empty(root: &Path) -> Result<(), String> {
    match probe_registry_mode(root, LifecycleProbe::VerifyRetired)? {
        true => Ok(()),
        false => Err(format!(
            "containment_unknown: registry {} is not retired and empty",
            root.display()
        )),
    }
}

fn probe_registry_mode(root: &Path, mode: LifecycleProbe) -> Result<bool, String> {
    let anchored;
    let root = if root.is_relative() {
        anchored = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(root);
        anchored.as_path()
    } else {
        root
    };
    let metadata = match fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(e.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(false);
    }
    if root
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".lifecycle-"))
    {
        return match mode {
            LifecycleProbe::Retire => probe_lifecycle(root),
            LifecycleProbe::VerifyRetired => probe_lifecycle_mode(root, mode),
        };
    }
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        let metadata = match entry.file_type() {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        if metadata.is_symlink() {
            return Ok(false);
        }
        if metadata.is_dir() && !probe_registry_mode(&entry.path(), mode)? {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(crate) fn ensure_registry_empty(root: &Path) -> Result<(), String> {
    match probe_registry(root) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "containment_unknown: protected phase registry {} is not proven empty",
            root.display()
        )),
        Err(error) => Err(format!(
            "containment_unknown: cannot verify protected phase registry {}: {error}",
            root.display()
        )),
    }
}
pub(crate) fn probe_lifecycle(path: &Path) -> Result<bool, String> {
    probe_lifecycle_mode(path, LifecycleProbe::Retire)
}
fn probe_lifecycle_mode(path: &Path, mode: LifecycleProbe) -> Result<bool, String> {
    if !real_directory(path) {
        return Ok(false);
    }
    if matches!(mode, LifecycleProbe::VerifyRetired)
        && !fs::symlink_metadata(path.join("retired"))
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Ok(false);
    }
    let lease = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.join("lease"))
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return match mode {
                LifecycleProbe::Retire => retire_prelease_startup(path),
                LifecycleProbe::VerifyRetired => Ok(false),
            }
        }
        Err(_) => return Ok(false),
    };
    if lease.try_lock_exclusive().is_err() {
        return Ok(false);
    }
    let bytes = match fs::read(path.join("ready.json")) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match fs::read(path.join("proof.json")) {
                Ok(b) => Some(b),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Ok(false),
            }
        }
        Err(_) => return Ok(false),
    };
    if let Some(bytes) = bytes {
        let d: BuildDescriptorV2 = match serde_json::from_slice(&bytes) {
            Ok(d) => d,
            Err(_) => return Ok(false),
        };
        if d.version != 2 || !same_private_directory(&d.lifecycle_dir, path) {
            return Ok(false);
        }
        let receipt = match fs::read(path.join("receipt.json")) {
            Ok(b) => serde_json::from_slice::<ExitOutcome>(&b)
                .ok()
                .is_some_and(|o| o.containment_empty),
            Err(_) => false,
        };
        if !receipt && !probe_descriptor(&d)? {
            return Ok(false);
        }
    }
    if matches!(mode, LifecycleProbe::Retire) {
        retire_lifecycle(path)?;
    }
    Ok(true)
}
/// Nonmutating proof check for maintenance holding the exact lifecycle lease.
/// Reuse the guardian's v2 receipt/FIFO/job policy; never authorize persisted PIDs.
#[cfg(test)]
pub(crate) fn lifecycle_empty_with_owned(
    path: &Path,
    lease: &super::locks::LockedFile,
) -> Result<bool, String> {
    if !lease
        .verifies(&path.join("lease"), super::locks::LockMode::Exclusive)
        .map_err(|e| e.to_string())?
        || !real_directory(path)
    {
        return Ok(false);
    }
    for name in ["ready.json", "proof.json", "receipt.json"] {
        match fs::symlink_metadata(path.join(name)) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Ok(false);
                }
                let file = fs::File::open(path.join(name)).map_err(|error| error.to_string())?;
                if super::locks::file_link_count(&file).map_err(|error| error.to_string())? != 1 {
                    return Ok(false);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    let bytes = match fs::read(path.join("ready.json")) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match fs::read(path.join("proof.json")) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(e) => return Err(e.to_string()),
    };
    let Some(bytes) = bytes else {
        return Ok(prelease_directory_is_empty(path, true));
    };
    let descriptor: BuildDescriptorV2 = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    if descriptor.version != 2 || !same_private_directory(&descriptor.lifecycle_dir, path) {
        return Ok(false);
    }
    let receipt = fs::read(path.join("receipt.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ExitOutcome>(&bytes).ok())
        .is_some_and(|outcome| outcome.containment_empty);
    Ok(receipt || probe_descriptor(&descriptor)?)
}

fn real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}
// Private recovery evidence can be reached through an ancestor alias (/var and
// /private/var on macOS). This comparison never changes destination-key inputs.
fn same_private_directory(expected: &Path, observed: &Path) -> bool {
    if !real_directory(expected) || !real_directory(observed) {
        return false;
    }
    match (fs::canonicalize(expected), fs::canonicalize(observed)) {
        (Ok(expected), Ok(observed)) => expected == observed,
        _ => false,
    }
}
// The only safe missing-lease repair is the empty directory created immediately
// before PreparedChild creates its lease. No launch capability can exist yet.
fn prelease_directory_is_empty(path: &Path, allow_lease: bool) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        if !allow_lease || entry.file_name() != "lease" {
            return false;
        }
    }
    true
}
fn retire_prelease_startup(path: &Path) -> Result<bool, String> {
    if !prelease_directory_is_empty(path, false) {
        return Ok(false);
    }
    let lease = match OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path.join("lease"))
    {
        Ok(lease) => lease,
        Err(_) => return Ok(false),
    };
    if lease.try_lock_exclusive().is_err() || !prelease_directory_is_empty(path, true) {
        return Ok(false);
    }
    retire_lifecycle(path)?;
    Ok(true)
}
/// Call only while holding the destination's exclusive storage writer lease.
/// Unlike probing, collection may remove complete phase workspaces.
pub(crate) fn collect_registry(root: &Path) -> Result<(), String> {
    let anchored;
    let root = if root.is_relative() {
        anchored = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(root);
        anchored.as_path()
    } else {
        root
    };
    ensure_registry_empty(root)?;
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let mut collected = 0;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("phase-"))
        {
            continue;
        }
        let metadata = entry.file_type().map_err(|e| e.to_string())?;
        if metadata.is_symlink() {
            return Err("containment_unknown: refusing symlinked phase workspace".into());
        }
        if !metadata.is_dir() {
            continue;
        }
        // Recheck this confined direct child after the all-registry gate.
        ensure_registry_empty(&entry.path())?;
        fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
        collected += 1;
        if collected >= 128 {
            break;
        }
    }
    Ok(())
}
pub(crate) fn retire_lifecycle(path: &Path) -> Result<(), String> {
    let marker = path.join("retired");
    let fresh = match crate::process_lifecycle::FreshLifecycleFile::create(&marker) {
        Ok(fresh) => fresh,
        // Existing tombstones retain their historical no-op behavior and are
        // never converted into creation evidence, including a racing creator.
        Err(_) if fs::symlink_metadata(&marker).is_ok() => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut output =
        super::maintenance_controls::WorkspaceControlFile::from_created(fresh, &marker);
    if let Some(tracked) = output.tracking() {
        let _ = super::maintenance_origin::record_created_workspace_control(tracked);
    }
    output
        .write_all(b"retired-v2\n")
        .map_err(|e| e.to_string())?;
    output.finish_created().map_err(|e| e.to_string())
}
#[cfg(unix)]
pub(crate) fn probe_descriptor(d: &BuildDescriptorV2) -> Result<bool, String> {
    use std::io::Read;
    use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
    let ContainmentProof::UnixFifo {
        path,
        device,
        inode,
    } = &d.proof
    else {
        return Ok(false);
    };
    let mut fifo = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(f) => f,
        Err(_) => return Ok(false),
    };
    let m = fifo.metadata().map_err(|e| e.to_string())?;
    if !m.file_type().is_fifo() || m.dev() != *device || m.ino() != *inode {
        return Ok(false);
    }
    let mut byte = [0];
    match fifo.read(&mut byte) {
        Ok(0) => Ok(true),
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
#[cfg(any(test, windows))]
pub(crate) fn windows_job_name(d: &BuildDescriptorV2) -> Result<String, String> {
    if d.build_id.len() != 64 || !d.build_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("containment_unknown: invalid Windows job identity".into());
    }
    // Kernel object names are diagnostic identifiers, never control capabilities.
    // build_id is independently random; nonce remains private to guardian control.
    Ok(format!("Global\\codebase-graph-v2-{}", d.build_id))
}
#[cfg(windows)]
pub(crate) fn probe_descriptor(d: &BuildDescriptorV2) -> Result<bool, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND},
        System::{
            JobObjects::{
                JobObjectBasicAccountingInformation, OpenJobObjectW, QueryInformationJobObject,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            },
            SystemServices::JOB_OBJECT_QUERY,
        },
    };
    let expected = windows_job_name(d)?;
    if !matches!(&d.proof,ContainmentProof::WindowsJob{name}if name==&expected) {
        return Ok(false);
    }
    let name: Vec<u16> = std::ffi::OsStr::new(&expected)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let handle = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, name.as_ptr()) };
    if handle.is_null() {
        return Ok(unsafe { GetLastError() } == ERROR_FILE_NOT_FOUND);
    }
    let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
    let result = unsafe {
        QueryInformationJobObject(
            handle,
            JobObjectBasicAccountingInformation,
            (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
            std::ptr::null_mut(),
        )
    };
    unsafe { CloseHandle(handle) };
    Ok(result != 0 && info.ActiveProcesses == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor() -> BuildDescriptorV2 {
        BuildDescriptorV2 {
            version: 2,
            build_id: "a".repeat(64),
            lifecycle_dir: PathBuf::new(),
            endpoint: String::new(),
            nonce: "b".repeat(64),
            worker_pid: 0,
            guardian_pid: 0,
            proof: ContainmentProof::Pending,
        }
    }
    #[test]
    fn windows_job_name_does_not_expose_or_depend_on_control_nonce() {
        let mut d = descriptor();
        let original = windows_job_name(&d).unwrap();
        assert_eq!(
            original,
            format!("Global\\codebase-graph-v2-{}", d.build_id)
        );
        assert!(!original.contains(&d.nonce));
        d.nonce = "control-capability-must-stay-private".into();
        assert_eq!(windows_job_name(&d).unwrap(), original);
        assert!(!original.contains(&d.nonce));
        d.build_id = "c".repeat(64);
        assert_ne!(windows_job_name(&d).unwrap(), original);
    }
    #[test]
    fn windows_job_name_rejects_malformed_build_identity() {
        for invalid in [
            String::new(),
            "a".repeat(63),
            "a".repeat(65),
            "g".repeat(64),
            format!("{}\\", "a".repeat(63)),
        ] {
            let mut d = descriptor();
            d.build_id = invalid;
            assert!(windows_job_name(&d).is_err());
        }
    }
}
