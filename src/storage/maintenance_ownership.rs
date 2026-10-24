//! Ownership capabilities for maintenance. Inventory observations are never
//! capabilities; every capability below retains the actual OS locks.
use super::atomic::write_json_atomically;
#[cfg(test)]
use super::containment::{probe_descriptor, BuildDescriptorV2};
use super::inventory::real_path;
#[cfg(test)]
use super::inventory::{bound_roles_overlap, default_direct, observe_binding, ObservedBinding};
#[cfg(test)]
use super::inventory::{same_physical_path, DirectPair, MaintenanceTarget};
use super::layout::{DirectLayout, ManagedLayout};
use super::locks::{
    open_locked, open_shared_coordination, path_identity, try_open_existing_locked,
    try_open_locked_in_existing_parent, FileIdentity, LockMode, LockedFile, WriterLease,
};
use crate::error::NativeError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// This record is invalidation evidence, never deletion authority. Every read
// and write has a fixed byte bound; no JSON manifest/configuration is read here.
const REFERENCE_EPOCH_SIZE: usize = 2048;
const REFERENCE_EPOCH_MAGIC: &[u8; 8] = b"CGRFE001";
const REFERENCE_EPOCH_FILE: &str = ".reference-epoch";
const REFERENCE_ACTIVITY_FILE: &str = ".reference-activity.lock";
const REFERENCE_SERIAL_FILE: &str = ".reference-epoch.lock";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReferenceScopeRecord {
    kind: u8,
    destination: FileIdentity,
    writer: FileIdentity,
    address: [u8; 32],
    reference_roots: [Option<FileIdentity>; 2],
}

/// Observational scope, not authority or a coverage-creation permit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReferenceDomain {
    artifact_root: PathBuf,
    artifact_identity: Option<FileIdentity>,
    use_identity: Option<FileIdentity>,
    destination_root: PathBuf,
    writer_path: PathBuf,
    raw_address: Option<[PathBuf; 2]>,
    reference_roots: [Option<PathBuf>; 2],
    scope: ReferenceScopeRecord,
}
impl ReferenceDomain {
    pub(crate) fn managed(layout: &ManagedLayout) -> Result<Self, NativeError> {
        Self::observe(
            layout.artifacts_root(),
            layout.storage_root().to_owned(),
            layout.writer_lock_path(),
            [Some(layout.generations_root()), Some(layout.runs_root())],
            1,
        )
    }
    pub(crate) fn direct(layout: &DirectLayout) -> Result<Self, NativeError> {
        let mut domain = Self::observe(
            layout.artifact_root_path(),
            layout
                .db_path()
                .parent()
                .ok_or_else(|| invalid("reference_destination_parent_missing"))?
                .to_owned(),
            layout.writer_lock_path(),
            [None, None],
            2,
        )?;
        let addresses = [
            layout.db_path().to_owned(),
            layout.manifest_path().to_owned(),
        ];
        let mut hash = Sha256::new();
        hash.update(addresses[0].as_os_str().as_encoded_bytes());
        hash.update([0]);
        hash.update(addresses[1].as_os_str().as_encoded_bytes());
        domain.scope.address = hash.finalize().into();
        domain.raw_address = Some(addresses);
        Ok(domain)
    }
    fn observe(
        artifact_root: PathBuf,
        destination_root: PathBuf,
        writer_path: PathBuf,
        reference_roots: [Option<PathBuf>; 2],
        kind: u8,
    ) -> Result<Self, NativeError> {
        let artifact_root = absolute(&artifact_root)?;
        let destination_root = absolute(&destination_root)?;
        let writer_path = absolute(&writer_path)?;
        let reference_roots = [
            reference_roots[0].as_deref().map(absolute).transpose()?,
            reference_roots[1].as_deref().map(absolute).transpose()?,
        ];
        let scope = ReferenceScopeRecord {
            kind,
            destination: directory_identity(&destination_root)?,
            writer: path_identity(&writer_path)?,
            address: Sha256::digest(writer_path.as_os_str().as_encoded_bytes()).into(),
            reference_roots: [
                reference_roots[0]
                    .as_deref()
                    .map(directory_identity)
                    .transpose()?,
                reference_roots[1]
                    .as_deref()
                    .map(directory_identity)
                    .transpose()?,
            ],
        };
        let artifact_identity = match fs::symlink_metadata(&artifact_root) {
            Ok(_) => Some(directory_identity(&artifact_root)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let use_identity = optional_resource_identity(&artifact_root.join(".use.lock"))?;
        Ok(Self {
            artifact_root,
            artifact_identity,
            use_identity,
            destination_root,
            writer_path,
            raw_address: None,
            reference_roots,
            scope,
        })
    }
    pub(crate) fn revalidate(&self) -> Result<bool, NativeError> {
        let current_root = match fs::symlink_metadata(&self.artifact_root) {
            Ok(_) => Some(directory_identity(&self.artifact_root)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let address = if let Some(paths) = &self.raw_address {
            let mut hash = Sha256::new();
            hash.update(paths[0].as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash.update(paths[1].as_os_str().as_encoded_bytes());
            <[u8; 32]>::from(hash.finalize())
        } else {
            Sha256::digest(self.writer_path.as_os_str().as_encoded_bytes()).into()
        };
        Ok(address == self.scope.address
            && current_root == self.artifact_identity
            && optional_resource_identity(&self.artifact_root.join(".use.lock"))?
                == self.use_identity
            && directory_identity(&self.destination_root)? == self.scope.destination
            && path_identity(&self.writer_path)? == self.scope.writer
            && self.reference_roots[0]
                .as_deref()
                .map(directory_identity)
                .transpose()?
                == self.scope.reference_roots[0]
            && self.reference_roots[1]
                .as_deref()
                .map(directory_identity)
                .transpose()?
                == self.scope.reference_roots[1])
    }
    pub(crate) fn allows_runs_root(&self, runs_root: &Path) -> Result<bool, NativeError> {
        Ok(self.scope.kind != 1
            || self.scope.reference_roots[1] == Some(directory_identity(runs_root)?))
    }
    pub(crate) fn same_scope(&self, other: &Self) -> bool {
        self.scope == other.scope
            && self.artifact_identity == other.artifact_identity
            && self.use_identity == other.use_identity
    }
    pub(crate) fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReferenceEpochRecord {
    version: u32,
    root: FileIdentity,
    use_lock: FileIdentity,
    use_incarnation: [u8; 32],
    cohort: FileIdentity,
    scope: ReferenceScopeRecord,
    incarnation: [u8; 32],
    activity: FileIdentity,
    serial: FileIdentity,
    counter: u64,
    covered: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReferenceEpoch {
    file: FileIdentity,
    record: ReferenceEpochRecord,
}
impl ReferenceEpoch {
    pub(crate) fn counter(&self) -> u64 {
        self.record.counter
    }
}
#[derive(Debug)]
// Fixed inline protocol/lease state: boxing would add a separately accounted allocation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum EpochObservation {
    Verified(ReferenceEpoch),
    Pending(PendingEpochReason),
}

#[cfg(test)]
type PendingEpochReason = String;
#[cfg(not(test))]
type PendingEpochReason = ();
fn pending_epoch(_reason: &str) -> EpochObservation {
    #[cfg(test)]
    {
        EpochObservation::Pending(_reason.into())
    }
    #[cfg(not(test))]
    {
        EpochObservation::Pending(())
    }
}

fn encode_reference_epoch(
    record: &ReferenceEpochRecord,
) -> Result<[u8; REFERENCE_EPOCH_SIZE], NativeError> {
    let payload = serde_json::to_vec(record)?;
    if payload.len() > REFERENCE_EPOCH_SIZE - 44 {
        return Err(invalid("reference_epoch_record_oversized"));
    }
    let mut bytes = [0; REFERENCE_EPOCH_SIZE];
    bytes[..8].copy_from_slice(REFERENCE_EPOCH_MAGIC);
    bytes[8..12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes[12..12 + payload.len()].copy_from_slice(&payload);
    let checksum = Sha256::digest(&bytes[..REFERENCE_EPOCH_SIZE - 32]);
    bytes[REFERENCE_EPOCH_SIZE - 32..].copy_from_slice(&checksum);
    Ok(bytes)
}
fn read_reference_epoch(root: &Path) -> Result<ReferenceEpoch, NativeError> {
    let path = root.join(REFERENCE_EPOCH_FILE);
    reject_file_alias(&path)?;
    let identity = path_identity(&path)?;
    let mut file = fs::File::open(&path)?;
    if file.metadata()?.len() != REFERENCE_EPOCH_SIZE as u64
        || super::locks::file_identity(&file)? != identity
    {
        return Err(invalid("reference_epoch_record_invalid"));
    }
    let mut bytes = [0; REFERENCE_EPOCH_SIZE];
    file.read_exact(&mut bytes)?;
    let len = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| invalid("reference_epoch_length_invalid"))?,
    ) as usize;
    if &bytes[..8] != REFERENCE_EPOCH_MAGIC
        || len > REFERENCE_EPOCH_SIZE - 44
        || Sha256::digest(&bytes[..REFERENCE_EPOCH_SIZE - 32]).as_slice()
            != &bytes[REFERENCE_EPOCH_SIZE - 32..]
        || path_identity(&path)? != identity
    {
        return Err(invalid("reference_epoch_record_invalid"));
    }
    let record: ReferenceEpochRecord = serde_json::from_slice(&bytes[12..12 + len])?;
    if record.version != 2 {
        return Err(invalid("reference_epoch_version_unknown"));
    }
    Ok(ReferenceEpoch {
        file: identity,
        record,
    })
}
fn epoch_root_matches(
    root: &Path,
    record: &ReferenceEpochRecord,
    held_root_use: Option<&LockedFile>,
) -> Result<bool, NativeError> {
    for name in [
        ".use.lock",
        ".use-cohort.json",
        REFERENCE_ACTIVITY_FILE,
        REFERENCE_SERIAL_FILE,
    ] {
        reject_file_alias(&root.join(name))?;
    }
    let cohort_path = root.join(".use-cohort.json");
    reject_file_alias(&cohort_path)?;
    let mut cohort_file = fs::File::open(&cohort_path)?;
    let len = cohort_file.metadata()?.len();
    if len > 1024 || super::locks::file_identity(&cohort_file)? != record.cohort {
        return Ok(false);
    }
    let mut bytes = [0; 1024];
    cohort_file.read_exact(&mut bytes[..len as usize])?;
    let cohort: ArtifactCohortRecord = serde_json::from_slice(&bytes[..len as usize])?;
    let actual_incarnation = if let Some(held) = held_root_use {
        if held.identity()? != record.use_lock || !held.is_current()? {
            return Ok(false);
        }
        held.coordination_incarnation()?
    } else {
        super::locks::read_coordination_incarnation(&fs::File::open(root.join(".use.lock"))?)?
    };
    if cohort.version != 2
        || cohort.root != record.root
        || cohort.coordination != record.use_lock
        || cohort.coordination_incarnation != record.use_incarnation
        || actual_incarnation != Some(record.use_incarnation)
        || path_identity(&cohort_path)? != record.cohort
    {
        return Ok(false);
    }
    Ok(record.root == directory_identity(root)?
        && record.use_lock == path_identity(&root.join(".use.lock"))?
        && record.activity == path_identity(&root.join(REFERENCE_ACTIVITY_FILE))?
        && record.serial == path_identity(&root.join(REFERENCE_SERIAL_FILE))?)
}

/// Borrows actual Exclusive root ownership throughout genuine domain creation.
/// Fixed identities are captured before invoking other creation factories; no
/// factory needs to reopen the exclusively locked coordination header.
pub(crate) struct RootCreationAuthority<'a> {
    lease: &'a ArtifactUseLease,
    nonce: [u8; 32],
    cohort: FileIdentity,
    binding: FileIdentity,
}
impl RootCreationAuthority<'_> {
    pub(crate) fn root(&self) -> &Path {
        &self.lease.root
    }
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.lease.identity
    }
    pub(crate) fn coordination_identity(&self) -> Result<FileIdentity, NativeError> {
        self.lease.lock.identity()
    }
    pub(crate) fn incarnation(&self) -> Result<[u8; 32], NativeError> {
        Ok(self.nonce)
    }
    pub(crate) fn cohort_identity(&self) -> Result<FileIdentity, NativeError> {
        Ok(self.cohort)
    }
    pub(crate) fn publication_binding_identity(&self) -> Result<FileIdentity, NativeError> {
        Ok(self.binding)
    }
}

/// Mutating entry points call this BEFORE any coordinator/worker/state lock
/// can create a missing Managed root. Only actual create_dir success supplies
/// birth authority; observing an existing directory never enrolls it.
pub(crate) fn initialize_fresh_managed_domain(layout: &ManagedLayout) -> Result<bool, NativeError> {
    let Some(fresh) = super::layout::create_fresh_reference_root(layout.storage_root())? else {
        return Ok(false);
    };
    layout.ensure_roots()?;
    let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive)?;
    let domain = ReferenceDomain::managed(layout)?;
    // Cleanup controls remain optional for primary creation. Partial files are
    // finite Unknown and are never repaired/adopted on a later entry.
    Ok(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap_or(false))
}

/// Exact first mutating Direct boundary. Existing destinations never acquire
/// coverage from a later fresh cache or from observing an empty directory.
pub(crate) fn initialize_fresh_direct_domain(layout: &DirectLayout) -> Result<bool, NativeError> {
    let Some(parent) = layout.db_path().parent() else {
        return Ok(false);
    };
    if layout.manifest_path().parent() != Some(parent) {
        return Ok(false);
    }
    let Some(fresh) = super::layout::create_fresh_reference_root(parent)? else {
        return Ok(false);
    };
    ArtifactUseLease::ensure_root(&layout.artifact_root_path())?;
    let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive)?;
    let domain = ReferenceDomain::direct(layout)?;
    Ok(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap_or(false))
}

/// Seed only from a complete, newly-created reference domain and its actual
/// exclusive writer. Ordinary observation/admission and old-domain producers
/// have no creation API. A fresh cache beside an old domain is insufficient.
pub(crate) fn initialize_fresh_reference_domain(
    domain: &ReferenceDomain,
    fresh: &super::layout::FreshReferenceRoot,
    writer: &WriterLease,
) -> Result<bool, NativeError> {
    if fresh.identity() != domain.scope.destination
        || path_identity(fresh.path())? != fresh.identity()
        || !domain.revalidate()?
        || !writer.verifies(&domain.writer_path, LockMode::Exclusive)?
    {
        return Err(invalid("reference_domain_creation_unverified"));
    }
    let Some(root_use) = ArtifactUseLease::try_exclusive(&domain.artifact_root)? else {
        return Ok(false);
    };
    if fs::symlink_metadata(domain.artifact_root.join(REFERENCE_EPOCH_FILE)).is_ok() {
        return Ok(false);
    }
    let Some(use_incarnation) = root_use.lock.coordination_incarnation()? else {
        return Ok(false);
    };
    {
        // Every supported startup uses this same stable inode, including an
        // idle destination which has never needed a helper process. Admission
        // remains noncreating; birth supplies the missing coordination file.
        let destination = super::locks::RootedDirectory::open(fresh.path(), fresh.identity())?;
        let worker = if let Some(pair) = &domain.raw_address {
            DirectLayout::new(&pair[0], &pair[1]).worker_lock_path()
        } else {
            domain.destination_root.join("worker.lock")
        };
        let worker_name = worker
            .file_name()
            .ok_or_else(|| invalid("worker_coordination_name_missing"))?
            .as_encoded_bytes();
        match destination.create_new(worker_name) {
            Ok(file) => file.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                destination.open_readonly_file(worker_name)?;
            }
            Err(error) => return Err(error.into()),
        }
        destination.sync_namespace()?;
        let creation = RootCreationAuthority {
            lease: &root_use,
            nonce: use_incarnation,
            cohort: match path_identity(&domain.artifact_root.join(".use-cohort.json")) {
                Ok(id) => id,
                Err(_) => return Ok(false),
            },
            binding: match path_identity(&domain.artifact_root.join(".publication.binding")) {
                Ok(id) => id,
                Err(_) => return Ok(false),
            },
        };
        if super::maintenance_workspace_gate::initialize_fresh_workspace_gate(
            fresh, writer, &creation,
        )
        .is_err()
        {
            return Ok(false);
        }
        if domain.scope.kind == 1
            && super::maintenance_generation_gate::initialize_fresh_generation_gate(
                fresh, writer, &creation,
            )
            .is_err()
        {
            // Optional cleanup activation never adds a primary publication
            // requirement. No covered epoch is published after failed birth.
            return Ok(false);
        }
    }
    let mut incarnation = [0; 32];
    getrandom::fill(&mut incarnation)
        .map_err(|_| invalid("reference_epoch_entropy_unavailable"))?;
    let activity = open_locked(
        domain.artifact_root.join(REFERENCE_ACTIVITY_FILE),
        LockMode::Exclusive,
    )?;
    let serial = open_locked(
        domain.artifact_root.join(REFERENCE_SERIAL_FILE),
        LockMode::Exclusive,
    )?;
    let record = ReferenceEpochRecord {
        version: 2,
        root: root_use.identity,
        use_lock: root_use.lock.identity()?,
        use_incarnation,
        cohort: path_identity(&domain.artifact_root.join(".use-cohort.json"))?,
        scope: domain.scope.clone(),
        incarnation,
        activity: activity.identity()?,
        serial: serial.identity()?,
        counter: 0,
        covered: true,
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(domain.artifact_root.join(REFERENCE_EPOCH_FILE))?;
    file.write_all(&encode_reference_epoch(&record)?)?;
    file.sync_all()?;
    super::atomic::sync_dir(&domain.artifact_root)?;
    Ok(true)
}

/// Noncreating fixed-record observation. No authority survives this call and
/// no epoch is captured in the middle of a supported producer's activity.
pub(crate) fn observe_reference_epoch(
    domain: &ReferenceDomain,
) -> Result<EpochObservation, NativeError> {
    let root = &domain.artifact_root;
    let activity =
        match try_open_existing_locked(&root.join(REFERENCE_ACTIVITY_FILE), LockMode::Exclusive) {
            Ok(Some(guard)) => guard,
            Ok(None) => return Ok(pending_epoch("reference_activity_in_use")),
            Err(_) => return Ok(pending_epoch("reference_epoch_missing_or_unverifiable")),
        };
    let epoch = match read_reference_epoch(root) {
        Ok(epoch) => epoch,
        Err(_) => return Ok(pending_epoch("reference_epoch_missing_or_unverifiable")),
    };
    let verified = (|| -> Result<bool, NativeError> {
        Ok(activity.is_current()?
            && domain.revalidate()?
            && epoch_root_matches(root, &epoch.record, None)?
            && epoch.record.scope == domain.scope
            && epoch.record.covered)
    })();
    if !matches!(verified, Ok(true)) {
        return Ok(pending_epoch("reference_domain_unverified"));
    }
    Ok(EpochObservation::Verified(epoch))
}

// Private foundation only. Borrowed paths/bytes keep their owning Work and
// reservations OFF the authority stack. This is not PreparedMaintenance,
// current config/reference completeness, Ready admission or deletion authority.
pub(super) struct CanonicalEpochPaths<'a> {
    pub(super) root: &'a Path,
    pub(super) destination: &'a Path,
    pub(super) writer: &'a Path,
    pub(super) root_use: &'a Path,
    pub(super) cohort: &'a Path,
    pub(super) epoch: &'a Path,
    pub(super) activity: &'a Path,
    pub(super) serial: &'a Path,
    pub(super) reference_roots: [Option<&'a Path>; 2],
}
pub(super) struct CanonicalEpochFrame<'a> {
    pub(super) paths: CanonicalEpochPaths<'a>,
    pub(super) expected: &'a ReferenceEpoch,
    pub(super) bytes: &'a [u8],
    pub(super) cohort: super::locks::FixedFileStamp,
    pub(super) coordination_header: [u8; super::locks::COORDINATION_HEADER_BYTES],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanonicalEpochAdvance {
    pub(crate) before: u64,
    pub(crate) after: u64,
}

fn encode_fixed_epoch(
    record: &ReferenceEpochRecord,
    bytes: &mut [u8],
) -> Result<(), super::locks::FixedControlError> {
    use super::locks::{FixedControlError as Error, FixedControlProblem as Problem};
    if bytes.len() != REFERENCE_EPOCH_SIZE {
        return Err(Error::Invalid(Problem::BufferTooSmall));
    }
    bytes.fill(0);
    let len = {
        let mut cursor = std::io::Cursor::new(&mut bytes[12..REFERENCE_EPOCH_SIZE - 32]);
        serde_json::to_writer(&mut cursor, record)
            .map_err(|_| Error::Invalid(Problem::InvalidRecord))?;
        cursor.position() as usize
    };
    bytes[..8].copy_from_slice(REFERENCE_EPOCH_MAGIC);
    bytes[8..12].copy_from_slice(&(len as u32).to_le_bytes());
    let checksum = Sha256::digest(&bytes[..REFERENCE_EPOCH_SIZE - 32]);
    bytes[REFERENCE_EPOCH_SIZE - 32..].copy_from_slice(&checksum);
    Ok(())
}

/// Pure bounded preparation, no locks or writes. The caller reserved both
/// buffers in the shared account. Fixed producer codecs use a slice writer;
/// no arbitrary JSON is decoded and no expected-E bytes are relabeled later.
pub(super) fn prepare_canonical_epoch_frame<'a>(
    paths: CanonicalEpochPaths<'a>,
    expected: &'a ReferenceEpoch,
    prepared: &'a mut super::maintenance_budget::ReservedBytes,
    read: &mut super::maintenance_budget::ReservedBytes,
    scratch: &mut super::locks::FixedControlScratch,
) -> Result<CanonicalEpochFrame<'a>, super::locks::FixedControlError> {
    use super::locks::{
        open_fixed_control, read_fixed_control, FixedControlError as Error,
        FixedControlKind as Kind, FixedControlProblem as Problem, FixedFileStamp,
    };
    if !expected.record.covered || expected.record.version != 2 {
        return Err(Error::Invalid(Problem::InvalidRecord));
    }
    let next = expected
        .record
        .counter
        .checked_add(1)
        .ok_or(Error::Invalid(Problem::EpochExhausted))?;
    if prepared.as_slice().len() < 2 * REFERENCE_EPOCH_SIZE
        || read.as_slice().len() < REFERENCE_EPOCH_SIZE
    {
        return Err(Error::Invalid(Problem::BufferTooSmall));
    }
    let mut cohort_file = open_fixed_control(paths.cohort, Kind::File, false, scratch)?;
    let cohort = FixedFileStamp::capture(&cohort_file, Kind::File)?;
    if cohort.identity() != expected.record.cohort || cohort.len() > 1024 {
        return Err(Error::Invalid(Problem::InvalidRecord));
    }
    let len = read_fixed_control(&mut cohort_file, &cohort, read)?;
    // These are the exact two compact encodings of the current producer
    // contract. A different JSON spelling remains unknown, never an implicit
    // controlled migration or a reason to repair the record.
    let mut recognized = false;
    for origin in [
        CohortOrigin::GuardedCreation,
        CohortOrigin::ControlledMigration,
    ] {
        let record = ArtifactCohortRecord {
            version: 2,
            root: expected.record.root,
            coordination: expected.record.use_lock,
            coordination_incarnation: expected.record.use_incarnation,
            origin,
        };
        let mut cursor = std::io::Cursor::new(&mut prepared.as_mut_slice()[..REFERENCE_EPOCH_SIZE]);
        serde_json::to_writer(&mut cursor, &record)
            .map_err(|_| Error::Invalid(Problem::InvalidRecord))?;
        let encoded_len = cursor.position() as usize;
        if encoded_len == len && prepared.as_slice()[..len] == read.as_slice()[..len] {
            recognized = true;
            break;
        }
    }
    if !recognized {
        return Err(Error::Invalid(Problem::InvalidRecord));
    }
    let current = open_fixed_control(paths.cohort, Kind::File, false, scratch)?;
    cohort.recheck(&current)?;
    encode_fixed_epoch(
        &expected.record,
        &mut prepared.as_mut_slice()[..REFERENCE_EPOCH_SIZE],
    )?;
    let mut updated = expected.record.clone();
    updated.counter = next;
    encode_fixed_epoch(
        &updated,
        &mut prepared.as_mut_slice()[REFERENCE_EPOCH_SIZE..2 * REFERENCE_EPOCH_SIZE],
    )?;
    Ok(CanonicalEpochFrame {
        paths,
        expected,
        bytes: &prepared.as_slice()[..2 * REFERENCE_EPOCH_SIZE],
        cohort,
        coordination_header: super::locks::encode_coordination_incarnation(
            expected.record.use_incarnation,
        ),
    })
}

fn fixed_epoch_open(
    path: &Path,
    kind: super::locks::FixedControlKind,
    write: bool,
    scratch: &mut super::locks::FixedControlScratch,
    grant: &mut super::maintenance_budget::MutationGrant,
) -> Result<fs::File, super::locks::FixedControlError> {
    grant.take_entry()?;
    let file = super::locks::open_fixed_control(path, kind, write, scratch)?;
    grant.check_time()?;
    Ok(file)
}

/// Exact private canonical E -> E+1 step, retaining owned exclusive activity
/// and serial handles through a BORROWED callback. No full Work/closure owner
/// moves in or drops on error/panic; callers retain their writer/root guards.
/// No production caller exists until full bounded admission is reviewed.
#[cfg(test)]
fn with_canonical_epoch_advance(
    frame: &CanonicalEpochFrame<'_>,
    writer: &WriterLease,
    root_use: &ArtifactUseLease,
    read: &mut super::maintenance_budget::ReservedBytes,
    scratch: &mut super::locks::FixedControlScratch,
    grant: &mut super::maintenance_budget::MutationGrant,
    apply: &mut impl FnMut(CanonicalEpochAdvance) -> Result<(), super::locks::FixedControlError>,
) -> Result<CanonicalEpochAdvance, super::locks::FixedControlError> {
    if root_use.identity != frame.expected.record.root || root_use.reference_activity.is_some() {
        return Err(super::locks::FixedControlError::Invalid(
            super::locks::FixedControlProblem::GuardMismatch,
        ));
    }
    with_canonical_epoch_handles(
        frame,
        EpochLock::Legacy(writer),
        EpochLock::Legacy(&root_use.lock),
        read,
        scratch,
        grant,
        0,
        &mut |_, _| Ok(()),
        &mut |advance, _| apply(advance),
    )
}

