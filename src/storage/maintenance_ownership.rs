//! Ownership capabilities for maintenance. Inventory observations are never
//! capabilities; every capability below retains the actual OS locks.
use super::atomic::write_json_atomically;
use super::containment::{probe_descriptor, BuildDescriptorV2};
use super::inventory::{
    bound_roles_overlap, default_direct, observe_binding, real_path, same_physical_path,
    DirectPair, MaintenanceTarget, ObservedBinding,
};
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
pub(crate) enum EpochObservation {
    Verified(ReferenceEpoch),
    Pending(String),
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
    if record.version != 1 {
        return Err(invalid("reference_epoch_version_unknown"));
    }
    Ok(ReferenceEpoch {
        file: identity,
        record,
    })
}
fn epoch_root_matches(root: &Path, record: &ReferenceEpochRecord) -> Result<bool, NativeError> {
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
    if cohort.version != 1
        || cohort.root != record.root
        || cohort.coordination != record.use_lock
        || path_identity(&cohort_path)? != record.cohort
    {
        return Ok(false);
    }
    Ok(record.root == directory_identity(root)?
        && record.use_lock == path_identity(&root.join(".use.lock"))?
        && record.activity == path_identity(&root.join(REFERENCE_ACTIVITY_FILE))?
        && record.serial == path_identity(&root.join(REFERENCE_SERIAL_FILE))?)
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
    let activity = open_locked(
        domain.artifact_root.join(REFERENCE_ACTIVITY_FILE),
        LockMode::Exclusive,
    )?;
    let serial = open_locked(
        domain.artifact_root.join(REFERENCE_SERIAL_FILE),
        LockMode::Exclusive,
    )?;
    let record = ReferenceEpochRecord {
        version: 1,
        root: root_use.identity,
        use_lock: root_use.lock.identity()?,
        cohort: path_identity(&domain.artifact_root.join(".use-cohort.json"))?,
        scope: domain.scope.clone(),
        incarnation: Sha256::digest(super::layout::managed_generation_id().as_bytes()).into(),
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
            Ok(None) => {
                return Ok(EpochObservation::Pending(
                    "reference_activity_in_use".into(),
                ))
            }
            Err(_) => {
                return Ok(EpochObservation::Pending(
                    "reference_epoch_missing_or_unverifiable".into(),
                ))
            }
        };
    let epoch = match read_reference_epoch(root) {
        Ok(epoch) => epoch,
        Err(_) => {
            return Ok(EpochObservation::Pending(
                "reference_epoch_missing_or_unverifiable".into(),
            ))
        }
    };
    let verified = (|| -> Result<bool, NativeError> {
        Ok(activity.is_current()?
            && domain.revalidate()?
            && epoch_root_matches(root, &epoch.record)?
            && epoch.record.scope == domain.scope
            && epoch.record.covered)
    })();
    if !matches!(verified, Ok(true)) {
        return Ok(EpochObservation::Pending(
            "reference_domain_unverified".into(),
        ));
    }
    Ok(EpochObservation::Verified(epoch))
}

// Private foundation only. Borrowed paths/bytes keep their owning Work and
// reservations OFF the authority stack. This is not PreparedMaintenance,
// current config/reference completeness, Ready admission or deletion authority.
struct CanonicalEpochPaths<'a> {
    root: &'a Path,
    destination: &'a Path,
    writer: &'a Path,
    root_use: &'a Path,
    cohort: &'a Path,
    epoch: &'a Path,
    activity: &'a Path,
    serial: &'a Path,
    reference_roots: [Option<&'a Path>; 2],
}
struct CanonicalEpochFrame<'a> {
    paths: CanonicalEpochPaths<'a>,
    expected: &'a ReferenceEpoch,
    bytes: &'a [u8],
    cohort: super::locks::FixedFileStamp,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanonicalEpochAdvance {
    before: u64,
    after: u64,
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
fn prepare_canonical_epoch_frame<'a>(
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
    if !expected.record.covered || expected.record.version != 1 {
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
            version: 1,
            root: expected.record.root,
            coordination: expected.record.use_lock,
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
fn with_canonical_epoch_advance(
    frame: &CanonicalEpochFrame<'_>,
    writer: &WriterLease,
    root_use: &ArtifactUseLease,
    read: &mut super::maintenance_budget::ReservedBytes,
    scratch: &mut super::locks::FixedControlScratch,
    grant: &mut super::maintenance_budget::MutationGrant,
    apply: &mut impl FnMut(CanonicalEpochAdvance) -> Result<(), super::locks::FixedControlError>,
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
    if root_use.identity != expected.root
        || use_stamp.identity() != expected.use_lock
        || !root_use
            .lock
            .matches_fixed_current(&use_stamp, LockMode::Exclusive)?
        || root_use.reference_activity.is_some()
    {
        return Err(Error::Invalid(Problem::GuardMismatch));
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
    grant.take_entries(2)?; // epoch effect plus its fresh-address/readback check
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
    apply(advance)?;
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
    reason: Option<String>,
}
impl ReferenceActivity {
    fn untracked(reason: &str) -> Self {
        Self {
            guard: None,
            reason: Some(reason.into()),
        }
    }
    fn begin(
        root: &Path,
        expected: Option<&ReferenceDomain>,
        nonblocking: bool,
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
            match super::locks::open_existing_shared_coordination(
                &root.join(REFERENCE_ACTIVITY_FILE),
            ) {
                Ok(guard) => guard,
                Err(error) => return Err(error),
            }
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
            || !epoch_root_matches(root, &epoch.record)?
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
                reason: Some("reference_coverage_unverified".into()),
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
            reason: mismatch.then(|| "reference_domain_mismatch".into()),
        }))
    }
}

