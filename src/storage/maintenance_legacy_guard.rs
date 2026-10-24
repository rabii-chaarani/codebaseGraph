//! Administrative legacy quiescence. Control coverage is independent of payload
//! origin. Only actual current OS exclusion plus a bound operator confirmation
//! can enroll an old scope; observations and serialized versions grant nothing.
use super::layout::DirectLayout;
use super::locks::{
    self, FileIdentity, FixedControlError, FixedControlKind as Kind,
    FixedControlProblem as Problem, FixedControlScratch, FixedFileStamp, FixedLockedFile, LockMode,
    LockedFile, RootedDirectory, WriterLease,
};
use super::maintenance_budget::{CleanupMemory, MutationGrant};
use super::maintenance_controls as codec;
use crate::error::NativeError;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Path, PathBuf};

pub(super) const ADMIN_GATE: &[u8] = b".legacy-root-cohort.use.lock";
pub(super) const ADMIN_STATE: &[u8] = b".legacy-root-cohort.state";
pub(super) const ADMIN_OWNER: &[u8] = b".legacy-root-cohort.owner.lock";
pub(super) const GATE: &[u8] = b".legacy-source.use.lock";
pub(super) const STATE: &[u8] = b".legacy-source.state";
pub(super) const OWNER: &[u8] = b".legacy-source.owner.lock";
pub(super) const BINDING_BYTES: usize = 1024;
pub(super) const RECORD_BYTES: usize = 1024;
pub(super) const SLOT0: u64 = BINDING_BYTES as u64;
pub(super) const SLOT1: u64 = SLOT0 + codec::SLOT_BYTES as u64;
pub(super) const SELECTOR: u64 = SLOT1 + codec::SLOT_BYTES as u64;
pub(super) const STATE_BYTES: u64 = SELECTOR + codec::SELECTOR_BYTES as u64;
pub(super) fn invalid(p: Problem) -> FixedControlError {
    FixedControlError::Invalid(p)
}
pub(super) fn primary(reason: &str) -> NativeError {
    NativeError::InvalidInput(reason.to_owned())
}
pub(super) fn id(out: &mut [u8], v: FileIdentity) {
    let (a, b) = v.fixed_words();
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..16].copy_from_slice(&b.to_le_bytes());
}
pub(super) fn read_id(bytes: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    )
}
pub(super) fn checksum(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let hash = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
}
pub(super) fn checked(bytes: &[u8]) -> bool {
    let end = bytes.len() - 32;
    Sha256::digest(&bytes[..end])[..] == bytes[end..]
}
pub(super) fn component(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= 255
        && bytes != b"."
        && bytes != b".."
        && !bytes.iter().any(|b| matches!(b, 0 | b'/' | b'\\'))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyScopeKind {
    DefaultDirect,
    ManagedGeneration,
    NativeStaging,
    ManagedAdministration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyPhase {
    Active,
    Preparing,
    Retiring,
    Removed,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Binding {
    pub(super) kind: LegacyScopeKind,
    pub(super) root: FileIdentity,
    pub(super) gate: FileIdentity,
    pub(super) state: FileIdentity,
    pub(super) owner: FileIdentity,
    pub(super) nonce: [u8; 32],
    pub(super) owner_nonce: [u8; 32],
    pub(super) key: [u8; 64],
    pub(super) database: [u8; 256],
    pub(super) database_len: usize,
    pub(super) cache: [u8; 256],
    pub(super) cache_len: usize,
    pub(super) repository: FileIdentity,
}
impl Binding {
    pub(super) fn encode(self) -> [u8; BINDING_BYTES] {
        let mut out = [0; BINDING_BYTES];
        out[..8].copy_from_slice(b"CGLGB001");
        out[8] = match self.kind {
            LegacyScopeKind::DefaultDirect => 1,
            LegacyScopeKind::ManagedGeneration => 2,
            LegacyScopeKind::NativeStaging => 3,
            LegacyScopeKind::ManagedAdministration => 4,
        };
        id(&mut out[16..32], self.root);
        id(&mut out[32..48], self.gate);
        id(&mut out[48..64], self.state);
        id(&mut out[64..80], self.owner);
        out[80..112].copy_from_slice(&self.nonce);
        out[112..144].copy_from_slice(&self.owner_nonce);
        out[144..208].copy_from_slice(&self.key);
        out[208..210].copy_from_slice(&(self.database_len as u16).to_le_bytes());
        out[210..212].copy_from_slice(&(self.cache_len as u16).to_le_bytes());
        out[216..472].copy_from_slice(&self.database);
        out[472..728].copy_from_slice(&self.cache);
        id(&mut out[728..744], self.repository);
        checksum(&mut out);
        out
    }
    pub(super) fn decode(bytes: &[u8; BINDING_BYTES]) -> Result<Self, FixedControlError> {
        if &bytes[..8] != b"CGLGB001"
            || !checked(bytes)
            || bytes[9..16]
                .iter()
                .chain(bytes[212..216].iter())
                .chain(bytes[744..992].iter())
                .any(|b| *b != 0)
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let kind = match bytes[8] {
            1 => LegacyScopeKind::DefaultDirect,
            2 => LegacyScopeKind::ManagedGeneration,
            3 => LegacyScopeKind::NativeStaging,
            4 => LegacyScopeKind::ManagedAdministration,
            _ => return Err(invalid(Problem::InvalidRecord)),
        };
        let database_len = u16::from_le_bytes(bytes[208..210].try_into().unwrap()) as usize;
        let cache_len = u16::from_le_bytes(bytes[210..212].try_into().unwrap()) as usize;
        let database: [u8; 256] = bytes[216..472].try_into().unwrap();
        let cache: [u8; 256] = bytes[472..728].try_into().unwrap();
        if database_len > 255
            || cache_len > 255
            || (kind != LegacyScopeKind::NativeStaging && !component(&database[..database_len]))
            || !component(&cache[..cache_len])
            || database[database_len..]
                .iter()
                .chain(cache[cache_len..].iter())
                .any(|b| *b != 0)
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        Ok(Self {
            kind,
            root: read_id(&bytes[16..32]),
            gate: read_id(&bytes[32..48]),
            state: read_id(&bytes[48..64]),
            owner: read_id(&bytes[64..80]),
            nonce: bytes[80..112].try_into().unwrap(),
            owner_nonce: bytes[112..144].try_into().unwrap(),
            key: bytes[144..208].try_into().unwrap(),
            database,
            database_len,
            cache,
            cache_len,
            repository: read_id(&bytes[728..744]),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Record {
    pub(super) phase: LegacyPhase,
    pub(super) version: u64,
    pub(super) member: FileIdentity,
    pub(super) database: Option<FixedFileStamp>,
    pub(super) manifest: Option<FixedFileStamp>,
    pub(super) cache: FileIdentity,
    pub(super) root_use: FileIdentity,
    pub(super) confirmation: [u8; 32],
    pub(super) job: [u8; 32],
    pub(super) member_name: [u8; 128],
    pub(super) member_len: usize,
    pub(super) freeze_cache: bool,
    pub(super) operator_generation: u64,
    pub(super) operator_pending: bool,
    pub(super) pending_source_stamp: Option<FixedFileStamp>,
    pub(super) root_default_source: Option<super::maintenance_admission::LegacyConfirmationSource>,
}
impl Record {
    pub(super) fn encode(self) -> [u8; RECORD_BYTES] {
        let mut out = [0; RECORD_BYTES];
        out[..8].copy_from_slice(b"CGLGR002");
        out[8] = match self.phase {
            LegacyPhase::Active => 0,
            LegacyPhase::Preparing => 1,
            LegacyPhase::Retiring => 2,
            LegacyPhase::Removed => 3,
        };
        out[9] = u8::from(self.freeze_cache)
            | if self.database.is_some() { 2 } else { 0 }
            | if self.manifest.is_some() { 4 } else { 0 }
            | if self.operator_pending { 8 } else { 0 }
            | if self.pending_source_stamp.is_some() {
                16
            } else {
                0
            }
            | if self.root_default_source.is_some() {
                32
            } else {
                0
            };
        out[10..12].copy_from_slice(&(self.member_len as u16).to_le_bytes());
        out[16..24].copy_from_slice(&self.version.to_le_bytes());
        id(&mut out[24..40], self.member);
        if let Some(stamp) = self.database {
            stamp.encode_checkpoint((&mut out[40..120]).try_into().unwrap());
        }
        if let Some(stamp) = self.manifest {
            stamp.encode_checkpoint((&mut out[120..200]).try_into().unwrap());
        }
        id(&mut out[200..216], self.cache);
        id(&mut out[216..232], self.root_use);
        out[232..264].copy_from_slice(&self.confirmation);
        out[264..296].copy_from_slice(&self.job);
        out[296..424].copy_from_slice(&self.member_name);
        out[424..432].copy_from_slice(&self.operator_generation.to_le_bytes());
        if let Some(stamp) = self.pending_source_stamp {
            stamp.encode_checkpoint((&mut out[432..512]).try_into().unwrap());
        }
        if let Some(source) = self.root_default_source {
            source.encode_checkpoint((&mut out[512..1024]).try_into().unwrap());
        }
        out
    }
    pub(super) fn decode(bytes: &[u8], version: u64) -> Result<Self, FixedControlError> {
        if bytes.len() != RECORD_BYTES
            || &bytes[..8] != b"CGLGR002"
            || bytes[9] > 63
            || bytes[12..16].iter().any(|b| *b != 0)
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let phase = match bytes[8] {
            0 => LegacyPhase::Active,
            1 => LegacyPhase::Preparing,
            2 => LegacyPhase::Retiring,
            3 => LegacyPhase::Removed,
            _ => return Err(invalid(Problem::InvalidRecord)),
        };
        let member_len = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
        let member_name: [u8; 128] = bytes[296..424].try_into().unwrap();
        if member_len > 128
            || member_name[member_len..].iter().any(|b| *b != 0)
            || member_len > 0 && !component(&member_name[..member_len])
            || u64::from_le_bytes(bytes[16..24].try_into().unwrap()) != version
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        Ok(Self {
            phase,
            version,
            member: read_id(&bytes[24..40]),
            database: if bytes[9] & 2 != 0 {
                Some(FixedFileStamp::decode_checkpoint(
                    (&bytes[40..120]).try_into().unwrap(),
                )?)
            } else {
                if bytes[40..120].iter().any(|b| *b != 0) {
                    return Err(invalid(Problem::InvalidRecord));
                }
                None
            },
            manifest: if bytes[9] & 4 != 0 {
                Some(FixedFileStamp::decode_checkpoint(
                    (&bytes[120..200]).try_into().unwrap(),
                )?)
            } else {
                if bytes[120..200].iter().any(|b| *b != 0) {
                    return Err(invalid(Problem::InvalidRecord));
                }
                None
            },
            cache: read_id(&bytes[200..216]),
            root_use: read_id(&bytes[216..232]),
            confirmation: bytes[232..264].try_into().unwrap(),
            job: bytes[264..296].try_into().unwrap(),
            member_name,
            member_len,
            freeze_cache: bytes[9] & 1 == 1,
            operator_generation: u64::from_le_bytes(bytes[424..432].try_into().unwrap()),
            operator_pending: bytes[9] & 8 != 0,
            pending_source_stamp: if bytes[9] & 16 != 0 {
                Some(FixedFileStamp::decode_checkpoint(
                    (&bytes[432..512]).try_into().unwrap(),
                )?)
            } else {
                if bytes[432..512].iter().any(|v| *v != 0) {
                    return Err(invalid(Problem::InvalidRecord));
                }
                None
            },
            root_default_source: if bytes[9] & 32 != 0 {
                Some(
                    super::maintenance_admission::LegacyConfirmationSource::decode_checkpoint(
                        (&bytes[512..1024]).try_into().unwrap(),
                    )?,
                )
            } else {
                if bytes[512..1024].iter().any(|v| *v != 0) {
                    return Err(invalid(Problem::InvalidRecord));
                }
                None
            },
        })
    }
}
#[derive(Clone, Copy)]
pub(super) struct Observation {
    pub(super) binding: Binding,
    pub(super) record: Record,
    pub(super) commit: codec::CommitFact,
    pub(super) state_stamp: FixedFileStamp,
    pub(super) pending: bool,
}
pub(super) fn observe_files(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
) -> Result<Observation, FixedControlError> {
    observe_files_named(root, gate, state, OWNER)
}
pub(super) fn observe_files_named(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
) -> Result<Observation, FixedControlError> {
    observe_source_snapshot(root, gate, state, owner_name, false)
}
fn observe_source_snapshot(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
    allow_pending: bool,
) -> Result<Observation, FixedControlError> {
    let gate_stamp = FixedFileStamp::capture(gate, Kind::File)?;
    let state_stamp = FixedFileStamp::capture(state, Kind::File)?;
    if state_stamp.len() != STATE_BYTES && state_stamp.len() != SOURCE_CAPSULE_STATE_BYTES {
        return Err(invalid(Problem::InvalidRecord));
    }
    let mut binding_bytes = [0; BINDING_BYTES];
    locks::read_fixed_range(state, 0, &mut binding_bytes)?;
    let binding = Binding::decode(&binding_bytes)?;
    if binding.root != root.identity()
        || binding.gate != gate_stamp.identity()
        || binding.state != state_stamp.identity()
        || locks::read_coordination_incarnation(gate)
            .map_err(|_| invalid(Problem::InvalidRecord))?
            != Some(binding.nonce)
    {
        return Err(invalid(Problem::Changed));
    }
    let owner = root.open_readonly_file(owner_name)?;
    let owner_stamp = FixedFileStamp::capture(&owner, Kind::File)?;
    if owner_stamp.identity() != binding.owner
        || locks::read_coordination_incarnation(&owner)
            .map_err(|_| invalid(Problem::InvalidRecord))?
            != Some(binding.owner_nonce)
    {
        return Err(invalid(Problem::Changed));
    }
    let mut slots = [[0; codec::SLOT_BYTES]; 2];
    let mut selector = [0; codec::SELECTOR_BYTES];
    locks::read_fixed_range(state, SLOT0, &mut slots[0])?;
    locks::read_fixed_range(state, SLOT1, &mut slots[1])?;
    locks::read_fixed_range(state, SELECTOR, &mut selector)?;
    let sf = slots
        .each_ref()
        .map(|bytes| codec::decode_slot(bytes, &binding.nonce).ok());
    let commit = codec::decode_commit(&selector[..codec::COMMIT_BYTES], &binding.nonce).ok();
    let intent = codec::decode_intent(&selector[codec::COMMIT_BYTES..], &binding.nonce).ok();
    if selector[codec::COMMIT_BYTES..]
        .iter()
        .any(|byte| *byte != 0)
        && intent.is_none()
    {
        return Err(invalid(Problem::InvalidRecord));
    }
    let (commit, pending) = match codec::recovery_step(commit, intent, sf) {
        codec::RecoveryStep::Committed(c) if !allow_pending || intent.is_none() => (c, false),
        codec::RecoveryStep::Committed(_) if allow_pending => {
            let i = intent.ok_or_else(|| invalid(Problem::InvalidRecord))?;
            let before = sf[1 - i.target].ok_or_else(|| invalid(Problem::InvalidRecord))?;
            if before.generation != i.before {
                return Err(invalid(Problem::InvalidRecord));
            }
            (
                codec::CommitFact {
                    generation: i.before,
                    active: 1 - i.target,
                    slot_digest: before.digest,
                },
                true,
            )
        }
        codec::RecoveryStep::RewriteIntendedSlot(i)
        | codec::RecoveryStep::CommitIntendedSlot(i)
            if allow_pending =>
        {
            let intended =
                &selector[codec::COMMIT_BYTES + 96..codec::COMMIT_BYTES + 96 + codec::SLOT_BYTES];
            let intended_fact = codec::decode_slot(intended, &binding.nonce)
                .map_err(|_| invalid(Problem::InvalidRecord))?;
            if intended_fact.payload_len != RECORD_BYTES || intended_fact.digest != i.slot_digest {
                return Err(invalid(Problem::InvalidRecord));
            }
            Record::decode(&intended[56..56 + RECORD_BYTES], i.after)?;
            let before = sf[1 - i.target].ok_or_else(|| invalid(Problem::InvalidRecord))?;
            if before.generation != i.before {
                return Err(invalid(Problem::InvalidRecord));
            }
            (
                codec::CommitFact {
                    generation: i.before,
                    active: 1 - i.target,
                    slot_digest: before.digest,
                },
                true,
            )
        }
        _ => return Err(invalid(Problem::Busy)),
    };
    let fact = sf[commit.active].ok_or_else(|| invalid(Problem::InvalidRecord))?;
    if fact.payload_len != RECORD_BYTES {
        return Err(invalid(Problem::InvalidRecord));
    }
    let record_bytes: [u8; RECORD_BYTES] = slots[commit.active][56..56 + RECORD_BYTES]
        .try_into()
        .unwrap();
    let record = Record::decode(&record_bytes, commit.generation)?;
    if let Some(capsule) = read_creation_capsule(root, state)? {
        if capsule.template.kind != binding.kind
            || capsule.template.repository != binding.repository
            || capsule.template.key != binding.key
            || capsule.template.database != binding.database
            || capsule.template.database_len != binding.database_len
            || capsule.template.cache != binding.cache
            || capsule.template.cache_len != binding.cache_len
        {
            return Err(invalid(Problem::Changed));
        }
        let mut hash = Sha256::new();
        hash.update(b"CGLegacySourceBirthCommitted01");
        hash.update(capsule.permission.job);
        hash.update(binding_bytes);
        let final_nonce: [u8; 32] = hash.finalize().into();
        if locks::created_file_nonce(state)? != Some(final_nonce) {
            return Err(invalid(Problem::Busy));
        }
    }
    state_stamp.recheck(state)?;
    gate_stamp.recheck(gate)?;
    Ok(Observation {
        binding,
        record,
        commit,
        state_stamp,
        pending,
    })
}
/// A current OS Shared use lease; it is retained through producer/native use.
#[derive(Debug)]
pub(crate) struct LegacySourceUseGuard {
    gate: LockedFile,
    state: File,
    _root_pin: File,
    state_stamp: FixedFileStamp,
    root: PathBuf,
    nonce: [u8; 32],
}
#[derive(Debug)]
pub(crate) struct LegacyNativeUseGuard {
    use_guard: LegacySourceUseGuard,
    database: PathBuf,
}
fn primary_observe(
    parent: &Path,
) -> Result<Option<(LegacySourceUseGuard, Binding, Record)>, NativeError> {
    let path = parent.join(std::str::from_utf8(GATE).unwrap());
    let gate = match locks::try_open_existing_locked(&path, LockMode::Shared) {
        Ok(Some(g)) => g,
        Ok(None) => return Err(primary("legacy source transition is busy")),
        Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    // Unrecognized names never become administrative authority or a new primary
    // requirement. Once a genuine bound cohort is recognized, torn state defers.
    let Some(nonce) = gate.coordination_incarnation()? else {
        return Ok(None);
    };
    let root = RootedDirectory::open_readonly_root(parent, locks::path_identity(parent)?)?;
    let state = match root.open_readonly_file(STATE) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let gate_file = root.open_readonly_file(GATE)?;
    let mut prefix = [0; BINDING_BYTES];
    if locks::read_fixed_range(&state, 0, &mut prefix).is_err() {
        return Ok(None);
    }
    let binding = match Binding::decode(&prefix) {
        Ok(b) if b.nonce == nonce => b,
        _ => return Ok(None),
    };
    let observed = match observe_files(&root, &gate_file, &state) {
        Ok(o) => o,
        Err(_) if recognized_source_binding(&root, &gate_file, &state, OWNER) => {
            return Err(primary("legacy source publication requires recovery"))
        }
        Err(_) => return Ok(None),
    };
    if binding.gate != gate.identity()? || !gate.is_current()? {
        return Err(primary("legacy source gate changed"));
    }
    let guard = LegacySourceUseGuard {
        gate,
        state,
        _root_pin: root.pin_handle()?,
        state_stamp: observed.state_stamp,
        root: parent.to_owned(),
        nonce,
    };
    Ok(Some((guard, observed.binding, observed.record)))
}
pub(crate) fn before_artifact_producer(
    root: &Path,
) -> Result<Option<LegacySourceUseGuard>, NativeError> {
    let Some(parent) = root.parent() else {
        return Ok(None);
    };
    let Some((guard, binding, record)) = primary_observe(parent)? else {
        return Ok(None);
    };
    if root.file_name().map(|n| n.as_encoded_bytes()) != Some(&binding.cache[..binding.cache_len]) {
        return Ok(None);
    }
    if (record.phase != LegacyPhase::Active || record.operator_pending)
        && (binding.kind == LegacyScopeKind::DefaultDirect || record.freeze_cache)
    {
        return Err(primary("legacy source is closed for maintenance"));
    }
    if binding.kind == LegacyScopeKind::ManagedGeneration {
        return Ok(None);
    }
    Ok(Some(guard))
}
pub(crate) fn before_native_open(
    database: &Path,
) -> Result<Option<LegacyNativeUseGuard>, NativeError> {
    let Some(parent) = database.parent() else {
        return Ok(None);
    };
    if let Some((guard, binding, record)) = primary_observe(parent)? {
        if binding.kind == LegacyScopeKind::DefaultDirect
            && database.file_name().map(|n| n.as_encoded_bytes())
                == Some(&binding.database[..binding.database_len])
        {
            if record.phase != LegacyPhase::Active || record.operator_pending {
                return Err(primary(
                    "legacy source is closed for maintenance; rebuild or recovery is required",
                ));
            }
            return Ok(Some(LegacyNativeUseGuard {
                use_guard: guard,
                database: database.to_owned(),
            }));
        }
    }
    if database.file_name().is_some_and(|n| n == "graph.ldb")
        && parent
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "generations")
    {
        if let Some(storage) = parent.parent().and_then(Path::parent) {
            if let Some((guard, binding, record)) = primary_observe(storage)? {
                if binding.kind == LegacyScopeKind::ManagedGeneration {
                    let matches = record.member_len > 0
                        && parent.file_name().map(|n| n.as_encoded_bytes())
                            == Some(&record.member_name[..record.member_len]);
                    if (record.phase != LegacyPhase::Active || record.operator_pending) && matches {
                        return Err(primary("legacy generation is closed for maintenance"));
                    }
                    if !matches {
                        return Ok(None);
                    }
                    return Ok(Some(LegacyNativeUseGuard {
                        use_guard: guard,
                        database: database.to_owned(),
                    }));
                }
            }
        }
    }
    Ok(None)
}
pub(crate) fn recheck_before_native(
    guard: Option<&LegacyNativeUseGuard>,
    database: &Path,
) -> Result<(), NativeError> {
    if let Some(guard) = guard {
        if guard.database != database
            || !guard.use_guard.gate.is_current()?
            || guard
                .use_guard
                .state_stamp
                .recheck(&guard.use_guard.state)
                .is_err()
            || locks::read_coordination_incarnation(&guard.use_guard.gate_file()?)?
                != Some(guard.use_guard.nonce)
        {
            return Err(primary("legacy native source changed before open"));
        }
        Ok(())
    } else {
        let _temporary = before_native_open(database)?;
        Ok(())
    }
}
impl LegacySourceUseGuard {
    fn gate_file(&self) -> Result<File, NativeError> {
        Ok(File::open(
            self.root.join(std::str::from_utf8(GATE).unwrap()),
        )?)
    }
}

pub(super) const STAGING_GATE: &[u8] = b".legacy-staging.use.lock";
pub(super) const STAGING_STATE: &[u8] = b".legacy-staging.state";
pub(super) const STAGING_OWNER: &[u8] = b".legacy-staging.owner.lock";
/// The ordinary staging producer retains this Shared kernel lease through the
/// last native COPY reader and disposal of its staged result.
#[derive(Debug)]
pub(crate) struct LegacyStagingProducerGuard {
    _gate: LockedFile,
    _root: File,
}
pub(crate) fn before_staging_producer(
    path: &Path,
) -> Result<Option<LegacyStagingProducerGuard>, NativeError> {
    let staging = if path.file_name().is_some_and(|n| n == "native-staging") {
        path
    } else if path
        .parent()
        .is_some_and(|p| p.file_name().is_some_and(|n| n == "native-staging"))
    {
        path.parent().unwrap()
    } else {
        return Ok(None);
    };
    let Some(state_path) = staging
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == ".codebaseGraph"))
    else {
        return Ok(None);
    };
    // The conventional address alone grants no cleanup authority. Publishing a
    // producer coordination lease is safe even before administrative enrollment;
    // it makes already-running upgraded staging producers visible to enrollment.
    let root = RootedDirectory::open_readonly_root(state_path, locks::path_identity(state_path)?)?;
    let gate_path = state_path.join(std::str::from_utf8(STAGING_GATE).unwrap());
    let gate = match locks::open_existing_shared_coordination(&gate_path) {
        Ok(gate) => gate,
        Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            match locks::create_new_coordination_in(&root, STAGING_GATE, &gate_path) {
                Ok(gate) => {
                    let mut nonce = [0; 32];
                    getrandom::fill(&mut nonce)
                        .map_err(|_| primary("legacy staging nonce unavailable"))?;
                    gate.initialize_creation_incarnation(nonce)?;
                    gate.share_after_creation()?
                }
                Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    locks::open_existing_shared_coordination(&gate_path)?
                }
                Err(e) => return Err(e),
            }
        }
        Err(e) => return Err(e),
    };
    if !gate.is_current()? {
        return Err(primary("legacy staging gate changed"));
    }
    if let (Ok(state), Ok(gate_file)) = (
        root.open_readonly_file(STAGING_STATE),
        root.open_readonly_file(STAGING_GATE),
    ) {
        if let Ok(observed) = observe_files_named(&root, &gate_file, &state, STAGING_OWNER) {
            if observed.binding.kind == LegacyScopeKind::NativeStaging
                && observed.binding.gate == gate.identity()?
                && (observed.record.phase != LegacyPhase::Active
                    || observed.record.operator_pending)
            {
                return Err(primary("legacy staging source is closed for maintenance"));
            }
        } else if recognized_source_binding(&root, &gate_file, &state, STAGING_OWNER) {
            return Err(primary("legacy staging publication requires recovery"));
        }
    }
    Ok(Some(LegacyStagingProducerGuard {
        _gate: gate,
        _root: root.pin_handle()?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn fixture() -> PathBuf {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path =
            std::env::temp_dir().join(format!("dc-legacy-use-{:032x}", u128::from_le_bytes(nonce)));
        std::fs::create_dir_all(path.join(".codebaseGraph/native-staging")).unwrap();
        path
    }
    #[test]
    fn dc_legacy_staging_retains_actual_shared_gate_through_arc_result_lifetime() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<LegacyStagingProducerGuard>();
        let repo = fixture();
        let staging = repo.join(".codebaseGraph/native-staging");
        let guard = Arc::new(before_staging_producer(&staging).unwrap().unwrap());
        let result = guard.clone();
        drop(guard);
        let gate = staging
            .parent()
            .unwrap()
            .join(std::str::from_utf8(STAGING_GATE).unwrap());
        assert!(locks::try_open_existing_locked(&gate, LockMode::Exclusive)
            .unwrap()
            .is_none());
        drop(result);
        assert!(locks::try_open_existing_locked(&gate, LockMode::Exclusive)
            .unwrap()
            .is_some());
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    fn dc_legacy_staging_custom_root_is_not_enrolled_or_created() {
        let repo = fixture();
        let custom = repo.join("custom");
        assert!(before_staging_producer(&custom).unwrap().is_none());
        assert!(!custom.exists());
        std::fs::remove_dir_all(repo).unwrap();
    }
    #[test]
    fn dc_legacy_staging_closed_record_refuses_new_actual_producer() {
        let repo = fixture();
        let state_path = repo.join(".codebaseGraph");
        let staging = state_path.join("native-staging");
        drop(before_staging_producer(&staging).unwrap());
        let gate_path = state_path.join(std::str::from_utf8(STAGING_GATE).unwrap());
        let gate = locks::open_locked(&gate_path, LockMode::Exclusive).unwrap();
        let owner = locks::open_locked(
            state_path.join(std::str::from_utf8(STAGING_OWNER).unwrap()),
            LockMode::Exclusive,
        )
        .unwrap();
        owner.initialize_creation_incarnation([9; 32]).unwrap();
        let state_file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(state_path.join(std::str::from_utf8(STAGING_STATE).unwrap()))
            .unwrap();
        let mut cache = [0; 256];
        cache[..14].copy_from_slice(b"native-staging");
        let binding = Binding {
            kind: LegacyScopeKind::NativeStaging,
            root: locks::path_identity(&state_path).unwrap(),
            gate: gate.identity().unwrap(),
            state: locks::file_identity(&state_file).unwrap(),
            owner: owner.identity().unwrap(),
            nonce: gate.coordination_incarnation().unwrap().unwrap(),
            owner_nonce: owner.coordination_incarnation().unwrap().unwrap(),
            key: [0; 64],
            database: [0; 256],
            database_len: 0,
            cache,
            cache_len: 14,
            repository: locks::path_identity(&repo).unwrap(),
        };
        let member = locks::path_identity(&staging).unwrap();
        let record = Record {
            phase: LegacyPhase::Preparing,
            version: 2,
            member,
            database: None,
            manifest: None,
            cache: member,
            root_use: binding.gate,
            confirmation: [1; 32],
            job: [2; 32],
            member_name: [0; 128],
            member_len: 0,
            freeze_cache: false,
            operator_generation: 3,
            operator_pending: false,
            pending_source_stamp: None,
            root_default_source: None,
        };
        let mut slot = [0; codec::SLOT_BYTES];
        let fact = codec::encode_slot(&mut slot, &binding.nonce, 2, &record.encode()).unwrap();
        let mut selector = [0; codec::SELECTOR_BYTES];
        codec::encode_commit(
            &mut selector[..codec::COMMIT_BYTES],
            &binding.nonce,
            codec::CommitFact {
                generation: 2,
                active: 0,
                slot_digest: fact.digest,
            },
        )
        .unwrap();
        use std::io::{Seek, SeekFrom, Write};
        let mut file = &state_file;
        file.write_all(&binding.encode()).unwrap();
        file.write_all(&slot).unwrap();
        file.seek(SeekFrom::Start(SELECTOR)).unwrap();
        file.write_all(&selector).unwrap();
        file.sync_all().unwrap();
        drop((gate, owner));
        assert!(before_staging_producer(&staging).is_err());
        std::fs::remove_dir_all(repo).unwrap();
    }
}

pub(super) fn write_range(
    file: &File,
    offset: u64,
    bytes: &[u8],
    grant: &mut MutationGrant,
) -> Result<(), FixedControlError> {
    grant.check()?;
    locks::write_fixed_range(file, offset, bytes, grant)?;
    grant.take_entry()?;
    file.sync_all()?;
    grant.check()?;
    Ok(())
}

pub(super) fn publish_record(
    state: &File,
    binding: Binding,
    current: codec::CommitFact,
    record: Record,
    grant: &mut MutationGrant,
) -> Result<codec::CommitFact, FixedControlError> {
    if record.version
        != current
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?
    {
        return Err(invalid(Problem::InvalidRecord));
    }
    let mut slot = [0; codec::SLOT_BYTES];
    let fact = codec::encode_slot(&mut slot, &binding.nonce, record.version, &record.encode())
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let target = 1 - current.active;
    let mut intent = [0; codec::INTENT_BYTES];
    codec::encode_intent(&mut intent, &binding.nonce, current, &slot)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    // Fixed own-control codecs only. No source payload/config/reference decoding
    // or hashing occurs in this short admitted transition.
    write_range(state, SELECTOR + codec::COMMIT_BYTES as u64, &intent, grant)?;
    write_range(state, if target == 0 { SLOT0 } else { SLOT1 }, &slot, grant)?;
    let commit = codec::CommitFact {
        generation: record.version,
        active: target,
        slot_digest: fact.digest,
    };
    let mut bytes = [0; codec::COMMIT_BYTES];
    codec::encode_commit(&mut bytes, &binding.nonce, commit)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    write_range(state, SELECTOR, &bytes, grant)?;
    write_range(
        state,
        SELECTOR + codec::COMMIT_BYTES as u64,
        &[0; codec::INTENT_BYTES],
        grant,
    )?;
    Ok(commit)
}

pub(super) fn recover_source_state(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
    grant: &mut MutationGrant,
) -> Result<(), FixedControlError> {
    let mut wire = [0; BINDING_BYTES];
    locks::read_fixed_range(state, 0, &mut wire)?;
    let binding = Binding::decode(&wire)?;
    let owner = root.open_readonly_file(owner_name)?;
    if root.identity() != binding.root
        || FixedFileStamp::capture(gate, Kind::File)?.identity() != binding.gate
        || FixedFileStamp::capture(state, Kind::File)?.identity() != binding.state
        || FixedFileStamp::capture(&owner, Kind::File)?.identity() != binding.owner
        || locks::read_coordination_incarnation(gate)
            .map_err(|_| invalid(Problem::InvalidRecord))?
            != Some(binding.nonce)
        || locks::read_coordination_incarnation(&owner)
            .map_err(|_| invalid(Problem::InvalidRecord))?
            != Some(binding.owner_nonce)
    {
        return Err(invalid(Problem::Changed));
    }
    let mut slots = [[0; codec::SLOT_BYTES]; 2];
    let mut selector = [0; codec::SELECTOR_BYTES];
    locks::read_fixed_range(state, SLOT0, &mut slots[0])?;
    locks::read_fixed_range(state, SLOT1, &mut slots[1])?;
    locks::read_fixed_range(state, SELECTOR, &mut selector)?;
    let sf = slots
        .each_ref()
        .map(|b| codec::decode_slot(b, &binding.nonce).ok());
    let commit = codec::decode_commit(&selector[..codec::COMMIT_BYTES], &binding.nonce).ok();
    let intent = codec::decode_intent(&selector[codec::COMMIT_BYTES..], &binding.nonce).ok();
    let step = codec::recovery_step(commit, intent, sf);
    let intended = match step {
        codec::RecoveryStep::RewriteIntendedSlot(i)
        | codec::RecoveryStep::CommitIntendedSlot(i) => Some(i),
        codec::RecoveryStep::Committed(_) => None,
        codec::RecoveryStep::Unknown => return Err(invalid(Problem::InvalidRecord)),
    };
    if let Some(i) = intended {
        let slot =
            &selector[codec::COMMIT_BYTES + 96..codec::COMMIT_BYTES + 96 + codec::SLOT_BYTES];
        let fact = codec::decode_slot(slot, &binding.nonce)
            .map_err(|_| invalid(Problem::InvalidRecord))?;
        if fact.payload_len != RECORD_BYTES {
            return Err(invalid(Problem::InvalidRecord));
        }
        Record::decode(&slot[56..56 + RECORD_BYTES], i.after)?;
        if matches!(step, codec::RecoveryStep::RewriteIntendedSlot(_)) {
            write_range(
                state,
                if i.target == 0 { SLOT0 } else { SLOT1 },
                slot,
                grant,
            )?;
        }
        let mut commit_wire = [0; codec::COMMIT_BYTES];
        codec::encode_commit(
            &mut commit_wire,
            &binding.nonce,
            codec::CommitFact {
                generation: i.after,
                active: i.target,
                slot_digest: i.slot_digest,
            },
        )
        .map_err(|_| invalid(Problem::InvalidRecord))?;
        write_range(state, SELECTOR, &commit_wire, grant)?;
    }
    if intent.is_some() {
        write_range(
            state,
            SELECTOR + codec::COMMIT_BYTES as u64,
            &[0; codec::INTENT_BYTES],
            grant,
        )?;
    }
    Ok(())
}

pub(crate) fn reactivate_legacy_source(
    database: &Path,
    manifest: &Path,
    writer: &WriterLease,
) -> Result<(), crate::error::NativeError> {
    fn run(
        database: &Path,
        manifest: &Path,
        writer: &WriterLease,
    ) -> Result<(), FixedControlError> {
        let Some(parent) = database
            .parent()
            .filter(|p| p.file_name().is_some_and(|n| n == ".codebaseGraph"))
        else {
            return Ok(());
        };
        if manifest != parent.join("manifest.json") {
            return Ok(());
        }
        let memory = CleanupMemory::new(1024 * 1024).map_err(FixedControlError::Resource)?;
        let path_charge = database
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add(manifest.as_os_str().as_encoded_bytes().len())
            .and_then(|n| n.checked_mul(16))
            .and_then(|n| n.checked_add(32768))
            .ok_or(FixedControlError::Resource(
                super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
            ))?;
        let _floor = memory
            .reserve(path_charge)
            .map_err(FixedControlError::Resource)?;
        let mut scratch = FixedControlScratch::new(
            memory
                .reserve(FixedControlScratch::required_bytes())
                .map_err(FixedControlError::Resource)?,
        )?;
        let root = RootedDirectory::open_fixed(
            parent,
            locks::path_identity(parent).map_err(|_| invalid(Problem::Changed))?,
            true,
            &mut scratch,
        )?;
        let state = match root.open_existing(STATE) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut wire = [0; BINDING_BYTES];
        if locks::read_fixed_range(&state, 0, &mut wire).is_err() {
            return Ok(());
        }
        let binding = match Binding::decode(&wire) {
            Ok(b) if b.kind == LegacyScopeKind::DefaultDirect => b,
            _ => return Ok(()),
        };
        let mut hash = Sha256::new();
        hash.update(database.as_os_str().as_encoded_bytes());
        hash.update([0]);
        hash.update(manifest.as_os_str().as_encoded_bytes());
        let digest = hash.finalize();
        let mut key = [0; 64];
        for (i, b) in digest.iter().enumerate() {
            key[2 * i] = b"0123456789abcdef"[(b >> 4) as usize];
            key[2 * i + 1] = b"0123456789abcdef"[(b & 15) as usize];
        }
        if binding.key != key
            || database.file_name().map(|n| n.as_encoded_bytes())
                != Some(&binding.database[..binding.database_len])
        {
            return Ok(());
        }
        let layout = DirectLayout::new(database, manifest);
        let writer_file =
            locks::open_fixed_control(&layout.writer_lock_path(), Kind::File, false, &mut scratch)?;
        let writer_stamp = FixedFileStamp::capture(&writer_file, Kind::File)?;
        if !writer
            .matches_fixed_current(&writer_stamp, LockMode::Exclusive)
            .map_err(|_| invalid(Problem::Changed))?
        {
            return Err(invalid(Problem::Changed));
        }
        let gate_file = root.open_existing(GATE)?;
        let gate = FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        let mut grant = MutationGrant::start();
        recover_source_state(&root, gate.file(), &state, OWNER, &mut grant)?;
        let observed = observe_files(&root, gate.file(), &state)?;
        if observed.binding.encode() != binding.encode() {
            return Err(invalid(Problem::Changed));
        }
        if observed.record.phase == LegacyPhase::Active {
            return Ok(());
        }
        if observed.record.phase == LegacyPhase::Retiring {
            return Err(invalid(Problem::Busy));
        }
        let family_path = parent.join(".database-use-family.lock");
        let family_file = locks::open_fixed_control(&family_path, Kind::File, true, &mut scratch)?;
        let _family = FixedLockedFile::try_lock(family_file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        let (_, native_path) = super::maintenance_ownership::database_lock_path(database, false)
            .map_err(|_| invalid(Problem::InvalidRecord))?;
        let native_file = locks::open_fixed_control(&native_path, Kind::File, true, &mut scratch)?;
        let _native = FixedLockedFile::try_lock(native_file, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        let artifact = root.open_directory(&binding.cache[..binding.cache_len])?;
        if artifact.identity() != observed.record.cache {
            return Err(invalid(Problem::Changed));
        }
        let artifact_use = artifact.open_existing(b".use.lock")?;
        let artifact_use = FixedLockedFile::try_lock(artifact_use, LockMode::Exclusive)?
            .ok_or_else(|| invalid(Problem::Busy))?;
        if FixedFileStamp::capture(artifact_use.file(), Kind::File)?.identity()
            != observed.record.root_use
        {
            return Err(invalid(Problem::Changed));
        }
        let mut next = observed.record;
        next.phase = LegacyPhase::Active;
        next.operator_pending = false;
        next.pending_source_stamp = None;
        next.version = next
            .version
            .checked_add(1)
            .ok_or_else(|| invalid(Problem::InvalidRecord))?;
        // Keeping the same job is harmless for Active, but a fresh nonce makes
        // stale preparation certificates unambiguously unrelated on reclose.
        getrandom::fill(&mut next.job).map_err(|_| invalid(Problem::InvalidRecord))?;
        publish_record(&state, binding, observed.commit, next, &mut grant)?;
        Ok(())
    }
    run(database, manifest, writer).map_err(|e| match e {
        FixedControlError::Io(e) => crate::error::NativeError::Io(e),
        _ => crate::error::NativeError::InvalidInput(
            "legacy Direct source is busy, changed, or requires retirement recovery".to_owned(),
        ),
    })
}

/// All source-state codecs are prepared outside the execution permit. This
/// owner is only an image; the retained source kernel verifies actual custody.
pub(super) struct PreparedSourcePublication {
    before: codec::CommitFact,
    after: Record,
    binding: Binding,
    slot: [u8; codec::SLOT_BYTES],
    intent: [u8; codec::INTENT_BYTES],
    commit_image: [u8; codec::COMMIT_BYTES],
    commit: codec::CommitFact,
    _floor: super::maintenance_budget::MemoryReservation,
}
impl PreparedSourcePublication {
    pub(super) fn prepare(
        binding: Binding,
        before: codec::CommitFact,
        after: Record,
        memory: &CleanupMemory,
    ) -> Result<super::maintenance_budget::MemoryBox<Self>, FixedControlError> {
        let floor = memory
            .reserve(std::mem::size_of::<Self>() * 2 + 4096)
            .map_err(FixedControlError::Resource)?;
        if after.version
            != before
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid(Problem::InvalidRecord))?
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let mut slot = [0; codec::SLOT_BYTES];
        let fact = codec::encode_slot(&mut slot, &binding.nonce, after.version, &after.encode())
            .map_err(|_| invalid(Problem::InvalidRecord))?;
        let mut intent = [0; codec::INTENT_BYTES];
        codec::encode_intent(&mut intent, &binding.nonce, before, &slot)
            .map_err(|_| invalid(Problem::InvalidRecord))?;
        let commit = codec::CommitFact {
            generation: after.version,
            active: 1 - before.active,
            slot_digest: fact.digest,
        };
        let mut commit_image = [0; codec::COMMIT_BYTES];
        codec::encode_commit(&mut commit_image, &binding.nonce, commit)
            .map_err(|_| invalid(Problem::InvalidRecord))?;
        super::maintenance_budget::MemoryBox::new(
            Self {
                before,
                after,
                binding,
                slot,
                intent,
                commit_image,
                commit,
                _floor: floor,
            },
            memory,
        )
        .map_err(FixedControlError::Resource)
    }
    pub(super) fn publish(
        &self,
        state: &File,
        binding: Binding,
        before: codec::CommitFact,
        after: Record,
        grant: &mut MutationGrant,
    ) -> Result<codec::CommitFact, FixedControlError> {
        if self.before != before || self.binding != binding || self.after != after {
            return Err(invalid(Problem::Changed));
        }
        write_range(
            state,
            SELECTOR + codec::COMMIT_BYTES as u64,
            &self.intent,
            grant,
        )?;
        write_range(
            state,
            if self.commit.active == 0 {
                SLOT0
            } else {
                SLOT1
            },
            &self.slot,
            grant,
        )?;
        write_range(state, SELECTOR, &self.commit_image, grant)?;
        write_range(
            state,
            SELECTOR + codec::COMMIT_BYTES as u64,
            &[0; codec::INTENT_BYTES],
            grant,
        )?;
        Ok(self.commit)
    }
}

pub(super) const SOURCE_CAPSULE_BYTES: usize = 2112;
pub(super) const SOURCE_CAPSULE_OFFSET: u64 = STATE_BYTES;
pub(super) const SOURCE_CAPSULE_STATE_BYTES: u64 = STATE_BYTES + SOURCE_CAPSULE_BYTES as u64;
#[derive(Clone, Copy)]
pub(super) struct SourceCreationCapsule {
    pub(super) mode: u8,
    pub(super) template: Binding,
    pub(super) permission: Record,
}
impl SourceCreationCapsule {
    pub(super) fn encode(&self) -> [u8; SOURCE_CAPSULE_BYTES] {
        let mut out = [0; SOURCE_CAPSULE_BYTES];
        out[..8].copy_from_slice(b"CGLGCR01");
        out[8] = self.mode;
        out[16..1040].copy_from_slice(&self.template.encode());
        out[1040..2064].copy_from_slice(&self.permission.encode());
        checksum(&mut out);
        out
    }
    pub(super) fn decode(bytes: &[u8; SOURCE_CAPSULE_BYTES]) -> Result<Self, FixedControlError> {
        if &bytes[..8] != b"CGLGCR01"
            || !checked(bytes)
            || !(1..=3).contains(&bytes[8])
            || bytes[9..16]
                .iter()
                .chain(bytes[2064..2080].iter())
                .any(|v| *v != 0)
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        let template = Binding::decode(bytes[16..1040].try_into().unwrap())?;
        let permission = Record::decode(&bytes[1040..2064], 1)?;
        let zero = FileIdentity::from_fixed_words(0, 0);
        if template.gate != zero
            || template.state != zero
            || template.owner != zero
            || template.nonce != permission.job
            || permission.phase != LegacyPhase::Active
            || !permission.operator_pending
            || permission.version != 1
        {
            return Err(invalid(Problem::InvalidRecord));
        }
        Ok(Self {
            mode: bytes[8],
            template,
            permission,
        })
    }
}
pub(super) fn read_creation_capsule(
    root: &RootedDirectory,
    state: &File,
) -> Result<Option<SourceCreationCapsule>, FixedControlError> {
    let before = FixedFileStamp::capture(state, Kind::File)?;
    if before.len() != SOURCE_CAPSULE_STATE_BYTES {
        return Ok(None);
    }
    let mut bytes = [0; SOURCE_CAPSULE_BYTES];
    locks::read_fixed_range(state, SOURCE_CAPSULE_OFFSET, &mut bytes)?;
    let capsule = SourceCreationCapsule::decode(&bytes)?;
    if capsule.template.root != root.identity() {
        return Err(invalid(Problem::Changed));
    }
    before.recheck(state)?;
    Ok(Some(capsule))
}

/// Advisory decoding of an exact owned intent. No record becomes current until
/// the later exclusive source callback publishes these already encoded bytes.
pub(super) struct PreparedSourceRecovery {
    pub(super) before: Observation,
    pub(super) after: Record,
    pub(super) commit: codec::CommitFact,
    slot: [u8; codec::SLOT_BYTES],
    commit_image: [u8; codec::COMMIT_BYTES],
    gate_header: [u8; locks::COORDINATION_HEADER_BYTES],
    owner_header: [u8; locks::COORDINATION_HEADER_BYTES],
    _floor: super::maintenance_budget::MemoryReservation,
}
pub(super) fn prepare_source_observation(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
    memory: &CleanupMemory,
) -> Result<
    (
        Observation,
        Option<super::maintenance_budget::MemoryBox<PreparedSourceRecovery>>,
    ),
    FixedControlError,
> {
    let data =
        prepare_source_observation_boxed(root, gate, state, owner_name, memory)?.into_inner();
    Ok((data.observed.into_inner(), data.recovery))
}

/// Off-permit source facts and the existing opaque recovery image. The hot
/// observe_files/snapshot/codec functions remain allocation-free and unchanged.
pub(super) struct PreparedSourceObservationData {
    pub(super) observed: super::maintenance_budget::MemoryBox<Observation>,
    pub(super) recovery: Option<super::maintenance_budget::MemoryBox<PreparedSourceRecovery>>,
}
#[inline(never)]
fn source_snapshot_boxed(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<Observation>, FixedControlError> {
    let observed = observe_source_snapshot(root, gate, state, owner_name, true)?;
    super::maintenance_budget::MemoryBox::new(observed, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
pub(super) fn prepare_source_observation_boxed(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedSourceObservationData>, FixedControlError>
{
    let floor = memory
        .reserve(std::mem::size_of::<PreparedSourceRecovery>() * 2 + 16384)
        .map_err(FixedControlError::Resource)?;
    let observed = source_snapshot_boxed(root, gate, state, owner_name, memory)?;
    let recovery = if observed.pending {
        Some(prepare_pending_source_recovery(
            root, gate, state, owner_name, &observed, floor, memory,
        )?)
    } else {
        None
    };
    super::maintenance_budget::MemoryBox::new(
        PreparedSourceObservationData { observed, recovery },
        memory,
    )
    .map_err(FixedControlError::Resource)
}

// Pending slot/commit images exist only after the snapshot stage returned;
// they cannot multiply the stack above immutable source decoding.
#[inline(never)]
fn prepare_pending_source_recovery(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
    before: &Observation,
    floor: super::maintenance_budget::MemoryReservation,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedSourceRecovery>, FixedControlError> {
    let mut intent = [0; codec::INTENT_BYTES];
    locks::read_fixed_range(state, SELECTOR + codec::COMMIT_BYTES as u64, &mut intent)?;
    let fact = codec::decode_intent(&intent, &before.binding.nonce)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let slot: [u8; codec::SLOT_BYTES] = intent[96..96 + codec::SLOT_BYTES].try_into().unwrap();
    let after = Record::decode(&slot[56..56 + RECORD_BYTES], fact.after)?;
    let commit = codec::CommitFact {
        generation: fact.after,
        active: fact.target,
        slot_digest: fact.slot_digest,
    };
    let mut commit_image = [0; codec::COMMIT_BYTES];
    codec::encode_commit(&mut commit_image, &before.binding.nonce, commit)
        .map_err(|_| invalid(Problem::InvalidRecord))?;
    let owner = root.open_readonly_file(owner_name)?;
    let mut gate_header = [0; locks::COORDINATION_HEADER_BYTES];
    let mut owner_header = [0; locks::COORDINATION_HEADER_BYTES];
    locks::read_fixed_range(gate, 0, &mut gate_header)?;
    locks::read_fixed_range(&owner, 0, &mut owner_header)?;
    before.state_stamp.recheck(state)?;
    let prepared = super::maintenance_budget::MemoryBox::new(
        PreparedSourceRecovery {
            before: *before,
            after,
            commit,
            slot,
            commit_image,
            gate_header,
            owner_header,
            _floor: floor,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)?;
    Ok(prepared)
}

impl PreparedSourceRecovery {
    pub(super) fn apply(
        &self,
        root: &RootedDirectory,
        gate: &FixedLockedFile,
        owner: &FixedLockedFile,
        state: &File,
        grant: &mut MutationGrant,
    ) -> Result<(), FixedControlError> {
        self.before.state_stamp.recheck(state)?;
        let binding = self.before.binding;
        let mut gate_header = [0; locks::COORDINATION_HEADER_BYTES];
        let mut owner_header = [0; locks::COORDINATION_HEADER_BYTES];
        locks::read_fixed_range(gate.file(), 0, &mut gate_header)?;
        locks::read_fixed_range(owner.file(), 0, &mut owner_header)?;
        if root.identity() != binding.root
            || gate.mode() != LockMode::Exclusive
            || owner.mode() != LockMode::Exclusive
            || FixedFileStamp::capture(gate.file(), Kind::File)?.identity() != binding.gate
            || FixedFileStamp::capture(owner.file(), Kind::File)?.identity() != binding.owner
            || gate_header != self.gate_header
            || owner_header != self.owner_header
        {
            return Err(invalid(Problem::Changed));
        }
        write_range(
            state,
            if self.commit.active == 0 {
                SLOT0
            } else {
                SLOT1
            },
            &self.slot,
            grant,
        )?;
        write_range(state, SELECTOR, &self.commit_image, grant)?;
        write_range(
            state,
            SELECTOR + codec::COMMIT_BYTES as u64,
            &[0; codec::INTENT_BYTES],
            grant,
        )?;
        Ok(())
    }
}

fn recognized_source_binding(
    root: &RootedDirectory,
    gate: &File,
    state: &File,
    owner_name: &[u8],
) -> bool {
    let check = || -> Result<bool, FixedControlError> {
        let mut bytes = [0; BINDING_BYTES];
        locks::read_fixed_range(state, 0, &mut bytes)?;
        let binding = Binding::decode(&bytes)?;
        let owner = root.open_readonly_file(owner_name)?;
        Ok(binding.root == root.identity()
            && binding.gate == FixedFileStamp::capture(gate, Kind::File)?.identity()
            && binding.state == FixedFileStamp::capture(state, Kind::File)?.identity()
            && binding.owner == FixedFileStamp::capture(&owner, Kind::File)?.identity()
            && locks::read_coordination_incarnation(gate).ok().flatten() == Some(binding.nonce)
            && locks::read_coordination_incarnation(&owner).ok().flatten()
                == Some(binding.owner_nonce))
    };
    check().unwrap_or(false)
}

#[cfg(test)]
mod source_observation_layout_tests {
    use super::*;
    #[test]
    fn prepared_source_observation_keeps_snapshot_and_recovery_images_boxed() {
        // Measured fixed owner is48B on the qualified local compiler/target;
        // larger protocol images must not be copied above hot snapshot reads.
        assert!(std::mem::size_of::<PreparedSourceObservationData>() <= 128);
        let _: fn(PreparedSourceObservationData) = |data| {
            let _: super::super::maintenance_budget::MemoryBox<Observation> = data.observed;
            let _: Option<super::super::maintenance_budget::MemoryBox<PreparedSourceRecovery>> =
                data.recovery;
        };
    }
}