pub(super) enum EpochLock<'a> {
    #[cfg(test)]
    Legacy(&'a LockedFile),
    Fixed(&'a super::locks::FixedLockedFile),
}
impl EpochLock<'_> {
    fn matches_fixed_current(
        &self,
        stamp: &super::locks::FixedFileStamp,
        mode: LockMode,
    ) -> Result<bool, super::locks::FixedControlError> {
        match self {
            #[cfg(test)]
            Self::Legacy(lock) => lock.matches_fixed_current(stamp, mode),
            Self::Fixed(lock) => lock.matches_fixed_current(stamp, mode),
        }
    }
    fn matches_fixed_header(
        &self,
        header: &[u8; super::locks::COORDINATION_HEADER_BYTES],
    ) -> Result<bool, super::locks::FixedControlError> {
        match self {
            #[cfg(test)]
            Self::Legacy(lock) => lock.matches_fixed_header(header),
            Self::Fixed(lock) => lock.matches_fixed_header(header),
        }
    }
}
// Keep independent borrowed authorities and caller-owned scratch explicit.
#[allow(clippy::too_many_arguments)]
pub(super) fn with_canonical_epoch_handles(
    frame: &CanonicalEpochFrame<'_>,
    writer: EpochLock<'_>,
    root_use: EpochLock<'_>,
    read: &mut super::maintenance_budget::ReservedBytes,
    scratch: &mut super::locks::FixedControlScratch,
    grant: &mut super::maintenance_budget::MutationGrant,
    effect_entries: usize,
    validate: &mut impl FnMut(
        &mut super::locks::FixedControlScratch,
        &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(), super::locks::FixedControlError>,
    apply: &mut impl FnMut(
        CanonicalEpochAdvance,
        &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(), super::locks::FixedControlError>,
) -> Result<CanonicalEpochAdvance, super::locks::FixedControlError> {
    use super::locks::{
        read_fixed_control, FixedControlError as Error, FixedControlKind as Kind,
        FixedControlProblem as Problem, FixedFileStamp, FixedLockedFile,
    };
    let expected = &frame.expected.record;
    grant.check()?;
    if read.as_slice().len() < REFERENCE_EPOCH_SIZE || frame.bytes.len() != 2 * REFERENCE_EPOCH_SIZE
    {
        return Err(Error::Invalid(Problem::BufferTooSmall));
    }
    // Check only fixed identities/modes here; generic revalidate/claim helpers
    // allocate paths, decode configuration or scan registries and are excluded.
    let writer_current = fixed_epoch_open(frame.paths.writer, Kind::File, false, scratch, grant)?;
    let writer_stamp = FixedFileStamp::capture(&writer_current, Kind::File)?;
    if writer_stamp.identity() != expected.scope.writer
        || !writer.matches_fixed_current(&writer_stamp, LockMode::Exclusive)?
    {
        return Err(Error::Invalid(Problem::GuardMismatch));
    }
    let use_current = fixed_epoch_open(frame.paths.root_use, Kind::File, false, scratch, grant)?;
    let use_stamp = FixedFileStamp::capture(&use_current, Kind::File)?;
    if use_stamp.identity() != expected.use_lock
        || !root_use.matches_fixed_current(&use_stamp, LockMode::Exclusive)?
    {
        return Err(Error::Invalid(Problem::GuardMismatch));
    }
    if !root_use.matches_fixed_header(&frame.coordination_header)? {
        return Err(Error::Invalid(Problem::Changed));
    }
    let activity_file = fixed_epoch_open(frame.paths.activity, Kind::File, true, scratch, grant)?;
    let activity = FixedLockedFile::try_lock(activity_file, LockMode::Exclusive)?
        .ok_or(Error::Invalid(Problem::Busy))?;
    if FixedFileStamp::capture(activity.file(), Kind::File)?.identity() != expected.activity {
        return Err(Error::Invalid(Problem::Changed));
    }
    let serial_file = fixed_epoch_open(frame.paths.serial, Kind::File, true, scratch, grant)?;
    let serial = FixedLockedFile::try_lock(serial_file, LockMode::Exclusive)?
        .ok_or(Error::Invalid(Problem::Busy))?;
    if FixedFileStamp::capture(serial.file(), Kind::File)?.identity() != expected.serial {
        return Err(Error::Invalid(Problem::Changed));
    }
    // No shared activity has been acquired by this operation; no observer tries
    // to upgrade its own shared guard. Both real controls remain owned below.
    for (path, identity) in [
        (frame.paths.root, expected.root),
        (frame.paths.destination, expected.scope.destination),
    ] {
        let file = fixed_epoch_open(path, Kind::Directory, false, scratch, grant)?;
        if FixedFileStamp::capture(&file, Kind::Directory)?.identity() != identity {
            return Err(Error::Invalid(Problem::Changed));
        }
    }
    for (path, identity) in frame
        .paths
        .reference_roots
        .iter()
        .zip(expected.scope.reference_roots)
    {
        match (path, identity) {
            (Some(path), Some(identity)) => {
                let file = fixed_epoch_open(path, Kind::Directory, false, scratch, grant)?;
                if FixedFileStamp::capture(&file, Kind::Directory)?.identity() != identity {
                    return Err(Error::Invalid(Problem::Changed));
                }
            }
            (None, None) => (),
            _ => return Err(Error::Invalid(Problem::InvalidRecord)),
        }
    }
    let cohort = fixed_epoch_open(frame.paths.cohort, Kind::File, false, scratch, grant)?;
    frame.cohort.recheck(&cohort)?;
    let mut epoch_file = fixed_epoch_open(frame.paths.epoch, Kind::File, true, scratch, grant)?;
    let epoch_stamp = FixedFileStamp::capture(&epoch_file, Kind::File)?;
    if epoch_stamp.identity() != frame.expected.file
        || epoch_stamp.len() != REFERENCE_EPOCH_SIZE as u64
    {
        return Err(Error::Invalid(Problem::Changed));
    }
    read_fixed_control(&mut epoch_file, &epoch_stamp, read)?;
    if read.as_slice()[..REFERENCE_EPOCH_SIZE] != frame.bytes[..REFERENCE_EPOCH_SIZE] {
        return Err(Error::Invalid(Problem::EpochMismatch));
    }
    // Recheck the actual control addresses, including lock inode replacement,
    // AFTER taking their locks and BEFORE changing the captured epoch E.
    for (path, identity) in [
        (frame.paths.writer, writer_stamp.identity()),
        (frame.paths.root_use, use_stamp.identity()),
        (frame.paths.activity, expected.activity),
        (frame.paths.serial, expected.serial),
    ] {
        let current = fixed_epoch_open(path, Kind::File, false, scratch, grant)?;
        if FixedFileStamp::capture(&current, Kind::File)?.identity() != identity {
            return Err(Error::Invalid(Problem::Changed));
        }
    }
    let current_root = fixed_epoch_open(frame.paths.root, Kind::Directory, false, scratch, grant)?;
    if FixedFileStamp::capture(&current_root, Kind::Directory)?.identity() != expected.root {
        return Err(Error::Invalid(Problem::Changed));
    }
    // Reserve the epoch file effect before starting it. Encoding and checksum
    // were already prepared outside authority; no decode/hash/state lock here.
    validate(scratch, grant)?;
    grant.take_entries(effect_entries.checked_add(2).ok_or(Error::Resource(
        super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
    ))?)?; // complete effect reservation before E changes
    grant.check_time()?;
    epoch_file.seek(SeekFrom::Start(0))?;
    epoch_file.write_all(&frame.bytes[REFERENCE_EPOCH_SIZE..])?;
    grant.check_time()?;
    epoch_file.sync_all()?;
    grant.check_time()?;
    // This independent entry was already reserved with the complete effect.
    let mut current =
        super::locks::open_fixed_control(frame.paths.epoch, Kind::File, false, scratch)?;
    let written_stamp = FixedFileStamp::capture(&current, Kind::File)?;
    if written_stamp.identity() != frame.expected.file
        || written_stamp.len() != REFERENCE_EPOCH_SIZE as u64
    {
        return Err(Error::Invalid(Problem::Changed));
    }
    read_fixed_control(&mut current, &written_stamp, read)?;
    if read.as_slice()[..REFERENCE_EPOCH_SIZE] != frame.bytes[REFERENCE_EPOCH_SIZE..] {
        return Err(Error::Invalid(Problem::Changed));
    }
    grant.check_time()?;
    let advance = CanonicalEpochAdvance {
        before: expected.counter,
        after: expected
            .counter
            .checked_add(1)
            .ok_or(Error::Invalid(Problem::EpochExhausted))?,
    };
    apply(advance, grant)?;
    grant.check_time()?;
    // Explicit uses document retention through callback; RAII also covers all
    // preceding errors and callback unwind. No caller-owned guard is released.
    drop(serial);
    drop(activity);
    Ok(advance)
}

#[derive(Debug)]
struct ReferenceActivity {
    guard: Option<LockedFile>,
    #[cfg(test)]
    reason: Option<String>,
    producer_binding: Option<ArtifactRootProducerBinding>,
}

#[derive(Debug, Clone, Copy)]
struct ArtifactRootProducerBinding {
    root: FileIdentity,
    coordination: FileIdentity,
    incarnation: [u8; 32],
    cohort: FileIdentity,
}
impl ReferenceActivity {
    fn untracked(_reason: &str) -> Self {
        Self {
            guard: None,
            #[cfg(test)]
            reason: Some(_reason.into()),
            producer_binding: None,
        }
    }
    fn begin(
        root: &Path,
        expected: Option<&ReferenceDomain>,
        nonblocking: bool,
        held_root_use: &LockedFile,
    ) -> Result<Option<Self>, NativeError> {
        // Never create missing epoch/control proof or silently revive coverage.
        match fs::symlink_metadata(root.join(REFERENCE_EPOCH_FILE)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(Self::untracked("reference_epoch_missing")))
            }
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        let activity = if nonblocking {
            match try_open_existing_locked(&root.join(REFERENCE_ACTIVITY_FILE), LockMode::Shared) {
                Ok(Some(guard)) => guard,
                Ok(None) => return Ok(None),
                Err(error) => return Err(error),
            }
        } else {
            // Existing-only shared handle; no lock creation can repair proof.
            super::locks::open_existing_shared_coordination(&root.join(REFERENCE_ACTIVITY_FILE))?
        };
        let serial = if nonblocking {
            match try_open_existing_locked(&root.join(REFERENCE_SERIAL_FILE), LockMode::Exclusive) {
                Ok(Some(guard)) => guard,
                Ok(None) => return Ok(None),
                Err(error) => return Err(error),
            }
        } else {
            super::locks::open_existing_exclusive(&root.join(REFERENCE_SERIAL_FILE))?
        };
        let mut epoch = read_reference_epoch(root)?;
        if !activity.is_current()?
            || !serial.is_current()?
            || !epoch_root_matches(root, &epoch.record, Some(held_root_use))?
        {
            return Err(invalid("reference_epoch_incarnation_changed"));
        }
        let mismatch = match expected {
            Some(domain) => !domain.revalidate()? || epoch.record.scope != domain.scope,
            None => false,
        };
        if !epoch.record.covered {
            return Ok(Some(Self {
                guard: Some(activity),
                #[cfg(test)]
                reason: Some("reference_coverage_unverified".into()),
                producer_binding: None,
            }));
        }
        epoch.record.counter = epoch
            .record
            .counter
            .checked_add(1)
            .ok_or_else(|| invalid("reference_epoch_exhausted"))?;
        if mismatch {
            epoch.record.covered = false;
        }
        let path = root.join(REFERENCE_EPOCH_FILE);
        let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
        if super::locks::file_identity(&file)? != epoch.file {
            return Err(invalid("reference_epoch_identity_changed"));
        }
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&encode_reference_epoch(&epoch.record)?)?;
        file.sync_all()?;
        if path_identity(&path)? != epoch.file || !activity.is_current()? || !serial.is_current()? {
            return Err(invalid("reference_epoch_identity_changed"));
        }
        Ok(Some(Self {
            guard: Some(activity),
            #[cfg(test)]
            reason: mismatch.then(|| "reference_domain_mismatch".into()),
            producer_binding: (!mismatch).then_some(ArtifactRootProducerBinding {
                root: epoch.record.root,
                coordination: epoch.record.use_lock,
                incarnation: epoch.record.use_incarnation,
                cohort: epoch.record.cohort,
            }),
        }))
    }
}

/// Scope-bound producer activity. Missing old coverage permits ordinary work
/// only as untracked; it can never authorize canonical artifact collection.
#[derive(Debug)]
pub(crate) struct ReferenceMutation<'a> {
    _root_use: Option<LockedFile>,
    activity: ReferenceActivity,
    borrowed_root_use: Option<&'a ArtifactUseLease>,
}
impl ReferenceMutation<'static> {
    /// Producing paths may create a previously absent artifact root. Acquire
    /// and retain its real use guard BEFORE any data effect, so a concurrent
    /// domain initializer cannot bless an in-flight untracked producer.
    pub(crate) fn begin_producing(root: &Path) -> Result<Self, NativeError> {
        let producer = ArtifactUseLease::acquire_shared(root)?;
        Ok(Self {
            _root_use: Some(producer.lock),
            activity: producer
                .reference_activity
                .unwrap_or_else(|| ReferenceActivity::untracked("reference_epoch_missing")),
            borrowed_root_use: None,
        })
    }

    pub(crate) fn begin_existing(root: &Path) -> Result<Self, NativeError> {
        Self::begin(root, None, false)?.ok_or_else(|| invalid("reference_activity_in_use"))
    }
    pub(crate) fn begin_in_domain(domain: &ReferenceDomain) -> Result<Self, NativeError> {
        Self::begin(&domain.artifact_root, Some(domain), false)?
            .ok_or_else(|| invalid("reference_activity_in_use"))
    }
    pub(crate) fn try_begin_in_domain(
        domain: &ReferenceDomain,
    ) -> Result<Option<Self>, NativeError> {
        Self::begin(&domain.artifact_root, Some(domain), true)
    }
    fn begin(
        root: &Path,
        expected: Option<&ReferenceDomain>,
        nonblocking: bool,
    ) -> Result<Option<Self>, NativeError> {
        let root = absolute(root)?;
        let root_use = if nonblocking {
            match try_open_existing_locked(&root.join(".use.lock"), LockMode::Shared) {
                Ok(Some(guard)) => guard,
                Ok(None) => return Ok(None),
                Err(error) => {
                    if !matches!(fs::symlink_metadata(root.join(REFERENCE_EPOCH_FILE)), Err(ref probe) if probe.kind() == std::io::ErrorKind::NotFound)
                    {
                        return Err(error);
                    }
                    return Ok(Some(Self {
                        _root_use: None,
                        activity: ReferenceActivity::untracked("reference_root_unverifiable"),
                        borrowed_root_use: None,
                    }));
                }
            }
        } else {
            match super::locks::open_existing_shared_coordination(&root.join(".use.lock")) {
                Ok(guard) => guard,
                Err(error) => {
                    if !matches!(fs::symlink_metadata(root.join(REFERENCE_EPOCH_FILE)), Err(ref probe) if probe.kind() == std::io::ErrorKind::NotFound)
                    {
                        return Err(error);
                    }
                    return Ok(Some(Self {
                        _root_use: None,
                        activity: ReferenceActivity::untracked("reference_root_unverifiable"),
                        borrowed_root_use: None,
                    }));
                }
            }
        };
        let Some(activity) = ReferenceActivity::begin(&root, expected, nonblocking, &root_use)?
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            _root_use: Some(root_use),
            activity,
            borrowed_root_use: None,
        }))
    }
}
impl Drop for ReferenceMutation<'_> {
    fn drop(&mut self) {
        drop(self.activity.guard.take());
        drop(self._root_use.take());
    }
}

/// Same-root producer provenance, never cleanup/destination authority. Fields
/// cannot be reconstructed from a bool, path, serialized record or root name.
/// Both real guards are borrowed from the live mutation; no ownership escapes.
#[derive(Debug)]
pub(crate) struct ArtifactRootProducerCapability<'a> {
    root: &'a Path,
    root_use: &'a LockedFile,
    activity: &'a LockedFile,
    binding: ArtifactRootProducerBinding,
    _not_send_sync: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl ArtifactRootProducerCapability<'_> {
    pub(crate) fn root(&self) -> &Path {
        self.root
    }
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.binding.root
    }
    pub(crate) fn coordination_identity(&self) -> FileIdentity {
        self.binding.coordination
    }
    pub(crate) fn incarnation(&self) -> [u8; 32] {
        self.binding.incarnation
    }
    pub(crate) fn cohort_identity(&self) -> FileIdentity {
        self.binding.cohort
    }

    /// Existing-only checks. A failed optional attestation must not be promoted
    /// into a mandatory primary publication/read error by its consumer.
    pub(crate) fn revalidate(&self) -> Result<bool, NativeError> {
        if directory_identity(self.root)? != self.binding.root
            || !self
                .root_use
                .verifies(&self.root.join(".use.lock"), self.root_use.mode())?
            || self.root_use.identity()? != self.binding.coordination
            || self.root_use.coordination_incarnation()? != Some(self.binding.incarnation)
            || !self
                .activity
                .verifies(&self.root.join(REFERENCE_ACTIVITY_FILE), LockMode::Shared)?
        {
            return Ok(false);
        }
        let path = self.root.join(".use-cohort.json");
        if !real_path(&path) {
            return Ok(false);
        }
        reject_file_alias(&path)?;
        let file = fs::File::open(&path)?;
        if super::locks::file_identity(&file)? != self.binding.cohort
            || path_identity(&path)? != self.binding.cohort
        {
            return Ok(false);
        }
        let stamp =
            super::locks::FixedFileStamp::capture(&file, super::locks::FixedControlKind::File)
                .map_err(|_| invalid("artifact_producer_cohort_changed"))?;
        let len = file.metadata()?.len();
        if len > 1024 {
            return Ok(false);
        }
        let mut bytes = [0; 1024];
        (&file).read_exact(&mut bytes[..len as usize])?;
        let Ok(record) = serde_json::from_slice::<ArtifactCohortRecord>(&bytes[..len as usize])
        else {
            return Ok(false);
        };
        stamp
            .recheck(&file)
            .map_err(|_| invalid("artifact_producer_cohort_changed"))?;
        Ok(record.version == 2
            && record.root == self.binding.root
            && record.coordination == self.binding.coordination
            && record.coordination_incarnation == self.binding.incarnation
            && file.metadata()?.len() == len
            && path_identity(&path)? == self.binding.cohort
            && self.root_use.is_current()?
            && self.activity.is_current()?)
    }
}

impl ReferenceMutation<'_> {
    pub(crate) fn artifact_root_producer_capability(
        &self,
    ) -> Result<Option<ArtifactRootProducerCapability<'_>>, NativeError> {
        let Some(binding) = self.activity.producer_binding else {
            return Ok(None);
        };
        let Some(activity) = self.activity.guard.as_ref() else {
            return Ok(None);
        };
        let root_use = self
            ._root_use
            .as_ref()
            .or_else(|| self.borrowed_root_use.map(|lease| &lease.lock));
        let Some(root_use) = root_use else {
            return Ok(None);
        };
        let Some(root) = root_use.path().parent() else {
            return Ok(None);
        };
        let capability = ArtifactRootProducerCapability {
            root,
            root_use,
            activity,
            binding,
            _not_send_sync: std::marker::PhantomData,
        };
        if !capability.revalidate()? {
            return Ok(None);
        }
        Ok(Some(capability))
    }

    #[cfg(test)]
    pub(crate) fn tracked(&self) -> bool {
        self.activity.reason.is_none()
    }
    #[cfg(test)]
    pub(crate) fn pending_reason(&self) -> Option<&str> {
        self.activity.reason.as_deref()
    }
}

#[derive(Debug)]
#[cfg(test)]
// Fixed inline protocol/lease state: boxing would add a separately accounted allocation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum MaintenanceAdmission<'a> {
    Acquired(MaintenanceLease<'a>),
    Deferred(String),
    Unknown(String),
}

#[derive(Debug)]
#[cfg(test)]
enum DestinationWriter<'a> {
    Owned(WriterLease),
    Borrowed(&'a WriterLease),
}
#[cfg(test)]
impl DestinationWriter<'_> {
    fn lease(&self) -> &WriterLease {
        match self {
            Self::Owned(lease) => lease,
            Self::Borrowed(lease) => lease,
        }
    }
}

/// A current-cohort lease can be acquired before legacy migration is approved.
/// `cohort_verified` establishes destination-wide controlled migration;
/// `artifact_cohort_verified` establishes root-only producer provenance. S3 owns
/// confirmation policy and persistence, using `cohort_record` under this lease.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct MaintenanceLease<'a> {
    writer: DestinationWriter<'a>,
    writer_path: PathBuf,
    state_path: Option<PathBuf>,
    artifact: ArtifactUseLease,
    _database: Vec<DatabaseUseLease>,
    _workers: Vec<LockedFile>,
    _phases: Vec<LockedFile>,
    target: MaintenanceTarget,
    binding: ObservedBinding,
}
#[cfg(test)]
impl MaintenanceLease<'_> {
    /// Invalidation only. A future prepared transition must validate its old
    /// epoch under this authority BEFORE calling; this is not a deletion seal.
    pub(crate) fn begin_reference_mutation(&self) -> Result<ReferenceMutation<'_>, NativeError> {
        self.artifact.revalidate()?;
        let activity =
            ReferenceActivity::begin(&self.artifact.root, None, false, &self.artifact.lock)?
                .ok_or_else(|| invalid("reference_activity_in_use"))?;
        Ok(ReferenceMutation {
            _root_use: None,
            activity,
            borrowed_root_use: Some(&self.artifact),
        })
    }

    pub(crate) fn target(&self) -> &MaintenanceTarget {
        &self.target
    }

    pub(crate) fn revalidate(&self) -> Result<(), NativeError> {
        if !self
            .writer
            .lease()
            .verifies(&self.writer_path, LockMode::Exclusive)?
        {
            return Err(invalid("writer_identity_changed"));
        }
        self.artifact.revalidate()?;
        if observe_binding(&self.target).map_err(invalid)? != self.binding
            || bound_roles_overlap(&self.target, &self.binding)
        {
            return Err(invalid("destination_binding_changed"));
        }
        for guard in self._workers.iter().chain(&self._phases) {
            if !guard.is_current()? {
                return Err(invalid("worker_or_phase_identity_changed"));
            }
        }
        for guard in &self._database {
            guard.revalidate()?;
        }
        Ok(())
    }

    /// Root provenance does not establish destination-wide legacy quiescence.
    pub(crate) fn artifact_cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
        self.artifact.cohort_verified()
    }

    #[cfg(test)]
    pub(crate) fn cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
        if self.artifact.lock.coordination_incarnation()?.is_none() {
            return Ok(false);
        }
        let expected = self.cohort_record()?;
        let path = self.cohort_record_path();
        if !real_path(&path) {
            return Ok(false);
        }
        reject_file_alias(&path)?;
        let identity = path_identity(&path)?;
        let file = fs::File::open(&path)?;
        if super::locks::file_identity(&file)? != identity {
            return Ok(false);
        }
        let observed: DestinationCohortRecord = match serde_json::from_reader(&file) {
            Ok(record) => record,
            Err(_) => return Ok(false),
        };
        Ok(path_identity(&path)? == identity && observed == expected)
    }

    /// Short metadata-only state transition. The caller must prepare validation
    /// and traversal outside this closure. Contention never waits under writer.
    pub(crate) fn with_state_lock<T>(
        &self,
        transition: impl FnOnce() -> Result<T, NativeError>,
    ) -> Result<Option<T>, NativeError> {
        self.revalidate()?;
        let _state = if let Some(path) = &self.state_path {
            let Some(lease) = try_open_locked_in_existing_parent(path, LockMode::Exclusive)? else {
                return Ok(None);
            };
            Some(lease)
        } else {
            None
        };
        transition().map(Some)
    }

    /// Data for S3's durable, explicit controlled-migration record. Returning
    /// this value does not write it or authorize legacy retirement by itself.
    pub(crate) fn cohort_record(&self) -> Result<DestinationCohortRecord, NativeError> {
        self.revalidate()?;
        let pair = self
            .target
            .direct_pair
            .clone()
            .or(self.binding.current_direct.clone());
        let (database, manifest) = match &pair {
            Some(pair) => (
                optional_resource_identity(&pair.database)?,
                optional_resource_identity(&pair.manifest)?,
            ),
            None => (None, None),
        };
        Ok(DestinationCohortRecord {
            version: 2,
            writer_path: self.writer_path.clone(),
            repository: path_identity(&self.target.repository_root)?,
            destination_root: directory_identity(
                self.writer_path
                    .parent()
                    .ok_or_else(|| invalid("destination_parent_missing"))?,
            )?,
            writer: self.writer.lease().identity()?,
            artifact_root: self.artifact.identity,
            artifact_coordination: self.artifact.lock.identity()?,
            artifact_incarnation: self
                .artifact
                .lock
                .coordination_incarnation()?
                .ok_or_else(|| invalid("artifact_coordination_incarnation_unknown"))?,
            direct_pair: pair,
            database,
            manifest,
            resource_use: self
                ._database
                .iter()
                .map(DatabaseUseLease::exclusive_proof)
                .collect::<Result<_, _>>()?,
            origin: DestinationCohortOrigin::ControlledMigration,
        })
    }

    /// S3 may persist this data only after proving fresh creation and durable
    /// coherent ACTIVE publication. It grants no operator-confirmation status
    /// and covers only these exact files/generation, never legacy siblings.
    pub(crate) fn publication_cohort_record(
        &self,
        database: &DatabaseUseLease,
        manifest: &DatabaseUseLease,
    ) -> Result<PublicationCohortRecord, NativeError> {
        self.revalidate()?;
        let database = database.exclusive_proof()?;
        let manifest = manifest.exclusive_proof()?;
        if database.file.is_none() || manifest.file.is_none() {
            return Err(invalid("publication_files_missing"));
        }
        let pair = self
            .target
            .direct_pair
            .as_ref()
            .or(self.binding.current_direct.as_ref());
        let scope = if let Some(pair) = pair {
            if database.resource != absolute(&pair.database)?
                || manifest.resource != absolute(&pair.manifest)?
            {
                return Err(invalid("publication_wrong_direct_destination"));
            }
            PublicationScope::Direct(pair.clone())
        } else {
            let generation = database
                .resource
                .parent()
                .ok_or_else(|| invalid("publication_generation_missing"))?;
            let generations = self.binding.storage.join("generations");
            if database.resource != generation.join("graph.ldb")
                || manifest.resource != generation.join("manifest.json")
                || !real_path(generation)
                || !same_physical_path(
                    generation
                        .parent()
                        .ok_or_else(|| invalid("generation_parent_missing"))?,
                    &generations,
                )
            {
                return Err(invalid("publication_wrong_managed_generation"));
            }
            let name = generation
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("gen-"))
                .ok_or_else(|| invalid("publication_generation_name_invalid"))?;
            super::layout::validate_generation_id(name)?;
            PublicationScope::ManagedGeneration {
                root: generation.to_owned(),
                identity: directory_identity(generation)?,
            }
        };
        Ok(PublicationCohortRecord {
            version: 2,
            origin: PublicationCohortOrigin::GuardedPublication,
            repository: path_identity(&self.target.repository_root)?,
            writer_path: self.writer_path.clone(),
            writer: self.writer.lease().identity()?,
            artifact_root: self.artifact.identity,
            artifact_coordination: self.artifact.lock.identity()?,
            artifact_incarnation: self.artifact.lock.coordination_incarnation()?,
            scope,
            database,
            manifest,
        })
    }

    pub(crate) fn direct_publication_cohort_record(
        &self,
    ) -> Result<PublicationCohortRecord, NativeError> {
        let database = self
            ._database
            .first()
            .ok_or_else(|| invalid("not_a_direct_destination"))?;
        let manifest = self
            ._database
            .get(1)
            .ok_or_else(|| invalid("not_a_direct_destination"))?;
        self.publication_cohort_record(database, manifest)
    }

    pub(crate) fn publication_cohort_verified(
        &self,
        record: &PublicationCohortRecord,
        database: &DatabaseUseLease,
        manifest: &DatabaseUseLease,
    ) -> Result<bool, NativeError> {
        Ok(self
            .verified_publication_cohort(record, database, manifest)?
            .is_some())
    }

    /// Informational records remain available for untracked roots. Cleanup
    /// origin/admission must instead receive this typed, actually nonce-bound
    /// capability; neither v1 nor None==None equality establishes ownership.
    pub(crate) fn verified_publication_cohort<'a>(
        &'a self,
        record: &'a PublicationCohortRecord,
        database: &DatabaseUseLease,
        manifest: &DatabaseUseLease,
    ) -> Result<Option<VerifiedPublicationCohort<'a>>, NativeError> {
        if record.version != 2
            || record.artifact_incarnation.is_none()
            || !self.artifact.cohort_verified()?
        {
            return Ok(None);
        }
        let current = self.publication_cohort_record(database, manifest)?;
        if current.artifact_incarnation.is_none() || *record != current {
            return Ok(None);
        }
        Ok(Some(VerifiedPublicationCohort {
            _record: record,
            _artifact: &self.artifact,
        }))
    }

    pub(crate) fn direct_publication_cohort_verified(
        &self,
        record: &PublicationCohortRecord,
    ) -> Result<bool, NativeError> {
        let database = self
            ._database
            .first()
            .ok_or_else(|| invalid("not_a_direct_destination"))?;
        let manifest = self
            ._database
            .get(1)
            .ok_or_else(|| invalid("not_a_direct_destination"))?;
        self.publication_cohort_verified(record, database, manifest)
    }

    pub(crate) fn cohort_record_path(&self) -> PathBuf {
        self.writer_path.with_extension("maintenance-cohort.json")
    }
}