/// Scope-bound producer activity. Missing old coverage permits ordinary work
/// only as untracked; it can never authorize canonical artifact collection.
#[derive(Debug)]
pub(crate) struct ReferenceMutation<'a> {
    _root_use: Option<LockedFile>,
    activity: ReferenceActivity,
    _borrowed: std::marker::PhantomData<&'a LockedFile>,
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
            _borrowed: std::marker::PhantomData,
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
                        _borrowed: std::marker::PhantomData,
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
                        _borrowed: std::marker::PhantomData,
                    }));
                }
            }
        };
        let Some(activity) = ReferenceActivity::begin(&root, expected, nonblocking)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            _root_use: Some(root_use),
            activity,
            _borrowed: std::marker::PhantomData,
        }))
    }
}
impl Drop for ReferenceMutation<'_> {
    fn drop(&mut self) {
        drop(self.activity.guard.take());
        drop(self._root_use.take());
    }
}

impl ReferenceMutation<'_> {
    pub(crate) fn tracked(&self) -> bool {
        self.activity.reason.is_none()
    }
    pub(crate) fn pending_reason(&self) -> Option<&str> {
        self.activity.reason.as_deref()
    }
}

#[derive(Debug)]
pub(crate) enum MaintenanceAdmission<'a> {
    Acquired(MaintenanceLease<'a>),
    Deferred(String),
    Unknown(String),
}

