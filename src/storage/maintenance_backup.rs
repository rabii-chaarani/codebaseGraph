//! Replacement-backup recovery facts and single-leaf effects. Journal ownership
//! and durable intent belong to the candidate engine, never to this adapter.
use super::locks::{FileIdentity, FixedControlKind, FixedFileStamp, RootedDirectory};
use super::maintenance_budget::{CleanupMemory, MemoryReservation, MutationGrant};
use super::maintenance_legacy_association::LegacyAssociation;
use super::maintenance_ownership::{expected_reference_current, ReferenceDomain, ReferenceEpoch};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupIssue {
    Unknown,
    Changed,
    ResourceLimit,
    Cancelled,
    Io(io::ErrorKind),
}
impl From<io::Error> for BackupIssue {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// Only the exact historical producer spelling is recognized. A name is an
/// address, never origin, association or loader usability evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackupAddress {
    key: [u8; 32],
    name: [u8; 255],
    len: u8,
}
impl BackupAddress {
    pub(crate) fn parse(name: &[u8]) -> Option<Self> {
        if !name.starts_with(b".replaced-") || name.get(74) != Some(&b'-') {
            return None;
        }
        let digits = name.get(75..)?;
        if digits.is_empty() || (digits.len() > 1 && digits[0] == b'0') {
            return None;
        }
        let mut timestamp = 0u128;
        for byte in digits {
            if !byte.is_ascii_digit() {
                return None;
            }
            timestamp = timestamp
                .checked_mul(10)?
                .checked_add(u128::from(*byte - b'0'))?;
        }
        let mut key = [0; 32];
        for (index, pair) in name.get(10..74)?.chunks_exact(2).enumerate() {
            fn nibble(byte: u8) -> Option<u8> {
                match byte {
                    b'0'..=b'9' => Some(byte - b'0'),
                    b'a'..=b'f' => Some(byte - b'a' + 10),
                    _ => None,
                }
            }
            key[index] = nibble(pair[0])? * 16 + nibble(pair[1])?;
        }
        let mut owned = [0; 255];
        owned.get_mut(..name.len())?.copy_from_slice(name);
        Some(Self {
            key,
            name: owned,
            len: name.len() as u8,
        })
    }
    pub(crate) fn key(&self) -> [u8; 32] {
        self.key
    }
    pub(crate) fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.len)]
    }
    fn canonical(&self) -> [u8; 64] {
        let mut bytes = [0; 64];
        for (index, byte) in self.key.iter().enumerate() {
            bytes[index * 2] = b"0123456789abcdef"[usize::from(byte >> 4)];
            bytes[index * 2 + 1] = b"0123456789abcdef"[usize::from(byte & 15)];
        }
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackupPlan {
    pub(crate) address: BackupAddress,
    pub(crate) root: FileIdentity,
    pub(crate) shard: FileIdentity,
    pub(crate) source: FileIdentity,
    pub(crate) payload: FixedFileStamp,
    pub(crate) payload_sha256: [u8; 32],
    pub(crate) source_epoch: u64,
}

/// Entirely advisory, thread-local and accounted. The completed association is
/// the opaque full native/manifest/cache join, including selected LoaderUsable;
/// it cannot be replaced with a schema boolean or a backup-name inference.
/// Its one selected-payload validation already happened in the association job.
pub(crate) struct PreparedBackup {
    association: LegacyAssociation,
    _database: File,
    _manifest: File,
    _root: RootedDirectory,
    _shard: RootedDirectory,
    _source: RootedDirectory,
    _payload: File,
    plan: BackupPlan,
    _floor: MemoryReservation,
}

// Keep the owned source files, sealed association, epoch and budget explicit at verification.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_backup(
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    address: BackupAddress,
    association: LegacyAssociation,
    database: File,
    manifest: File,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<PreparedBackup, BackupIssue> {
    // Reserve before constructing any retained or transient handles/buffers.
    let floor = memory
        .reserve(std::mem::size_of::<PreparedBackup>() + 8192)
        .map_err(|_| BackupIssue::ResourceLimit)?;
    if cancel() {
        return Err(BackupIssue::Cancelled);
    }
    if association.source_epoch() != expected
        || !expected_reference_current(domain, expected, memory)
            .map_err(|_| BackupIssue::Changed)?
    {
        return Err(BackupIssue::Changed);
    }
    let root = RootedDirectory::open_readonly_root(
        domain.artifact_root(),
        association.artifact_root_identity(),
    )?;
    if !association.matches(&database, &manifest, &root) {
        return Err(BackupIssue::Changed);
    }
    let canonical = address.canonical();
    let shard = root.open_directory_readonly(&canonical[..2])?;
    match shard.open_directory_readonly(&canonical) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => return Err(BackupIssue::Unknown),
        Err(error) => return Err(error.into()),
    }
    let source = shard.open_directory_readonly(address.name())?;
    let mut payload = source.open_readonly_file(b"partition.json")?;
    let stamp = FixedFileStamp::capture(&payload, FixedControlKind::File)
        .map_err(|_| BackupIssue::Changed)?;
    if association.selected_backup_loader_usable() != Some(true)
        || !association.selected_backup_matches(address.key(), source.identity(), stamp)
    {
        return Err(BackupIssue::Unknown);
    }
    // This is a digest pass over the SAME certified selected payload, not a
    // second payload validation, and never runs with dispatcher authority.
    let mut hash = Sha256::new();
    let mut buffer = [0; 4096];
    loop {
        if cancel() {
            return Err(BackupIssue::Cancelled);
        }
        let count = payload.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    stamp.recheck(&payload).map_err(|_| BackupIssue::Changed)?;
    // Keep DB + manifest + one cache payload as the aggregate maximum. The
    // replacement handle is retained only after its exact stamp is rechecked.
    drop(payload);
    let payload = source.open_readonly_file(b"partition.json")?;
    stamp.recheck(&payload).map_err(|_| BackupIssue::Changed)?;
    if cancel() {
        return Err(BackupIssue::Cancelled);
    }
    if shard.open_directory_readonly(address.name())?.identity() != source.identity()
        || !association.matches(&database, &manifest, &root)
        || !expected_reference_current(domain, expected, memory)
            .map_err(|_| BackupIssue::Changed)?
    {
        return Err(BackupIssue::Changed);
    }
    let plan = BackupPlan {
        address,
        root: root.identity(),
        shard: shard.identity(),
        source: source.identity(),
        payload: stamp,
        payload_sha256: hash.finalize().into(),
        source_epoch: expected.counter(),
    };
    Ok(PreparedBackup {
        association,
        _database: database,
        _manifest: manifest,
        _root: root,
        _shard: shard,
        _source: source,
        _payload: payload,
        plan,
        _floor: floor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_backup_address_is_exact_and_lossless() {
        let name = format!(".replaced-{}-{}", "ab".repeat(32), u128::MAX);
        let address = BackupAddress::parse(name.as_bytes()).unwrap();
        assert_eq!(address.key(), [0xab; 32]);
        assert_eq!(address.name(), name.as_bytes());
        assert_eq!(address.canonical(), b"ab".repeat(32).as_slice());
    }
    #[test]
    fn backup_names_do_not_supply_origin_or_guess_other_formats() {
        let key = "ab".repeat(32);
        for name in [
            format!(".replaced-{key}-"),
            format!(".replaced-{key}-01"),
            format!(".replaced-{key}-+1"),
            format!(".replaced-{key}-{}0", u128::MAX),
            format!(".replaced-{key}-1/child"),
            format!(".replaced-{}-1", key.to_uppercase()),
            format!("{key}.bak"),
            format!(".replaced-{key}-1\0"),
        ] {
            assert!(BackupAddress::parse(name.as_bytes()).is_none(), "{name}");
        }
    }
}

/// Resolve the selected CURRENT source without invoking repair, snapshots or
/// loader admission. The caller has already captured the exact target binding.
/// Only these two source handles survive association preparation.
pub(crate) fn open_current_source(
    target: &super::inventory::MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<(File, File), BackupIssue> {
    use super::locks::{open_fixed_control, FixedControlScratch};
    let path_bytes = target
        .repository_root
        .as_os_str()
        .len()
        .checked_add(target.state_root.as_os_str().len())
        .and_then(|n| {
            n.checked_add(
                target
                    .storage_root
                    .as_ref()
                    .map_or(0, |p| p.as_os_str().len()),
            )
        })
        .and_then(|n| {
            n.checked_add(target.direct_pair.as_ref().map_or(0, |p| {
                p.database.as_os_str().len() + p.manifest.as_os_str().len()
            }))
        })
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(8192))
        .ok_or(BackupIssue::ResourceLimit)?;
    let _paths = memory
        .reserve(path_bytes)
        .map_err(|_| BackupIssue::ResourceLimit)?;
    let mut scratch = FixedControlScratch::new(
        memory
            .reserve(FixedControlScratch::required_bytes())
            .map_err(|_| BackupIssue::ResourceLimit)?,
    )
    .map_err(|_| BackupIssue::Unknown)?;
    let (database, manifest) = if let Some(pair) = &target.direct_pair {
        (pair.database.clone(), pair.manifest.clone())
    } else {
        let storage = target.storage_root.as_ref().ok_or(BackupIssue::Unknown)?;
        let root = RootedDirectory::open_readonly_root(
            storage,
            super::locks::path_identity(storage).map_err(|_| BackupIssue::Changed)?,
        )?;
        let mut pointer = root.open_readonly_file(b"active.json")?;
        let before = FixedFileStamp::capture(&pointer, FixedControlKind::File)
            .map_err(|_| BackupIssue::Changed)?;
        if before.len() > 1024 {
            return Err(BackupIssue::Unknown);
        }
        let mut wire = [0u8; 1024];
        let mut filled = 0;
        loop {
            let n = pointer.read(&mut wire[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
            if filled == wire.len() {
                break;
            }
        }
        before.recheck(&pointer).map_err(|_| BackupIssue::Changed)?;
        before
            .recheck(&root.open_readonly_file(b"active.json")?)
            .map_err(|_| BackupIssue::Changed)?;
        let active: super::managed::ActiveGeneration =
            serde_json::from_slice(&wire[..filled]).map_err(|_| BackupIssue::Unknown)?;
        if active.schema_version != 2 {
            return Err(BackupIssue::Unknown);
        }
        super::layout::validate_generation_id(&active.generation_id)
            .map_err(|_| BackupIssue::Unknown)?;
        let generation = super::layout::ManagedLayout::new(storage)
            .generation(&active.generation_id)
            .map_err(|_| BackupIssue::Unknown)?;
        (generation.db_path(), generation.manifest_path())
    };
    let database = open_fixed_control(&database, FixedControlKind::File, false, &mut scratch)
        .map_err(|_| BackupIssue::Changed)?;
    let manifest = open_fixed_control(&manifest, FixedControlKind::File, false, &mut scratch)
        .map_err(|_| BackupIssue::Changed)?;
    Ok((database, manifest))
}

use super::inventory::MaintenanceTarget;
use super::locks::{FixedControlError, FixedControlProblem};
use super::maintenance_admission::{
    self as admission, PreparedCanonical, PreparedControlUpdate, PreparedPrivateMetadata,
    PreparedTargetBinding,
};
use super::maintenance_budget::MemoryBox;
use super::maintenance_candidate::CandidateReport;
use super::maintenance_journal::BackupRestoreRecord;
const BACKUP_CONTROL_BYTES: usize = 2048;
const SAVED_CURSOR_BYTES: usize = super::maintenance::CURSOR_BYTES;
const MARKER: &[u8] = b".restore-origin";
fn changed() -> FixedControlError {
    FixedControlError::Invalid(FixedControlProblem::Changed)
}
fn fixed_issue(e: BackupIssue) -> FixedControlError {
    match e {
        BackupIssue::ResourceLimit => {
            FixedControlError::Resource(super::maintenance_budget::ResourceLimit::CeilingExceeded)
        }
        BackupIssue::Io(kind) => std::io::Error::from(kind).into(),
        _ => changed(),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetainedAssociationPrefix {
    len: u64,
    digest: [u8; 32],
}
#[derive(Clone, Copy)]
struct RestoreControl {
    cursor: [u8; SAVED_CURSOR_BYTES],
    record: BackupRestoreRecord,
    database: FixedFileStamp,
    manifest: FixedFileStamp,
    association: [u8; 32],
    epoch: u64,
    source_receipt: Option<FixedFileStamp>,
    prefix: Option<RetainedAssociationPrefix>,
}
impl RestoreControl {
    fn encode(&self) -> [u8; BACKUP_CONTROL_BYTES] {
        let mut out = [0; BACKUP_CONTROL_BYTES];
        out[..8].copy_from_slice(b"CGBACK01");
        out[8..8 + SAVED_CURSOR_BYTES].copy_from_slice(&self.cursor);
        out[1120..1632].copy_from_slice(&self.record.encode());
        self.database
            .encode_checkpoint((&mut out[1632..1712]).try_into().unwrap());
        self.manifest
            .encode_checkpoint((&mut out[1712..1792]).try_into().unwrap());
        out[1792..1824].copy_from_slice(&self.association);
        out[1824..1832].copy_from_slice(&self.epoch.to_le_bytes());
        if let Some(receipt) = self.source_receipt {
            out[1832] = 1;
            receipt.encode_checkpoint((&mut out[1840..1920]).try_into().unwrap());
        }
        if let Some(prefix) = self.prefix {
            out[1920..1928].copy_from_slice(&prefix.len.to_le_bytes());
            out[1928..1960].copy_from_slice(&prefix.digest);
        }
        let digest = Sha256::digest(&out[..2016]);
        out[2016..].copy_from_slice(&digest);
        out
    }
    fn decode(wire: &[u8]) -> Result<Self, FixedControlError> {
        if wire.len() != BACKUP_CONTROL_BYTES
            || &wire[..8] != b"CGBACK01"
            || wire[1113..1120]
                .iter()
                .chain(wire[1833..1840].iter())
                .chain(wire[1960..2016].iter())
                .any(|b| *b != 0)
            || (wire[1920..1928] == [0; 8] && wire[1928..1960].iter().any(|b| *b != 0))
            || wire[1832] > 1
            || (wire[1832] == 0 && wire[1840..1920].iter().any(|b| *b != 0))
            || Sha256::digest(&wire[..2016])[..] != wire[2016..]
        {
            return Err(changed());
        }
        Ok(Self {
            cursor: wire[8..1113].try_into().unwrap(),
            record: BackupRestoreRecord::decode(wire[1120..1632].try_into().unwrap())
                .map_err(|_| changed())?,
            database: FixedFileStamp::decode_checkpoint(wire[1632..1712].try_into().unwrap())?,
            manifest: FixedFileStamp::decode_checkpoint(wire[1712..1792].try_into().unwrap())?,
            association: wire[1792..1824].try_into().unwrap(),
            epoch: u64::from_le_bytes(wire[1824..1832].try_into().unwrap()),
            source_receipt: if wire[1832] == 1 {
                Some(FixedFileStamp::decode_checkpoint(
                    wire[1840..1920].try_into().unwrap(),
                )?)
            } else {
                None
            },
            prefix: if wire[1920..1928] == [0; 8] {
                None
            } else {
                Some(RetainedAssociationPrefix {
                    len: u64::from_le_bytes(wire[1920..1928].try_into().unwrap()),
                    digest: wire[1928..1960].try_into().unwrap(),
                })
            },
        })
    }
}
pub(crate) fn is_backup_payload(payload: &[u8]) -> bool {
    payload.starts_with(b"CGBACK01")
        || super::maintenance_legacy_stream::LegacyStreamHeader::decode(payload).is_ok_and(
            |header| header.kind() == super::maintenance_legacy_stream::LegacyStreamKind::Backup,
        )
}

/// Warm state is advisory. Durable control and the actual fresh creator marker
/// determine restart behavior; dropping this state cannot authorize adoption.
#[derive(Default)]
pub(crate) struct BackupPreparationState {
    certified: Option<MemoryBox<PreparedBackup>>,
    created: Option<CreatedDestination>,
    selected: super::maintenance_legacy_association::SelectedBackupAssociationState,
    address: Option<BackupAddress>,
}
impl BackupPreparationState {
    pub(crate) fn has_parked_payloads(&self) -> bool {
        self.certified.is_some() || self.selected.job.is_some() || self.selected.transport.is_some()
    }
    pub(crate) fn clear(&mut self) {
        self.certified = None;
        self.created = None;
        self.selected.clear();
        self.address = None;
    }
    pub(crate) fn accept_certified(
        &mut self,
        backup: PreparedBackup,
        memory: &CleanupMemory,
    ) -> Result<(), BackupIssue> {
        self.certified =
            Some(MemoryBox::new(backup, memory).map_err(|_| BackupIssue::ResourceLimit)?);
        Ok(())
    }
}
struct CreatedDestination {
    directory: FileIdentity,
    epoch: u64,
    marker: FixedFileStamp,
}
enum BackupAction {
    PreparationPending,
    Association {
        step: MemoryBox<super::maintenance_legacy_stream::PreparedLegacyStreamStep>,
        committed: MemoryBox<Option<super::maintenance_legacy_stream::CommittedLegacyStreamStep>>,
    },
    Control {
        metadata: MemoryBox<PreparedPrivateMetadata>,
        update: MemoryBox<PreparedControlUpdate>,
        reset_stream: Option<u64>,
    },
    Create {
        ready: MemoryBox<PreparedCanonical>,
        metadata: MemoryBox<PreparedPrivateMetadata>,
        control: MemoryBox<RestoreControl>,
        marker_wire: [u8; 512],
        created: Option<CreatedDestination>,
    },
    Restore {
        ready: MemoryBox<PreparedCanonical>,
        metadata: MemoryBox<PreparedPrivateMetadata>,
        control: MemoryBox<RestoreControl>,
        database: File,
        manifest: File,
        source: RootedDirectory,
        payload: File,
        marker_wire: [u8; 512],
    },
    CleanupSource {
        ready: MemoryBox<PreparedCanonical>,
        metadata: MemoryBox<PreparedPrivateMetadata>,
        control: MemoryBox<RestoreControl>,
        restored: FixedFileStamp,
        source: RootedDirectory,
        receipt: Option<File>,
        scratch: MemoryBox<[u8; 4096]>,
    },
}
pub(crate) struct BackupWork {
    action: BackupAction,
    unknown: bool,
}
fn control_work(
    metadata: MemoryBox<PreparedPrivateMetadata>,
    control: &[u8],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<MemoryBox<BackupWork>, FixedControlError> {
    let reset_stream = if control.len() == SAVED_CURSOR_BYTES {
        let current = RestoreControl::decode(metadata.payload().ok_or_else(changed)?)?;
        if current.record.stage != 4 {
            return Err(changed());
        }
        verify_backup_prefix(&metadata, &current, memory, cancel)?
            .then(|| current.prefix.unwrap().len)
    } else {
        None
    };
    let update = boxed_backup_control(&metadata, control, memory)?;
    MemoryBox::new(
        BackupWork {
            unknown: false,
            action: BackupAction::Control {
                metadata,
                update,
                reset_stream,
            },
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn association_prefix_digest(
    metadata: &PreparedPrivateMetadata,
    len: u64,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<[u8; 32], FixedControlError> {
    let _floor = memory.reserve(8192).map_err(FixedControlError::Resource)?;
    if metadata.stream_len() != len {
        return Err(changed());
    }
    let mut hash = Sha256::new();
    let mut bytes = [0; 4096];
    let mut offset = 0u64;
    while offset < len {
        if cancel() {
            return Err(changed());
        }
        let n = (len - offset).min(bytes.len() as u64) as usize;
        metadata.read_stream_exact(offset, &mut bytes[..n])?;
        hash.update(&bytes[..n]);
        offset += n as u64;
    }
    Ok(hash.finalize().into())
}
fn retained_prefix_len(
    prefix: Option<RetainedAssociationPrefix>,
    stage: u8,
    stream_len: u64,
) -> Result<Option<u64>, FixedControlError> {
    let Some(prefix) = prefix else {
        return if stream_len == 0 {
            Ok(None)
        } else {
            Err(changed())
        };
    };
    if stage == 4 && stream_len == 0 {
        return Ok(None);
    }
    if stream_len != prefix.len {
        return Err(changed());
    }
    Ok(Some(prefix.len))
}
fn verify_backup_prefix(
    metadata: &PreparedPrivateMetadata,
    control: &RestoreControl,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<bool, FixedControlError> {
    let Some(len) =
        retained_prefix_len(control.prefix, control.record.stage, metadata.stream_len())?
    else {
        return Ok(false);
    };
    if association_prefix_digest(metadata, len, memory, cancel)? != control.prefix.unwrap().digest {
        return Err(changed());
    }
    Ok(true)
}

fn only_origin(directory: &RootedDirectory, scratch: &mut [u8; 4096]) -> std::io::Result<bool> {
    let mut known = true;
    match directory.visit_entries(scratch, &mut |name| {
        if name == super::maintenance_origin::ORIGIN_NAME {
            Ok(())
        } else {
            known = false;
            Err(io::ErrorKind::Interrupted.into())
        }
    }) {
        Ok(()) => Ok(true),
        Err(error) if !known && error.kind() == io::ErrorKind::Interrupted => Ok(false),
        Err(error) => Err(error),
    }
}
fn marker_wire(record: BackupRestoreRecord) -> [u8; 512] {
    // The pre-mkdir intent remains the immutable creation marker even when
    // later control records carry the actual new destination identity.
    let mut intent = record;
    intent.stage = 1;
    intent.destination = FileIdentity::from_fixed_words(0, 0);
    intent.encode()
}
fn creator(
    directory: &RootedDirectory,
    record: BackupRestoreRecord,
    expected_wire: &[u8; 512],
) -> Result<FixedFileStamp, FixedControlError> {
    if super::locks::created_file_nonce(&directory.pin_handle()?)? != Some(record.operation) {
        return Err(changed());
    }
    let file = directory.open_readonly_file(MARKER)?;
    let stamp = FixedFileStamp::capture(&file, FixedControlKind::File)?;
    if stamp.len() != 512 || super::locks::created_file_nonce(&file)? != Some(record.operation) {
        return Err(changed());
    }
    let mut wire = [0; 512];
    super::locks::read_fixed_range(&file, 0, &mut wire)?;
    if wire != *expected_wire {
        return Err(changed());
    }
    stamp.recheck(&file)?;
    stamp.recheck(&directory.open_readonly_file(MARKER)?)?;
    Ok(stamp)
}
// Construct charged retained values before entering the backup state-machine
// frames; large by-value admission Results never overlap their downstream work.
#[inline(never)]
fn boxed_backup_canonical(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
    required: bool,
) -> Result<Option<MemoryBox<PreparedCanonical>>, FixedControlError> {
    let ready = match admission::prepare_canonical(target, domain, expected, memory) {
        Ok(ready) => ready,
        Err(_) if !required => return Ok(None),
        Err(error) => return Err(error),
    };
    MemoryBox::new(ready, memory)
        .map(Some)
        .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn boxed_backup_metadata(
    target: &MaintenanceTarget,
    scope: &admission::RootOriginBinding,
    memory: &CleanupMemory,
    required: bool,
) -> Result<Option<MemoryBox<PreparedPrivateMetadata>>, FixedControlError> {
    let metadata = match admission::prepare_private_metadata(target, scope, memory) {
        Ok(metadata) => metadata,
        Err(_) if !required => return Ok(None),
        Err(error) => return Err(error),
    };
    MemoryBox::new(metadata, memory)
        .map(Some)
        .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn boxed_backup_control(
    metadata: &PreparedPrivateMetadata,
    control: &[u8],
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedControlUpdate>, FixedControlError> {
    let update = admission::prepare_metadata_control(
        &metadata.origin_scope(),
        metadata.committed().ok_or_else(changed)?,
        control,
        memory,
    )?;
    MemoryBox::new(update, memory).map_err(FixedControlError::Resource)
}

/// All source association and checksums are prepared off the mutation permit.
/// The record is recoverable intent, never an origin receipt for old bytes.
#[inline(never)]
pub(crate) fn prepare_work(
    target: &MaintenanceTarget,
    _binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    if cancel() {
        return Err(changed());
    }
    let Some(ready) =
        boxed_backup_canonical(target, domain, expected, memory, state.certified.is_some())?
    else {
        return Ok(None);
    };
    let Some(metadata) = boxed_backup_metadata(
        target,
        &ready.origin_scope(),
        memory,
        state.certified.is_some(),
    )?
    else {
        return Ok(None);
    };
    if !matches!(
        metadata.recovery(),
        super::maintenance_controls::RecoveryStep::Committed(_)
    ) {
        return Ok(None);
    }
    let Some(payload) = metadata.payload() else {
        return Ok(None);
    };
    if !payload.starts_with(b"CGBACK01") {
        prepare_selection(
            target, _binding, domain, expected, state, ready, metadata, memory, cancel,
        )
    } else {
        prepare_restore(
            target, domain, expected, state, ready, metadata, memory, cancel,
        )
    }
}
#[inline(never)]
// Keep borrowed domain/epoch separate from owned canonical/metadata capabilities and budget.
#[allow(clippy::too_many_arguments)]
fn prepare_selection(
    target: &MaintenanceTarget,
    _binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    if state.certified.is_none() {
        return prepare_proof(
            target, _binding, domain, expected, state, ready, metadata, memory, cancel,
        );
    }

    prepare_certified_selection(expected, state, ready, metadata, memory, cancel)
}

#[inline(never)]
fn prepare_certified_selection(
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    let payload = metadata.payload().ok_or_else(changed)?;
    let backup = state.certified.take().ok_or_else(changed)?;
    if backup.association.source_epoch() != expected
        || backup.plan.root != ready.origin_scope().root
    {
        return Err(changed());
    }
    let (mut cursor, prefix) = if let Some(cursor) =
        super::maintenance_candidate::idle_control_cursor(payload, ready.origin_scope())
            .map_err(|_| changed())?
    {
        if metadata.stream_len() != 0 {
            return Err(changed());
        }
        (cursor, None)
    } else {
        let header = super::maintenance_legacy_stream::LegacyStreamHeader::decode(payload)?;
        if header.kind() != super::maintenance_legacy_stream::LegacyStreamKind::Backup
            || !header.done()
            || metadata.stream_len() != header.committed_end()
            || header.committed_end() == 0
        {
            return Err(changed());
        }
        let prefix = RetainedAssociationPrefix {
            len: header.committed_end(),
            digest: association_prefix_digest(&metadata, header.committed_end(), memory, cancel)?,
        };
        (
            super::maintenance::MaintenanceCursor::decode(
                header.cursor(),
                ready.origin_scope().incarnation,
            )
            .map_err(|_| changed())?,
            Some(prefix),
        )
    };
    let mut address_bytes = [0; 128];
    if backup.plan.address.name().len() > address_bytes.len() {
        return Err(changed());
    }
    address_bytes[..backup.plan.address.name().len()].copy_from_slice(backup.plan.address.name());
    cursor.inspected(
        super::maintenance::WorkCategory::TemporaryOrBackup,
        super::maintenance::CursorAddress(address_bytes),
    );
    let cursor = cursor.encode();
    let mut operation = [0; 32];
    getrandom::fill(&mut operation).map_err(|_| changed())?;
    if operation == [0; 32] {
        return Err(changed());
    }
    let mut name = [0; 128];
    let address = backup.plan.address;
    if address.name().len() > name.len() {
        return Err(changed());
    }
    name[..address.name().len()].copy_from_slice(address.name());
    let control = RestoreControl {
        cursor,
        record: BackupRestoreRecord {
            stage: 1,
            source_epoch: expected.counter(),
            root: backup.plan.root,
            shard: backup.plan.shard,
            source: backup.plan.source,
            payload: backup.plan.payload,
            digest: backup.plan.payload_sha256,
            destination: FileIdentity::from_fixed_words(0, 0),
            operation,
            name,
            name_len: address.name().len() as u8,
        },
        database: backup.association.database_stamp(),
        manifest: backup.association.manifest_stamp(),
        association: backup.association.cache_certificate(),
        epoch: expected.counter(),
        source_receipt: None,
        prefix,
    };
    control_work(metadata, &control.encode(), memory, cancel).map(Some)
}
#[inline(never)]
// Preserve distinct authority ownership and native-proof stage inputs without an aggregate context.
#[allow(clippy::too_many_arguments)]
fn prepare_proof(
    target: &MaintenanceTarget,
    _binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    let Some((root, address)) =
        select_backup_proof_address(domain, state, &ready, &metadata, memory, cancel)?
    else {
        return Ok(None);
    };
    drop(metadata);
    drop(ready);
    let progress = advance_backup_association(
        target, domain, expected, state, root, address, memory, cancel,
    )?;
    finish_backup_association(
        target, _binding, domain, expected, state, address, progress, memory, cancel,
    )
}

// Cursor decoding and backup selection complete before native association
// begins. Neither cursor/control images nor certification outputs live above it.
#[inline(never)]
fn select_backup_proof_address(
    domain: &ReferenceDomain,
    state: &mut BackupPreparationState,
    ready: &PreparedCanonical,
    metadata: &PreparedPrivateMetadata,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<(RootedDirectory, BackupAddress)>, FixedControlError> {
    let payload = metadata.payload().ok_or_else(changed)?;

    let cursor = if let Some(cursor) =
        super::maintenance_candidate::idle_control_cursor(payload, ready.origin_scope())
            .map_err(|_| changed())?
    {
        cursor
    } else if let Ok(header) = super::maintenance_legacy_stream::LegacyStreamHeader::decode(payload)
    {
        if header.kind() != super::maintenance_legacy_stream::LegacyStreamKind::Backup {
            return Ok(None);
        }
        super::maintenance::MaintenanceCursor::decode(
            header.cursor(),
            ready.origin_scope().incarnation,
        )
        .map_err(|_| changed())?
    } else {
        return Ok(None);
    };
    let root =
        RootedDirectory::open_readonly_root(domain.artifact_root(), ready.origin_scope().root)?;
    let address = if let Some(address) = state.address {
        address
    } else {
        let Some(address) =
            select_missing_backup(&root, &cursor, memory, cancel).map_err(fixed_issue)?
        else {
            return Ok(None);
        };
        state.address = Some(address);
        address
    };
    Ok(Some((root, address)))
}

#[inline(never)]
// Keep actual native-source handles and epoch/budget controls explicit at this stack boundary.
#[allow(clippy::too_many_arguments)]
fn advance_backup_association(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    root: RootedDirectory,
    address: BackupAddress,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<super::maintenance_legacy_stream::AssociationSourceProgress, FixedControlError> {
    let progress = if state.selected.needs_source_handles() {
        let (database, manifest) = open_current_source(target, memory).map_err(fixed_issue)?;
        super::maintenance_legacy_stream::prepare_association_source(
            target,
            domain,
            expected,
            database,
            manifest,
            root,
            domain.artifact_root(),
            &target.repository_root,
            None,
            Some(address),
            &mut state.selected,
            memory,
            cancel,
        )?
    } else {
        drop(root);
        super::maintenance_legacy_stream::prepare_parked_association_source(
            target,
            domain,
            expected,
            domain.artifact_root(),
            &target.repository_root,
            None,
            Some(address),
            &mut state.selected,
            memory,
            cancel,
        )?
    };
    Ok(progress)
}

// Certification retains real handles only after the stream stage returned.
// Its recursive control preparation never retains a native association frame.
#[inline(never)]
// Separate owned source handles from certification state across the bounded recursion boundary.
#[allow(clippy::too_many_arguments)]
fn finish_backup_association(
    target: &MaintenanceTarget,
    _binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    address: BackupAddress,
    progress: super::maintenance_legacy_stream::AssociationSourceProgress,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    match progress {
        super::maintenance_legacy_stream::AssociationSourceProgress::Pending => {
            pending_work(memory).map(Some)
        }
        super::maintenance_legacy_stream::AssociationSourceProgress::Step(step) => {
            association_work(step, memory).map(Some)
        }
        super::maintenance_legacy_stream::AssociationSourceProgress::Complete {
            association,
            database,
            manifest,
        } => {
            let prepared = prepare_backup(
                domain,
                expected,
                address,
                association,
                database,
                manifest,
                memory,
                cancel,
            )
            .map_err(fixed_issue)?;
            state.address = None;
            state
                .accept_certified(prepared, memory)
                .map_err(fixed_issue)?;
            prepare_work(target, _binding, domain, expected, state, memory, cancel)
        }
    }
}
#[inline(never)]
fn boxed_restore_control(
    payload: &[u8],
    memory: &CleanupMemory,
) -> Result<MemoryBox<RestoreControl>, FixedControlError> {
    let control = RestoreControl::decode(payload)?;
    MemoryBox::new(control, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
// Restore stages carry separate canonical/metadata capabilities and cooperative resource controls.
#[allow(clippy::too_many_arguments)]
fn prepare_restore(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    let payload = metadata.payload().ok_or_else(changed)?;

    let control = boxed_restore_control(payload, memory)?;
    if control.record.root != ready.origin_scope().root || control.association == [0; 32] {
        return Err(changed());
    }
    verify_backup_prefix(&metadata, &control, memory, cancel)?;
    let address =
        BackupAddress::parse(&control.record.name[..usize::from(control.record.name_len)])
            .ok_or_else(changed)?;
    let canonical = address.canonical();
    let root = RootedDirectory::open_readonly_root(domain.artifact_root(), control.record.root)?;
    let shard = root.open_directory_readonly(&canonical[..2])?;
    if shard.identity() != control.record.shard {
        return Err(changed());
    }
    let (database, manifest) = open_current_source(target, memory).map_err(fixed_issue)?;
    control.database.recheck(&database)?;
    control.manifest.recheck(&manifest)?;
    if control.record.stage == 4 {
        let destination = shard.open_directory_readonly(&canonical)?;
        if destination.identity() != control.record.destination {
            return Err(changed());
        }
        creator(&destination, control.record, &marker_wire(control.record))?;
        let restored = destination.open_readonly_file(b"partition.json")?;
        if super::locks::file_identity(&restored).map_err(|_| changed())?
            != control.record.payload.identity()
        {
            return Err(changed());
        }
        verify_digest(
            &restored,
            control.record.payload.len(),
            control.record.digest,
            cancel,
        )?;
    }
    let source = match shard.open_directory_readonly(address.name()) {
        Ok(source) => source,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                && control.record.stage == 4
                && control.source_receipt.is_some() =>
        {
            return control_work(metadata, &control.cursor, memory, cancel).map(Some);
        }
        Err(error) => return Err(error.into()),
    };
    if source.identity() != control.record.source {
        return Err(changed());
    }
    if control.record.stage == 4 {
        return prepare_cleanup(
            ready, metadata, control, shard, source, database, manifest, canonical, expected,
            memory, cancel,
        );
    }
    if control.record.stage == 1 {
        return prepare_creation(
            ready, metadata, control, shard, source, canonical, expected, state, memory, cancel,
        );
    }
    let destination = shard.open_directory_readonly(&canonical)?;
    if destination.identity() != control.record.destination {
        return Err(changed());
    }
    creator(&destination, control.record, &marker_wire(control.record))?;
    if control.record.stage == 3 {
        return prepare_verified(
            metadata,
            control,
            root,
            source,
            address,
            destination,
            expected,
            memory,
            cancel,
        );
    }
    prepare_move(
        ready,
        metadata,
        control,
        source,
        database,
        manifest,
        destination,
        expected,
        memory,
        cancel,
    )
}
#[inline(never)]
// Cleanup retains distinct rooted handles, payload files and sealed control without changing layout.
#[allow(clippy::too_many_arguments)]
fn prepare_cleanup(
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: MemoryBox<RestoreControl>,
    shard: RootedDirectory,
    source: RootedDirectory,
    database: File,
    manifest: File,
    canonical: [u8; 64],
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    let Some(receipt_stamp) = control.source_receipt else {
        let mut work = control_work(metadata, &control.cursor, memory, cancel)?;
        work.unknown = true;
        return Ok(Some(work));
    };
    let mut scratch = [0; 4096];
    if !only_origin(&source, &mut scratch)? {
        let mut work = control_work(metadata, &control.cursor, memory, cancel)?;
        work.unknown = true;
        return Ok(Some(work));
    }
    let receipt = match source.open_readonly_file(super::maintenance_origin::ORIGIN_NAME) {
        Ok(receipt) => {
            receipt_stamp.recheck(&receipt)?;
            Some(receipt)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let destination = shard.open_directory_readonly(&canonical)?;
    if destination.identity() != control.record.destination {
        return Err(changed());
    }
    creator(&destination, control.record, &marker_wire(control.record))?;
    drop((database, manifest));
    let restored = destination.open_readonly_file(b"partition.json")?;
    if super::locks::file_identity(&restored).map_err(|_| changed())?
        != control.record.payload.identity()
    {
        return Err(changed());
    }
    verify_digest(
        &restored,
        control.record.payload.len(),
        control.record.digest,
        cancel,
    )?;
    let restored = FixedFileStamp::capture(&restored, FixedControlKind::File)?;
    control.epoch = expected.counter();
    MemoryBox::new(
        BackupWork {
            unknown: false,
            action: BackupAction::CleanupSource {
                ready,
                metadata,
                control,
                restored,
                source,
                receipt,
                scratch: MemoryBox::new(scratch, memory).map_err(FixedControlError::Resource)?,
            },
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
// Creation keeps actual source/shard handles separate from mutable restore control and budget.
#[allow(clippy::too_many_arguments)]
fn prepare_creation(
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: MemoryBox<RestoreControl>,
    shard: RootedDirectory,
    source: RootedDirectory,
    canonical: [u8; 64],
    expected: &ReferenceEpoch,
    state: &mut BackupPreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    if let Some(created) = state.created.take() {
        let directory = shard.open_directory_readonly(&canonical)?;
        if directory.identity() != created.directory
            || creator(&directory, control.record, &marker_wire(control.record))? != created.marker
            || created.epoch != expected.counter()
        {
            return Err(changed());
        }
        control.record.stage = 2;
        control.record.destination = created.directory;
        control.epoch = created.epoch;
        return control_work(metadata, &control.encode(), memory, cancel).map(Some);
    }
    match shard.open_directory_readonly(&canonical) {
        Ok(directory) => {
            let _marker = creator(&directory, control.record, &marker_wire(control.record))?;
            control.record.stage = 2;
            control.record.destination = directory.identity();
            control.epoch = expected.counter();
            return control_work(metadata, &control.encode(), memory, cancel).map(Some);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if expected.counter() != control.epoch {
        return Err(changed());
    }
    control
        .record
        .payload
        .recheck(&source.open_readonly_file(b"partition.json")?)?;
    let marker_wire = marker_wire(control.record);
    MemoryBox::new(
        BackupWork {
            unknown: false,
            action: BackupAction::Create {
                ready,
                metadata,
                control,
                marker_wire,
                created: None,
            },
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
// Destination verification retains separate rooted capabilities, source epoch and bounded resources.
#[allow(clippy::too_many_arguments)]
fn prepare_verified(
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: MemoryBox<RestoreControl>,
    root: RootedDirectory,
    source: RootedDirectory,
    address: BackupAddress,
    destination: RootedDirectory,
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    // An actual destination observation, rather than a stored usable claim,
    // confirms the exact old payload survived its no-replace move.
    let payload = destination.open_readonly_file(b"partition.json")?;
    if super::locks::file_identity(&payload).map_err(|_| changed())?
        != control.record.payload.identity()
    {
        return Err(changed());
    }
    verify_digest(
        &payload,
        control.record.payload.len(),
        control.record.digest,
        cancel,
    )?;
    let origin = super::maintenance_origin::observe_detached_artifact_origin(
        &root,
        &source,
        &payload,
        address.key(),
        memory,
        cancel,
    );
    let mut scratch = [0; 4096];
    if let Ok(super::maintenance_origin::OriginObservation::Observed(facts)) = origin {
        if facts.directory_created
            && facts.directory == source.identity()
            && facts.payload == control.record.payload.identity()
            && facts.byte_len == control.record.payload.len()
            && facts.payload_sha256 == control.record.digest
            && only_origin(&source, &mut scratch)?
        {
            let receipt = source.open_readonly_file(super::maintenance_origin::ORIGIN_NAME)?;
            let stamp = FixedFileStamp::capture(&receipt, FixedControlKind::File)?;
            if stamp.identity() == facts.receipt {
                control.source_receipt = Some(stamp);
            }
        }
    }
    control.record.stage = 4;
    control.epoch = expected.counter();
    control_work(metadata, &control.encode(), memory, cancel).map(Some)
}
#[inline(never)]
// No-replace transport keeps source/destination handles explicit alongside control and epoch.
#[allow(clippy::too_many_arguments)]
fn prepare_move(
    ready: MemoryBox<PreparedCanonical>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: MemoryBox<RestoreControl>,
    source: RootedDirectory,
    database: File,
    manifest: File,
    destination: RootedDirectory,
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<BackupWork>>, FixedControlError> {
    if control.record.stage != 2 {
        return Err(changed());
    }
    let payload = match source.open_readonly_file(b"partition.json") {
        Ok(payload) => {
            if expected.counter() != control.epoch {
                return Err(changed());
            }
            control.record.payload.recheck(&payload)?;
            verify_digest(
                &payload,
                control.record.payload.len(),
                control.record.digest,
                cancel,
            )?;
            match destination.open_readonly_file(b"partition.json") {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                _ => return Err(changed()),
            }
            payload
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let payload = destination.open_readonly_file(b"partition.json")?;
            if super::locks::file_identity(&payload).map_err(|_| changed())?
                != control.record.payload.identity()
            {
                return Err(changed());
            }
            verify_digest(
                &payload,
                control.record.payload.len(),
                control.record.digest,
                cancel,
            )?;
            control.record.stage = 3;
            control.epoch = expected.counter();
            return control_work(metadata, &control.encode(), memory, cancel).map(Some);
        }
        Err(error) => return Err(error.into()),
    };
    MemoryBox::new(
        BackupWork {
            unknown: false,
            action: BackupAction::Restore {
                ready,
                metadata,
                marker_wire: marker_wire(control.record),
                control,
                database,
                manifest,
                source,
                payload,
            },
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn pending_work(memory: &CleanupMemory) -> Result<MemoryBox<BackupWork>, FixedControlError> {
    MemoryBox::new(
        BackupWork {
            unknown: false,
            action: BackupAction::PreparationPending,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn association_work(
    step: MemoryBox<super::maintenance_legacy_stream::PreparedLegacyStreamStep>,
    memory: &CleanupMemory,
) -> Result<MemoryBox<BackupWork>, FixedControlError> {
    MemoryBox::new(
        BackupWork {
            unknown: false,
            action: BackupAction::Association {
                step,
                committed: MemoryBox::new(None, memory).map_err(FixedControlError::Resource)?,
            },
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}

fn verify_digest(
    file: &File,
    len: u64,
    expected: [u8; 32],
    cancel: &dyn Fn() -> bool,
) -> Result<(), FixedControlError> {
    let before = FixedFileStamp::capture(file, FixedControlKind::File)?;
    if before.len() != len {
        return Err(changed());
    }
    let mut bytes = [0; 4096];
    let mut offset = 0;
    let mut digest = Sha256::new();
    while offset < len {
        if cancel() {
            return Err(changed());
        }
        let n = usize::try_from((len - offset).min(bytes.len() as u64)).map_err(|_| changed())?;
        super::locks::read_fixed_range(file, offset, &mut bytes[..n])?;
        digest.update(&bytes[..n]);
        offset += n as u64;
    }
    before.recheck(file)?;
    if digest.finalize()[..] != expected {
        return Err(changed());
    }
    Ok(())
}
impl BackupWork {
    pub(crate) fn advance(
        &mut self,
        grant: &mut MutationGrant,
        _confirmation: Option<&super::maintenance_scheduler::LegacyQuiescenceConfirmation<'_>>,
    ) -> Result<CandidateReport, FixedControlError> {
        let mut report = CandidateReport {
            pending: true,
            unknown: self.unknown,
            reasons: u64::from(self.unknown),
            checked_categories: 1 << 1,
            inspected_entries: 1,
            ..Default::default()
        };
        match &mut self.action {
            BackupAction::PreparationPending => {}
            BackupAction::Association { step, committed } => {
                **committed = Some(step.advance(grant)?);
            }
            BackupAction::Control {
                metadata,
                update,
                reset_stream,
            } => {
                admission::with_private_metadata(
                    metadata,
                    grant,
                    update.effects() + usize::from(reset_stream.is_some()) * 2,
                    &mut |mutation, grant| {
                        if let Some(len) = reset_stream {
                            mutation.reset_stream(*len, grant)?;
                        }
                        mutation.publish_control(update, grant)
                    },
                )?;
            }
            BackupAction::Create {
                ready,
                metadata,
                control,
                marker_wire,
                created,
            } => {
                let address = BackupAddress::parse(
                    &control.record.name[..usize::from(control.record.name_len)],
                )
                .ok_or_else(changed)?;
                let canonical = address.canonical();
                let mut observed = None;
                admission::with_prepared_canonical(ready, grant, 30, &mut |mutation, grant| {
                    if mutation.directory().identity() != control.record.root
                        || mutation.epoch().before != control.epoch
                    {
                        return Err(changed());
                    }
                    admission::with_reserved_canonical_private_metadata(
                        metadata,
                        mutation,
                        grant,
                        0,
                        &mut |_, _| Ok(()),
                    )?;
                    let shard = mutation
                        .directory()
                        .open_directory_readonly(&canonical[..2])?;
                    if shard.identity() != control.record.shard {
                        return Err(changed());
                    }
                    let source = shard.open_directory_readonly(address.name())?;
                    if source.identity() != control.record.source {
                        return Err(changed());
                    }
                    control
                        .record
                        .payload
                        .recheck(&source.open_readonly_file(b"partition.json")?)?;
                    grant.check_time()?;
                    let directory = shard.create_directory(&canonical)?;
                    super::locks::set_created_file_nonce(
                        &directory.pin_handle()?,
                        &control.record.operation,
                    )?;
                    let marker = directory.create_new(MARKER)?;
                    super::locks::set_created_file_nonce(&marker, &control.record.operation)?;
                    super::locks::write_fixed_range(&marker, 0, marker_wire, grant)?;
                    directory.sync_namespace()?;
                    shard.sync_namespace()?;
                    let stamp = creator(&directory, control.record, marker_wire)?;
                    #[cfg(all(test, debug_assertions))]
                    fault::cut(control.record.root, BackupFault::AfterCreatorSync);
                    observed = Some(CreatedDestination {
                        directory: directory.identity(),
                        epoch: mutation.epoch().after,
                        marker: stamp,
                    });
                    Ok(())
                })?;
                *created = observed;
                report.mutated_entries = 1;
            }
            BackupAction::CleanupSource {
                ready,
                metadata,
                control,
                restored,
                source,
                receipt,
                scratch,
            } => {
                let address = BackupAddress::parse(
                    &control.record.name[..usize::from(control.record.name_len)],
                )
                .ok_or_else(changed)?;
                let canonical = address.canonical();
                admission::with_prepared_canonical(ready, grant, 30, &mut |mutation, grant| {
                    if mutation.directory().identity() != control.record.root
                        || mutation.epoch().before != control.epoch
                    {
                        return Err(changed());
                    }
                    admission::with_reserved_canonical_private_metadata(
                        metadata,
                        mutation,
                        grant,
                        0,
                        &mut |_, _| Ok(()),
                    )?;
                    let shard = mutation
                        .directory()
                        .open_directory_readonly(&canonical[..2])?;
                    if shard.identity() != control.record.shard
                        || shard.open_directory_readonly(address.name())?.identity()
                            != control.record.source
                    {
                        return Err(changed());
                    }
                    let destination = shard.open_directory_readonly(&canonical)?;
                    if destination.identity() != control.record.destination
                        || source.identity() != control.record.source
                    {
                        return Err(changed());
                    }
                    restored.recheck(&destination.open_readonly_file(b"partition.json")?)?;
                    if !source.contains_only(
                        Some(super::maintenance_origin::ORIGIN_NAME),
                        scratch,
                        grant,
                    )? {
                        return Err(changed());
                    }
                    let expected = control.source_receipt.ok_or_else(changed)?;
                    if let Some(file) = receipt {
                        expected.recheck(file)?;
                        expected.recheck(
                            &source.open_readonly_file(super::maintenance_origin::ORIGIN_NAME)?,
                        )?;
                        grant.check_time()?;
                        source.unlink_file(
                            super::maintenance_origin::ORIGIN_NAME,
                            expected.identity(),
                        )?;
                        source.sync_namespace()?;
                    } else {
                        match source.open_readonly_file(super::maintenance_origin::ORIGIN_NAME) {
                            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                            _ => return Err(changed()),
                        }
                    }
                    grant.check_time()?;
                    shard.remove_empty_directory(address.name(), control.record.source)?;
                    shard.sync_namespace()?;
                    Ok(())
                })?;
                report.mutated_entries = 1;
                report.reclaimed_entries = 1;
            }
            BackupAction::Restore {
                ready,
                metadata,
                control,
                database,
                manifest,
                source,
                payload,
                marker_wire,
            } => {
                let address = BackupAddress::parse(
                    &control.record.name[..usize::from(control.record.name_len)],
                )
                .ok_or_else(changed)?;
                let canonical = address.canonical();
                admission::with_prepared_canonical(ready, grant, 32, &mut |mutation, grant| {
                    if mutation.directory().identity() != control.record.root
                        || mutation.epoch().before != control.epoch
                    {
                        return Err(changed());
                    }
                    admission::with_reserved_canonical_private_metadata(
                        metadata,
                        mutation,
                        grant,
                        0,
                        &mut |_, _| Ok(()),
                    )?;
                    control.database.recheck(database)?;
                    control.manifest.recheck(manifest)?;
                    let shard = mutation
                        .directory()
                        .open_directory_readonly(&canonical[..2])?;
                    if shard.identity() != control.record.shard
                        || shard.open_directory_readonly(address.name())?.identity()
                            != control.record.source
                        || source.identity() != control.record.source
                    {
                        return Err(changed());
                    }
                    let destination = shard.open_directory_readonly(&canonical)?;
                    if destination.identity() != control.record.destination {
                        return Err(changed());
                    }
                    creator(&destination, control.record, marker_wire)?;
                    control.record.payload.recheck(payload)?;
                    control
                        .record
                        .payload
                        .recheck(&source.open_readonly_file(b"partition.json")?)?;
                    grant.check_time()?;
                    source.rename_file_no_replace(
                        b"partition.json",
                        &destination,
                        b"partition.json",
                        control.record.payload.identity(),
                    )?;
                    let restored = destination.open_readonly_file(b"partition.json")?;
                    if super::locks::file_identity(&restored).map_err(|_| changed())?
                        != control.record.payload.identity()
                    {
                        return Err(changed());
                    }
                    destination.sync_namespace()?;
                    source.sync_namespace()?;
                    shard.sync_namespace()?;
                    #[cfg(all(test, debug_assertions))]
                    fault::cut(control.record.root, BackupFault::AfterPayloadRestore);
                    Ok(())
                })?;
                report.mutated_entries = 1;
            }
        }
        Ok(report)
    }
}
pub(crate) fn finish_work(
    work: &mut BackupWork,
    state: &mut BackupPreparationState,
) -> Result<(), FixedControlError> {
    if let BackupAction::Association { committed, .. } = &mut work.action {
        if let Some(committed) = committed.take() {
            super::maintenance_legacy_stream::finish_association_source_step(
                &mut state.selected,
                committed,
            )?;
        }
    }
    if let BackupAction::CleanupSource { receipt, .. } = &mut work.action {
        drop(receipt.take());
    }
    if let BackupAction::Create { created, .. } = &mut work.action {
        state.created = created.take();
    }
    Ok(())
}

#[cfg(all(test, debug_assertions))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackupFault {
    AfterCreatorSync,
    AfterPayloadRestore,
}
#[cfg(all(test, debug_assertions))]
pub(crate) use fault::arm_backup_fault;
#[cfg(all(test, debug_assertions))]
mod fault {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    };
    type FaultCut = (FileIdentity, BackupFault, u64, bool);
    static CUTS: Mutex<[Option<FaultCut>; 8]> = Mutex::new([None; 8]);
    static NEXT: AtomicU64 = AtomicU64::new(1);
    pub(crate) struct BackupFaultGuard {
        slot: usize,
        nonce: u64,
    }
    impl Drop for BackupFaultGuard {
        fn drop(&mut self) {
            let mut cuts = CUTS.lock().unwrap();
            if cuts[self.slot].is_some_and(|(_, _, nonce, _)| nonce == self.nonce) {
                cuts[self.slot] = None;
            }
        }
    }
    impl BackupFaultGuard {
        pub(crate) fn reached(&self) -> bool {
            CUTS.lock().unwrap()[self.slot]
                .is_some_and(|(_, _, nonce, hit)| nonce == self.nonce && hit)
        }
    }
    pub(crate) fn arm_backup_fault(root: FileIdentity, point: BackupFault) -> BackupFaultGuard {
        let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut cuts = CUTS.lock().unwrap();
        let slot = cuts
            .iter()
            .position(Option::is_none)
            .expect("backup fixture fault slots");
        cuts[slot] = Some((root, point, nonce, false));
        BackupFaultGuard { slot, nonce }
    }
    pub(super) fn cut(root: FileIdentity, point: BackupFault) {
        let hit = {
            let mut cuts = CUTS.lock().unwrap();
            if let Some(slot) = cuts
                .iter()
                .position(|cut| cut.is_some_and(|(r, p, _, hit)| r == root && p == point && !hit))
            {
                cuts[slot].as_mut().unwrap().3 = true;
                true
            } else {
                false
            }
        };
        if hit {
            panic!("fixture backup restore cut: {point:?}");
        }
    }
}

/// Noncreating qualification observation of the actual durable role control.
/// A released cursor is distinguishable from an uninitialized installation.
#[cfg(all(test, debug_assertions))]
pub(crate) fn backup_control_stage_for_test(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<Option<u8>, FixedControlError> {
    let domain = if let Some(pair) = &target.direct_pair {
        ReferenceDomain::direct(&super::layout::DirectLayout::new(
            &pair.database,
            &pair.manifest,
        ))
    } else {
        ReferenceDomain::managed(&super::layout::ManagedLayout::new(
            target.storage_root.as_ref().ok_or_else(changed)?,
        ))
    }
    .map_err(|_| changed())?;
    let epoch = match super::maintenance_ownership::observe_reference_epoch(&domain)
        .map_err(|_| changed())?
    {
        super::maintenance_ownership::EpochObservation::Verified(epoch) => epoch,
        _ => return Err(changed()),
    };
    let ready = admission::prepare_canonical(target, &domain, &epoch, memory)?;
    let metadata = admission::prepare_private_metadata(target, &ready.origin_scope(), memory)?;
    match metadata.payload() {
        Some(payload) if payload.starts_with(b"CGBACK01") => {
            Ok(Some(RestoreControl::decode(payload)?.record.stage))
        }
        Some(payload) if payload.len() == SAVED_CURSOR_BYTES => {
            super::maintenance::MaintenanceCursor::decode(
                payload.try_into().unwrap(),
                ready.origin_scope().incarnation,
            )
            .map_err(|_| changed())?;
            Ok(Some(0))
        }
        _ => Ok(None),
    }
}

/// Bounded fair address discovery only. A historical-looking name or a missing
/// destination is never a substitute for selected native/cache association.
pub(crate) fn select_missing_backup(
    root: &RootedDirectory,
    cursor: &super::maintenance::MaintenanceCursor,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<BackupAddress>, BackupIssue> {
    let _floor = memory
        .reserve(16 * 1024)
        .map_err(|_| BackupIssue::ResourceLimit)?;
    let mut selection = cursor.selection();
    let mut root_scratch = [0; 4096];
    let mut shard_scratch = [0; 4096];
    root.visit_entries(&mut root_scratch, &mut |prefix| {
        if cancel() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if prefix.len() != 2
            || !prefix
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Ok(());
        }
        let shard = root.open_directory_readonly(prefix)?;
        shard.visit_entries(&mut shard_scratch, &mut |name| {
            if cancel() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let Some(address) = BackupAddress::parse(name) else {
                return Ok(());
            };
            let canonical = address.canonical();
            if &canonical[..2] != prefix || address.name().len() > 128 {
                return Ok(());
            }
            match shard.open_directory_readonly(&canonical) {
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return Ok(()),
            }
            let mut bytes = [0; 128];
            bytes[..address.name().len()].copy_from_slice(address.name());
            selection.observe(
                super::maintenance::WorkCategory::TemporaryOrBackup,
                super::maintenance::CursorAddress(bytes),
            );
            Ok(())
        })
    })
    .map_err(|error| {
        if error.kind() == io::ErrorKind::Interrupted {
            BackupIssue::Cancelled
        } else {
            BackupIssue::Io(error.kind())
        }
    })?;
    Ok(selection.selected().and_then(|(_, address)| {
        let len = address.0.iter().position(|byte| *byte == 0).unwrap_or(128);
        BackupAddress::parse(&address.0[..len])
    }))
}

#[cfg(test)]
mod retained_prefix_tests {
    use super::*;
    #[test]
    fn dc_backup_restore_prefix_rejects_foreign_tail_and_only_stage4_recovers_truncation() {
        let prefix = Some(RetainedAssociationPrefix {
            len: 4096,
            digest: [7; 32],
        });
        assert_eq!(retained_prefix_len(None, 1, 0).unwrap(), None);
        assert!(retained_prefix_len(None, 4, 1).is_err());
        for stage in 1..=4 {
            assert_eq!(
                retained_prefix_len(prefix, stage, 4096).unwrap(),
                Some(4096)
            );
            assert!(retained_prefix_len(prefix, stage, 4097).is_err());
            assert!(retained_prefix_len(prefix, stage, 4095).is_err());
            if stage < 4 {
                assert!(retained_prefix_len(prefix, stage, 0).is_err());
            }
        }
        assert_eq!(retained_prefix_len(prefix, 4, 0).unwrap(), None);
    }
    #[test]
    fn dc_backup_restore_control_preserves_sealed_prefix_and_legacy_zero_fields() {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dc-backup-prefix-{:032x}",
            u128::from_le_bytes(nonce)
        ));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let stamp = FixedFileStamp::capture(&file, FixedControlKind::File).unwrap();
        let id = stamp.identity();
        let address = format!(".replaced-{}-1", "a".repeat(64));
        let mut name = [0; 128];
        name[..address.len()].copy_from_slice(address.as_bytes());
        let mut control = RestoreControl {
            cursor: [0; SAVED_CURSOR_BYTES],
            record: BackupRestoreRecord {
                stage: 1,
                source_epoch: 7,
                root: id,
                shard: id,
                source: id,
                payload: stamp,
                digest: [3; 32],
                destination: id,
                operation: [4; 32],
                name,
                name_len: address.len() as u8,
            },
            database: stamp,
            manifest: stamp,
            association: [5; 32],
            epoch: 7,
            source_receipt: None,
            prefix: None,
        };
        assert_eq!(
            RestoreControl::decode(&control.encode()).unwrap().prefix,
            None
        );
        control.prefix = Some(RetainedAssociationPrefix {
            len: 6144,
            digest: [6; 32],
        });
        let encoded = control.encode();
        assert_eq!(
            RestoreControl::decode(&encoded).unwrap().prefix,
            control.prefix
        );
        assert!(encoded[1960..2016].iter().all(|byte| *byte == 0));
        let mut zero_len = encoded;
        zero_len[1920..1928].fill(0);
        let digest = Sha256::digest(&zero_len[..2016]);
        zero_len[2016..].copy_from_slice(&digest);
        assert!(RestoreControl::decode(&zero_len).is_err());
        let mut foreign_reserved = encoded;
        foreign_reserved[1960] = 1;
        let digest = Sha256::digest(&foreign_reserved[..2016]);
        foreign_reserved[2016..].copy_from_slice(&digest);
        assert!(RestoreControl::decode(&foreign_reserved).is_err());
        let mut corrupt = encoded;
        corrupt[1928] ^= 1;
        assert!(RestoreControl::decode(&corrupt).is_err());
        drop(file);
        std::fs::remove_file(path).unwrap();
    }
}