#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VerifiedPublicationCohort<'a> {
    _record: &'a PublicationCohortRecord,
    _artifact: &'a ArtifactUseLease,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct PublicationCohortRecord {
    version: u32,
    origin: PublicationCohortOrigin,
    repository: FileIdentity,
    writer_path: PathBuf,
    writer: FileIdentity,
    artifact_root: FileIdentity,
    artifact_coordination: FileIdentity,
    #[serde(default)]
    artifact_incarnation: Option<[u8; 32]>,
    scope: PublicationScope,
    database: DatabaseUseProof,
    manifest: DatabaseUseProof,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg(test)]
enum PublicationCohortOrigin {
    GuardedPublication,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg(test)]
enum PublicationScope {
    Direct(DirectPair),
    ManagedGeneration {
        root: PathBuf,
        identity: FileIdentity,
    },
}

/// S3 persists this record only after its explicit operator-controlled protocol.
/// A normal producer never emits destination-wide migration authority. Exact
/// Direct file identities invalidate the record after replacement/publication;
/// S3 owns the new publication's ACTIVE incarnation and subsequent eligibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct DestinationCohortRecord {
    version: u32,
    writer_path: PathBuf,
    repository: FileIdentity,
    destination_root: FileIdentity,
    writer: FileIdentity,
    artifact_root: FileIdentity,
    artifact_coordination: FileIdentity,
    artifact_incarnation: [u8; 32],
    direct_pair: Option<DirectPair>,
    database: Option<FileIdentity>,
    manifest: Option<FileIdentity>,
    resource_use: Vec<DatabaseUseProof>,
    origin: DestinationCohortOrigin,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg(test)]
enum DestinationCohortOrigin {
    ControlledMigration,
}

fn optional_resource_identity(path: &Path) -> Result<Option<FileIdentity>, NativeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            reject_file_alias(path)?;
            path_identity(path).map(Some)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CohortOrigin {
    GuardedCreation,
    ControlledMigration,
}

/// Exact physical root and stable lock identity, produced only under upgraded
/// producer ownership or explicit operator-controlled migration. No PID/time
/// or mere record presence supplies proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ArtifactCohortRecord {
    version: u32,
    root: FileIdentity,
    coordination: FileIdentity,
    coordination_incarnation: [u8; 32],
    origin: CohortOrigin,
}

/// Creation-only evidence. This type has no public constructor and is minted
/// only by the genuine fresh-root branch while retaining actual Exclusive use.
pub(crate) struct FreshPublicationRoot<'a> {
    anchor: &'a super::locks::RootedDirectory,
    root_use: &'a LockedFile,
    incarnation: [u8; 32],
}
impl FreshPublicationRoot<'_> {
    pub(crate) fn anchor(&self) -> &super::locks::RootedDirectory {
        self.anchor
    }
    pub(crate) fn incarnation(&self) -> [u8; 32] {
        self.incarnation
    }
    pub(crate) fn coordination_identity(&self) -> std::io::Result<FileIdentity> {
        if self.root_use.mode() != LockMode::Exclusive {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        self.root_use.identity().map_err(|error| match error {
            NativeError::Io(error) => error,
            _ => std::io::ErrorKind::InvalidInput.into(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct ArtifactUseLease {
    root: PathBuf,
    identity: FileIdentity,
    lock: LockedFile,
    reference_activity: Option<ReferenceActivity>,
    legacy_source: Option<super::maintenance_legacy_guard::LegacySourceUseGuard>,
}
impl ArtifactUseLease {
    pub(crate) fn ensure_root(root: &Path) -> Result<(), NativeError> {
        let root = absolute(root)?;
        fs::create_dir_all(
            root.parent()
                .ok_or_else(|| invalid("artifact_parent_missing"))?,
        )?;
        match fs::create_dir(&root) {
            Ok(()) => {
                let identity = directory_identity(&root)?;
                if let Some(lock) =
                    try_open_locked_in_existing_parent(root.join(".use.lock"), LockMode::Exclusive)?
                {
                    let lease = Self {
                        root,
                        identity,
                        lock,
                        reference_activity: None,
                        legacy_source: None,
                    };
                    lease.write_guarded_creation_record()?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                directory_identity(&root)?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    fn write_guarded_creation_record(&self) -> Result<(), NativeError> {
        let mut incarnation = [0; 32];
        getrandom::fill(&mut incarnation)
            .map_err(|_| invalid("artifact_coordination_entropy_unavailable"))?;
        self.revalidate()?;
        // Failure to prepare optional provenance cannot introduce a new primary
        // write requirement. No header/cohort is published if anchoring fails.
        let anchor = match super::locks::RootedDirectory::open(&self.root, self.identity) {
            Ok(anchor) => anchor,
            Err(_) => return Ok(()),
        };
        let named_use = anchor.open_existing(b".use.lock")?;
        if super::locks::file_identity(&named_use)? != self.lock.identity()? {
            return Err(invalid("artifact_coordination_incarnation_changed"));
        }
        drop(named_use);
        self.lock.initialize_creation_incarnation(incarnation)?;
        {
            let fresh = FreshPublicationRoot {
                anchor: &anchor,
                root_use: &self.lock,
                incarnation,
            };
            let mut scratch = super::maintenance_controls::PublicationFactoryScratch::new();
            // Partial controls remain finite Unknown. Primary publication must
            // use S3's approved safe lanes without inventing origin attestation.
            let _ =
                super::maintenance_controls::initialize_publication_controls(&fresh, &mut scratch);
        }
        let record = self.record(CohortOrigin::GuardedCreation)?;
        let mut file = anchor.create_new(b".use-cohort.json")?;
        serde_json::to_writer(&mut file, &record)?;
        file.sync_all()?;
        #[cfg(not(windows))]
        anchor.sync_namespace()?;
        #[cfg(windows)]
        {
            // The existing primary cohort policy never fails an otherwise
            // successful write for unsupported Windows directory flushing.
            // Publisher attestation independently requires both actual flushes.
            let _ = anchor.sync_namespace();
        }
        self.revalidate()?;
        Ok(())
    }

    pub(crate) fn acquire_shared(root: &Path) -> Result<Self, NativeError> {
        let root = absolute(root)?;
        let parent = root
            .parent()
            .ok_or_else(|| invalid("artifact_parent_missing"))?;
        fs::create_dir_all(parent)?;
        let created = match fs::create_dir(&root) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(e) => return Err(e.into()),
        };
        let identity = directory_identity(&root)?;
        if fs::symlink_metadata(root.join(REFERENCE_EPOCH_FILE)).is_ok()
            && fs::symlink_metadata(root.join(".use.lock")).is_err()
        {
            return Err(invalid("reference_use_lock_missing"));
        }
        let lock = open_locked(
            root.join(".use.lock"),
            if created {
                LockMode::Exclusive
            } else {
                LockMode::Shared
            },
        )?;
        let mut lease = Self {
            root,
            identity,
            lock,
            reference_activity: None,
            legacy_source: None,
        };
        lease.revalidate()?;
        // Only the process that created this incarnation can establish guarded
        // creation. Existing unproven roots remain legacy until explicit S3
        // confirmation. A create_new write cannot replace an existing proof.
        if created {
            lease.write_guarded_creation_record()?;
            let incarnation = lease.lock.coordination_incarnation()?;
            lease.lock = lease.lock.share_after_creation()?;
            lease.revalidate()?;
            // Check the actual held header after any Windows downgrade gap.
            if lease.lock.coordination_incarnation()? != incarnation {
                return Err(invalid("artifact_coordination_incarnation_changed"));
            }
        }
        lease.legacy_source =
            super::maintenance_legacy_guard::before_artifact_producer(&lease.root)?;
        lease.reference_activity = ReferenceActivity::begin(&lease.root, None, false, &lease.lock)?;
        Ok(lease)
    }

    pub(crate) fn bind_reference_domain(
        &mut self,
        domain: &ReferenceDomain,
    ) -> Result<(), NativeError> {
        self.revalidate()?;
        if domain.artifact_identity != Some(self.identity) {
            return Err(invalid("reference_artifact_domain_mismatch"));
        }
        self.reference_activity =
            ReferenceActivity::begin(&self.root, Some(domain), false, &self.lock)?;
        Ok(())
    }
    pub(crate) fn acquire_in_domain(domain: &ReferenceDomain) -> Result<Self, NativeError> {
        let mut lease = Self::acquire_shared(&domain.artifact_root)?;
        lease.bind_reference_domain(domain)?;
        Ok(lease)
    }

    /// Cache-consumer guard: existing-only readonly Shared ownership, with no
    /// creation, reference epoch increment, optional cohort decode or repair.
    /// Mere use never establishes collection ownership for a caller-owned root.
    pub(crate) fn try_existing_shared_read(root: &Path) -> Result<Option<Self>, NativeError> {
        let root = absolute(root)?;
        let identity = match directory_identity(&root) {
            Ok(identity) => identity,
            Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None)
            }
            Err(error) => return Err(error),
        };
        let lock =
            match super::locks::try_open_existing_shared_coordination(&root.join(".use.lock")) {
                Ok(Some(lock)) => lock,
                Ok(None) => return Ok(None),
                Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(None)
                }
                Err(error) => return Err(error),
            };
        let lease = Self {
            root,
            identity,
            lock,
            reference_activity: None,
            legacy_source: None,
        };
        lease.revalidate()?;
        Ok(Some(lease))
    }

    pub(crate) fn try_exclusive(root: &Path) -> Result<Option<Self>, NativeError> {
        let root = absolute(root)?;
        let identity = directory_identity(&root)?;
        let Some(lock) =
            try_open_locked_in_existing_parent(root.join(".use.lock"), LockMode::Exclusive)?
        else {
            return Ok(None);
        };
        let lease = Self {
            root,
            identity,
            lock,
            reference_activity: None,
            legacy_source: None,
        };
        lease.revalidate()?;
        Ok(Some(lease))
    }

    fn revalidate(&self) -> Result<(), NativeError> {
        if directory_identity(&self.root)? != self.identity
            || !self
                .lock
                .verifies(&self.root.join(".use.lock"), self.lock_mode())?
        {
            return Err(invalid("artifact_root_incarnation_changed"));
        }
        Ok(())
    }
    fn lock_mode(&self) -> LockMode {
        self.lock.mode()
    }
    fn record(&self, origin: CohortOrigin) -> Result<ArtifactCohortRecord, NativeError> {
        self.revalidate()?;
        Ok(ArtifactCohortRecord {
            version: 2,
            root: self.identity,
            coordination: self.lock.identity()?,
            coordination_incarnation: self
                .lock
                .coordination_incarnation()?
                .ok_or_else(|| invalid("artifact_coordination_incarnation_unknown"))?,
            origin,
        })
    }
    #[cfg(test)]
    fn cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
        let path = self.root.join(".use-cohort.json");
        if !real_path(&path) {
            return Ok(false);
        }
        reject_file_alias(&path)?;
        let identity = path_identity(&path)?;
        let file = fs::File::open(&path)?;
        if super::locks::file_identity(&file)? != identity {
            return Ok(false);
        }
        let record: ArtifactCohortRecord = match serde_json::from_reader(&file) {
            Ok(record) => record,
            Err(_) => return Ok(false),
        };
        Ok(path_identity(&path)? == identity
            && record.version == 2
            && record.root == self.identity
            && record.coordination == self.lock.identity()?
            && self.lock.coordination_incarnation()? == Some(record.coordination_incarnation))
    }
    pub(crate) fn into_lock(self) -> LockedFile {
        let mut lock = self.lock;
        if let Some(activity) = self.reference_activity {
            if let Some(guard) = activity.guard {
                lock.retain(guard);
            }
        }
        lock
    }
}

/// Physical-parent family ownership covers all equivalent filename spellings,
/// including absent-to-created resources. Basename guards remain secondary;
/// filesystem case folding/normalization is never emulated as authority. Raw
/// Direct pair-derived writer/journal keys stay unchanged.
#[derive(Debug)]
pub(crate) struct DatabaseUseLease {
    resource: PathBuf,
    parent: PathBuf,
    parent_identity: FileIdentity,
    family: Arc<LockedFile>,
    lock: Arc<LockedFile>,
    parent_pin: fs::File,
    legacy_source: Option<super::maintenance_legacy_guard::LegacyNativeUseGuard>,
}
impl DatabaseUseLease {
    pub(crate) fn acquire_shared(database: &Path) -> Result<Self, NativeError> {
        let startup = super::maintenance_generation_gate::before_native_open(database)?;
        let legacy_source = super::maintenance_legacy_guard::before_native_open(database)?;
        let (parent, path) = database_lock_path(database, true)?;
        let parent_identity = directory_identity(&parent)?;
        let parent_pin =
            super::locks::RootedDirectory::open_readonly_root(&parent, parent_identity)?
                .pin_handle()?;
        let family = Arc::new(open_shared_coordination(
            &parent.join(".database-use-family.lock"),
        )?);
        let lock = Arc::new(open_shared_coordination(&path)?);
        let lease = Self {
            resource: absolute(database)?,
            parent,
            parent_identity,
            family,
            lock,
            parent_pin,
            legacy_source,
        };
        lease.revalidate()?;
        super::maintenance_generation_gate::recheck_before_native(startup.as_ref(), database)?;
        super::maintenance_legacy_guard::recheck_before_native(
            lease.legacy_source.as_ref(),
            database,
        )?;
        drop(startup);
        Ok(lease)
    }

    #[cfg(test)]
    pub(crate) fn try_exclusive(database: &Path) -> Result<Option<Self>, NativeError> {
        Ok(Self::try_exclusive_many(&[database])?.and_then(|mut leases| leases.pop()))
    }

    /// One acquisition group owns each physical parent and each basename guard
    /// once. This is explicit group authority, never process-wide reentrancy.
    /// All guards remain live until the final group member using them drops.
    #[cfg(test)]
    pub(crate) fn try_exclusive_many(
        databases: &[&Path],
    ) -> Result<Option<Vec<Self>>, NativeError> {
        let mut families: Vec<(FileIdentity, Arc<LockedFile>)> = Vec::new();
        let mut resource_locks: Vec<(FileIdentity, PathBuf, Arc<LockedFile>)> = Vec::new();
        let mut leases = Vec::new();
        for database in databases {
            let (parent, path) = database_lock_path(database, false)?;
            let parent_identity = directory_identity(&parent)?;
            let parent_pin =
                super::locks::RootedDirectory::open_readonly_root(&parent, parent_identity)?
                    .pin_handle()?;
            let family = if let Some((_, guard)) = families
                .iter()
                .find(|(identity, _)| *identity == parent_identity)
            {
                Arc::clone(guard)
            } else {
                let Some(guard) = try_open_locked_in_existing_parent(
                    parent.join(".database-use-family.lock"),
                    LockMode::Exclusive,
                )?
                else {
                    return Ok(None);
                };
                let guard = Arc::new(guard);
                families.push((parent_identity, Arc::clone(&guard)));
                guard
            };
            let name = path
                .file_name()
                .ok_or_else(|| invalid("database_coordination_name_missing"))?
                .to_owned();
            let lock = if let Some((_, _, guard)) = resource_locks
                .iter()
                .find(|(identity, key, _)| *identity == parent_identity && key.as_os_str() == name)
            {
                Arc::clone(guard)
            } else {
                let Some(guard) = try_open_locked_in_existing_parent(&path, LockMode::Exclusive)?
                else {
                    return Ok(None);
                };
                let guard = Arc::new(guard);
                resource_locks.push((parent_identity, PathBuf::from(name), Arc::clone(&guard)));
                guard
            };
            let lease = Self {
                resource: absolute(database)?,
                parent,
                parent_identity,
                family,
                lock,
                parent_pin,
                legacy_source: None,
            };
            lease.revalidate()?;
            leases.push(lease);
        }
        Ok(Some(leases))
    }

    fn revalidate(&self) -> Result<(), NativeError> {
        let addressed_parent = fs::canonicalize(
            self.resource
                .parent()
                .ok_or_else(|| invalid("database_parent_missing"))?,
        )?;
        if super::locks::file_identity(&self.parent_pin)? != self.parent_identity
            || directory_identity(&self.parent)? != self.parent_identity
            || directory_identity(&addressed_parent)? != self.parent_identity
            || !self.family.is_current()?
            || !self.lock.is_current()?
        {
            return Err(invalid("database_parent_incarnation_changed"));
        }
        Ok(())
    }

    /// Identity evidence, never a transferable deletion capability. S3 retains
    /// this exclusive lease while validating/detaching the corresponding file.
    #[cfg(test)]
    pub(crate) fn exclusive_proof(&self) -> Result<DatabaseUseProof, NativeError> {
        self.revalidate()?;
        if self.family.mode() != LockMode::Exclusive || self.lock.mode() != LockMode::Exclusive {
            return Err(invalid("database_use_not_exclusive"));
        }
        Ok(DatabaseUseProof {
            resource: self.resource.clone(),
            file: optional_resource_identity(&self.resource)?,
            parent: self.parent_identity,
            family_coordination_path: self.family.path().to_owned(),
            family_coordination: self.family.identity()?,
            coordination_path: self.lock.path().to_owned(),
            coordination: self.lock.identity()?,
        })
    }

    /// Producer conversion retains BOTH guards through the enclosing Direct
    /// session. A member of a shared exclusive group cannot be extracted alone.
    pub(crate) fn into_lock(self) -> Result<LockedFile, NativeError> {
        let mut lock =
            Arc::try_unwrap(self.lock).map_err(|_| invalid("database_use_group_still_shared"))?;
        let family =
            Arc::try_unwrap(self.family).map_err(|_| invalid("database_use_group_still_shared"))?;
        lock.retain(family);
        Ok(lock)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct DatabaseUseProof {
    resource: PathBuf,
    file: Option<FileIdentity>,
    parent: FileIdentity,
    family_coordination_path: PathBuf,
    family_coordination: FileIdentity,
    coordination_path: PathBuf,
    coordination: FileIdentity,
}

pub(super) fn database_lock_path(
    database: &Path,
    create_parent: bool,
) -> Result<(PathBuf, PathBuf), NativeError> {
    let database = absolute(database)?;
    reject_file_alias(&database)?;
    let parent = database
        .parent()
        .ok_or_else(|| invalid("database_parent_missing"))?;
    if create_parent {
        fs::create_dir_all(parent)?;
    }
    let parent = fs::canonicalize(parent)?;
    let basename = database
        .file_name()
        .ok_or_else(|| invalid("database_name_missing"))?
        .to_str()
        .ok_or_else(|| invalid("database_name_not_utf8"))?
        .to_lowercase();
    let hash = format!("{:x}", Sha256::digest(basename.as_bytes()));
    let lock = parent.join(format!(".database-use-{hash}.lock"));
    Ok((parent, lock))
}
fn absolute(path: &Path) -> Result<PathBuf, NativeError> {
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    })
}
fn directory_identity(path: &Path) -> Result<FileIdentity, NativeError> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(invalid("use_root_not_directory"));
    }
    path_identity(path)
}
fn reject_file_alias(path: &Path) -> Result<(), NativeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("resource_symlink_or_not_file"));
    }
    let file = fs::File::open(path)?;
    if super::locks::file_link_count(&file)? != 1
        || super::locks::file_identity(&file)? != path_identity(path)?
    {
        return Err(invalid("resource_hardlink_or_identity_ambiguous"));
    }
    Ok(())
}
fn invalid(reason: impl Into<String>) -> NativeError {
    NativeError::InvalidInput(reason.into())
}

/// Pure, noncreating observation held through the inventory scans. The caller
/// may release state.lock before traversal; writer/use ownership and repeated
/// tree observations remain necessary and uncertainty stays partial.
#[cfg(test)]
pub(crate) fn observe_artifact_cohort(
    root: &Path,
) -> Result<Option<ArtifactUseLease>, NativeError> {
    let root = absolute(root)?;
    let identity = directory_identity(&root)?;
    let Some(lock) = try_open_existing_locked(&root.join(".use.lock"), LockMode::Exclusive)? else {
        return Ok(None);
    };
    let lease = ArtifactUseLease {
        root,
        identity,
        lock,
        reference_activity: None,
        legacy_source: None,
    };
    if lease.cohort_verified()? {
        Ok(Some(lease))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
pub(crate) fn claim_maintenance(
    target: &MaintenanceTarget,
) -> Result<MaintenanceAdmission<'static>, NativeError> {
    claim(target, None)
}
#[cfg(test)]
pub(crate) fn claim_maintenance_with_owned<'a>(
    target: &MaintenanceTarget,
    writer: &'a WriterLease,
) -> Result<MaintenanceAdmission<'a>, NativeError> {
    claim(target, Some(writer))
}

#[cfg(test)]
fn claim<'a>(
    target: &MaintenanceTarget,
    owned: Option<&'a WriterLease>,
) -> Result<MaintenanceAdmission<'a>, NativeError> {
    let binding = match observe_binding(target) {
        Ok(binding) => binding,
        Err(reason) => return Ok(MaintenanceAdmission::Unknown(reason)),
    };
    if bound_roles_overlap(target, &binding) {
        return Ok(MaintenanceAdmission::Unknown(
            "managed_direct_storage_role_overlap".into(),
        ));
    }
    // Explicit Direct selection names its own destination. Otherwise managed
    // storage takes precedence when configured; default-only installs use Direct.
    let direct = target
        .direct_pair
        .clone()
        .or(binding.current_direct.clone());
    let (
        writer_path,
        worker_path,
        worker_state,
        workers_root,
        phase_root,
        state_path,
        artifact_root,
    ) = if let Some(pair) = &direct {
        let layout = DirectLayout::new(&pair.database, &pair.manifest);
        (
            layout.writer_lock_path(),
            layout.worker_lock_path(),
            layout.worker_state_path(),
            layout.worker_workspace_root_path(),
            layout.phase_workspace_root(),
            None,
            layout.artifact_root_path(),
        )
    } else {
        let layout = ManagedLayout::new(&binding.storage);
        (
            layout.writer_lock_path(),
            layout.worker_lock_path(),
            layout.worker_state_path(),
            binding.storage.join("workers"),
            layout.phase_workspace_root(),
            Some(layout.state_lock_path()),
            layout.artifacts_root(),
        )
    };
    if !real_path(writer_path.parent().unwrap_or_else(|| Path::new(".")))
        || !real_path(&artifact_root)
    {
        return Ok(MaintenanceAdmission::Unknown(
            "destination_missing_or_symlinked".into(),
        ));
    }
    let writer = if let Some(writer) = owned {
        if !writer.verifies(&writer_path, LockMode::Exclusive)? {
            return Err(invalid("owned_writer_wrong_destination_or_mode"));
        }
        DestinationWriter::Borrowed(writer)
    } else {
        let Some(writer) = try_open_locked_in_existing_parent(&writer_path, LockMode::Exclusive)?
        else {
            return Ok(MaintenanceAdmission::Deferred("writer_in_use".into()));
        };
        DestinationWriter::Owned(writer)
    };
    let Some(worker) = try_open_locked_in_existing_parent(&worker_path, LockMode::Exclusive)?
    else {
        return Ok(MaintenanceAdmission::Deferred("worker_in_use".into()));
    };
    let mut phases = Vec::new();
    for root in [&workers_root, &phase_root] {
        if let Err(reason) = guard_registry(root, &mut phases) {
            return Ok(if reason == "startup_in_use" {
                MaintenanceAdmission::Deferred(reason)
            } else {
                MaintenanceAdmission::Unknown(reason)
            });
        }
    }
    // Legacy v1 is not process authority; preserve its evidence until controlled
    // migration. v2 can only be idle when the existing guardian proves emptiness.
    let worker_metadata = match fs::symlink_metadata(&worker_state) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Ok(MaintenanceAdmission::Unknown(
                "worker_state_inspection_unknown".into(),
            ))
        }
    };
    if let Some(metadata) = worker_metadata {
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || !real_path(&worker_state)
            || reject_file_alias(&worker_state).is_err()
        {
            return Ok(MaintenanceAdmission::Unknown(
                "worker_state_identity_unknown".into(),
            ));
        }
        let value: serde_json::Value = match fs::File::open(&worker_state)
            .map_err(NativeError::from)
            .and_then(|f| serde_json::from_reader(f).map_err(NativeError::from))
        {
            Ok(v) => v,
            Err(_) => {
                return Ok(MaintenanceAdmission::Unknown(
                    "worker_state_unreadable".into(),
                ))
            }
        };
        if value.get("version").and_then(|v| v.as_u64()) == Some(2) {
            let descriptor: BuildDescriptorV2 = match serde_json::from_value(value) {
                Ok(d) => d,
                Err(_) => {
                    return Ok(MaintenanceAdmission::Unknown(
                        "worker_descriptor_unknown".into(),
                    ))
                }
            };
            if !matches!(probe_descriptor(&descriptor), Ok(true)) {
                return Ok(MaintenanceAdmission::Unknown(
                    "worker_containment_unknown".into(),
                ));
            }
        } else {
            return Ok(MaintenanceAdmission::Unknown(
                "legacy_quiescence_required".into(),
            ));
        }
    }
    let mut databases = Vec::new();
    if let Some(pair) = &direct {
        for path in [&pair.database, &pair.manifest] {
            if !real_path(path.parent().unwrap_or_else(|| Path::new(".")))
                || reject_file_alias(path).is_err()
            {
                return Ok(MaintenanceAdmission::Unknown(
                    "direct_resource_identity_unknown".into(),
                ));
            }
        }
        if resources_overlap(&pair.database, &pair.manifest) {
            return Ok(MaintenanceAdmission::Unknown(
                "direct_pair_resource_overlap".into(),
            ));
        }
        if let Some(reason) = overlapping_pairs(target, pair)? {
            return Ok(MaintenanceAdmission::Unknown(reason));
        }
        let Some(group) = DatabaseUseLease::try_exclusive_many(&[&pair.database, &pair.manifest])?
        else {
            return Ok(MaintenanceAdmission::Deferred(
                "database_parent_in_use".into(),
            ));
        };
        databases = group;
    }
    let Some(artifact) = ArtifactUseLease::try_exclusive(&artifact_root)? else {
        return Ok(MaintenanceAdmission::Deferred(
            "artifact_root_in_use".into(),
        ));
    };
    let lease = MaintenanceLease {
        writer,
        writer_path,
        state_path,
        artifact,
        _database: databases,
        _workers: vec![worker],
        _phases: phases,
        target: target.clone(),
        binding,
    };
    lease.revalidate()?;
    Ok(MaintenanceAdmission::Acquired(lease))
}