#[derive(Debug)]
enum DestinationWriter<'a> {
    Owned(WriterLease),
    Borrowed(&'a WriterLease),
}
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
impl MaintenanceLease<'_> {
    /// Invalidation only. A future prepared transition must validate its old
    /// epoch under this authority BEFORE calling; this is not a deletion seal.
    pub(crate) fn begin_reference_mutation(&self) -> Result<ReferenceMutation<'_>, NativeError> {
        self.artifact.revalidate()?;
        let activity = ReferenceActivity::begin(&self.artifact.root, None, false)?
            .ok_or_else(|| invalid("reference_activity_in_use"))?;
        Ok(ReferenceMutation {
            _root_use: None,
            activity,
            _borrowed: std::marker::PhantomData,
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

    pub(crate) fn cohort_verified(&self) -> Result<bool, NativeError> {
        self.revalidate()?;
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
            version: 1,
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
            version: 1,
            origin: PublicationCohortOrigin::GuardedPublication,
            repository: path_identity(&self.target.repository_root)?,
            writer_path: self.writer_path.clone(),
            writer: self.writer.lease().identity()?,
            artifact_root: self.artifact.identity,
            artifact_coordination: self.artifact.lock.identity()?,
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
        Ok(*record == self.publication_cohort_record(database, manifest)?)
    }

    pub(crate) fn direct_publication_cohort_verified(
        &self,
        record: &PublicationCohortRecord,
    ) -> Result<bool, NativeError> {
        Ok(*record == self.direct_publication_cohort_record()?)
    }

    pub(crate) fn cohort_record_path(&self) -> PathBuf {
        self.writer_path.with_extension("maintenance-cohort.json")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PublicationCohortRecord {
    version: u32,
    origin: PublicationCohortOrigin,
    repository: FileIdentity,
    writer_path: PathBuf,
    writer: FileIdentity,
    artifact_root: FileIdentity,
    artifact_coordination: FileIdentity,
    scope: PublicationScope,
    database: DatabaseUseProof,
    manifest: DatabaseUseProof,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PublicationCohortOrigin {
    GuardedPublication,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
pub(crate) struct DestinationCohortRecord {
    version: u32,
    writer_path: PathBuf,
    repository: FileIdentity,
    destination_root: FileIdentity,
    writer: FileIdentity,
    artifact_root: FileIdentity,
    artifact_coordination: FileIdentity,
    direct_pair: Option<DirectPair>,
    database: Option<FileIdentity>,
    manifest: Option<FileIdentity>,
    resource_use: Vec<DatabaseUseProof>,
    origin: DestinationCohortOrigin,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
    origin: CohortOrigin,
}

#[derive(Debug)]
pub(crate) struct ArtifactUseLease {
    root: PathBuf,
    identity: FileIdentity,
    lock: LockedFile,
    reference_activity: Option<ReferenceActivity>,
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
                    try_open_locked_in_existing_parent(root.join(".use.lock"), LockMode::Shared)?
                {
                    let lease = Self {
                        root,
                        identity,
                        lock,
                        reference_activity: None,
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
        let record = self.record(CohortOrigin::GuardedCreation)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(".use-cohort.json"))?;
        file.write_all(&serde_json::to_vec(&record)?)?;
        file.sync_all()?;
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
        let lock = open_locked(root.join(".use.lock"), LockMode::Shared)?;
        let mut lease = Self {
            root,
            identity,
            lock,
            reference_activity: None,
        };
        lease.revalidate()?;
        // Only the process that created this incarnation can establish guarded
        // creation. Existing unproven roots remain legacy until explicit S3
        // confirmation. A create_new write cannot replace an existing proof.
        if created {
            lease.write_guarded_creation_record()?;
        }
        lease.reference_activity = ReferenceActivity::begin(&lease.root, None, false)?;
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
        self.reference_activity = ReferenceActivity::begin(&self.root, Some(domain), false)?;
        Ok(())
    }
    pub(crate) fn acquire_in_domain(domain: &ReferenceDomain) -> Result<Self, NativeError> {
        let mut lease = Self::acquire_shared(&domain.artifact_root)?;
        lease.bind_reference_domain(domain)?;
        Ok(lease)
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
            version: 1,
            root: self.identity,
            coordination: self.lock.identity()?,
            origin,
        })
    }
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
            && record.version == 1
            && record.root == self.identity
            && record.coordination == self.lock.identity()?)
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
}
impl DatabaseUseLease {
    pub(crate) fn acquire_shared(database: &Path) -> Result<Self, NativeError> {
        let (parent, path) = database_lock_path(database, true)?;
        let parent_identity = directory_identity(&parent)?;
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
        };
        lease.revalidate()?;
        Ok(lease)
    }

    pub(crate) fn try_exclusive(database: &Path) -> Result<Option<Self>, NativeError> {
        Ok(Self::try_exclusive_many(&[database])?.and_then(|mut leases| leases.pop()))
    }

    /// One acquisition group owns each physical parent and each basename guard
    /// once. This is explicit group authority, never process-wide reentrancy.
    /// All guards remain live until the final group member using them drops.
    pub(crate) fn try_exclusive_many(
        databases: &[&Path],
    ) -> Result<Option<Vec<Self>>, NativeError> {
        let mut families: Vec<(FileIdentity, Arc<LockedFile>)> = Vec::new();
        let mut resource_locks: Vec<(FileIdentity, PathBuf, Arc<LockedFile>)> = Vec::new();
        let mut leases = Vec::new();
        for database in databases {
            let (parent, path) = database_lock_path(database, false)?;
            let parent_identity = directory_identity(&parent)?;
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
        if directory_identity(&self.parent)? != self.parent_identity
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
pub(crate) struct DatabaseUseProof {
    resource: PathBuf,
    file: Option<FileIdentity>,
    parent: FileIdentity,
    family_coordination_path: PathBuf,
    family_coordination: FileIdentity,
    coordination_path: PathBuf,
    coordination: FileIdentity,
}

fn database_lock_path(
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
    };
    if lease.cohort_verified()? {
        Ok(Some(lease))
    } else {
        Ok(None)
    }
}

pub(crate) fn claim_maintenance(
    target: &MaintenanceTarget,
) -> Result<MaintenanceAdmission<'static>, NativeError> {
    claim(target, None)
}
pub(crate) fn claim_maintenance_with_owned<'a>(
    target: &MaintenanceTarget,
    writer: &'a WriterLease,
) -> Result<MaintenanceAdmission<'a>, NativeError> {
    claim(target, Some(writer))
}

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
        assert!(!path.exists());
        assert!(observed(&domain).counter() > old.counter());
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
        let shared = DatabaseUseLease::acquire_shared(&database).unwrap();
        fs::rename(&parent, fixture.root.join("detached-parent")).unwrap();
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
        fs::set_permissions(&state, fs::Permissions::from_mode(0)).unwrap();
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
        fs::rename(&root, fixture.layout.storage_root().join("detached")).unwrap();
        drop(ArtifactUseLease::acquire_shared(&root).unwrap());
        start_tx.send(()).unwrap();
        drop(exclusive);
        assert!(producer.join().unwrap().is_err());
        assert!(observe_artifact_cohort(&root).unwrap().is_some());
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
        fs::rename(&old, &detached).unwrap();
        drop(ArtifactUseLease::acquire_shared(&old).unwrap());
        assert!(lease.revalidate().is_err());
        drop(lease);
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
        assert_eq!(observed(&domain).counter(), advance.after);
        fixed_epoch_controls_released(&paths);
        assert!(writer
            .verifies(&layout.writer_lock_path(), LockMode::Exclusive)
            .unwrap());
        root.revalidate().unwrap();
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
        fs::rename(&paths.root, &old).unwrap();
        fs::create_dir(&paths.root).unwrap();
        for entry in fs::read_dir(&old).unwrap() {
            let entry = entry.unwrap();
            fs::rename(entry.path(), paths.root.join(entry.file_name())).unwrap();
        }
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