/// Hold every existing lifecycle lease through detachment; no retirement or
/// deletion is performed here. Missing lease beside startup/launch state is
/// unknown, and existing v1/ambiguous guardian evidence is preserved unchanged.
#[cfg(test)]
fn guard_registry(root: &Path, guards: &mut Vec<LockedFile>) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("phase_registry_identity_unknown".into());
    }
    if root
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".lifecycle-"))
    {
        let lease = try_open_existing_locked(&root.join("lease"), LockMode::Exclusive)
            .map_err(|e| format!("startup_lease_unknown: {e}"))?
            .ok_or("startup_in_use")?;
        if !super::containment::lifecycle_empty_with_owned(root, &lease)? {
            return Err("phase_containment_unknown".into());
        }
        guards.push(lease);
        return Ok(());
    }
    let mut has_files = false;
    let mut has_lifecycle = false;
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_symlink() {
            return Err("phase_registry_symlink".into());
        }
        if ty.is_dir() {
            has_lifecycle |= entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".lifecycle-"));
            guard_registry(&entry.path(), guards)?;
        } else {
            has_files = true;
        }
    }
    if has_files && !has_lifecycle {
        return Err("phase_execution_evidence_without_lifecycle".into());
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct PairRegistration {
    database: PathBuf,
    manifest: PathBuf,
}

/// Stable registration records are evidence of possible overlap, never proof
/// of quiescence. Keep them after a session so an alternate pair cannot vanish
/// from the retirement decision as soon as its current reader exits.
pub(crate) fn register_direct_pair(layout: &DirectLayout) -> Result<(), NativeError> {
    let first = layout.registration_path();
    let second = layout
        .manifest_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(
            first
                .file_name()
                .ok_or_else(|| invalid("registration_name_missing"))?,
        );
    let record = PairRegistration {
        database: absolute(layout.db_path())?,
        manifest: absolute(layout.manifest_path())?,
    };
    for path in [first, second] {
        if path.exists() {
            reject_file_alias(&path)?;
            let existing: PairRegistration = serde_json::from_slice(&fs::read(&path)?)?;
            if existing.database != record.database || existing.manifest != record.manifest {
                return Err(invalid("direct_registration_conflict"));
            }
        } else {
            write_json_atomically(&path, &record)?;
        }
    }
    Ok(())
}
#[cfg(test)]
fn overlapping_pairs(
    target: &MaintenanceTarget,
    pair: &DirectPair,
) -> Result<Option<String>, NativeError> {
    let mut pairs = vec![default_direct(target)];
    for resource in [&pair.database, &pair.manifest] {
        let parent = resource
            .parent()
            .ok_or_else(|| invalid("resource_parent_missing"))?;
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".direct-registration-")
            {
                if !real_path(&entry.path()) || reject_file_alias(&entry.path()).is_err() {
                    return Ok(Some("direct_registration_identity_unknown".into()));
                }
                let record: PairRegistration =
                    match serde_json::from_slice(&fs::read(entry.path())?) {
                        Ok(record) => record,
                        Err(_) => return Ok(Some("direct_registration_invalid".into())),
                    };
                pairs.push(DirectPair {
                    database: record.database,
                    manifest: record.manifest,
                });
            }
        }
    }
    for other in pairs {
        if other == *pair {
            continue;
        }
        if resources_overlap(&other.database, &pair.database)
            || resources_overlap(&other.manifest, &pair.manifest)
            || resources_overlap(&other.database, &pair.manifest)
            || resources_overlap(&other.manifest, &pair.database)
        {
            return Ok(Some("direct_registered_pair_overlap".into()));
        }
    }
    Ok(None)
}
#[cfg(test)]
fn resources_overlap(a: &Path, b: &Path) -> bool {
    // Actual distinct physical files are positive evidence on a case-sensitive
    // filesystem. The conservative coordination-key fallback is for absent or
    // otherwise unproven identities, not a reason to conflate known files.
    if let (Ok(a), Ok(b)) = (path_identity(a), path_identity(b)) {
        return a == b;
    }
    same_physical_path(a, b)
        || match (database_lock_path(a, false), database_lock_path(b, false)) {
            (Ok((_, a)), Ok((_, b))) => a == b,
            _ => true,
        }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::storage::direct::DirectStore;
    use crate::storage::managed::ManagedStore;
    use std::time::{Duration, Instant};

    struct Fixture {
        root: PathBuf,
        target: MaintenanceTarget,
        layout: ManagedLayout,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "cleanup-s2-{}",
                super::super::layout::managed_generation_id()
            ));
            fs::create_dir(&root).unwrap();
            let root = fs::canonicalize(root).unwrap();
            let state = root.join(".codebaseGraph");
            fs::create_dir(&state).unwrap();
            let layout = ManagedLayout::new(state.join("storage"));
            layout.ensure_roots().unwrap();
            fs::write(state.join("config.json"), br#"{"schema_version":2}"#).unwrap();
            for path in [layout.writer_lock_path(), layout.state_lock_path()] {
                drop(open_locked(path, LockMode::Exclusive).unwrap());
            }
            let target = MaintenanceTarget {
                repository_root: root.clone(),
                state_root: state,
                storage_root: None,
                direct_pair: None,
                config_path: None,
            };
            Self {
                root,
                target,
                layout,
            }
        }
        fn direct(&self) -> (MaintenanceTarget, DirectLayout) {
            let pair = default_direct(&self.target);
            let layout = DirectLayout::new(&pair.database, &pair.manifest);
            fs::write(&pair.database, b"db").unwrap();
            fs::write(&pair.manifest, br#"{"files":{}}"#).unwrap();
            drop(ArtifactUseLease::acquire_shared(&layout.artifact_root_path()).unwrap());
            let mut target = self.target.clone();
            target.direct_pair = Some(pair);
            (target, layout)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn acquired(admission: MaintenanceAdmission<'_>) -> MaintenanceLease<'_> {
        match admission {
            MaintenanceAdmission::Acquired(lease) => lease,
            other => panic!("expected acquired: {other:?}"),
        }
    }

    fn tracked_domain(fixture: &Fixture, name: &str) -> (ManagedLayout, ReferenceDomain) {
        let layout = ManagedLayout::new(fixture.root.join(name));
        let fresh = super::super::layout::create_fresh_reference_root(layout.storage_root())
            .unwrap()
            .unwrap();
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        drop(writer);
        (layout, domain)
    }
    fn observed(domain: &ReferenceDomain) -> ReferenceEpoch {
        match observe_reference_epoch(domain).unwrap() {
            EpochObservation::Verified(epoch) => epoch,
            other => panic!("expected verified epoch: {other:?}"),
        }
    }

    #[test]
    fn dc_workspace_root_removal_restarts_after_partial_effects_and_failed_done() {
        use crate::storage::maintenance_budget::{CleanupMemory, MutationGrant};
        use crate::storage::run_workspace::RunWorkspace;
        use crate::storage::{
            maintenance_controls as control, maintenance_workspace_gate as removal,
        };
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "root-removal-restart");
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
        let root = run.retire_for_maintenance().unwrap();
        // This unit isolates root-final transport after receipt collection. All
        // these files were produced by this fixture; production drains them via
        // the origin candidate/journal path, exercised by controller tests.
        for entry in fs::read_dir(root.join(".workspace-origins")).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let mut terminal = control::prepare_terminal_workspace(&root, &domain, &memory).unwrap();
        let mut initial =
            removal::prepare_workspace_root_removal(&root, &domain, Some(&terminal), &memory)
                .unwrap();
        let mut grant = MutationGrant::for_test(100, Duration::from_secs(2));
        assert_eq!(
            removal::begin_workspace_root_removal(&mut terminal, &mut initial, &mut grant).unwrap(),
            removal::WorkspaceRemovalProgress::Pending
        );
        drop(initial);
        drop(terminal);
        let mut saw_actual_removal = false;
        let mut failed_done = false;
        for _ in 0..16 {
            let mut next = removal::prepare_pending_workspace_root_removal(&domain, &memory)
                .unwrap()
                .expect("durable exact removing intent");
            let mut grant = MutationGrant::for_test(100, Duration::from_secs(2));
            let root_existed_before = root.exists();
            let result = removal::advance_workspace_root_removal(
                &mut next,
                &mut grant,
                0,
                &mut |outcome, _| {
                    match outcome {
                        removal::WorkspaceRootOutcome::Removed(_) => assert!(
                            root_existed_before,
                            "actual-rmdir token fabricated after a prior invocation"
                        ),
                        removal::WorkspaceRootOutcome::RecoveredMissing(_) => assert!(
                            !root_existed_before,
                            "missing-root recovery claimed while root was present"
                        ),
                    }
                    assert!(!root.exists());
                    failed_done = true;
                    Err(crate::storage::locks::FixedControlProblem::InvalidRecord.into())
                },
            );
            // Actual namespace removal and delivery of its durable Done event
            // can occupy different grants. Observe the former independently.
            saw_actual_removal |= root_existed_before && !root.exists();
            if failed_done {
                assert!(result.is_err());
                break;
            }
            assert_eq!(result.unwrap(), removal::WorkspaceRemovalProgress::Pending);
        }
        assert!(saw_actual_removal);
        assert!(
            failed_done,
            "first legitimate completion callback was not exercised"
        );
        assert!(!root.exists());
        // Failed durable completion keeps Removing. Recovery cannot invent the
        // prior invocation's actual-rmdir event; it reports RecoveredMissing.
        let mut pending = removal::prepare_pending_workspace_root_removal(&domain, &memory)
            .unwrap()
            .unwrap();
        let mut called = false;
        let mut grant = MutationGrant::for_test(100, Duration::from_secs(2));
        assert_eq!(
            removal::advance_workspace_root_removal(
                &mut pending,
                &mut grant,
                0,
                &mut |outcome, _| {
                    assert!(matches!(
                        outcome,
                        removal::WorkspaceRootOutcome::RecoveredMissing(_)
                    ));
                    called = true;
                    Ok(())
                }
            )
            .unwrap(),
            removal::WorkspaceRemovalProgress::Complete
        );
        assert!(called);
        assert!(
            removal::prepare_pending_workspace_root_removal(&domain, &memory)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn dc_workspace_root_removal_unknown_sibling_never_starts_intent() {
        use crate::storage::maintenance_budget::CleanupMemory;
        use crate::storage::run_workspace::RunWorkspace;
        use crate::storage::{
            maintenance_controls as control, maintenance_workspace_gate as removal,
        };
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "root-removal-unknown");
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
        let root = run.retire_for_maintenance().unwrap();
        for entry in fs::read_dir(root.join(".workspace-origins")).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let terminal = control::prepare_terminal_workspace(&root, &domain, &memory).unwrap();
        fs::write(root.join("foreign-owned"), b"preserve").unwrap();
        assert!(
            removal::prepare_workspace_root_removal(&root, &domain, Some(&terminal), &memory)
                .is_err()
        );
        assert_eq!(fs::read(root.join("foreign-owned")).unwrap(), b"preserve");
        assert!(
            removal::prepare_pending_workspace_root_removal(&domain, &memory)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn dc_s2_workspace_terminal_admission_preserves_local_source_with_unrelated_producer() {
        use crate::storage::maintenance_budget::{CleanupMemory, MutationGrant};
        use crate::storage::maintenance_controls as control;
        use crate::storage::run_workspace::RunWorkspace;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "terminal-source");
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
        let mut output =
            control::WorkspaceOutputFile::create_new(&run.staging_root().join("owned.csv"))
                .unwrap()
                .expect("genuine registered output");
        output.write_all(b"a,b\n1,2\n").unwrap();
        let completed = output.finish().unwrap();
        assert_eq!(completed.len(), 8);
        assert_eq!(
            completed.digest(),
            <[u8; 32]>::from(Sha256::digest(b"a,b\n1,2\n"))
        );
        drop(completed);
        let root = run.retire_for_maintenance().unwrap();
        assert!(root.exists());
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let mut prepared = control::prepare_terminal_workspace(&root, &domain, &memory).unwrap();
        let source = prepared.source_epoch();
        let unrelated = ReferenceMutation::begin_in_domain(&domain).unwrap();
        let mut called = false;
        let mut grant = MutationGrant::for_test(100, Duration::from_secs(2));
        let advance =
            control::with_terminal_workspace(&mut prepared, &mut grant, 0, &mut |lease, _| {
                assert_eq!(lease.source_epoch(), source);
                assert_eq!(lease.root_identity(), path_identity(&root).unwrap());
                called = true;
                Ok(())
            })
            .unwrap();
        assert!(called);
        assert_eq!(advance.after, advance.before + 1);
        assert_eq!(prepared.source_epoch(), source);
        drop(unrelated);
        assert_eq!(observed(&domain).counter(), advance.after);
    }

    #[test]
    fn dc_s2_workspace_live_output_prevents_terminal_and_preserves_root() {
        use crate::storage::maintenance_controls::WorkspaceOutputFile;
        use crate::storage::run_workspace::RunWorkspace;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "terminal-live-output");
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
        let root = run.root().to_owned();
        let output = WorkspaceOutputFile::create_new(&run.staging_root().join("live.csv"))
            .unwrap()
            .unwrap();
        assert!(run.retire_for_maintenance().is_err());
        assert!(root.exists());
        drop(output);
        assert!(root.exists());
    }

    #[test]
    fn dc_s2_workspace_child_owns_startup_gate_and_late_start_refuses_closed() {
        use crate::storage::maintenance_controls::WorkspaceStartupBinding;
        use crate::storage::run_workspace::RunWorkspace;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "terminal-child-startup");
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
        let binding = WorkspaceStartupBinding::discover(run.root())
            .unwrap()
            .unwrap();
        let child = binding.acquire().unwrap();
        child.recheck_building().unwrap();
        drop(child);
        let root = run.retire_for_maintenance().unwrap();
        assert!(binding.acquire().is_err());
        assert!(root.exists());
    }

    #[test]
    fn dc_s2_producer_capability_borrows_live_tracked_root_and_activity() {
        let fixture = Fixture::new();
        let (_layout, domain) = tracked_domain(&fixture, "producer-capability");
        let mutation = ReferenceMutation::begin_in_domain(&domain).unwrap();
        let capability = mutation
            .artifact_root_producer_capability()
            .unwrap()
            .unwrap();
        assert_eq!(capability.root(), domain.artifact_root());
        assert_eq!(
            capability.root_identity(),
            directory_identity(domain.artifact_root()).unwrap()
        );
        assert_eq!(
            capability.coordination_identity(),
            path_identity(&domain.artifact_root().join(".use.lock")).unwrap()
        );
        assert_eq!(
            capability.incarnation(),
            mutation.activity.producer_binding.unwrap().incarnation
        );
        assert!(capability.revalidate().unwrap());
        assert!(try_open_existing_locked(
            &domain.artifact_root().join(".use.lock"),
            LockMode::Exclusive
        )
        .unwrap()
        .is_none());
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        trait AmbiguousIfSend<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
        let _ = <ArtifactRootProducerCapability<'_> as AmbiguousIfSend<_>>::check;
        trait AmbiguousIfSync<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfSync<()> for T {}
        impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
        let _ = <ArtifactRootProducerCapability<'_> as AmbiguousIfSync<_>>::check;
        trait AmbiguousIfClone<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfClone<()> for T {}
        impl<T: Clone> AmbiguousIfClone<u8> for T {}
        let _ = <ArtifactRootProducerCapability<'_> as AmbiguousIfClone<_>>::check;
        // Consume this non-owning proof before releasing or checking its lender.
        #[allow(clippy::drop_non_drop)]
        drop(capability);
        // Capability drop cannot release its lender's actual ownership.
        assert!(try_open_existing_locked(
            &domain.artifact_root().join(".use.lock"),
            LockMode::Exclusive
        )
        .unwrap()
        .is_none());
        drop(mutation);
        assert!(try_open_existing_locked(
            &domain.artifact_root().join(".use.lock"),
            LockMode::Exclusive
        )
        .unwrap()
        .is_some());
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Verified(_)
        ));
    }

    #[test]
    fn dc_s2_optional_producer_capability_never_repairs_untracked_or_corrupt_proof() {
        let fixture = Fixture::new();
        let untracked =
            ReferenceMutation::begin_producing(&fixture.layout.artifacts_root()).unwrap();
        assert!(untracked
            .artifact_root_producer_capability()
            .unwrap()
            .is_none());
        fs::write(
            fixture.layout.artifacts_root().join("primary-data"),
            b"primary work remains allowed",
        )
        .unwrap();
        drop(untracked);
        let (_layout, domain) = tracked_domain(&fixture, "corrupt-producer-capability");
        let mutation = ReferenceMutation::begin_in_domain(&domain).unwrap();
        let path = domain.artifact_root().join(".use-cohort.json");
        let capability = mutation
            .artifact_root_producer_capability()
            .unwrap()
            .unwrap();
        let original = fs::read(&path).unwrap();
        for bytes in [b"invalid".to_vec(), vec![b' '; 1025]] {
            fs::write(&path, &bytes).unwrap();
            assert!(!capability.revalidate().unwrap());
            assert!(mutation
                .artifact_root_producer_capability()
                .unwrap()
                .is_none());
            fs::write(
                domain.artifact_root().join("primary-data"),
                b"optional attestation is absent",
            )
            .unwrap();
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, original).unwrap();
        assert!(capability.revalidate().unwrap());
    }

    #[test]
    fn dc_s2_producer_capability_uses_actual_borrowed_exclusive_root() {
        let fixture = Fixture::new();
        let (layout, _domain) = tracked_domain(&fixture, "borrowed-producer-capability");
        write_json_atomically(&fixture.target.state_root.join("config.json"), &serde_json::json!({
            "schema_version": 2, "repo_root": fixture.root, "storage_root": layout.storage_root()
        })).unwrap();
        let mut target = fixture.target.clone();
        target.storage_root = Some(layout.storage_root().to_owned());
        let lease = acquired(claim_maintenance(&target).unwrap());
        let mutation = lease.begin_reference_mutation().unwrap();
        assert!(mutation._root_use.is_none());
        let capability = mutation
            .artifact_root_producer_capability()
            .unwrap()
            .unwrap();
        assert_eq!(capability.root_use.mode(), LockMode::Exclusive);
        assert!(capability.revalidate().unwrap());
        // Consume this non-owning proof before releasing or checking its lender.
        #[allow(clippy::drop_non_drop)]
        drop(capability);
        drop(mutation);
        // Borrowed mutation/capability never releases caller-owned root guard.
        assert!(try_open_existing_locked(
            &layout.artifacts_root().join(".use.lock"),
            LockMode::Shared
        )
        .unwrap()
        .is_none());
        lease.artifact.revalidate().unwrap();
        drop(lease);
        assert!(try_open_existing_locked(
            &layout.artifacts_root().join(".use.lock"),
            LockMode::Shared
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn dc_s2_factory_creates_bound_fixed_names_before_tracked_producer_use() {
        use super::super::maintenance_controls::{self as controls, PublisherBinding};
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "bound-publisher-factory");
        let root = layout.artifacts_root();
        let binding_path = root.join(".publication.binding");
        let mutation = ReferenceMutation::begin_in_domain(&domain).unwrap();
        let mut capability = mutation
            .artifact_root_producer_capability()
            .unwrap()
            .unwrap();
        // This is the positive native activation qualification. Optional
        // unsupported-filesystem fallback is tested separately and cannot make
        // this test pass without a durable, actually usable publisher binding.
        assert!(binding_path.is_file(), "factory activation requires durable native namespace flushing on this fixture filesystem");
        let bytes = fs::read(&binding_path).unwrap();
        let binding = PublisherBinding::decode(&bytes).unwrap();
        assert_eq!(binding.root, capability.root_identity());
        assert_eq!(binding.coordination, capability.coordination_identity());
        assert_eq!(binding.incarnation, capability.incarnation());
        assert_eq!(
            binding.stream,
            Some(path_identity(&root.join(".publication.stream")).unwrap())
        );
        assert!(fs::read(root.join(".publication.stream"))
            .unwrap()
            .is_empty());
        let mut publication = controls::try_begin_root_publication(&mut capability)
            .unwrap()
            .unwrap();
        assert_eq!(publication.directory().identity(), binding.root);
        assert_eq!(publication.root(), root);
        assert_eq!(
            publication.root_cohort_identity(),
            path_identity(&root.join(".use-cohort.json")).unwrap()
        );
        assert_eq!(
            publication.publication_binding_identity(),
            path_identity(&binding_path).unwrap()
        );
        assert_eq!(publication.binding(), &binding);
        assert_eq!(publication.committed().generation, 0);
        assert!(publication.revalidate_existing().unwrap());
        let mut update = controls::PrimaryPublicationScratch::new();
        let next = publication
            .publish_primary_control(b"producer creation intent", &mut update)
            .unwrap();
        assert_eq!(next.generation, 1);
        assert!(publication.revalidate_existing().unwrap());
        let mut committed = [0; controls::PAYLOAD_BYTES];
        let len = publication.read_primary_control(&mut committed).unwrap();
        assert_eq!(&committed[..len], b"producer creation intent");
        assert!(
            try_open_existing_locked(&root.join(".publication.selector"), LockMode::Exclusive)
                .unwrap()
                .is_none()
        );
        drop(publication);
        assert!(
            try_open_existing_locked(&root.join(".publication.selector"), LockMode::Exclusive)
                .unwrap()
                .is_some()
        );
        let reopened = controls::try_begin_root_publication(&mut capability)
            .unwrap()
            .unwrap();
        assert_eq!(reopened.committed().generation, 1);
        drop(reopened);
        // Namespace lease drop cannot release the caller's root/activity guards.
        assert!(ArtifactUseLease::try_exclusive(&root).unwrap().is_none());
        // Consume this non-owning proof before releasing or checking its lender.
        #[allow(clippy::drop_non_drop)]
        drop(capability);
        drop(mutation);
        assert!(ArtifactUseLease::try_exclusive(&root).unwrap().is_some());
    }

    #[test]
    fn dc_s2_factory_partial_names_are_never_repaired_or_retried_on_acquisition() {
        use super::super::maintenance_controls::{
            initialize_publication_controls, PublicationFactoryScratch,
        };
        let fixture = Fixture::new();
        let root = fixture.root.join("partial-factory");
        fs::create_dir(&root).unwrap();
        let lock = open_locked(root.join(".use.lock"), LockMode::Exclusive).unwrap();
        lock.initialize_creation_incarnation([9; 32]).unwrap();
        let anchor =
            super::super::locks::RootedDirectory::open(&root, directory_identity(&root).unwrap())
                .unwrap();
        fs::write(
            root.join(".publication.data1"),
            b"unknown preexisting control",
        )
        .unwrap();
        let fresh = FreshPublicationRoot {
            anchor: &anchor,
            root_use: &lock,
            incarnation: [9; 32],
        };
        let mut scratch = PublicationFactoryScratch::new();
        assert!(initialize_publication_controls(&fresh, &mut scratch).is_err());
        assert!(!root.join(".publication.binding").exists());
        let first_id = path_identity(&root.join(".publication.data0")).unwrap();
        assert!(initialize_publication_controls(&fresh, &mut scratch).is_err());
        assert_eq!(
            path_identity(&root.join(".publication.data0")).unwrap(),
            first_id
        );
        assert_eq!(
            fs::read(root.join(".publication.data1")).unwrap(),
            b"unknown preexisting control"
        );
        drop(lock);
        let producer = ReferenceMutation::begin_producing(&root).unwrap();
        assert!(producer
            .artifact_root_producer_capability()
            .unwrap()
            .is_none());
        fs::write(root.join("primary-data"), b"primary work remains possible").unwrap();
        assert!(!root.join(".publication.binding").exists());
        assert!(!root.join(".use-cohort.json").exists());
    }

    #[test]
    fn dc_s2_cache_consumer_is_existing_readonly_and_does_not_advance_or_decode_optional_proof() {
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "read-only-cache-consumer");
        let root = layout.artifacts_root();
        fs::write(root.join("partition.json"), b"validated cached data").unwrap();
        let before_epoch = fs::read(root.join(REFERENCE_EPOCH_FILE)).unwrap();
        let cohort_path = root.join(".use-cohort.json");
        let before_cohort = fs::read(&cohort_path).unwrap();
        fs::write(&cohort_path, b"invalid optional cleanup proof").unwrap();
        let lock_path = root.join(".use.lock");
        let before_lock = fs::read(&lock_path).unwrap();
        let probe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        struct RestorePermissions {
            path: PathBuf,
            permissions: fs::Permissions,
        }
        impl Drop for RestorePermissions {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.path, self.permissions.clone());
            }
        }
        let original = fs::metadata(&lock_path).unwrap().permissions();
        let _restore = RestorePermissions {
            path: lock_path.clone(),
            permissions: original.clone(),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o444)).unwrap();
        }
        #[cfg(windows)]
        {
            let mut readonly = original.clone();
            readonly.set_readonly(true);
            fs::set_permissions(&lock_path, readonly).unwrap();
        }
        let reader = ArtifactUseLease::try_existing_shared_read(&root)
            .unwrap()
            .unwrap();
        assert!(reader.reference_activity.is_none());
        assert_eq!(
            fs::read(root.join("partition.json")).unwrap(),
            b"validated cached data"
        );
        assert_eq!(reader.lock.mode(), LockMode::Shared);
        // The probe's rights were granted BEFORE readonly permissions changed,
        // so this failure proves real lock exclusion rather than an ACL denial.
        assert!(fs2::FileExt::try_lock_exclusive(&probe).is_err());
        drop(reader);
        fs2::FileExt::try_lock_exclusive(&probe).unwrap();
        fs2::FileExt::unlock(&probe).unwrap();
        drop(probe);
        fs::set_permissions(&lock_path, original).unwrap();
        assert_eq!(fs::read(&lock_path).unwrap(), before_lock);
        assert_eq!(
            fs::read(root.join(REFERENCE_EPOCH_FILE)).unwrap(),
            before_epoch
        );
        assert_eq!(
            fs::read(&cohort_path).unwrap(),
            b"invalid optional cleanup proof"
        );
        fs::write(&cohort_path, before_cohort).unwrap();
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Verified(_)
        ));
    }

    #[test]
    fn dc_s2_cache_consumer_missing_guard_never_creates_or_attests_caller_owned_root() {
        let fixture = Fixture::new();
        let missing = fixture.root.join("missing-cache-root");
        assert!(ArtifactUseLease::try_existing_shared_read(&missing)
            .unwrap()
            .is_none());
        assert!(!missing.exists());
        let caller = fixture.root.join("caller-owned-cache");
        fs::create_dir(&caller).unwrap();
        fs::write(caller.join("payload"), b"caller data").unwrap();
        assert!(ArtifactUseLease::try_existing_shared_read(&caller)
            .unwrap()
            .is_none());
        assert_eq!(fs::read(caller.join("payload")).unwrap(), b"caller data");
        assert_eq!(fs::read_dir(&caller).unwrap().count(), 1);
        assert!(
            matches!(observe_artifact_cohort(&caller), Err(NativeError::Io(error)) if error.kind()==std::io::ErrorKind::NotFound)
        );
        assert!(!caller.join(".use.lock").exists());
        assert!(!caller.join(".use-cohort.json").exists());
    }

    #[test]
    fn dc_epoch_initial_creator_excludes_fresh_coverage_until_effects_finish() {
        let fixture = Fixture::new();
        let layout = ManagedLayout::new(fixture.root.join("creator-domain"));
        let fresh = super::super::layout::create_fresh_reference_root(layout.storage_root())
            .unwrap()
            .unwrap();
        let producer = ReferenceMutation::begin_producing(&layout.artifacts_root()).unwrap();
        assert!(!producer.tracked());
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(!initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        assert!(!domain.artifact_root().join(REFERENCE_EPOCH_FILE).exists());
        fs::write(
            domain.artifact_root().join("creator-effect"),
            b"before coverage",
        )
        .unwrap();
        drop(producer);
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        drop(writer);
        assert_eq!(observed(&domain).counter(), 0);
    }

    #[test]
    fn dc_epoch_new_cache_cannot_initialize_old_reference_domain() {
        let fixture = Fixture::new();
        assert!(
            super::super::layout::create_fresh_reference_root(fixture.layout.storage_root())
                .unwrap()
                .is_none()
        );
        let mutation =
            ReferenceMutation::begin_producing(&fixture.layout.artifacts_root()).unwrap();
        assert!(!mutation.tracked());
        drop(mutation);
        let domain = ReferenceDomain::managed(&fixture.layout).unwrap();
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        assert!(!domain.artifact_root().join(REFERENCE_EPOCH_FILE).exists());
    }

    #[test]
    fn dc_epoch_direct_recovery_invalidates_and_idle_read_does_not() {
        use crate::storage::direct::{DirectPublishJournal, DirectPublishPhase};
        let fixture = Fixture::new();
        let parent = fixture.root.join("fresh-direct-domain");
        let layout = DirectLayout::new(parent.join("graph.ldb"), parent.join("manifest.json"));
        let store = DirectStore::new(layout.clone()).unwrap();
        drop(store.begin_write().unwrap());
        let domain = ReferenceDomain::direct(&layout).unwrap();
        let old = observed(&domain);
        let mutation = ReferenceMutation::begin_in_domain(&domain).unwrap();
        fs::write(layout.db_candidate_path(), b"recovered database").unwrap();
        fs::write(layout.manifest_candidate_path(), b"{}").unwrap();
        let journal = DirectPublishJournal {
            phase: DirectPublishPhase::Prepared,
            db_path: layout.db_path().to_owned(),
            db_candidate_path: layout.db_candidate_path(),
            manifest_path: layout.manifest_path().to_owned(),
            manifest_candidate_path: layout.manifest_candidate_path(),
            db_sha256: format!("{:x}", Sha256::digest(b"recovered database")),
            manifest_sha256: format!("{:x}", Sha256::digest(b"{}")),
            sidecar_sha256: std::collections::BTreeMap::new(),
        };
        write_json_atomically(&layout.journal_path(), &journal).unwrap();
        drop(mutation);
        let before_recovery = observed(&domain);
        drop(store.begin_read().unwrap());
        assert_eq!(fs::read(layout.db_path()).unwrap(), b"recovered database");
        assert!(!layout.journal_path().exists());
        let after = observed(&domain);
        assert!(after.counter() > before_recovery.counter());
        assert_ne!(old, after);
        drop(store.begin_read().unwrap());
        assert_eq!(after, observed(&domain));
    }

    fn publish_tracked_generation(store: &ManagedStore, bytes: &[u8]) -> String {
        let mut session = store.begin_write().unwrap();
        fs::write(session.candidate_db_path(), bytes).unwrap();
        fs::write(session.candidate_manifest_path(), b"{}").unwrap();
        let id = session
            .publish_with_stats(&crate::protocol::GraphSummary {
                node_count: 0,
                edge_count: 0,
            })
            .unwrap();
        session.finish();
        id
    }

    #[test]
    fn dc_epoch_managed_publication_reader_drop_and_generation_removal() {
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "managed-publication-domain");
        let store = ManagedStore::new(layout.clone());
        let old = observed(&domain);
        let first = publish_tracked_generation(&store, b"first");
        let first_epoch = observed(&domain);
        assert!(first_epoch.counter() > old.counter());
        let reader = store.open_read().unwrap();
        assert_eq!(reader.generation_id, first);
        drop(reader);
        assert_eq!(
            first_epoch,
            observed(&domain),
            "active reader release is not a reference mutation"
        );
        let retained = store.open_read().unwrap();
        let second = publish_tracked_generation(&store, b"second");
        assert_ne!(first, second);
        assert!(layout.generation(&first).unwrap().root().exists());
        let before_removal = observed(&domain);
        drop(retained);
        assert!(layout.generation(&first).unwrap().root().exists());
        assert_eq!(observed(&domain), before_removal);
        store.cleanup().unwrap();
        assert!(!layout.generation(&first).unwrap().root().exists());
        assert!(observed(&domain).counter() > before_removal.counter());
    }

    #[test]
    fn dc_epoch_managed_recovery_promotion_invalidates_before_references_change() {
        use crate::storage::run_workspace::{RunPhase, RunWorkspace};
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "managed-recovery-domain");
        let store = ManagedStore::new(layout.clone());
        let base = publish_tracked_generation(&store, b"base");
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, Some(base)).unwrap();
        let id = super::super::layout::managed_generation_id();
        let generation = super::super::layout::GenerationPaths::new(
            run.candidate_root().join(format!("gen-{id}")),
            &id,
        );
        generation.ensure_root().unwrap();
        fs::write(generation.db_path(), b"recovered").unwrap();
        fs::write(generation.manifest_path(), b"{}").unwrap();
        fs::write(generation.ready_path(), b"ready").unwrap();
        write_json_atomically(
            &generation.metadata_path(),
            &serde_json::json!({"schema_version":2,"generation_id":id}),
        )
        .unwrap();
        run.register_candidate(id.clone()).unwrap();
        run.mark_phase(RunPhase::Publishing, None, None).unwrap();
        let run_root = run.abandon_for_test();
        let before = observed(&domain);
        assert_eq!(
            store.cleanup().unwrap().run_recovery.publishing_recovered,
            1
        );
        assert_eq!(store.open_read().unwrap().generation_id, id);
        assert!(!run_root.exists());
        assert!(observed(&domain).counter() > before.counter());
    }

    #[test]
    fn dc_epoch_retained_observation_does_not_block_current_writer_or_reader() {
        use std::sync::mpsc;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "parked-observation-domain");
        let store = ManagedStore::new(layout);
        let active = publish_tracked_generation(&store, b"readable");
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let background_domain = domain.clone();
        let background = std::thread::spawn(move || {
            let epoch = observed(&background_domain);
            ready_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            epoch
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(store.open_read().unwrap().generation_id, active);
        let writer = store.begin_write().unwrap();
        writer.abort(None).unwrap();
        resume_tx.send(()).unwrap();
        assert_ne!(background.join().unwrap(), observed(&domain));
    }

    #[test]
    fn dc_epoch_unassociated_old_orphan_is_preserved_without_repair() {
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "legacy-orphan-domain");
        let orphan = layout.runs_root().join("run-legacy");
        fs::create_dir(&orphan).unwrap();
        fs::write(orphan.join("lease.lock"), b"").unwrap();
        fs::write(
            orphan.join("journal.json"),
            br#"{"run_id":"legacy","phase":"created"}"#,
        )
        .unwrap();
        let before = observed(&domain);
        let report =
            crate::storage::run_workspace::RunWorkspace::cleanup_orphans(layout.runs_root())
                .unwrap();
        assert_eq!(report.deleted, 0);
        assert_eq!(report.skipped_locked, 1);
        assert!(orphan.join("journal.json").is_file());
        assert!(!orphan.join(".reference-scope.json").exists());
        assert_eq!(before, observed(&domain));
    }

    #[test]
    fn dc_epoch_observer_is_noncreating_and_does_not_advance() {
        let fixture = Fixture::new();
        let untracked = ReferenceDomain::managed(&fixture.layout).unwrap();
        let before = fs::read_dir(fixture.layout.artifacts_root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert!(matches!(
            observe_reference_epoch(&untracked).unwrap(),
            EpochObservation::Pending(_)
        ));
        let after = fs::read_dir(fixture.layout.artifacts_root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(before, after);
        let (_, domain) = tracked_domain(&fixture, "observed-domain");
        let before = observed(&domain);
        assert_eq!(before.counter(), 0);
        assert_eq!(before, observed(&domain));
        assert_eq!(
            fs::metadata(domain.artifact_root().join(REFERENCE_EPOCH_FILE))
                .unwrap()
                .len(),
            REFERENCE_EPOCH_SIZE as u64
        );
    }

    #[test]
    fn dc_epoch_activity_excludes_snapshots_until_all_producers_finish() {
        let fixture = Fixture::new();
        let (_, domain) = tracked_domain(&fixture, "active-domain");
        let old = observed(&domain);
        let first = ReferenceMutation::begin_in_domain(&domain).unwrap();
        assert!(first.tracked());
        let second = ArtifactUseLease::acquire_shared(domain.artifact_root())
            .unwrap()
            .into_lock();
        assert!(
            matches!(observe_reference_epoch(&domain).unwrap(), EpochObservation::Pending(reason) if reason == "reference_activity_in_use")
        );
        drop(first);
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        drop(second);
        let new = observed(&domain);
        assert_eq!(new.counter(), old.counter() + 2);
        assert_ne!(old, new);
    }

    #[test]
    fn dc_epoch_crash_before_effect_invalidates_prior_snapshot() {
        const CHILD_ROOT: &str = "CODEBASE_GRAPH_EPOCH_CRASH_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let _mutation = ReferenceMutation::begin_existing(Path::new(&root)).unwrap();
            assert!(_mutation.tracked());
            fs::write(
                Path::new(&root).join("child-entered"),
                b"durably invalidated",
            )
            .unwrap();
            // Deliberately skip Rust destructors to reproduce producer death.
            std::process::exit(0);
        }
        let fixture = Fixture::new();
        let (_, domain) = tracked_domain(&fixture, "crash-domain");
        let old = observed(&domain);
        let module = module_path!().split_once("::").unwrap().1;
        let test_name =
            format!("{module}::dc_epoch_crash_before_effect_invalidates_prior_snapshot");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test_name, "--nocapture"])
            .env(CHILD_ROOT, domain.artifact_root())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            domain.artifact_root().join("child-entered").is_file(),
            "child branch must actually execute"
        );
        assert!(!domain
            .artifact_root()
            .join("actual-artifact-effect")
            .exists());
        assert_eq!(observed(&domain).counter(), old.counter() + 1);
    }

    #[test]
    fn dc_epoch_mismatched_producer_disables_existing_coverage_before_effects() {
        let fixture = Fixture::new();
        let (_, domain) = tracked_domain(&fixture, "mismatched-domain");
        let old = observed(&domain);
        let mut other = domain.clone();
        other.scope.address = [42; 32];
        let mutation = ReferenceMutation::begin_in_domain(&other).unwrap();
        assert!(!mutation.tracked());
        assert_eq!(mutation.pending_reason(), Some("reference_domain_mismatch"));
        fs::write(
            domain.artifact_root().join("uncovered-effect"),
            b"preserved",
        )
        .unwrap();
        drop(mutation);
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        assert_ne!(read_reference_epoch(domain.artifact_root()).unwrap(), old);
    }

    #[test]
    fn dc_epoch_missing_or_unwritable_control_never_repairs_recognized_proof() {
        let fixture = Fixture::new();
        let (_, domain) = tracked_domain(&fixture, "missing-control-domain");
        let control = domain.artifact_root().join(".use.lock");
        fs::rename(&control, control.with_extension("old-use")).unwrap();
        assert!(ReferenceMutation::begin_in_domain(&domain).is_err());
        assert!(ArtifactUseLease::acquire_shared(domain.artifact_root()).is_err());
        assert!(!control.exists());
        assert!(!matches!(
            observe_reference_epoch(&domain),
            Ok(EpochObservation::Verified(_))
        ));
    }

    #[test]
    fn dc_epoch_exhaustion_rejects_before_effects_and_releases_activity() {
        let fixture = Fixture::new();
        let (_, domain) = tracked_domain(&fixture, "exhausted-domain");
        let mut epoch = observed(&domain);
        epoch.record.counter = u64::MAX;
        let mut file = OpenOptions::new()
            .write(true)
            .open(domain.artifact_root().join(REFERENCE_EPOCH_FILE))
            .unwrap();
        file.write_all(&encode_reference_epoch(&epoch.record).unwrap())
            .unwrap();
        file.sync_all().unwrap();
        drop(file);
        let before = observed(&domain);
        let effect = domain.artifact_root().join("must-not-exist");
        let result = (|| -> Result<(), NativeError> {
            let _mutation = ReferenceMutation::begin_producing(domain.artifact_root())?;
            fs::write(&effect, b"effect")?;
            Ok(())
        })();
        assert!(result.is_err());
        assert!(!effect.exists());
        assert_eq!(observed(&domain), before);
    }

    #[test]
    fn dc_epoch_scope_writer_replacement_stays_unknown() {
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "writer-replaced-domain");
        let writer = layout.writer_lock_path();
        fs::rename(&writer, writer.with_extension("old-writer")).unwrap();
        drop(open_locked(&writer, LockMode::Exclusive).unwrap());
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        let current = ReferenceDomain::managed(&layout).unwrap();
        let mutation = ReferenceMutation::begin_in_domain(&current).unwrap();
        assert!(!mutation.tracked());
        drop(mutation);
        assert!(matches!(
            observe_reference_epoch(&current).unwrap(),
            EpochObservation::Pending(_)
        ));
    }

    #[test]
    fn dc_epoch_scoped_run_lifetime_and_orphan_removal_invalidate() {
        use crate::storage::run_workspace::RunWorkspace;
        const CHILD_ROOT: &str = "CODEBASE_GRAPH_EPOCH_ORPHAN_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let layout = ManagedLayout::new(PathBuf::from(root));
            let domain = ReferenceDomain::managed(&layout).unwrap();
            let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
            write_json_atomically(&layout.storage_root().join("created-run.json"), &run.root())
                .unwrap();
            std::process::exit(0);
        }
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "run-domain");
        let old = observed(&domain);
        let run = RunWorkspace::create_scoped(layout.runs_root(), &domain, None).unwrap();
        let path = run.root().to_owned();
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        run.finish().unwrap();
        assert!(
            path.exists(),
            "producer completion retains its terminal root for bounded collection"
        );
        assert!(observed(&domain).counter() > old.counter());
        assert_eq!(
            RunWorkspace::cleanup_orphans(layout.runs_root())
                .unwrap()
                .deleted,
            1
        );
        assert!(
            !path.exists(),
            "explicit strict compatibility cleanup still removes the terminal root"
        );
        let module = module_path!().split_once("::").unwrap().1;
        let test_name =
            format!("{module}::dc_epoch_scoped_run_lifetime_and_orphan_removal_invalidate");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test_name, "--nocapture"])
            .env(CHILD_ROOT, layout.storage_root())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let orphan: PathBuf = serde_json::from_slice(
            &fs::read(layout.storage_root().join("created-run.json")).unwrap(),
        )
        .unwrap();
        assert!(orphan.is_dir());
        let before_removal = observed(&domain);
        assert_eq!(
            RunWorkspace::cleanup_orphans(layout.runs_root())
                .unwrap()
                .deleted,
            1
        );
        assert!(!orphan.exists());
        assert!(observed(&domain).counter() > before_removal.counter());
    }

    #[derive(Clone, Copy, Debug)]
    pub(crate) enum EpochControlFault {
        Corrupt,
        Unwritable,
        Exhausted,
    }

    pub(crate) fn break_epoch_control(domain: &ReferenceDomain, fault: EpochControlFault) {
        let path = domain.artifact_root().join(REFERENCE_EPOCH_FILE);
        match fault {
            EpochControlFault::Corrupt => fs::write(path, b"interrupted epoch write").unwrap(),
            EpochControlFault::Unwritable => {
                let mut permissions = fs::metadata(&path).unwrap().permissions();
                permissions.set_readonly(true);
                fs::set_permissions(path, permissions).unwrap();
            }
            EpochControlFault::Exhausted => {
                let mut epoch = read_reference_epoch(domain.artifact_root()).unwrap();
                epoch.record.counter = u64::MAX;
                let mut file = OpenOptions::new().write(true).open(path).unwrap();
                file.write_all(&encode_reference_epoch(&epoch.record).unwrap())
                    .unwrap();
                file.sync_all().unwrap();
            }
        }
    }

    #[test]
    fn dc_epoch_optional_managed_cleanup_preserves_committed_read_and_residue() {
        for fault in [
            EpochControlFault::Corrupt,
            EpochControlFault::Unwritable,
            EpochControlFault::Exhausted,
        ] {
            let fixture = Fixture::new();
            let (layout, domain) = tracked_domain(&fixture, "optional-managed-domain");
            let store = ManagedStore::new(layout.clone());
            let old = publish_tracked_generation(&store, b"old committed graph");
            let retained = store.open_read().unwrap();
            let active = publish_tracked_generation(&store, b"current committed graph");
            let old_path = layout.generation(&old).unwrap().db_path();
            let old_identity = path_identity(&old_path).unwrap();
            let epoch_path = domain.artifact_root().join(REFERENCE_EPOCH_FILE);
            let permissions = fs::metadata(&epoch_path).unwrap().permissions();
            break_epoch_control(&domain, fault);
            let epoch_bytes = fs::read(&epoch_path).unwrap();
            let report = store.cleanup().unwrap();
            assert_eq!(report.retired_generations_pending, 1, "{fault:?}");
            assert_eq!(report.retired_generations_deleted, 0, "{fault:?}");
            assert_eq!(
                store.open_read().unwrap().generation_id,
                active,
                "{fault:?}"
            );
            assert_eq!(fs::read(&old_path).unwrap(), b"old committed graph");
            assert_eq!(path_identity(&old_path).unwrap(), old_identity);
            assert!(
                ReferenceMutation::begin_in_domain(&domain).is_err(),
                "producer must reject {fault:?}"
            );
            assert_eq!(fs::read(&epoch_path).unwrap(), epoch_bytes);
            drop(retained);
            assert_eq!(path_identity(&old_path).unwrap(), old_identity);
            fs::set_permissions(epoch_path, permissions).unwrap();
        }
    }

    #[test]
    fn dc_epoch_optional_direct_cleanup_preserves_committed_read_but_not_journal_replay() {
        use crate::storage::direct::{DirectPublishJournal, DirectPublishPhase};
        for fault in [
            EpochControlFault::Corrupt,
            EpochControlFault::Unwritable,
            EpochControlFault::Exhausted,
        ] {
            let fixture = Fixture::new();
            let parent = fixture.root.join("optional-direct-domain");
            let layout = DirectLayout::new(parent.join("graph.ldb"), parent.join("manifest.json"));
            let store = DirectStore::new(layout.clone()).unwrap();
            let mut writer = store.begin_write().unwrap();
            fs::write(writer.db_candidate_path(), b"committed graph").unwrap();
            fs::write(writer.manifest_candidate_path(), b"{}").unwrap();
            writer.publish().unwrap();
            writer.finish();
            let domain = ReferenceDomain::direct(&layout).unwrap();
            let mutation = ReferenceMutation::begin_in_domain(&domain).unwrap();
            fs::write(layout.db_candidate_path(), b"unpublished residue").unwrap();
            fs::write(layout.manifest_candidate_path(), b"{}").unwrap();
            drop(mutation);
            let candidate_identity = path_identity(&layout.db_candidate_path()).unwrap();
            let database_identity = path_identity(layout.db_path()).unwrap();
            let epoch_path = domain.artifact_root().join(REFERENCE_EPOCH_FILE);
            let permissions = fs::metadata(&epoch_path).unwrap().permissions();
            break_epoch_control(&domain, fault);
            let epoch_bytes = fs::read(&epoch_path).unwrap();
            assert!(!layout.journal_path().exists());
            drop(store.begin_read().unwrap());
            assert!(store.candidate_cleanup_pending(), "{fault:?}");
            assert_eq!(
                path_identity(&layout.db_candidate_path()).unwrap(),
                candidate_identity
            );
            assert_eq!(
                fs::read(layout.db_candidate_path()).unwrap(),
                b"unpublished residue"
            );
            assert_eq!(path_identity(layout.db_path()).unwrap(), database_identity);
            assert_eq!(fs::read(layout.db_path()).unwrap(), b"committed graph");
            assert!(
                store.begin_write().is_err(),
                "producer must reject {fault:?}"
            );
            let journal = DirectPublishJournal {
                phase: DirectPublishPhase::Prepared,
                db_path: layout.db_path().to_owned(),
                db_candidate_path: layout.db_candidate_path(),
                manifest_path: layout.manifest_path().to_owned(),
                manifest_candidate_path: layout.manifest_candidate_path(),
                db_sha256: format!("{:x}", Sha256::digest(b"unpublished residue")),
                manifest_sha256: format!("{:x}", Sha256::digest(b"{}")),
                sidecar_sha256: std::collections::BTreeMap::new(),
            };
            write_json_atomically(&layout.journal_path(), &journal).unwrap();
            let journal_identity = path_identity(&layout.journal_path()).unwrap();
            assert!(
                store.begin_read().is_err(),
                "mandatory replay must reject {fault:?}"
            );
            assert_eq!(
                path_identity(&layout.journal_path()).unwrap(),
                journal_identity
            );
            assert_eq!(
                path_identity(&layout.db_candidate_path()).unwrap(),
                candidate_identity
            );
            assert_eq!(path_identity(layout.db_path()).unwrap(), database_identity);
            assert_eq!(
                fs::read(domain.artifact_root().join(REFERENCE_EPOCH_FILE)).unwrap(),
                epoch_bytes
            );
            fs::set_permissions(epoch_path, permissions).unwrap();
        }
    }

    #[test]
    fn dc_epoch_collector_composes_owned_exclusive_root_without_reacquisition() {
        let fixture = Fixture::new();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        let mutation = lease.begin_reference_mutation().unwrap();
        assert!(!mutation.tracked());
        assert_eq!(mutation.pending_reason(), Some("reference_epoch_missing"));
        drop(mutation);
        drop(lease);
    }

    #[test]
    fn dc_s2_family_replaced_parent_incarnation_invalidates_held_use() {
        let fixture = Fixture::new();
        let parent = fixture.root.join("replaceable-parent");
        fs::create_dir(&parent).unwrap();
        let database = parent.join("graph.ldb");
        let sentinel = parent.join("public-sentinel");
        fs::write(&sentinel, b"original public data").unwrap();
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        let detached = fixture.root.join("detached-parent");
        if let Err(error) = fs::rename(&parent, &detached) {
            #[cfg(windows)]
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                // Windows can reject renaming a directory with open descendant
                // handles. Prove that this native prevention leaves the public
                // incarnation and actual family exclusion intact; do not skip.
                assert_eq!(directory_identity(&parent).unwrap(), shared.parent_identity);
                assert_eq!(fs::read(&sentinel).unwrap(), b"original public data");
                assert!(!database.exists());
                assert!(!detached.exists());
                shared.revalidate().unwrap();
                assert!(DatabaseUseLease::try_exclusive(&database)
                    .unwrap()
                    .is_none());
                let other = fixture.root.join("independent-parent");
                fs::create_dir(&other).unwrap();
                let independent = DatabaseUseLease::try_exclusive(&other.join("graph.ldb"))
                    .unwrap()
                    .unwrap();
                drop(independent);
                drop(shared);
                assert!(DatabaseUseLease::try_exclusive(&database)
                    .unwrap()
                    .is_some());
                return;
            }
            panic!("unexpected parent rename failure: {error}");
        }
        assert_eq!(
            fs::read(detached.join("public-sentinel")).unwrap(),
            b"original public data"
        );
        fs::create_dir(&parent).unwrap();
        fs::write(&database, b"fresh incarnation").unwrap();
        assert!(shared.revalidate().is_err());
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_some());
    }

    #[test]
    fn dc_s2_family_group_deduplicates_same_parent_and_retains_all_guards() {
        let fixture = Fixture::new();
        let database = fixture.root.join("same-parent.ldb");
        let manifest = fixture.root.join("same-parent.json");
        fs::write(&database, b"db").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        let mut group = DatabaseUseLease::try_exclusive_many(&[&database, &manifest])
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&group[0].family, &group[1].family));
        let first = group[0].exclusive_proof().unwrap();
        let second = group[1].exclusive_proof().unwrap();
        assert_eq!(first.family_coordination, second.family_coordination);
        assert_ne!(first.coordination, second.coordination);
        // Dropping one resource cannot release its sibling's family authority.
        drop(group.remove(0));
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        drop(group);
        assert!(
            DatabaseUseLease::try_exclusive_many(&[&database, &manifest])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn dc_s2_family_group_handles_distinct_parents_and_duplicate_resource_keys() {
        let fixture = Fixture::new();
        let first = fixture.root.join("first");
        let second = fixture.root.join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let database = first.join("db.ldb");
        let manifest = second.join("manifest.json");
        fs::write(&database, b"db").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        let group = DatabaseUseLease::try_exclusive_many(&[&database, &manifest, &database])
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&group[0].family, &group[2].family));
        assert!(Arc::ptr_eq(&group[0].lock, &group[2].lock));
        assert!(!Arc::ptr_eq(&group[0].family, &group[1].family));
        assert_ne!(
            group[0].exclusive_proof().unwrap().family_coordination,
            group[1].exclusive_proof().unwrap().family_coordination
        );
    }

    #[test]
    fn dc_s2_family_producer_conversion_holds_parent_until_session_release() {
        let fixture = Fixture::new();
        let database = fixture.root.join("producer.ldb");
        let unrelated = fixture.root.join("other-name.ldb");
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        let converted = shared.into_lock().unwrap();
        assert!(DatabaseUseLease::try_exclusive(&unrelated)
            .unwrap()
            .is_none());
        // Shared operations in the same family continue to coexist.
        drop(DatabaseUseLease::acquire_shared(&unrelated).unwrap());
        fs::write(&database, b"created while owned").unwrap();
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        drop(converted);
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_some());
    }

    #[test]
    fn dc_s2_family_recreated_coordination_invalidates_destination_proof() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let lease = acquired(claim_maintenance(&target).unwrap());
        let record = lease.cohort_record().unwrap();
        write_json_atomically(&lease.cohort_record_path(), &record).unwrap();
        assert!(lease.cohort_verified().unwrap());
        let family = layout
            .db_path()
            .parent()
            .unwrap()
            .join(".database-use-family.lock");
        drop(lease);
        fs::rename(&family, family.with_extension("previous-family")).unwrap();
        drop(open_locked(&family, LockMode::Shared).unwrap());
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(!lease.cohort_verified().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_family_shared_existing_readable_coordination_requires_no_write_permission() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let database = fixture.root.join("owned.ldb");
        let unrelated = fixture.root.join("other-user.ldb");
        drop(DatabaseUseLease::acquire_shared(&database).unwrap());
        let family = fixture.root.join(".database-use-family.lock");
        fs::set_permissions(&family, fs::Permissions::from_mode(0o444)).unwrap();
        let shared = DatabaseUseLease::acquire_shared(&unrelated).unwrap();
        assert_eq!(shared.family.mode(), LockMode::Shared);
        drop(shared);
        fs::set_permissions(family, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_family_windows_shared_readonly_coordination_requires_no_write_permission() {
        let fixture = Fixture::new();
        let database = fixture.root.join("owned.ldb");
        let unrelated = fixture.root.join("other-user.ldb");
        drop(DatabaseUseLease::acquire_shared(&database).unwrap());
        let family = fixture.root.join(".database-use-family.lock");
        let mut permissions = fs::metadata(&family).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&family, permissions).unwrap();
        let result = DatabaseUseLease::acquire_shared(&unrelated);
        let mut permissions = fs::metadata(&family).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&family, permissions).unwrap();
        assert!(result.is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dc_s2_family_macos_unicode_nfc_nfd_existing_and_creation_aliases_exclude() {
        let fixture = Fixture::new();
        let database = fixture.root.join("caf\u{e9}.ldb");
        let alias = fixture.root.join("cafe\u{301}.ldb");
        // Guard the absent resource through creation under another spelling.
        let shared = DatabaseUseLease::acquire_shared(&alias).unwrap();
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        fs::write(&database, b"database").unwrap();
        assert_eq!(
            path_identity(&database).unwrap(),
            path_identity(&alias).unwrap()
        );
        assert_eq!(
            super::super::locks::file_link_count(&fs::File::open(&alias).unwrap()).unwrap(),
            1
        );
        assert!(DatabaseUseLease::try_exclusive(&database)
            .unwrap()
            .is_none());
        drop(shared);
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_none());
        drop(shared);
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dc_s2_family_macos_long_s_consumer_blocks_ascii_direct_even_with_valid_record() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let lease = acquired(claim_maintenance(&target).unwrap());
        write_json_atomically(&lease.cohort_record_path(), &lease.cohort_record().unwrap())
            .unwrap();
        assert!(lease.cohort_verified().unwrap());
        drop(lease);
        let name = layout.db_path().file_name().unwrap().to_str().unwrap();
        assert!(name.contains('s'));
        let alias = layout
            .db_path()
            .with_file_name(name.replacen('s', "\u{17f}", 1));
        assert_eq!(
            path_identity(&alias).unwrap(),
            path_identity(layout.db_path()).unwrap()
        );
        let shared = DatabaseUseLease::acquire_shared(&alias).unwrap();
        assert!(DatabaseUseLease::try_exclusive(layout.db_path())
            .unwrap()
            .is_none());
        assert!(
            matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Deferred(reason) if reason == "database_parent_in_use")
        );
        drop(shared);
        assert!(acquired(claim_maintenance(&target).unwrap())
            .cohort_verified()
            .unwrap());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dc_s2_family_macos_parent_aliases_deduplicate_physical_family() {
        let fixture = Fixture::new();
        let parent = fixture.root.join("caf\u{e9}");
        let alias = fixture.root.join("cafe\u{301}");
        fs::create_dir(&parent).unwrap();
        let database = parent.join("db.ldb");
        let manifest = alias.join("manifest.json");
        fs::write(&database, b"db").unwrap();
        fs::write(&manifest, b"{}").unwrap();
        assert_eq!(
            path_identity(&parent).unwrap(),
            path_identity(&alias).unwrap()
        );
        let group = DatabaseUseLease::try_exclusive_many(&[&database, &manifest])
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&group[0].family, &group[1].family));
    }

    #[test]
    fn dc_s2_review_managed_read_resolution_completes_with_paused_collector() {
        use std::sync::mpsc;
        let fixture = Fixture::new();
        let generation = fixture.layout.generation("readable").unwrap();
        generation.ensure_root().unwrap();
        fs::write(generation.db_path(), b"committed database fixture").unwrap();
        fs::write(generation.manifest_path(), br#"{"files":{}}"#).unwrap();
        fs::write(generation.ready_path(), b"").unwrap();
        write_json_atomically(
            &generation.metadata_path(),
            &serde_json::json!({"schema_version":2,"generation_id":"readable"}),
        )
        .unwrap();
        write_json_atomically(
            &fixture.layout.active_pointer_path(),
            &serde_json::json!({"schema_version":2,"generation_id":"readable"}),
        )
        .unwrap();
        let collector = acquired(claim_maintenance(&fixture.target).unwrap());
        let store = ManagedStore::new(fixture.layout.clone());
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let result = store
                .cleanup()
                .and_then(|_| store.resolve_active_read())
                .map(|snapshot| snapshot.map(|snapshot| snapshot.generation_id));
            tx.send(result).unwrap();
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2))
                .expect("reader must finish while collector remains paused")
                .unwrap(),
            Some("readable".into())
        );
        assert!(collector.artifact_cohort_verified().unwrap());
        drop(collector);
        reader.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_review_dangling_worker_state_is_unknown_and_preserved() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let state = fixture.layout.worker_state_path();
        symlink(fixture.root.join("missing-worker-evidence"), &state).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(reason) if reason == "worker_state_identity_unknown")
        );
        assert!(fs::symlink_metadata(state)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_review_unreadable_worker_state_is_unknown_and_preserved() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let state = fixture.layout.worker_state_path();
        fs::write(&state, b"unreadable or invalid containment evidence").unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o0)).unwrap();
        let observed = claim_maintenance(&fixture.target).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            matches!(observed, MaintenanceAdmission::Unknown(reason) if reason == "worker_state_unreadable" || reason == "worker_state_identity_unknown")
        );
        assert_eq!(
            fs::read(state).unwrap(),
            b"unreadable or invalid containment evidence"
        );
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_review_unreadable_windows_worker_state_is_unknown_and_preserved() {
        use std::os::windows::fs::OpenOptionsExt;
        let fixture = Fixture::new();
        let state = fixture.layout.worker_state_path();
        fs::write(&state, b"unavailable containment evidence").unwrap();
        let exclusive = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&state)
            .unwrap();
        assert!(matches!(
            claim_maintenance(&fixture.target).unwrap(),
            MaintenanceAdmission::Unknown(_)
        ));
        drop(exclusive);
        assert!(state.is_file());
    }

    #[test]
    fn dc_s2_review_legacy_database_new_cache_requires_destination_migration_proof() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        // The legacy database existed before this cache was created.
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert!(!lease.cohort_verified().unwrap());
        let record = lease.cohort_record().unwrap();
        assert_eq!(record.resource_use.len(), 2);
        let path = lease.cohort_record_path();
        write_json_atomically(&path, &record).unwrap();
        assert!(lease.cohort_verified().unwrap());
        drop(lease);
        // A recreated use-lock inode invalidates otherwise identical file proof.
        let (_, use_path) = database_lock_path(layout.db_path(), false).unwrap();
        fs::rename(&use_path, use_path.with_extension("old-use-lock")).unwrap();
        drop(open_locked(&use_path, LockMode::Shared).unwrap());
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(!lease.cohort_verified().unwrap());
        write_json_atomically(&path, &lease.cohort_record().unwrap()).unwrap();
        assert!(lease.cohort_verified().unwrap());
        drop(lease);
        let old = fixture.target.state_root.join("old-database-incarnation");
        fs::rename(layout.db_path(), old).unwrap();
        fs::write(layout.db_path(), b"new database incarnation").unwrap();
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(!lease.cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_review_publication_proof_does_not_bless_legacy_destination_or_sibling() {
        let fixture = Fixture::new();
        let collector = acquired(claim_maintenance(&fixture.target).unwrap());
        let mut use_leases = Vec::new();
        for id in ["fresh", "legacy"] {
            let generation = fixture.layout.generation(id).unwrap();
            generation.ensure_root().unwrap();
            fs::write(generation.db_path(), id).unwrap();
            fs::write(generation.manifest_path(), br#"{"files":{}}"#).unwrap();
            let group = DatabaseUseLease::try_exclusive_many(&[
                &generation.db_path(),
                &generation.manifest_path(),
            ])
            .unwrap()
            .unwrap();
            let mut group = group.into_iter();
            use_leases.push((group.next().unwrap(), group.next().unwrap()));
        }
        let (fresh_db, fresh_manifest) = &use_leases[0];
        let publication = collector
            .publication_cohort_record(fresh_db, fresh_manifest)
            .unwrap();
        assert!(collector
            .publication_cohort_verified(&publication, fresh_db, fresh_manifest)
            .unwrap());
        let (legacy_db, legacy_manifest) = &use_leases[1];
        assert!(!collector
            .publication_cohort_verified(&publication, legacy_db, legacy_manifest)
            .unwrap());
        write_json_atomically(&collector.cohort_record_path(), &publication).unwrap();
        assert!(!collector.cohort_verified().unwrap());
        assert_eq!(
            serde_json::to_value(publication).unwrap()["origin"],
            "guarded_publication"
        );
        assert_eq!(
            serde_json::to_value(collector.cohort_record().unwrap()).unwrap()["origin"],
            "controlled_migration"
        );
        drop(collector);
        drop(use_leases);
        let (target, _) = fixture.direct();
        let direct = acquired(claim_maintenance(&target).unwrap());
        let publication = direct.direct_publication_cohort_record().unwrap();
        assert!(direct
            .direct_publication_cohort_verified(&publication)
            .unwrap());
        write_json_atomically(&direct.cohort_record_path(), &publication).unwrap();
        assert!(!direct.cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_publication_nonce_proof_is_optional_but_never_numeric_only() {
        for mode in ["untracked", "corrupt", "recycled", "tracked"] {
            let fixture = Fixture::new();
            let (target, layout) = fixture.direct();
            let lease = acquired(claim_maintenance(&target).unwrap());
            let record = lease.direct_publication_cohort_record().unwrap();
            assert!(lease.direct_publication_cohort_verified(&record).unwrap());
            let mut legacy = record.clone();
            legacy.version = 1;
            assert!(!lease.direct_publication_cohort_verified(&legacy).unwrap());
            let mut missing = record.clone();
            missing.artifact_incarnation = None;
            assert!(!lease.direct_publication_cohort_verified(&missing).unwrap());
            let root = lease.artifact.root.clone();
            drop(lease);
            let path = root.join(".use.lock");
            let bytes = match mode {
                "untracked" => Vec::new(),
                "corrupt" => {
                    let mut bytes = fs::read(&path).unwrap();
                    bytes[9] ^= 1;
                    bytes
                }
                "recycled" => {
                    let mut nonce = record.artifact_incarnation.unwrap();
                    nonce[0] ^= 1;
                    super::super::locks::encode_coordination_incarnation(nonce).to_vec()
                }
                _ => fs::read(&path).unwrap(),
            };
            fs::write(&path, &bytes).unwrap();
            // Same numeric inode and primary files remain usable: an optional
            // cleanup proof cannot introduce a mandatory publication failure.
            assert_eq!(path_identity(&path).unwrap(), record.artifact_coordination);
            let lease = acquired(claim_maintenance(&target).unwrap());
            let informational = lease.direct_publication_cohort_record().unwrap();
            assert_eq!(
                lease.direct_publication_cohort_verified(&record).unwrap(),
                mode == "tracked"
            );
            assert_eq!(
                lease
                    .direct_publication_cohort_verified(&informational)
                    .unwrap(),
                mode == "tracked"
            );
            if matches!(mode, "untracked" | "corrupt") {
                assert!(informational.artifact_incarnation.is_none());
                assert!(!lease
                    .direct_publication_cohort_verified(&informational)
                    .unwrap());
            }
            drop(lease);
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(layout.db_path().is_file());
            assert!(layout.manifest_path().is_file());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dc_s2_review_linux_case_only_distinct_registrations_remain_distinct() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let upper = layout.db_path().with_file_name(
            layout
                .db_path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_uppercase(),
        );
        fs::write(&upper, b"distinct case-sensitive database").unwrap();
        assert_ne!(
            path_identity(&upper).unwrap(),
            path_identity(layout.db_path()).unwrap()
        );
        let other_manifest = fixture.target.state_root.join("other-manifest.json");
        fs::write(&other_manifest, b"{}").unwrap();
        register_direct_pair(&DirectLayout::new(&upper, &other_manifest)).unwrap();
        assert!(!resources_overlap(&upper, layout.db_path()));
        assert!(matches!(
            claim_maintenance(&target).unwrap(),
            MaintenanceAdmission::Acquired(_)
        ));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn dc_s2_review_hardlinked_coordination_invalidates_held_authority() {
        let fixture = Fixture::new();
        let path = fixture.layout.writer_lock_path();
        let writer = open_locked(&path, LockMode::Exclusive).unwrap();
        let alias = fixture.root.join("held-writer-alias.lock");
        fs::hard_link(&path, &alias).unwrap();
        assert!(!writer.verifies(&path, LockMode::Exclusive).unwrap());
        assert!(try_open_existing_locked(&alias, LockMode::Exclusive).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_review_windows_case_alias_shares_resource_coordination() {
        let fixture = Fixture::new();
        let database = fixture.root.join("CaseOnly.ldb");
        let alias = fixture.root.join("caseonly.LDB");
        fs::write(&database, b"database").unwrap();
        assert_eq!(
            path_identity(&database).unwrap(),
            path_identity(&alias).unwrap()
        );
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_none());
        drop(shared);
        assert!(DatabaseUseLease::try_exclusive(&alias).unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    fn dc_s2_review_windows_resource_and_coordination_hardlinks_are_rejected() {
        let fixture = Fixture::new();
        let database = fixture.root.join("native-windows.ldb");
        fs::write(&database, b"database").unwrap();
        let alias = fixture.root.join("native-windows-alias.ldb");
        fs::hard_link(&database, &alias).unwrap();
        assert!(DatabaseUseLease::acquire_shared(&database).is_err());
        assert!(DatabaseUseLease::try_exclusive(&alias).is_err());
        let writer_path = fixture.layout.writer_lock_path();
        let writer = open_locked(&writer_path, LockMode::Exclusive).unwrap();
        let lock_alias = fixture.root.join("writer-alias.lock");
        fs::hard_link(&writer_path, &lock_alias).unwrap();
        assert!(!writer.verifies(&writer_path, LockMode::Exclusive).unwrap());
        assert!(try_open_existing_locked(&lock_alias, LockMode::Exclusive).is_err());
    }

    #[test]
    fn dc_s2_shared_manifest_registration_across_database_parents_is_retained() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let other_parent = fixture.root.join("other-db-parent");
        fs::create_dir(&other_parent).unwrap();
        let alternate = DirectLayout::new(other_parent.join("other.ldb"), layout.manifest_path());
        register_direct_pair(&alternate).unwrap();
        assert!(
            matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "direct_registered_pair_overlap")
        );
    }

    #[test]
    fn dc_s2_external_bound_managed_store_does_not_require_local_state() {
        let fixture = Fixture::new();
        let external = ManagedLayout::new(fixture.root.join("external-storage"));
        external.ensure_roots().unwrap();
        drop(open_locked(external.writer_lock_path(), LockMode::Exclusive).unwrap());
        let config_path = fixture.root.join("external-config.json");
        write_json_atomically(&config_path, &serde_json::json!({"schema_version":2, "repo_root": fixture.root, "storage_root":external.storage_root()})).unwrap();
        fs::remove_dir_all(&fixture.target.state_root).unwrap();
        let mut target = fixture.target.clone();
        target.config_path = Some(config_path);
        target.storage_root = Some(external.storage_root().to_owned());
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert!(!fixture.target.state_root.exists());
    }

    #[test]
    fn dc_s2_current_v2_receipt_and_unknown_v1_follow_existing_guardian_policy() {
        use crate::storage::containment::{ContainmentProof, ExitOutcome};
        let fixture = Fixture::new();
        let lifecycle = fixture
            .layout
            .phase_workspace_root()
            .join("phase-1/.lifecycle-complete");
        fs::create_dir_all(&lifecycle).unwrap();
        drop(open_locked(lifecycle.join("lease"), LockMode::Exclusive).unwrap());
        let descriptor = BuildDescriptorV2 {
            version: 2,
            build_id: "build".into(),
            lifecycle_dir: lifecycle.clone(),
            endpoint: "unused".into(),
            nonce: "unused".into(),
            worker_pid: std::process::id(),
            guardian_pid: std::process::id(),
            proof: ContainmentProof::Pending,
        };
        write_json_atomically(&lifecycle.join("proof.json"), &descriptor).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "phase_containment_unknown")
        );
        let receipt = ExitOutcome {
            success: true,
            code: Some(0),
            signal: None,
            high_water_bytes: 0,
            budget_reason: None,
            cancelled: false,
            containment_empty: true,
            supervision_error: None,
        };
        write_json_atomically(&lifecycle.join("receipt.json"), &receipt).unwrap();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(lifecycle.join("proof.json").exists());
        drop(lease);
        write_json_atomically(
            &fixture.layout.worker_state_path(),
            &serde_json::json!({"version":1,"pid":std::process::id()}),
        )
        .unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "legacy_quiescence_required")
        );
        assert!(fixture.layout.worker_state_path().exists());
    }

    #[test]
    fn dc_s2_waiter_on_detached_incarnation_cannot_write_fresh_public_root() {
        use std::sync::mpsc;
        let fixture = Fixture::new();
        let root = fixture.layout.artifacts_root();
        let exclusive = open_locked(root.join(".use.lock"), LockMode::Exclusive).unwrap();
        let (opened_tx, opened_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel();
        let producer_root = root.clone();
        let producer = std::thread::spawn(move || {
            super::super::locks::before_lock_test_hook(move || {
                opened_tx.send(()).unwrap();
                start_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            });
            ArtifactUseLease::acquire_shared(&producer_root)
        });
        opened_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let detached = fixture.layout.storage_root().join("detached");
        let moved = fs::rename(&root, &detached);
        #[cfg(windows)]
        {
            // An open descendant prevents directory replacement on Windows.
            // Exercise that real exclusion and let the waiting producer use
            // only the unchanged physical root after its owner releases it.
            let error = moved.unwrap_err();
            assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
            let identity = path_identity(&root).unwrap();
            assert!(!detached.exists());
            start_tx.send(()).unwrap();
            drop(exclusive);
            let lease = producer.join().unwrap().unwrap();
            lease.revalidate().unwrap();
            assert_eq!(lease.identity, identity);
        }
        #[cfg(not(windows))]
        {
            moved.unwrap();
            drop(ArtifactUseLease::acquire_shared(&root).unwrap());
            start_tx.send(()).unwrap();
            drop(exclusive);
            assert!(producer.join().unwrap().is_err());
            assert!(observe_artifact_cohort(&root).unwrap().is_some());
        }
    }

    #[test]
    fn dc_s2_worker_launch_evidence_without_lifecycle_is_unknown() {
        let fixture = Fixture::new();
        let root = fixture
            .layout
            .storage_root()
            .join("workers/worker-incomplete");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("request.json"), b"execution evidence").unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "phase_execution_evidence_without_lifecycle")
        );
        assert!(root.join("request.json").exists());
    }

    #[test]
    fn dc_s2_exact_borrowed_writer_requires_exclusive_exact_destination() {
        let fixture = Fixture::new();
        let writer = open_locked(fixture.layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let lease = acquired(claim_maintenance_with_owned(&fixture.target, &writer).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert_eq!(lease.target(), &fixture.target);
        drop(lease);
        drop(writer);
        let shared = open_locked(fixture.layout.writer_lock_path(), LockMode::Shared).unwrap();
        assert!(claim_maintenance_with_owned(&fixture.target, &shared).is_err());
        let foreign = open_locked(fixture.root.join("foreign.lock"), LockMode::Exclusive).unwrap();
        assert!(claim_maintenance_with_owned(&fixture.target, &foreign).is_err());
    }

    #[test]
    fn dc_s2_writer_worker_and_startup_contention_are_nonblocking() {
        let fixture = Fixture::new();
        for (path, reason) in [
            (fixture.layout.writer_lock_path(), "writer_in_use"),
            (fixture.layout.worker_lock_path(), "worker_in_use"),
        ] {
            let held = open_locked(path, LockMode::Exclusive).unwrap();
            let started = Instant::now();
            assert!(
                matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Deferred(r) if r == reason)
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(held);
        }
        let startup = fixture
            .layout
            .phase_workspace_root()
            .join(".lifecycle-starting");
        fs::create_dir_all(&startup).unwrap();
        let held = open_locked(startup.join("lease"), LockMode::Exclusive).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Deferred(r) if r == "startup_in_use")
        );
        drop(held);
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(
            try_open_existing_locked(&startup.join("lease"), LockMode::Exclusive)
                .unwrap()
                .is_none()
        );
        drop(lease);
    }

    #[test]
    fn dc_s2_missing_startup_lease_keeps_unknown_evidence() {
        let fixture = Fixture::new();
        let startup = fixture
            .layout
            .phase_workspace_root()
            .join(".lifecycle-unknown");
        fs::create_dir_all(&startup).unwrap();
        fs::write(startup.join("request.json"), b"launch evidence").unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Unknown(r) if r.contains("startup_lease_unknown"))
        );
        assert_eq!(
            fs::read(startup.join("request.json")).unwrap(),
            b"launch evidence"
        );
        assert!(!startup.join("lease").exists());
    }

    #[test]
    fn dc_s2_owned_state_transition_never_waits_or_holds_state_during_validation() {
        let fixture = Fixture::new();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        let state = open_locked(fixture.layout.state_lock_path(), LockMode::Shared).unwrap();
        assert_eq!(lease.with_state_lock(|| Ok(7)).unwrap(), None);
        drop(state);
        assert_eq!(lease.with_state_lock(|| Ok(7)).unwrap(), Some(7));
    }

    #[test]
    fn dc_s2_producer_and_collector_exclude_whole_artifact_operation() {
        let fixture = Fixture::new();
        let producer = ArtifactUseLease::acquire_shared(&fixture.layout.artifacts_root()).unwrap();
        assert!(
            matches!(claim_maintenance(&fixture.target).unwrap(), MaintenanceAdmission::Deferred(r) if r == "artifact_root_in_use")
        );
        fs::write(
            fixture.layout.artifacts_root().join("produced"),
            b"still protected",
        )
        .unwrap();
        assert!(
            ArtifactUseLease::try_exclusive(&fixture.layout.artifacts_root())
                .unwrap()
                .is_none()
        );
        drop(producer);
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(try_open_existing_locked(
            &fixture.layout.artifacts_root().join(".use.lock"),
            LockMode::Shared
        )
        .unwrap()
        .is_none());
        drop(lease);
    }

    #[test]
    fn dc_s2_direct_reader_and_alternate_manifest_exclude_collector() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let reader = DirectStore::new(layout.clone())
            .unwrap()
            .begin_read()
            .unwrap();
        assert!(
            matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Deferred(r) if r == "writer_in_use")
        );
        let alternate = DirectLayout::new(
            layout.db_path(),
            fixture.target.state_root.join("other-manifest.json"),
        );
        let mut alt_target = target.clone();
        alt_target.direct_pair = Some(DirectPair {
            database: alternate.db_path().to_owned(),
            manifest: alternate.manifest_path().to_owned(),
        });
        drop(ArtifactUseLease::acquire_shared(&alternate.artifact_root_path()).unwrap());
        assert!(
            matches!(claim_maintenance(&alt_target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "direct_registered_pair_overlap")
        );
        assert!(DatabaseUseLease::try_exclusive(layout.db_path())
            .unwrap()
            .is_none());
        drop(reader);
        assert!(DatabaseUseLease::try_exclusive(layout.db_path())
            .unwrap()
            .is_some());
    }

    #[test]
    fn dc_s2_direct_borrowed_session_truthfully_defers_active_companions() {
        let fixture = Fixture::new();
        let (target, layout) = fixture.direct();
        let session = DirectStore::new(layout).unwrap().begin_write().unwrap();
        let started = Instant::now();
        assert!(
            matches!(claim_maintenance_with_owned(&target, session.writer_lease().unwrap()).unwrap(), MaintenanceAdmission::Deferred(r) if r == "database_parent_in_use")
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(session);
        let lease = acquired(claim_maintenance(&target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_managed_borrowed_session_defers_active_reference_use_without_deadlock() {
        let fixture = Fixture::new();
        let session = ManagedStore::new(fixture.layout.clone())
            .begin_write()
            .unwrap();
        let started = Instant::now();
        assert!(
            matches!(claim_maintenance_with_owned(&fixture.target, session.writer_lease().unwrap()).unwrap(), MaintenanceAdmission::Deferred(reason) if reason == "artifact_root_in_use")
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(session);
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_registered_shared_manifest_and_shared_database_are_retained() {
        for shared_database in [true, false] {
            let fixture = Fixture::new();
            let (target, layout) = fixture.direct();
            let alternate = if shared_database {
                DirectLayout::new(
                    layout.db_path(),
                    fixture.target.state_root.join("alternate.json"),
                )
            } else {
                DirectLayout::new(
                    fixture.target.state_root.join("alternate.ldb"),
                    layout.manifest_path(),
                )
            };
            register_direct_pair(&alternate).unwrap();
            assert!(
                matches!(claim_maintenance(&target).unwrap(), MaintenanceAdmission::Unknown(r) if r == "direct_registered_pair_overlap")
            );
        }
    }

    #[test]
    fn dc_s2_guarded_creation_and_controlled_proof_are_incarnation_bound() {
        let fixture = Fixture::new();
        let lease = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(lease.artifact_cohort_verified().unwrap());
        assert!(!lease.cohort_verified().unwrap());
        let record = lease.cohort_record().unwrap();
        let path = lease.cohort_record_path();
        write_json_atomically(&path, &record).unwrap();
        assert!(lease.cohort_verified().unwrap());
        let old = fixture.layout.artifacts_root();
        let artifact_record = fs::read(old.join(".use-cohort.json")).unwrap();
        let detached = fixture.layout.storage_root().join("detached-artifacts");
        #[cfg(windows)]
        {
            let error = fs::rename(&old, &detached).unwrap_err();
            assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
            lease.revalidate().unwrap();
            drop(lease);
        }
        fs::rename(&old, &detached).unwrap();
        drop(ArtifactUseLease::acquire_shared(&old).unwrap());
        #[cfg(not(windows))]
        {
            assert!(lease.revalidate().is_err());
            drop(lease);
        }
        // Copying the old proof onto a new incarnation is not authority.
        fs::write(old.join(".use-cohort.json"), artifact_record).unwrap();
        let fresh = acquired(claim_maintenance(&fixture.target).unwrap());
        assert!(!fresh.artifact_cohort_verified().unwrap());
        assert!(!fresh.cohort_verified().unwrap());
    }

    #[test]
    fn dc_s2_preexisting_root_never_becomes_modern_from_guard_acquisition() {
        let fixture = Fixture::new();
        let root = fixture.root.join("preexisting");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("legacy"), b"unknown").unwrap();
        drop(ArtifactUseLease::acquire_shared(&root).unwrap());
        assert!(!root.join(".use-cohort.json").exists());
        assert!(observe_artifact_cohort(&root).unwrap().is_none());
    }

    #[test]
    fn dc_s2_inventory_cohort_observer_is_noncreating_and_validates_lock_identity() {
        let fixture = Fixture::new();
        let root = fixture.root.join("missing");
        assert!(observe_artifact_cohort(&root).is_err());
        assert!(!root.exists());
        let root = fixture.root.join("existing");
        fs::create_dir(&root).unwrap();
        fs::write(root.join(".use-cohort.json"), b"true").unwrap();
        assert!(observe_artifact_cohort(&root).is_err());
        assert!(!root.join(".use.lock").exists());
        let root = fixture.layout.artifacts_root();
        let before = fs::read(root.join(".use-cohort.json")).unwrap();
        assert!(observe_artifact_cohort(&root).unwrap().is_some());
        assert_eq!(fs::read(root.join(".use-cohort.json")).unwrap(), before);
        fs::remove_file(root.join(".use.lock")).unwrap();
        drop(open_locked(root.join(".use.lock"), LockMode::Shared).unwrap());
        assert!(observe_artifact_cohort(&root).unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_cohort_recycled_numeric_lock_identity_does_not_restore_creation() {
        let fixture = Fixture::new();
        let root = fixture.layout.artifacts_root();
        let path = root.join(".use.lock");
        let mut record: ArtifactCohortRecord =
            serde_json::from_slice(&fs::read(root.join(".use-cohort.json")).unwrap()).unwrap();
        assert!(observe_artifact_cohort(&root).unwrap().is_some());
        fs::remove_file(&path).unwrap();
        drop(open_locked(&path, LockMode::Shared).unwrap());
        // Deterministically model inode ABA even on an allocator that did not
        // recycle this time: the stale tuple now equals the replacement tuple.
        // The creation nonce remains the old one; no old FD is retained.
        record.coordination = path_identity(&path).unwrap();
        write_json_atomically(&root.join(".use-cohort.json"), &record).unwrap();
        let before_record = fs::read(root.join(".use-cohort.json")).unwrap();
        let before_lock = fs::read(&path).unwrap();
        assert!(before_lock.is_empty());
        assert!(observe_artifact_cohort(&root).unwrap().is_none());
        assert_eq!(fs::read(&path).unwrap(), before_lock);
        assert_eq!(
            fs::read(root.join(".use-cohort.json")).unwrap(),
            before_record
        );
    }

    #[test]
    fn dc_s2_cohort_unknown_or_corrupt_creation_header_never_repairs_on_use_or_observation() {
        for corrupt in [false, true] {
            let fixture = Fixture::new();
            let root = fixture.layout.artifacts_root();
            let path = root.join(".use.lock");
            let bytes = if corrupt {
                let mut bytes = fs::read(&path).unwrap();
                bytes[9] ^= 1;
                bytes
            } else {
                Vec::new()
            };
            fs::write(&path, &bytes).unwrap();
            let before = fs::read(root.join(".use-cohort.json")).unwrap();
            assert!(observe_artifact_cohort(&root).unwrap().is_none());
            let reader = ArtifactUseLease::acquire_shared(&root).unwrap();
            assert!(!reader.cohort_verified().unwrap());
            drop(reader);
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read(root.join(".use-cohort.json")).unwrap(), before);
        }
    }

    #[test]
    fn dc_s2_fresh_creation_header_is_bound_and_really_downgraded_to_shared() {
        let fixture = Fixture::new();
        let root = fixture.root.join("fresh-nonce-root");
        let first = ArtifactUseLease::acquire_shared(&root).unwrap();
        let record: ArtifactCohortRecord =
            serde_json::from_slice(&fs::read(root.join(".use-cohort.json")).unwrap()).unwrap();
        assert_eq!(record.version, 2);
        assert_eq!(
            first.lock.coordination_incarnation().unwrap(),
            Some(record.coordination_incarnation)
        );
        let second = try_open_existing_locked(&root.join(".use.lock"), LockMode::Shared)
            .unwrap()
            .expect("actual second shared handle must be admitted");
        assert!(
            try_open_existing_locked(&root.join(".use.lock"), LockMode::Exclusive)
                .unwrap()
                .is_none()
        );
        drop(first);
        drop(second);
        assert!(observe_artifact_cohort(&root).unwrap().is_some());
    }

    #[test]
    fn dc_epoch_root_creation_nonce_is_bound_beyond_recycled_numeric_metadata() {
        let fixture = Fixture::new();
        let (_layout, domain) = tracked_domain(&fixture, "nonce-bound-epoch");
        let before = observed(&domain);
        let root = domain.artifact_root();
        let mut nonce = before.record.use_incarnation;
        nonce[0] ^= 1;
        fs::write(
            root.join(".use.lock"),
            super::super::locks::encode_coordination_incarnation(nonce),
        )
        .unwrap();
        let cohort_path = root.join(".use-cohort.json");
        let mut cohort: ArtifactCohortRecord =
            serde_json::from_slice(&fs::read(&cohort_path).unwrap()).unwrap();
        cohort.coordination_incarnation = nonce;
        // Keep the same cohort and lock inode IDs while changing their bound
        // creation generation; the old reference epoch must not become valid.
        fs::write(&cohort_path, serde_json::to_vec(&cohort).unwrap()).unwrap();
        assert_eq!(path_identity(&cohort_path).unwrap(), before.record.cohort);
        assert_eq!(
            path_identity(&root.join(".use.lock")).unwrap(),
            before.record.use_lock
        );
        assert!(matches!(
            observe_reference_epoch(&domain).unwrap(),
            EpochObservation::Pending(_)
        ));
        assert!(ReferenceMutation::begin_in_domain(&domain).is_err());
        assert_eq!(read_reference_epoch(root).unwrap(), before);
    }

    #[test]
    fn dc_s2_legacy_cohort_v1_is_not_upgraded_by_read_or_guard_acquisition() {
        let fixture = Fixture::new();
        let root = fixture.layout.artifacts_root();
        let record_path = root.join(".use-cohort.json");
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
        record["version"] = 1.into();
        record
            .as_object_mut()
            .unwrap()
            .remove("coordination_incarnation");
        fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
        let before = fs::read(&record_path).unwrap();
        assert!(observe_artifact_cohort(&root).unwrap().is_none());
        drop(ArtifactUseLease::acquire_shared(&root).unwrap());
        assert_eq!(fs::read(&record_path).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn dc_s2_hardlinks_symlinks_and_case_aliases_cannot_bypass_resource_guard() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let database = fixture.root.join("Graph.ldb");
        fs::write(&database, b"db").unwrap();
        let held = DatabaseUseLease::acquire_shared(&database).unwrap();
        assert!(
            DatabaseUseLease::try_exclusive(&fixture.root.join("graph.ldb"))
                .unwrap()
                .is_none()
        );
        let alias = fixture.root.join("alias.ldb");
        symlink(&database, &alias).unwrap();
        assert!(DatabaseUseLease::acquire_shared(&alias).is_err());
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&database, &alias).unwrap();
        assert!(DatabaseUseLease::acquire_shared(&alias).is_err());
        drop(held);
    }

    #[test]
    fn dc_s2_replaced_writer_inode_does_not_validate_borrowed_authority() {
        let fixture = Fixture::new();
        let path = fixture.layout.writer_lock_path();
        let writer = open_locked(&path, LockMode::Exclusive).unwrap();
        fs::remove_file(&path).unwrap();
        drop(open_locked(&path, LockMode::Shared).unwrap());
        assert!(claim_maintenance_with_owned(&fixture.target, &writer).is_err());
    }

    // Test-only owning preparation: all paths and budgeted buffers stay outside
    // the primitive's authority callback, including errors and panic unwind.
    struct FixedEpochPaths {
        root: PathBuf,
        destination: PathBuf,
        writer: PathBuf,
        root_use: PathBuf,
        cohort: PathBuf,
        epoch: PathBuf,
        activity: PathBuf,
        serial: PathBuf,
        references: [Option<PathBuf>; 2],
    }
    impl FixedEpochPaths {
        fn new(domain: &ReferenceDomain) -> Self {
            Self {
                root: domain.artifact_root.clone(),
                destination: domain.destination_root.clone(),
                writer: domain.writer_path.clone(),
                root_use: domain.artifact_root.join(".use.lock"),
                cohort: domain.artifact_root.join(".use-cohort.json"),
                epoch: domain.artifact_root.join(REFERENCE_EPOCH_FILE),
                activity: domain.artifact_root.join(REFERENCE_ACTIVITY_FILE),
                serial: domain.artifact_root.join(REFERENCE_SERIAL_FILE),
                references: domain.reference_roots.clone(),
            }
        }
        fn borrow(&self) -> CanonicalEpochPaths<'_> {
            CanonicalEpochPaths {
                root: &self.root,
                destination: &self.destination,
                writer: &self.writer,
                root_use: &self.root_use,
                cohort: &self.cohort,
                epoch: &self.epoch,
                activity: &self.activity,
                serial: &self.serial,
                reference_roots: [self.references[0].as_deref(), self.references[1].as_deref()],
            }
        }
    }
    struct FixedEpochBuffers {
        memory: super::super::maintenance_budget::CleanupMemory,
        scratch: super::super::locks::FixedControlScratch,
        prepared: super::super::maintenance_budget::ReservedBytes,
        read: super::super::maintenance_budget::ReservedBytes,
    }
    impl FixedEpochBuffers {
        fn new() -> Self {
            use super::super::{locks::FixedControlScratch, maintenance_budget::CleanupMemory};
            let memory = CleanupMemory::new(1024 * 1024).unwrap();
            let scratch = FixedControlScratch::new(
                memory
                    .reserve(FixedControlScratch::required_bytes())
                    .unwrap(),
            )
            .unwrap();
            let prepared = memory
                .reserve(2 * REFERENCE_EPOCH_SIZE)
                .unwrap()
                .bytes(2 * REFERENCE_EPOCH_SIZE)
                .unwrap();
            let read = memory
                .reserve(REFERENCE_EPOCH_SIZE)
                .unwrap()
                .bytes(REFERENCE_EPOCH_SIZE)
                .unwrap();
            Self {
                memory,
                scratch,
                prepared,
                read,
            }
        }
    }
    fn fixed_epoch_guards(layout: &ManagedLayout) -> (WriterLease, ArtifactUseLease) {
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let root = ArtifactUseLease::try_exclusive(&layout.artifacts_root())
            .unwrap()
            .unwrap();
        (writer, root)
    }
    fn fixed_epoch_controls_released(paths: &FixedEpochPaths) {
        for path in [&paths.activity, &paths.serial] {
            assert!(try_open_existing_locked(path, LockMode::Exclusive)
                .unwrap()
                .is_some());
        }
    }

    #[test]
    fn dc_fixed_epoch_exact_compare_precedes_increment_and_retains_real_controls() {
        use super::super::maintenance_budget::MutationGrant;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-advance");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        let (writer, root) = fixed_epoch_guards(&layout);
        let mut called = false;
        let advance = with_canonical_epoch_advance(
            &frame,
            &writer,
            &root,
            &mut buffers.read,
            &mut buffers.scratch,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
            &mut |advance| {
                called = true;
                assert_eq!(advance.before, expected.counter());
                assert_eq!(advance.after, expected.counter() + 1);
                for path in [&paths.activity, &paths.serial] {
                    assert!(try_open_existing_locked(path, LockMode::Exclusive)
                        .unwrap()
                        .is_none());
                }
                assert!(matches!(
                    observe_reference_epoch(&domain).unwrap(),
                    EpochObservation::Pending(_)
                ));
                Ok(())
            },
        )
        .unwrap();
        assert!(called);
        fixed_epoch_controls_released(&paths);
        assert!(writer
            .verifies(&layout.writer_lock_path(), LockMode::Exclusive)
            .unwrap());
        root.revalidate().unwrap();
        drop(root);
        assert_eq!(observed(&domain).counter(), advance.after);
        assert!(buffers.memory.used() <= buffers.memory.limit());
    }

    #[test]
    fn dc_fixed_epoch_stale_preparation_never_adopts_an_unrelated_increment() {
        use super::super::{
            locks::{FixedControlError, FixedControlProblem},
            maintenance_budget::MutationGrant,
        };
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-stale");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        drop(ReferenceMutation::begin_in_domain(&domain).unwrap());
        let before = fs::read(&paths.epoch).unwrap();
        let (writer, root) = fixed_epoch_guards(&layout);
        let mut called = false;
        assert!(matches!(
            with_canonical_epoch_advance(
                &frame,
                &writer,
                &root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                &mut |_| {
                    called = true;
                    Ok(())
                }
            ),
            Err(FixedControlError::Invalid(
                FixedControlProblem::EpochMismatch
            ))
        ));
        assert!(!called);
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
        fixed_epoch_controls_released(&paths);
    }

    #[test]
    fn dc_fixed_epoch_rejects_shared_writer_and_shared_root_without_reacquiring_them() {
        use super::super::{
            locks::{FixedControlError, FixedControlProblem},
            maintenance_budget::MutationGrant,
        };
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-wrong-guards");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        let shared_writer = open_locked(layout.writer_lock_path(), LockMode::Shared).unwrap();
        let root = ArtifactUseLease::try_exclusive(&layout.artifacts_root())
            .unwrap()
            .unwrap();
        let before = fs::read(&paths.epoch).unwrap();
        assert!(matches!(
            with_canonical_epoch_advance(
                &frame,
                &shared_writer,
                &root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                &mut |_| panic!("not admitted")
            ),
            Err(FixedControlError::Invalid(
                FixedControlProblem::GuardMismatch
            ))
        ));
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
        drop(root);
        drop(shared_writer);
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let shared_root = ArtifactUseLease::acquire_in_domain(&domain).unwrap();
        let before = fs::read(&paths.epoch).unwrap();
        assert!(matches!(
            with_canonical_epoch_advance(
                &frame,
                &writer,
                &shared_root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                &mut |_| panic!("not admitted")
            ),
            Err(FixedControlError::Invalid(
                FixedControlProblem::GuardMismatch
            ))
        ));
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
    }

    #[test]
    fn dc_fixed_epoch_activity_contention_returns_and_preserves_caller_ownership() {
        use super::super::{
            locks::{FixedControlError, FixedControlProblem},
            maintenance_budget::MutationGrant,
        };
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-contention");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        let (writer, root) = fixed_epoch_guards(&layout);
        let activity = try_open_existing_locked(&paths.activity, LockMode::Shared)
            .unwrap()
            .unwrap();
        let before = fs::read(&paths.epoch).unwrap();
        assert!(matches!(
            with_canonical_epoch_advance(
                &frame,
                &writer,
                &root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                &mut |_| panic!("not admitted")
            ),
            Err(FixedControlError::Invalid(FixedControlProblem::Busy))
        ));
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
        assert!(writer
            .verifies(&layout.writer_lock_path(), LockMode::Exclusive)
            .unwrap());
        root.revalidate().unwrap();
        drop(activity);
        fixed_epoch_controls_released(&paths);
    }

    #[test]
    fn dc_fixed_epoch_cohort_or_control_replacement_rejects_before_effect() {
        use super::super::maintenance_budget::MutationGrant;
        for replace_cohort in [false, true] {
            let fixture = Fixture::new();
            let (layout, domain) = tracked_domain(&fixture, "fixed-control-replaced");
            let expected = observed(&domain);
            let paths = FixedEpochPaths::new(&domain);
            let mut buffers = FixedEpochBuffers::new();
            let frame = prepare_canonical_epoch_frame(
                paths.borrow(),
                &expected,
                &mut buffers.prepared,
                &mut buffers.read,
                &mut buffers.scratch,
            )
            .unwrap();
            let target = if replace_cohort {
                &paths.cohort
            } else {
                &paths.serial
            };
            let bytes = fs::read(target).unwrap();
            fs::rename(target, target.with_extension("old")).unwrap();
            fs::write(target, bytes).unwrap();
            let before = fs::read(&paths.epoch).unwrap();
            let (writer, root) = fixed_epoch_guards(&layout);
            assert!(with_canonical_epoch_advance(
                &frame,
                &writer,
                &root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                &mut |_| panic!("not admitted")
            )
            .is_err());
            assert_eq!(fs::read(&paths.epoch).unwrap(), before);
            fixed_epoch_controls_released(&paths);
        }
    }

    #[test]
    fn dc_fixed_epoch_replaced_root_with_same_control_inodes_is_not_the_same_domain() {
        use super::super::maintenance_budget::MutationGrant;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-root-replaced");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        let (writer, root) = fixed_epoch_guards(&layout);
        let old = paths.root.with_extension("old");
        #[cfg(windows)]
        {
            let error = fs::rename(&paths.root, &old).unwrap_err();
            assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
            root.revalidate().unwrap();
            drop(root);
            drop(writer);
        }
        fs::rename(&paths.root, &old).unwrap();
        fs::create_dir(&paths.root).unwrap();
        for entry in fs::read_dir(&old).unwrap() {
            let entry = entry.unwrap();
            fs::rename(entry.path(), paths.root.join(entry.file_name())).unwrap();
        }
        #[cfg(windows)]
        let (writer, root) = fixed_epoch_guards(&layout);
        let before = fs::read(&paths.epoch).unwrap();
        assert!(with_canonical_epoch_advance(
            &frame,
            &writer,
            &root,
            &mut buffers.read,
            &mut buffers.scratch,
            &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
            &mut |_| panic!("not admitted")
        )
        .is_err());
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
        fixed_epoch_controls_released(&paths);
    }

    #[test]
    fn dc_fixed_epoch_callback_error_and_unwind_release_owned_controls_only() {
        use super::super::{
            locks::{FixedControlError, FixedControlProblem},
            maintenance_budget::MutationGrant,
        };
        for panic_in_callback in [false, true] {
            let fixture = Fixture::new();
            let (layout, domain) = tracked_domain(&fixture, "fixed-callback-exit");
            let expected = observed(&domain);
            let paths = FixedEpochPaths::new(&domain);
            let mut buffers = FixedEpochBuffers::new();
            let frame = prepare_canonical_epoch_frame(
                paths.borrow(),
                &expected,
                &mut buffers.prepared,
                &mut buffers.read,
                &mut buffers.scratch,
            )
            .unwrap();
            let (writer, root) = fixed_epoch_guards(&layout);
            let memory_before = buffers.memory.used();
            let mut callback = |_| -> Result<(), FixedControlError> {
                if panic_in_callback {
                    panic!("fixture callback unwind");
                }
                Err(FixedControlError::Invalid(
                    FixedControlProblem::InvalidRecord,
                ))
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_canonical_epoch_advance(
                    &frame,
                    &writer,
                    &root,
                    &mut buffers.read,
                    &mut buffers.scratch,
                    &mut MutationGrant::for_test(100, std::time::Duration::from_secs(10)),
                    &mut callback,
                )
            }));
            if panic_in_callback {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            fixed_epoch_controls_released(&paths);
            assert!(writer
                .verifies(&layout.writer_lock_path(), LockMode::Exclusive)
                .unwrap());
            root.revalidate().unwrap();
            drop(root);
            assert_eq!(observed(&domain).counter(), expected.counter() + 1);
            assert_eq!(buffers.memory.used(), memory_before);
        }
    }

    #[test]
    fn dc_fixed_epoch_zero_and_short_grants_have_no_epoch_effect() {
        use super::super::maintenance_budget::MutationGrant;
        for remaining in [0, 1, 16] {
            let fixture = Fixture::new();
            let (layout, domain) = tracked_domain(&fixture, "fixed-budget");
            let expected = observed(&domain);
            let paths = FixedEpochPaths::new(&domain);
            let mut buffers = FixedEpochBuffers::new();
            let frame = prepare_canonical_epoch_frame(
                paths.borrow(),
                &expected,
                &mut buffers.prepared,
                &mut buffers.read,
                &mut buffers.scratch,
            )
            .unwrap();
            let before = fs::read(&paths.epoch).unwrap();
            let (writer, root) = fixed_epoch_guards(&layout);
            let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(10));
            grant.take_entries(100 - remaining).unwrap();
            assert!(with_canonical_epoch_advance(
                &frame,
                &writer,
                &root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut grant,
                &mut |_| panic!("not admitted")
            )
            .is_err());
            assert_eq!(fs::read(&paths.epoch).unwrap(), before);
            fixed_epoch_controls_released(&paths);
        }
    }

    #[test]
    fn dc_fixed_epoch_complete_effect_reservation_allows_its_last_reserved_entry() {
        use super::super::maintenance_budget::MutationGrant;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-exact-grant");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        let (writer, root) = fixed_epoch_guards(&layout);
        let mut grant = MutationGrant::for_test(17, std::time::Duration::from_secs(10));
        let mut called = false;
        with_canonical_epoch_advance(
            &frame,
            &writer,
            &root,
            &mut buffers.read,
            &mut buffers.scratch,
            &mut grant,
            &mut |_| {
                called = true;
                Ok(())
            },
        )
        .unwrap();
        assert!(called);
        assert_eq!(grant.remaining_entries(), 0);
        root.revalidate().unwrap();
        drop(root);
        assert_eq!(observed(&domain).counter(), expected.counter() + 1);
    }

    #[test]
    fn dc_fixed_epoch_expired_grant_does_not_open_or_change_epoch_controls() {
        use super::super::maintenance_budget::MutationGrant;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-expired");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        let before = fs::read(&paths.epoch).unwrap();
        let (writer, root) = fixed_epoch_guards(&layout);
        assert!(matches!(
            with_canonical_epoch_advance(
                &frame,
                &writer,
                &root,
                &mut buffers.read,
                &mut buffers.scratch,
                &mut MutationGrant::for_test(100, std::time::Duration::ZERO),
                &mut |_| panic!("not admitted")
            ),
            Err(super::super::locks::FixedControlError::Grant(
                super::super::maintenance_budget::GrantExhausted::Time
            ))
        ));
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
        fixed_epoch_controls_released(&paths);
    }

    #[test]
    fn dc_fixed_epoch_preparation_delay_does_not_consume_fresh_mutation_grant() {
        use super::super::maintenance_budget::MutationGrant;
        let fixture = Fixture::new();
        let (layout, domain) = tracked_domain(&fixture, "fixed-slow-preparation");
        let expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let frame = prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(110));
        // Real writer ownership is available during the advisory pause.
        drop(
            try_open_existing_locked(&layout.writer_lock_path(), LockMode::Exclusive)
                .unwrap()
                .unwrap(),
        );
        let (writer, root) = fixed_epoch_guards(&layout);
        with_canonical_epoch_advance(
            &frame,
            &writer,
            &root,
            &mut buffers.read,
            &mut buffers.scratch,
            &mut MutationGrant::start(),
            &mut |_| Ok(()),
        )
        .unwrap();
        root.revalidate().unwrap();
        drop(root);
        assert_eq!(observed(&domain).counter(), expected.counter() + 1);
    }

    #[test]
    fn dc_fixed_epoch_preparation_is_noncreating_and_rejects_exhausted_or_oversized_controls() {
        let fixture = Fixture::new();
        let (_layout, domain) = tracked_domain(&fixture, "fixed-preparation-errors");
        let mut expected = observed(&domain);
        let paths = FixedEpochPaths::new(&domain);
        let mut buffers = FixedEpochBuffers::new();
        let before = fs::read(&paths.epoch).unwrap();
        expected.record.counter = u64::MAX;
        assert!(prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch
        )
        .is_err());
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
        let expected = observed(&domain);
        fs::write(&paths.cohort, vec![b'x'; 1025]).unwrap();
        assert!(prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch
        )
        .is_err());
        fs::remove_file(&paths.cohort).unwrap();
        assert!(prepare_canonical_epoch_frame(
            paths.borrow(),
            &expected,
            &mut buffers.prepared,
            &mut buffers.read,
            &mut buffers.scratch
        )
        .is_err());
        assert!(!paths.cohort.exists());
        assert_eq!(fs::read(&paths.epoch).unwrap(), before);
    }
}

/// Owned advisory epoch images. Paths and both images remain off the mutation
/// slot; a callback borrows only their fixed frame. No authority is stored.
pub(super) struct PreparedEpoch {
    domain: ReferenceDomain,
    expected: ReferenceEpoch,
    controls: [PathBuf; 5],
    images: super::maintenance_budget::ReservedBytes,
    cohort: super::locks::FixedFileStamp,
    header: [u8; super::locks::COORDINATION_HEADER_BYTES],
    _paths: super::maintenance_budget::MemoryReservation,
}
impl PreparedEpoch {
    pub(super) fn prepare(
        domain: &ReferenceDomain,
        expected: &ReferenceEpoch,
        memory: &super::maintenance_budget::CleanupMemory,
        read: &mut super::maintenance_budget::ReservedBytes,
        scratch: &mut super::locks::FixedControlScratch,
    ) -> Result<Self, super::locks::FixedControlError> {
        use super::locks::{FixedControlError as Error, FixedControlProblem as Problem};
        if domain.scope != expected.record.scope
            || domain.artifact_identity != Some(expected.record.root)
            || domain.use_identity != Some(expected.record.use_lock)
        {
            return Err(Error::Invalid(Problem::InvalidRecord));
        }
        let mut path_bytes = 16384usize;
        for path in [
            Some(&domain.artifact_root),
            Some(&domain.destination_root),
            Some(&domain.writer_path),
            domain.reference_roots[0].as_ref(),
            domain.reference_roots[1].as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            path_bytes = path_bytes
                .checked_add(
                    path.as_os_str()
                        .as_encoded_bytes()
                        .len()
                        .checked_mul(16)
                        .ok_or(Error::Resource(
                            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
                        ))?,
                )
                .ok_or(Error::Resource(
                    super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
                ))?;
        }
        let paths = memory.reserve(path_bytes).map_err(Error::Resource)?;
        let domain = domain.clone();
        let controls = [
            domain.artifact_root.join(".use.lock"),
            domain.artifact_root.join(".use-cohort.json"),
            domain.artifact_root.join(REFERENCE_EPOCH_FILE),
            domain.artifact_root.join(REFERENCE_ACTIVITY_FILE),
            domain.artifact_root.join(REFERENCE_SERIAL_FILE),
        ];
        let mut images = memory
            .reserve(2 * REFERENCE_EPOCH_SIZE)
            .map_err(Error::Resource)?
            .bytes(2 * REFERENCE_EPOCH_SIZE)
            .map_err(Error::Resource)?;
        let frame = prepare_canonical_epoch_frame(
            CanonicalEpochPaths {
                root: &domain.artifact_root,
                destination: &domain.destination_root,
                writer: &domain.writer_path,
                root_use: &controls[0],
                cohort: &controls[1],
                epoch: &controls[2],
                activity: &controls[3],
                serial: &controls[4],
                reference_roots: [
                    domain.reference_roots[0].as_deref(),
                    domain.reference_roots[1].as_deref(),
                ],
            },
            expected,
            &mut images,
            read,
            scratch,
        )?;
        let cohort = frame.cohort;
        let header = frame.coordination_header;
        Ok(Self {
            domain,
            expected: expected.clone(),
            controls,
            images,
            cohort,
            header,
            _paths: paths,
        })
    }
    pub(super) fn frame(&self) -> CanonicalEpochFrame<'_> {
        CanonicalEpochFrame {
            paths: CanonicalEpochPaths {
                root: &self.domain.artifact_root,
                destination: &self.domain.destination_root,
                writer: &self.domain.writer_path,
                root_use: &self.controls[0],
                cohort: &self.controls[1],
                epoch: &self.controls[2],
                activity: &self.controls[3],
                serial: &self.controls[4],
                reference_roots: [
                    self.domain.reference_roots[0].as_deref(),
                    self.domain.reference_roots[1].as_deref(),
                ],
            },
            expected: &self.expected,
            bytes: self.images.as_slice(),
            cohort: self.cohort,
            coordination_header: self.header,
        }
    }
}

impl PreparedEpoch {
    /// Local lane observes a stable fixed record without requiring exclusive
    /// global observation. A concurrently torn producer write simply retries.
    pub(super) fn prepare_local(
        domain: &ReferenceDomain,
        memory: &super::maintenance_budget::CleanupMemory,
        read: &mut super::maintenance_budget::ReservedBytes,
        scratch: &mut super::locks::FixedControlScratch,
    ) -> Result<Self, super::locks::FixedControlError> {
        let expected = read_reference_epoch(domain.artifact_root()).map_err(|_| {
            super::locks::FixedControlError::Invalid(
                super::locks::FixedControlProblem::InvalidRecord,
            )
        })?;
        Self::prepare(domain, &expected, memory, read, scratch)
    }
    /// Invalidate CURRENT global E under Shared activity + Exclusive serial.
    /// Immutable local source E/A/G are deliberately not parameters to this
    /// primitive. It cannot relabel a local sealed source as global references.
    #[cfg(test)]
    pub(super) fn with_local_invalidation(
        &self,
        read: &mut super::maintenance_budget::ReservedBytes,
        scratch: &mut super::locks::FixedControlScratch,
        grant: &mut super::maintenance_budget::MutationGrant,
        effect_entries: usize,
        apply: &mut impl FnMut(
            CanonicalEpochAdvance,
            &mut super::maintenance_budget::MutationGrant,
        ) -> Result<(), super::locks::FixedControlError>,
    ) -> Result<CanonicalEpochAdvance, super::locks::FixedControlError> {
        self.with_local_validated(
            read,
            scratch,
            grant,
            effect_entries,
            &mut |_, _| Ok(()),
            apply,
        )
    }
    pub(super) fn with_local_validated(
        &self,
        read: &mut super::maintenance_budget::ReservedBytes,
        scratch: &mut super::locks::FixedControlScratch,
        grant: &mut super::maintenance_budget::MutationGrant,
        effect_entries: usize,
        validate: &mut impl FnMut(
            &mut super::locks::FixedControlScratch,
            &mut super::maintenance_budget::MutationGrant,
        ) -> Result<(), super::locks::FixedControlError>,
        apply: &mut impl FnMut(
            CanonicalEpochAdvance,
            &mut super::maintenance_budget::MutationGrant,
        ) -> Result<(), super::locks::FixedControlError>,
    ) -> Result<CanonicalEpochAdvance, super::locks::FixedControlError> {
        self.with_local_validated_owned_root(
            None,
            read,
            scratch,
            grant,
            effect_entries,
            validate,
            apply,
        )
    }
    // Keep independent borrowed authorities and caller-owned scratch explicit.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn with_local_validated_owned_root(
        &self,
        borrowed_root: Option<&super::locks::FixedLockedFile>,
        read: &mut super::maintenance_budget::ReservedBytes,
        scratch: &mut super::locks::FixedControlScratch,
        grant: &mut super::maintenance_budget::MutationGrant,
        effect_entries: usize,
        validate: &mut impl FnMut(
            &mut super::locks::FixedControlScratch,
            &mut super::maintenance_budget::MutationGrant,
        ) -> Result<(), super::locks::FixedControlError>,
        apply: &mut impl FnMut(
            CanonicalEpochAdvance,
            &mut super::maintenance_budget::MutationGrant,
        ) -> Result<(), super::locks::FixedControlError>,
    ) -> Result<CanonicalEpochAdvance, super::locks::FixedControlError> {
        use super::locks::{
            read_fixed_control, FixedControlError as Error, FixedControlKind as Kind,
            FixedControlProblem as Problem, FixedFileStamp, FixedLockedFile,
        };
        grant.check()?;
        let frame = self.frame();
        let expected = &self.expected.record;
        let owned_root = if borrowed_root.is_none() {
            let root_file =
                fixed_epoch_open(frame.paths.root_use, Kind::File, false, scratch, grant)?;
            Some(
                FixedLockedFile::try_lock(root_file, LockMode::Shared)?
                    .ok_or(Error::Invalid(Problem::Busy))?,
            )
        } else {
            None
        };
        let root_use = borrowed_root
            .or(owned_root.as_ref())
            .ok_or(Error::Invalid(Problem::Busy))?;
        if borrowed_root.is_some() && root_use.mode() != LockMode::Exclusive {
            return Err(Error::Invalid(Problem::Busy));
        }
        if FixedFileStamp::capture(root_use.file(), Kind::File)?.identity() != expected.use_lock
            || !root_use.matches_fixed_header(&self.header)?
        {
            return Err(Error::Invalid(Problem::Changed));
        }
        let activity_file =
            fixed_epoch_open(frame.paths.activity, Kind::File, false, scratch, grant)?;
        let activity = FixedLockedFile::try_lock(activity_file, LockMode::Shared)?
            .ok_or(Error::Invalid(Problem::Busy))?;
        let serial_file = fixed_epoch_open(frame.paths.serial, Kind::File, true, scratch, grant)?;
        let serial = FixedLockedFile::try_lock(serial_file, LockMode::Exclusive)?
            .ok_or(Error::Invalid(Problem::Busy))?;
        for (path, kind, id) in [
            (frame.paths.root, Kind::Directory, expected.root),
            (frame.paths.root_use, Kind::File, expected.use_lock),
            (frame.paths.activity, Kind::File, expected.activity),
            (frame.paths.serial, Kind::File, expected.serial),
            (frame.paths.cohort, Kind::File, expected.cohort),
        ] {
            let file = fixed_epoch_open(path, kind, false, scratch, grant)?;
            if FixedFileStamp::capture(&file, kind)?.identity() != id {
                return Err(Error::Invalid(Problem::Changed));
            }
        }
        if FixedFileStamp::capture(activity.file(), Kind::File)?.identity() != expected.activity
            || FixedFileStamp::capture(serial.file(), Kind::File)?.identity() != expected.serial
        {
            return Err(Error::Invalid(Problem::Changed));
        }
        let mut epoch = fixed_epoch_open(frame.paths.epoch, Kind::File, true, scratch, grant)?;
        let stamp = FixedFileStamp::capture(&epoch, Kind::File)?;
        if stamp.identity() != self.expected.file || stamp.len() != REFERENCE_EPOCH_SIZE as u64 {
            return Err(Error::Invalid(Problem::Changed));
        }
        read_fixed_control(&mut epoch, &stamp, read)?;
        // Fixed2048 checksum/decimal patch is the explicit local invalidation
        // exception. No Serde/Value/string/path allocation or payload hash.
        let bytes = &mut read.as_mut_slice()[..REFERENCE_EPOCH_SIZE];
        if bytes[..8] != REFERENCE_EPOCH_MAGIC[..]
            || Sha256::digest(&bytes[..REFERENCE_EPOCH_SIZE - 32]).as_slice()
                != &bytes[REFERENCE_EPOCH_SIZE - 32..]
        {
            return Err(Error::Invalid(Problem::InvalidRecord));
        }
        let template = &self.images.as_slice()[..REFERENCE_EPOCH_SIZE];
        let marker = b"\"counter\":";
        let position = template
            .windows(marker.len())
            .position(|w| w == marker)
            .ok_or(Error::Invalid(Problem::InvalidRecord))?
            + marker.len();
        if bytes[12..position] != template[12..position] {
            return Err(Error::Invalid(Problem::Changed));
        }
        let mut end = position;
        let mut before = 0u64;
        while end < REFERENCE_EPOCH_SIZE - 32 && bytes[end].is_ascii_digit() {
            before = before
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(bytes[end] - b'0')))
                .ok_or(Error::Invalid(Problem::EpochExhausted))?;
            end += 1;
        }
        if end == position || (end - position > 1 && bytes[position] == b'0') {
            return Err(Error::Invalid(Problem::InvalidRecord));
        }
        let after = before
            .checked_add(1)
            .ok_or(Error::Invalid(Problem::EpochExhausted))?;
        let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let suffix = b",\"covered\":true}";
        if end + suffix.len() != 12 + length
            || bytes.get(end..end + suffix.len()) != Some(suffix.as_slice())
            || bytes[12 + length..REFERENCE_EPOCH_SIZE - 32]
                .iter()
                .any(|b| *b != 0)
        {
            return Err(Error::Invalid(Problem::InvalidRecord));
        }
        let mut decimal = [0u8; 20];
        let mut cursor = decimal.len();
        let mut number = after;
        loop {
            cursor -= 1;
            decimal[cursor] = b'0' + (number % 10) as u8;
            number /= 10;
            if number == 0 {
                break;
            }
        }
        let digits = &decimal[cursor..];
        let updated_end = position + digits.len();
        bytes[position..updated_end].copy_from_slice(digits);
        bytes[updated_end..updated_end + suffix.len()].copy_from_slice(suffix);
        bytes[updated_end + suffix.len()..REFERENCE_EPOCH_SIZE - 32].fill(0);
        bytes[8..12].copy_from_slice(&((updated_end + suffix.len() - 12) as u32).to_le_bytes());
        let checksum = Sha256::digest(&bytes[..REFERENCE_EPOCH_SIZE - 32]);
        bytes[REFERENCE_EPOCH_SIZE - 32..].copy_from_slice(&checksum);
        validate(scratch, grant)?;
        grant.take_entries(effect_entries.checked_add(2).ok_or(Error::Resource(
            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
        ))?)?;
        grant.check_time()?;
        epoch.seek(SeekFrom::Start(0))?;
        epoch.write_all(bytes)?;
        grant.check_time()?;
        epoch.sync_all()?;
        grant.check_time()?;
        // Exact fresh-address binding and actual readback; use the already
        // allocated second image as scratch, never allocate under authority.
        let current =
            super::locks::open_fixed_control(frame.paths.epoch, Kind::File, false, scratch)?;
        if FixedFileStamp::capture(&current, Kind::File)?.identity() != self.expected.file {
            return Err(Error::Invalid(Problem::Changed));
        }
        let mut verify = [0u8; REFERENCE_EPOCH_SIZE];
        let mut current = current;
        current.read_exact(&mut verify)?;
        if verify != bytes {
            return Err(Error::Invalid(Problem::Changed));
        }
        grant.check_time()?;
        let advance = CanonicalEpochAdvance { before, after };
        apply(advance, grant)?;
        drop(serial);
        drop(activity);
        drop(owned_root);
        Ok(advance)
    }
}

impl PreparedEpoch {
    pub(super) fn root_facts(&self) -> (FileIdentity, FileIdentity, [u8; 32], FileIdentity) {
        (
            self.expected.record.root,
            self.expected.record.use_lock,
            self.expected.record.use_incarnation,
            self.expected.record.cohort,
        )
    }
}

pub(super) struct FreshWorkspacePublisher {
    pub(super) anchor: super::locks::RootedDirectory,
    pub(super) nonce: [u8; 32],
    pub(super) owner: FileIdentity,
    pub(super) parent_root: FileIdentity,
    pub(super) parent_coordination: FileIdentity,
    pub(super) parent_nonce: [u8; 32],
    pub(super) parent_cohort: FileIdentity,
    pub(super) publication_binding: FileIdentity,
    pub(super) registry: FileIdentity,
}
/// Private genuine-creation hook. Kind1 is a scoped managed run, Kind2 is a
/// registered worker workspace. Prefix/shape or a borrowed root-use bool cannot
/// enter this factory; both the fresh physical root and live domain mutation
/// are required. Optional proof failure leaves primary work untracked.
pub(super) fn initialize_workspace_publisher(
    fresh: &super::layout::FreshReferenceRoot,
    domain: &ReferenceDomain,
    mutation: &ReferenceMutation<'_>,
    owner: &LockedFile,
    kind: u8,
) -> Result<Option<FreshWorkspacePublisher>, NativeError> {
    let Some(parent) = mutation.artifact_root_producer_capability()? else {
        return Ok(None);
    };
    if !parent.revalidate()? || parent.root() != domain.artifact_root() || !domain.revalidate()? {
        return Ok(None);
    }
    let registry = match (kind, domain.scope.kind) {
        (1, 1) => domain.reference_roots[1]
            .clone()
            .ok_or_else(|| invalid("run registry missing"))?,
        (1, 2) => fresh
            .path()
            .parent()
            .ok_or_else(|| invalid("run registry missing"))?
            .to_owned(),
        (3, 1) => domain.destination_root.join("phases"),
        (3, 2) => {
            let pair = domain
                .raw_address
                .as_ref()
                .ok_or_else(|| invalid("direct addresses missing"))?;
            DirectLayout::new(&pair[0], &pair[1]).phase_workspace_root()
        }
        (2, 1) => domain.destination_root.join("workers"),
        (2, 2) => {
            let pair = domain
                .raw_address
                .as_ref()
                .ok_or_else(|| invalid("direct addresses missing"))?;
            DirectLayout::new(&pair[0], &pair[1]).worker_workspace_root_path()
        }
        _ => return Ok(None),
    };
    let registry_id = directory_identity(&registry)?;
    if fresh.path().parent().map(directory_identity).transpose()? != Some(registry_id)
        || directory_identity(fresh.path())? != fresh.identity()
        || !owner.verifies(&fresh.path().join("lease.lock"), LockMode::Exclusive)?
    {
        return Ok(None);
    }
    let anchor = super::locks::RootedDirectory::open(fresh.path(), fresh.identity())?;
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| invalid("workspace entropy unavailable"))?;
    owner.initialize_creation_incarnation(nonce)?;
    let creation = FreshPublicationRoot {
        anchor: &anchor,
        root_use: owner,
        incarnation: nonce,
    };
    let mut scratch = super::maintenance_controls::PublicationFactoryScratch::new();
    if super::maintenance_controls::initialize_creation_controls(
        &creation,
        &mut scratch,
        &super::maintenance_controls::WorkspaceControl::building().encode(),
    )
    .is_err()
    {
        return Ok(None);
    }
    let publication_binding = super::locks::file_identity(
        &anchor.open_readonly_file(super::maintenance_controls::BINDING_NAME)?,
    )?;
    Ok(Some(FreshWorkspacePublisher {
        anchor,
        nonce,
        owner: owner.identity()?,
        parent_root: parent.root_identity(),
        parent_coordination: parent.coordination_identity(),
        parent_nonce: parent.incarnation(),
        parent_cohort: parent.cohort_identity(),
        publication_binding,
        registry: registry_id,
    }))
}
impl ReferenceDomain {
    pub(crate) fn workspace_startup_path(&self) -> PathBuf {
        self.destination_root.join(".workspace-startup.lock")
    }
    pub(crate) fn workspace_registry(
        &self,
        kind: u8,
        state_root: Option<&Path>,
    ) -> Option<PathBuf> {
        match (self.scope.kind, kind) {
            (1, 1) => Some(self.destination_root.join("runs")),
            (1, 2) => Some(self.destination_root.join("workers")),
            (1, 3) => Some(self.destination_root.join("phases")),
            (2, 1) => Some(
                state_root
                    .unwrap_or(&self.destination_root)
                    .join("direct-runs"),
            ),
            (2, 2) | (2, 3) => self.raw_address.as_ref().map(|pair| {
                let layout = DirectLayout::new(&pair[0], &pair[1]);
                if kind == 2 {
                    layout.worker_workspace_root_path()
                } else {
                    layout.phase_workspace_root()
                }
            }),
            _ => None,
        }
    }
}

/// Storage-private sealed-by-visibility producer interface. Implementations
/// borrow actual live owner/activity guards; serialized facts cannot implement
/// this at any external producer API boundary.
pub(super) trait PublicationRootAuthority {
    fn root(&self) -> &Path;
    fn root_identity(&self) -> FileIdentity;
    fn coordination_identity(&self) -> FileIdentity;
    fn incarnation(&self) -> [u8; 32];
    fn cohort_identity(&self) -> FileIdentity;
    fn revalidate(&self) -> Result<bool, NativeError>;
}
impl PublicationRootAuthority for ArtifactRootProducerCapability<'_> {
    fn root(&self) -> &Path {
        ArtifactRootProducerCapability::root(self)
    }
    fn root_identity(&self) -> FileIdentity {
        ArtifactRootProducerCapability::root_identity(self)
    }
    fn coordination_identity(&self) -> FileIdentity {
        ArtifactRootProducerCapability::coordination_identity(self)
    }
    fn incarnation(&self) -> [u8; 32] {
        ArtifactRootProducerCapability::incarnation(self)
    }
    fn cohort_identity(&self) -> FileIdentity {
        ArtifactRootProducerCapability::cohort_identity(self)
    }
    fn revalidate(&self) -> Result<bool, NativeError> {
        ArtifactRootProducerCapability::revalidate(self)
    }
}

impl PreparedEpoch {
    fn advisory_current(
        &self,
        read: &mut super::maintenance_budget::ReservedBytes,
        scratch: &mut super::locks::FixedControlScratch,
    ) -> Result<bool, super::locks::FixedControlError> {
        use super::locks::{
            open_fixed_control, read_fixed_control, FixedControlKind as Kind, FixedFileStamp,
            FixedLockedFile,
        };
        let frame = self.frame();
        let expected = &self.expected.record;
        let root_file = open_fixed_control(frame.paths.root_use, Kind::File, false, scratch)?;
        let Some(root_use) = FixedLockedFile::try_lock(root_file, LockMode::Shared)? else {
            return Ok(false);
        };
        let activity_file = open_fixed_control(frame.paths.activity, Kind::File, true, scratch)?;
        let Some(activity) = FixedLockedFile::try_lock(activity_file, LockMode::Exclusive)? else {
            return Ok(false);
        };
        if FixedFileStamp::capture(root_use.file(), Kind::File)?.identity() != expected.use_lock
            || !root_use.matches_fixed_header(&self.header)?
            || FixedFileStamp::capture(activity.file(), Kind::File)?.identity() != expected.activity
        {
            return Ok(false);
        }
        for (path, kind, id) in [
            (frame.paths.root, Kind::Directory, expected.root),
            (
                frame.paths.destination,
                Kind::Directory,
                expected.scope.destination,
            ),
            (frame.paths.writer, Kind::File, expected.scope.writer),
            (frame.paths.root_use, Kind::File, expected.use_lock),
            (frame.paths.activity, Kind::File, expected.activity),
        ] {
            let file = open_fixed_control(path, kind, false, scratch)?;
            if FixedFileStamp::capture(&file, kind)?.identity() != id {
                return Ok(false);
            }
        }
        for (path, id) in frame
            .paths
            .reference_roots
            .into_iter()
            .zip(expected.scope.reference_roots)
        {
            match (path, id) {
                (Some(path), Some(id)) => {
                    let file = open_fixed_control(path, Kind::Directory, false, scratch)?;
                    if FixedFileStamp::capture(&file, Kind::Directory)?.identity() != id {
                        return Ok(false);
                    }
                }
                (None, None) => (),
                _ => return Ok(false),
            }
        }
        let cohort = open_fixed_control(frame.paths.cohort, Kind::File, false, scratch)?;
        if frame.cohort.recheck(&cohort).is_err() {
            return Ok(false);
        }
        let mut epoch = open_fixed_control(frame.paths.epoch, Kind::File, false, scratch)?;
        let stamp = FixedFileStamp::capture(&epoch, Kind::File)?;
        if stamp.identity() != self.expected.file || stamp.len() != REFERENCE_EPOCH_SIZE as u64 {
            return Ok(false);
        }
        read_fixed_control(&mut epoch, &stamp, read)?;
        Ok(read.as_slice()[..REFERENCE_EPOCH_SIZE]
            == self.images.as_slice()[..REFERENCE_EPOCH_SIZE])
    }
}

pub(crate) fn expected_reference_current(
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<bool, super::locks::FixedControlError> {
    use super::locks::{FixedControlError as Error, FixedControlScratch};
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(Error::Resource)?,
    )?;
    let mut read = memory
        .reserve(REFERENCE_EPOCH_SIZE)
        .map_err(Error::Resource)?
        .bytes(REFERENCE_EPOCH_SIZE)
        .map_err(Error::Resource)?;
    let prepared = PreparedEpoch::prepare(domain, expected, memory, &mut read, &mut scratch)?;
    prepared.advisory_current(&mut read, &mut scratch)
}

impl ReferenceEpoch {
    /// Exact facts only; outer journal owns its checksum. Decoding cannot mint
    /// authority and resume must compare this ORIGINAL E under actual guards.
    pub(crate) fn encode_checkpoint(&self, out: &mut [u8; 512]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGECHK01");
        out[8..12].copy_from_slice(&self.record.version.to_le_bytes());
        out[12] = self.record.scope.kind;
        out[13] = u8::from(self.record.covered);
        out[14] = u8::from(self.record.scope.reference_roots[0].is_some());
        out[15] = u8::from(self.record.scope.reference_roots[1].is_some());
        out[16..24].copy_from_slice(&self.record.counter.to_le_bytes());
        for (index, identity) in [
            Some(self.file),
            Some(self.record.root),
            Some(self.record.use_lock),
            Some(self.record.cohort),
            Some(self.record.scope.destination),
            Some(self.record.scope.writer),
            Some(self.record.activity),
            Some(self.record.serial),
            self.record.scope.reference_roots[0],
            self.record.scope.reference_roots[1],
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(identity) = identity {
                let (a, b) = identity.fixed_words();
                out[24 + index * 16..32 + index * 16].copy_from_slice(&a.to_le_bytes());
                out[32 + index * 16..40 + index * 16].copy_from_slice(&b.to_le_bytes());
            }
        }
        out[184..216].copy_from_slice(&self.record.use_incarnation);
        out[216..248].copy_from_slice(&self.record.incarnation);
        out[248..280].copy_from_slice(&self.record.scope.address);
    }
    pub(crate) fn decode_checkpoint(
        input: &[u8; 512],
    ) -> Result<Self, super::locks::FixedControlError> {
        use super::locks::{FixedControlError as Error, FixedControlProblem as Problem};
        if &input[..8] != b"CGECHK01"
            || u32::from_le_bytes(input[8..12].try_into().unwrap()) != 2
            || !matches!(input[12], 1 | 2)
            || input[13..16].iter().any(|b| *b > 1)
            || input[280..].iter().any(|b| *b != 0)
        {
            return Err(Error::Invalid(Problem::InvalidRecord));
        }
        let identity = |index: usize| {
            FileIdentity::from_fixed_words(
                u64::from_le_bytes(input[24 + index * 16..32 + index * 16].try_into().unwrap()),
                u64::from_le_bytes(input[32 + index * 16..40 + index * 16].try_into().unwrap()),
            )
        };
        let mut roots = [None, None];
        for (index, root) in roots.iter_mut().enumerate() {
            if input[14 + index] == 1 {
                *root = Some(identity(8 + index));
            } else if input[152 + index * 16..168 + index * 16]
                .iter()
                .any(|b| *b != 0)
            {
                return Err(Error::Invalid(Problem::InvalidRecord));
            }
        }
        Ok(Self {
            file: identity(0),
            record: ReferenceEpochRecord {
                version: 2,
                root: identity(1),
                use_lock: identity(2),
                use_incarnation: input[184..216].try_into().unwrap(),
                cohort: identity(3),
                scope: ReferenceScopeRecord {
                    kind: input[12],
                    destination: identity(4),
                    writer: identity(5),
                    address: input[248..280].try_into().unwrap(),
                    reference_roots: roots,
                },
                incarnation: input[216..248].try_into().unwrap(),
                activity: identity(6),
                serial: identity(7),
                counter: u64::from_le_bytes(input[16..24].try_into().unwrap()),
                covered: input[13] == 1,
            },
        })
    }
}

// Only the actual administrative enrollment kernel implements this sealed
// borrowing interface. Foundations do not depend on orchestration modules.
pub(super) mod administrative_coverage_seal {
    pub trait Sealed {}
}
pub(crate) trait AdministrativeCoverageAuthority:
    administrative_coverage_seal::Sealed
{
    fn directory(&self) -> &super::locks::RootedDirectory;
    fn artifact_directory(&self) -> &super::locks::RootedDirectory;
    fn writer_identity(&self) -> Result<FileIdentity, super::locks::FixedControlError>;
    fn artifact_use_identity(&self) -> Result<FileIdentity, super::locks::FixedControlError>;
    fn proposed_artifact_incarnation(&self) -> [u8; 32];
    fn is_administrative(&self) -> bool;
}
#[path = "maintenance_legacy_coverage.rs"]
mod legacy_coverage;
pub(crate) use legacy_coverage::{prepare_legacy_coverage, PreparedLegacyCoverage};

#[path = "maintenance_automatic_direct.rs"]
mod automatic_direct;
pub(crate) use automatic_direct::{prepare_automatic_direct, PreparedAutomaticDirect};
