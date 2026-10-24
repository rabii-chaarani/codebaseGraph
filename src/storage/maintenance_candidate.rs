//! Candidate preparation owns advisory data only. Publication of journal
//! metadata and mutation of payloads require distinct retained admission guards.
use super::inventory::MaintenanceTarget;
use super::layout::{DirectLayout, ManagedLayout};
use super::locks::{self, FixedControlKind, FixedFileStamp, RootedDirectory};
use super::maintenance_admission::PreparedTargetBinding;
use super::maintenance_budget::{CleanupMemory, MemoryBox, MemoryReservation, MutationGrant};
use super::maintenance_origin::{self, OriginFacts, OriginObservation, OriginPhase};
use super::maintenance_ownership::{self as ownership, EpochObservation, ReferenceDomain};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::io::Read;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CandidateReport {
    pub(crate) pending: bool,
    pub(crate) deferred: bool,
    pub(crate) unknown: bool,
    pub(crate) observations_complete: bool,
    pub(crate) checked_categories: u8,
    pub(crate) inspected_entries: u64,
    pub(crate) mutated_entries: u64,
    pub(crate) reclaimed_entries: u64,
    /// Logical length of reclaimed regular files; never an allocated-byte claim.
    pub(crate) reclaimed_bytes: u64,
    pub(crate) reclaimed_allocated_bytes: Option<u64>,
    pub(crate) reasons: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateError {
    ResourceLimit,
    Io(std::io::ErrorKind),
    Stale,
    InvalidEvidence,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateStatus {
    Complete,
    Pending,
    Deferred,
    NeedsDisposal,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct CandidateAdvance {
    pub(crate) status: CandidateStatus,
    pub(crate) report: CandidateReport,
}
pub(crate) fn candidate_reason(bit: u8) -> Option<&'static str> {
    Some(match bit {
        0 => "unknown_program_origin",
        1 => "reference_in_use",
        2 => "maintenance_admission_busy",
        3 => "legacy_quiescence_required",
        4 => "needs_recovery",
        5 => "source_changed",
        6 => "resource_limit",
        7 => "retirement_in_progress",
        8 => "permission_denied",
        9 => "unsupported_layout",
        10 => "new_direct_active",
        11 => "cancelled",
        12 => "cleanup_io_error",
        _ => return None,
    })
}
fn io(error: std::io::Error) -> CandidateError {
    CandidateError::Io(error.kind())
}
fn evidence(error: crate::error::NativeError) -> CandidateError {
    match error {
        crate::error::NativeError::Io(error) => io(error),
        crate::error::NativeError::MemoryBudgetExceeded(_) => CandidateError::ResourceLimit,
        crate::error::NativeError::SourceChanged { .. } => CandidateError::Stale,
        _ => CandidateError::InvalidEvidence,
    }
}
fn admission(error: locks::FixedControlError) -> CandidateError {
    match error {
        locks::FixedControlError::Io(error) => io(error),
        locks::FixedControlError::Resource(_) => CandidateError::ResourceLimit,
        _ => CandidateError::Stale,
    }
}

const CONTROL_BYTES: usize =
    8 + super::maintenance_journal::RETIREMENT_FRAME_BYTES + super::maintenance::CURSOR_BYTES;
struct Control {
    leaf: Option<super::maintenance_journal::RetirementLeaf>,
    cursor: super::maintenance::MaintenanceCursor,
}
impl Control {
    fn decode(
        payload: &[u8],
        scope: super::maintenance_admission::RootOriginBinding,
    ) -> Result<Self, CandidateError> {
        if payload.len() == super::maintenance::CURSOR_BYTES {
            return Ok(Self {
                leaf: None,
                cursor: super::maintenance::MaintenanceCursor::decode(
                    payload.try_into().unwrap(),
                    scope.incarnation,
                )
                .map_err(|_| CandidateError::InvalidEvidence)?,
            });
        }
        if payload == b"CGBUILD1" {
            return Ok(Self {
                leaf: None,
                cursor: super::maintenance::MaintenanceCursor::new(scope.incarnation),
            });
        }
        if payload.len() != CONTROL_BYTES || &payload[..8] != b"CGCAND01" {
            return Err(CandidateError::InvalidEvidence);
        }
        let frame: &[u8; 512] = payload[8..520].try_into().unwrap();
        let leaf = if frame.iter().all(|b| *b == 0) {
            None
        } else {
            Some(
                super::maintenance_journal::RetirementLeaf::decode(frame)
                    .map_err(|_| CandidateError::InvalidEvidence)?,
            )
        };
        if leaf.is_some_and(|l| l.scope != scope) {
            return Err(CandidateError::Stale);
        }
        let cursor = super::maintenance::MaintenanceCursor::decode(
            payload[520..].try_into().unwrap(),
            scope.incarnation,
        )
        .map_err(|_| CandidateError::InvalidEvidence)?;
        Ok(Self { leaf, cursor })
    }
    fn encode(&self) -> [u8; CONTROL_BYTES] {
        let mut b = [0; CONTROL_BYTES];
        b[..8].copy_from_slice(b"CGCAND01");
        if let Some(leaf) = self.leaf {
            b[8..520].copy_from_slice(&leaf.encode());
        }
        b[520..].copy_from_slice(&self.cursor.encode());
        b
    }
}
/// Return an idle shared cursor only after strict installation-bound decoding.
/// An active leaf or another role is not available for role takeover.
pub(crate) fn idle_control_cursor(
    payload: &[u8],
    scope: super::maintenance_admission::RootOriginBinding,
) -> Result<Option<super::maintenance::MaintenanceCursor>, CandidateError> {
    if payload != b"CGBUILD1"
        && payload.len() != super::maintenance::CURSOR_BYTES
        && !payload.starts_with(b"CGCAND01")
    {
        return Ok(None);
    }
    let control = Control::decode(payload, scope)?;
    Ok(if control.leaf.is_none() {
        Some(control.cursor)
    } else {
        None
    })
}

/// Bulk owners remain off the mutation stack. The hot callback borrows these
/// charged frames; dropping it releases only a RefCell borrow, never the work.
pub(crate) struct PreparedCandidate {
    _binding: MemoryBox<PreparedTargetBinding>,
    work: RefCell<MemoryBox<Work>>,
    report: CandidateReport,
    _floor: MemoryReservation,
}
struct PreparedAppend {
    offset: u64,
    frame: [u8; 512],
}
enum Work {
    Scheduled {
        metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
        update: MemoryBox<super::maintenance_admission::PreparedControlUpdate>,
        inner: MemoryBox<Work>,
    },
    Legacy(MemoryBox<super::maintenance_legacy_engine::LegacyWork>),
    Generation(MemoryBox<super::maintenance_generation_engine::GenerationWork>),
    Backup(MemoryBox<super::maintenance_backup::BackupWork>),
    Terminal(MemoryBox<TerminalWork>),
    WorkspaceRoot {
        terminal: Option<MemoryBox<super::maintenance_controls::PreparedTerminalWorkspace>>,
        removal: MemoryBox<super::maintenance_workspace_gate::PreparedWorkspaceRemoval>,
    },
    Metadata {
        prepared: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
        update: MemoryBox<super::maintenance_admission::PreparedControlUpdate>,
        append: Option<PreparedAppend>,
        reset: Option<u64>,
    },
    Leaf {
        ready: MemoryBox<super::maintenance_admission::PreparedCanonical>,
        leaf: super::maintenance_journal::RetirementLeaf,
        stamp: FixedFileStamp,
        stream_stamp: FixedFileStamp,
        held_file: Option<std::fs::File>,
        payload_nonce: Option<[u8; 32]>,
        post_rename: Option<FixedFileStamp>,
        action: LeafAction,
    },
}
#[derive(Clone, Copy)]
enum LeafAction {
    Detach,
    UnlinkPayload,
    UnlinkReceipt,
}
pub(crate) struct CandidateHotFrame<'a> {
    prepared: &'a PreparedCandidate,
}
impl PreparedCandidate {
    pub(crate) fn borrow_hot_frame(&self) -> CandidateHotFrame<'_> {
        CandidateHotFrame { prepared: self }
    }
    /// The scheduler must retain a pending one-shot confirmation through
    /// unrelated modern housekeeping. Legacy enrollment variants opt in here.
    pub(crate) fn requires_legacy_confirmation(&self) -> bool {
        let work = self.work.borrow();
        let selected = match &**work {
            Work::Scheduled { inner, .. } => &**inner,
            other => other,
        };
        match selected {
            Work::Legacy(work) => work.requires_legacy_confirmation(),
            Work::Generation(work) => work.requires_legacy_confirmation(),
            _ => false,
        }
    }
}
fn lower_hex(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
fn decode_key(name: &[u8; 64]) -> [u8; 32] {
    let mut key = [0; 32];
    let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
    for (i, pair) in name.chunks_exact(2).enumerate() {
        key[i] = digit(pair[0]) * 16 + digit(pair[1]);
    }
    key
}
fn encode_key(key: &[u8; 32]) -> [u8; 64] {
    let mut name = [0; 64];
    let digits = b"0123456789abcdef";
    for (i, b) in key.iter().enumerate() {
        name[i * 2] = digits[(b >> 4) as usize];
        name[i * 2 + 1] = digits[(b & 15) as usize];
    }
    name
}
fn deferred(report: &mut CandidateReport, bit: u8) {
    report.pending = true;
    report.deferred = true;
    report.reasons |= 1u64 << bit;
}
fn reference(error: super::maintenance_references::ReferenceIssue) -> CandidateError {
    use super::maintenance_references::ReferenceIssue as R;
    match error {
        R::ResourceLimit(_) => CandidateError::ResourceLimit,
        R::Io(k) => CandidateError::Io(k),
        R::Stale => CandidateError::Stale,
        R::Cancelled => CandidateError::Cancelled,
        R::Unknown => CandidateError::InvalidEvidence,
    }
}
fn metadata_work(
    prepared: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    control: &Control,
    memory: &CleanupMemory,
    append: Option<[u8; 512]>,
) -> Result<Work, CandidateError> {
    let before = prepared
        .committed()
        .ok_or(CandidateError::InvalidEvidence)?;
    let update = super::maintenance_admission::prepare_metadata_control(
        &prepared.origin_scope(),
        before,
        &control.encode(),
        memory,
    )
    .map_err(admission)?;
    let update = MemoryBox::new(update, memory).map_err(|_| CandidateError::ResourceLimit)?;
    Ok(Work::Metadata {
        prepared,
        update,
        append: append.map(|frame| PreparedAppend {
            offset: u64::from_le_bytes(frame[32..40].try_into().unwrap()),
            frame,
        }),
        reset: None,
    })
}
fn next_stage(
    leaf: &mut super::maintenance_journal::RetirementLeaf,
    stage: super::maintenance_journal::RetirementStage,
) -> Result<(), CandidateError> {
    leaf.stage_generation = leaf
        .stage_generation
        .checked_add(1)
        .ok_or(CandidateError::InvalidEvidence)?;
    leaf.stage = stage;
    Ok(())
}
fn absent(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}
struct OriginValidation<'a> {
    incomplete: bool,
    detached: bool,
    cancel: &'a dyn Fn() -> bool,
}
fn validate_origin(
    root: &RootedDirectory,
    directory: &RootedDirectory,
    payload: &std::fs::File,
    key: [u8; 32],
    memory: &CleanupMemory,
    mode: OriginValidation<'_>,
) -> Result<Option<(OriginFacts, [u8; 32])>, CandidateError> {
    let (digest, receipt_stamp) = match receipt_digest(directory) {
        Ok(v) => v,
        Err(CandidateError::Io(std::io::ErrorKind::NotFound))
        | Err(CandidateError::InvalidEvidence) => return Ok(None),
        Err(error) => return Err(error),
    };
    let observation = if mode.detached {
        maintenance_origin::observe_detached_artifact_origin(
            root,
            directory,
            payload,
            key,
            memory,
            mode.cancel,
        )
    } else {
        maintenance_origin::observe_artifact_origin_checked(
            root,
            directory,
            payload,
            key,
            memory,
            mode.cancel,
        )
    }
    .map_err(|error| {
        if (mode.cancel)() {
            CandidateError::Cancelled
        } else {
            evidence(error)
        }
    })?;
    let receipt = directory
        .open_readonly_file(maintenance_origin::ORIGIN_NAME)
        .map_err(io)?;
    receipt_stamp.recheck(&receipt).map_err(admission)?;
    match observation {
        OriginObservation::Observed(origin)
            if matches!(origin.phase, OriginPhase::Complete | OriginPhase::Published) =>
        {
            Ok(Some((origin, digest)))
        }
        OriginObservation::Observed(mut origin) if mode.incomplete && origin.payload_created => {
            let (len, hash) = hash_payload(payload, mode.cancel)?;
            receipt_stamp.recheck(&receipt).map_err(admission)?;
            origin.byte_len = len;
            origin.payload_sha256 = hash;
            Ok(Some((origin, digest)))
        }
        _ => Ok(None),
    }
}
fn hash_payload(
    file: &std::fs::File,
    cancel: &dyn Fn() -> bool,
) -> Result<(u64, [u8; 32]), CandidateError> {
    let stamp = FixedFileStamp::capture(file, FixedControlKind::File).map_err(admission)?;
    let mut hash = Sha256::new();
    let mut offset = 0u64;
    let mut buffer = [0; 4096];
    while offset < stamp.len() {
        if cancel() {
            return Err(CandidateError::Cancelled);
        }
        let count = ((stamp.len() - offset).min(buffer.len() as u64)) as usize;
        locks::read_fixed_range(file, offset, &mut buffer[..count]).map_err(io)?;
        hash.update(&buffer[..count]);
        offset += count as u64;
    }
    stamp.recheck(file).map_err(admission)?;
    Ok((offset, hash.finalize().into()))
}
fn temp_address(name: &[u8]) -> Option<super::maintenance::CursorAddress> {
    if name.len() != 134
        || !name.starts_with(b".tmp-")
        || name[69] != b'-'
        || !lower_hex(&name[5..69])
        || !lower_hex(&name[70..])
    {
        return None;
    }
    let mut address = [0; 64];
    address[..32].copy_from_slice(&decode_key(name[5..69].try_into().ok()?));
    address[32..].copy_from_slice(&decode_key(name[70..].try_into().ok()?));
    Some(super::maintenance::CursorAddress::from_short(address))
}
fn temp_name(address: super::maintenance::CursorAddress) -> Result<[u8; 134], CandidateError> {
    let address = address.as_short().ok_or(CandidateError::InvalidEvidence)?;
    let mut name = [0; 134];
    name[..5].copy_from_slice(b".tmp-");
    name[5..69].copy_from_slice(&encode_key(address[..32].try_into().unwrap()));
    name[69] = b'-';
    name[70..].copy_from_slice(&encode_key(address[32..].try_into().unwrap()));
    Ok(name)
}
fn source_directory(
    root: &RootedDirectory,
    leaf: &super::maintenance_journal::RetirementLeaf,
) -> Result<RootedDirectory, CandidateError> {
    if leaf.source_kind == 1 {
        if temp_address(&leaf.source_name).is_none_or(|address| address.0[..32] != leaf.key) {
            return Err(CandidateError::InvalidEvidence);
        }
        root.open_directory_readonly(&leaf.source_name).map_err(io)
    } else {
        let name = encode_key(&leaf.key);
        root.open_directory_readonly(&name[..2])
            .and_then(|prefix| prefix.open_directory_readonly(&name))
            .map_err(io)
    }
}

fn stream_frame(leaf: &super::maintenance_journal::RetirementLeaf) -> [u8; 512] {
    let mut initial = *leaf;
    initial.stage = super::maintenance_journal::RetirementStage::AppendIntent;
    initial.stage_generation = 0;
    initial.encode()
}
fn verify_stream(
    root: &RootedDirectory,
    leaf: &super::maintenance_journal::RetirementLeaf,
) -> Result<FixedFileStamp, CandidateError> {
    let file = root
        .open_readonly_file(b".publication.stream")
        .map_err(io)?;
    let stamp = FixedFileStamp::capture(&file, FixedControlKind::File).map_err(admission)?;
    let mut frame = [0; 512];
    locks::read_fixed_range(&file, leaf.stream_offset, &mut frame).map_err(io)?;
    if frame != stream_frame(leaf) {
        return Err(CandidateError::InvalidEvidence);
    }
    stamp.recheck(&file).map_err(admission)?;
    Ok(stamp)
}

fn receipt_digest(
    directory: &RootedDirectory,
) -> Result<([u8; 32], FixedFileStamp), CandidateError> {
    let mut file = directory
        .open_readonly_file(maintenance_origin::ORIGIN_NAME)
        .map_err(io)?;
    let stamp = FixedFileStamp::capture(&file, FixedControlKind::File).map_err(admission)?;
    if stamp.len() > 2048 {
        return Err(CandidateError::InvalidEvidence);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 512];
    loop {
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    stamp.recheck(&file).map_err(admission)?;
    Ok((hash.finalize().into(), stamp))
}

/// Persistent offslot preparation is owned by the joined controller, never by
/// the dispatcher or a global/thread-local registry. Durable checkpoints remain
/// the restart authority; this is only a bounded warm preparation cache.
#[derive(Default)]
pub(crate) struct CandidatePreparationState {
    binding: Option<[u8; 32]>,
    legacy_interest: bool,
    legacy_metadata: Option<(
        u64,
        Option<super::maintenance_admission::LegacyConfirmationSource>,
    )>,
    legacy_jobs: super::maintenance_legacy_engine::LegacyPreparationState,
    backup: super::maintenance_backup::BackupPreparationState,
    generations: super::maintenance_generation_engine::GenerationPreparationState,
    legacy: Option<MemoryBox<super::maintenance_legacy_association::LegacyAssociationPreparation>>,
    association: Option<MemoryBox<super::maintenance_legacy_association::LegacyAssociation>>,
}
impl CandidatePreparationState {
    pub(crate) fn has_parked_payloads(&self) -> bool {
        self.legacy.is_some()
            || self.association.is_some()
            || self.legacy_jobs.has_parked_payloads()
            || self.backup.has_parked_payloads()
            || self.generations.has_parked_payloads()
    }
    pub(crate) fn set_legacy_confirmation_metadata(
        &mut self,
        metadata: Option<(
            u64,
            Option<super::maintenance_admission::LegacyConfirmationSource>,
        )>,
    ) {
        self.legacy_metadata = metadata;
    }
    pub(crate) fn set_legacy_interest(&mut self, interest: bool) {
        self.legacy_interest = interest;
        self.legacy_jobs.set_legacy_interest(interest);
    }
    pub(crate) fn clear(&mut self) {
        self.backup.clear();
        self.generations.clear();
        self.legacy_jobs.clear();
        drop(self.legacy.take());
        drop(self.association.take());
        self.binding = None;
    }
    fn bind(&mut self, digest: [u8; 32]) {
        if self.binding != Some(digest) {
            self.clear();
            self.binding = Some(digest);
        }
    }
}

#[inline(never)]
fn duplicate_target_binding_boxed(
    binding: &PreparedTargetBinding,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedTargetBinding>, CandidateError> {
    let mut retained = binding.duplicate(memory).map_err(admission)?;
    if !retained.is_current().map_err(admission)? {
        return Err(CandidateError::Stale);
    }
    MemoryBox::new(retained, memory).map_err(|_| CandidateError::ResourceLimit)
}
pub(crate) fn prepare_candidate_with_state(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    state: &mut CandidatePreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
) -> Result<Option<PreparedCandidate>, CandidateError> {
    state.bind(binding.digest());
    state
        .legacy_jobs
        .set_request_metadata(state.legacy_metadata);
    *report = CandidateReport::default();
    if cancel() {
        state.clear();
        return Err(CandidateError::Cancelled);
    }
    #[cfg(windows)]
    const PATH_BYTES: usize = 65536;
    #[cfg(not(windows))]
    const PATH_BYTES: usize = 4096;
    let floor = memory
        .reserve(
            std::mem::size_of::<PreparedCandidate>() + 24 * PATH_BYTES + 3 * 4096 + CONTROL_BYTES,
        )
        .map_err(|_| CandidateError::ResourceLimit)?;
    let retained = duplicate_target_binding_boxed(binding, memory)?;
    if let Some(legacy) = super::maintenance_legacy_engine::prepare_bootstrap_work(
        target,
        binding,
        &mut state.legacy_jobs,
        memory,
        cancel,
        state.legacy_interest,
    )
    .map_err(admission)?
    {
        return Ok(Some(PreparedCandidate {
            _binding: retained,
            work: RefCell::new(
                MemoryBox::new(Work::Legacy(legacy), memory)
                    .map_err(|_| CandidateError::ResourceLimit)?,
            ),
            report: *report,
            _floor: floor,
        }));
    }
    if state.legacy_jobs.needs_operator() {
        deferred(report, 3);
    }
    let domain = if let Some(pair) = &target.direct_pair {
        ReferenceDomain::direct(&DirectLayout::new(&pair.database, &pair.manifest))
    } else if let Some(storage) = &target.storage_root {
        ReferenceDomain::managed(&ManagedLayout::new(storage))
    } else {
        report.unknown = true;
        report.reasons |= 1 << 9;
        return Ok(None);
    }
    .map_err(evidence)?;
    // A pending one-shot claim pins the exact old DB/manifest pair. Until
    // its real enrollment consumes it, no other payload proof may open FDs.
    if state.legacy_interest {
        // The bound root bootstrap above is the sole initial consumer. Retain
        // the request while no actual role can claim it safely.
        deferred(report, 3);
        return Ok(None);
    }
    let expected = match ownership::observe_reference_epoch(&domain).map_err(evidence)? {
        EpochObservation::Verified(epoch) => epoch,
        EpochObservation::Pending(_) => {
            deferred(report, if binding.is_legacy() { 3 } else { 2 });
            return Ok(None);
        }
    };
    let Some(work) = prepare_role_work(
        target, binding, &domain, &expected, state, memory, cancel, report,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(PreparedCandidate {
        _binding: retained,
        work: RefCell::new(
            MemoryBox::new(work, memory).map_err(|_| CandidateError::ResourceLimit)?,
        ),
        report: *report,
        _floor: floor,
    }))
}
#[inline(never)]
fn boxed_canonical(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    memory: &CleanupMemory,
) -> Result<MemoryBox<super::maintenance_admission::PreparedCanonical>, CandidateError> {
    let value = super::maintenance_admission::prepare_canonical(target, domain, expected, memory)
        .map_err(admission)?;
    MemoryBox::new(value, memory).map_err(|_| CandidateError::ResourceLimit)
}
#[inline(never)]
fn boxed_metadata(
    target: &MaintenanceTarget,
    scope: &super::maintenance_admission::RootOriginBinding,
    memory: &CleanupMemory,
) -> Result<MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>, CandidateError> {
    let value = super::maintenance_admission::prepare_private_metadata(target, scope, memory)
        .map_err(admission)?;
    MemoryBox::new(value, memory).map_err(|_| CandidateError::ResourceLimit)
}
#[inline(never)]
fn candidate_origin_scope(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    memory: &CleanupMemory,
) -> Result<super::maintenance_admission::RootOriginBinding, CandidateError> {
    Ok(
        super::maintenance_admission::prepare_canonical(target, domain, expected, memory)
            .map_err(admission)?
            .origin_scope(),
    )
}
#[inline(never)]
// Keep independent borrowed authorities and caller-owned scratch explicit.
#[allow(clippy::too_many_arguments)]
fn prepare_role_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    state: &mut CandidatePreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
) -> Result<Option<Work>, CandidateError> {
    use super::maintenance::WorkCategory as C;
    let scope = match candidate_origin_scope(target, domain, expected, memory) {
        Ok(scope) => scope,
        Err(_) => {
            return prepare_artifact_candidate(
                target, domain, expected, None, memory, cancel, report,
            )
        }
    };
    let metadata = match boxed_metadata(target, &scope, memory) {
        Ok(metadata) => metadata,
        Err(_) => {
            return prepare_artifact_candidate(
                target, domain, expected, None, memory, cancel, report,
            )
        }
    };
    let Some(payload) = metadata.payload() else {
        return prepare_artifact_candidate(target, domain, expected, None, memory, cancel, report);
    };
    if super::maintenance_backup::is_backup_payload(payload) {
        return super::maintenance_backup::prepare_work(
            target,
            binding,
            domain,
            expected,
            &mut state.backup,
            memory,
            cancel,
        )
        .map(|x| x.map(Work::Backup))
        .map_err(admission);
    }
    if super::maintenance_legacy_engine::is_legacy_payload(payload) {
        return super::maintenance_legacy_engine::prepare_work(
            target,
            binding,
            domain,
            expected,
            &mut state.legacy_jobs,
            memory,
            cancel,
        )
        .map(|x| x.map(Work::Legacy))
        .map_err(admission);
    }
    if super::maintenance_generation_engine::is_generation_payload(payload) {
        return super::maintenance_generation_engine::prepare_work(
            target,
            binding,
            domain,
            expected,
            &mut state.generations,
            memory,
            cancel,
        )
        .map(|x| x.map(Work::Generation))
        .map_err(admission);
    }
    let Some(mut cursor) = idle_control_cursor(payload, scope)? else {
        return prepare_artifact_candidate(target, domain, expected, None, memory, cancel, report);
    };
    for _ in 0..8 {
        let category = cursor.next_category();
        let selected = match category {
            C::Canonical => prepare_artifact_candidate(
                target,
                domain,
                expected,
                Some(category),
                memory,
                cancel,
                report,
            )?,
            C::TemporaryOrBackup => {
                if let Some(backup) = super::maintenance_backup::prepare_work(
                    target,
                    binding,
                    domain,
                    expected,
                    &mut state.backup,
                    memory,
                    cancel,
                )
                .map_err(admission)?
                {
                    Some(Work::Backup(backup))
                } else {
                    prepare_artifact_candidate(
                        target,
                        domain,
                        expected,
                        Some(category),
                        memory,
                        cancel,
                        report,
                    )?
                }
            }
            C::Run | C::Phase => {
                let selected = prepare_terminal_candidate(
                    target,
                    domain,
                    memory,
                    cancel,
                    report,
                    category,
                    cursor.after(category),
                )?;
                report.checked_categories |= 1 << (category as u8);
                selected
            }
            C::Generation => {
                let mut selected = super::maintenance_generation_engine::prepare_work(
                    target,
                    binding,
                    domain,
                    expected,
                    &mut state.generations,
                    memory,
                    cancel,
                )
                .map(|x| x.map(Work::Generation))
                .map_err(admission)?;
                if selected.is_none() {
                    selected = super::maintenance_legacy_engine::prepare_native_work(
                        target, binding, domain, expected, &mut state.legacy_jobs,
                        super::maintenance_legacy_generations_engine::LegacyNativeScope::ManagedGeneration,
                        memory, cancel,
                    ).map(|x| x.map(Work::Legacy)).map_err(admission)?;
                }
                if let Some(observed) = state.generations.take_observation() {
                    report.checked_categories |= observed.checked_categories;
                    report.inspected_entries = report
                        .inspected_entries
                        .saturating_add(observed.inspected_entries);
                    report.unknown |= observed.unknown;
                    report.deferred |= observed.deferred;
                    report.reasons |= observed.reasons;
                }
                selected
            }
            C::LegacyNative | C::LegacySnapshot | C::DefaultDirect => {
                super::maintenance_legacy_engine::prepare_work(
                    target,
                    binding,
                    domain,
                    expected,
                    &mut state.legacy_jobs,
                    memory,
                    cancel,
                )
                .map(|x| x.map(Work::Legacy))
                .map_err(admission)?
            }
        };
        if let Some(work) = selected {
            if matches!(category, C::Run | C::Phase) {
                if let Some(address) = terminal_work_address(&work, domain, target) {
                    cursor.inspected(category, address);
                } else {
                    cursor.advance_empty(category);
                }
                report.pending = true;
                return scheduled_terminal_work(metadata, &scope, &cursor, work, memory).map(Some);
            }
            return Ok(Some(work));
        }
        cursor.advance_empty(category);
    }
    report.observations_complete =
        !report.deferred && !report.unknown && report.checked_categories == u8::MAX;
    Ok(None)
}

#[inline(never)]
fn scheduled_terminal_work(
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    scope: &super::maintenance_admission::RootOriginBinding,
    cursor: &super::maintenance::MaintenanceCursor,
    work: Work,
    memory: &CleanupMemory,
) -> Result<Work, CandidateError> {
    let update = super::maintenance_admission::prepare_metadata_control(
        scope,
        metadata
            .committed()
            .ok_or(CandidateError::InvalidEvidence)?,
        &cursor.encode(),
        memory,
    )
    .map_err(admission)?;
    Ok(Work::Scheduled {
        metadata,
        update: MemoryBox::new(update, memory).map_err(|_| CandidateError::ResourceLimit)?,
        inner: MemoryBox::new(work, memory).map_err(|_| CandidateError::ResourceLimit)?,
    })
}

#[inline(never)]
fn prepare_artifact_candidate(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    category: Option<super::maintenance::WorkCategory>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
) -> Result<Option<Work>, CandidateError> {
    let ready = match boxed_canonical(target, domain, expected, memory) {
        Ok(prepared) => prepared,
        Err(_) => {
            deferred(report, 2);
            return Ok(None);
        }
    };
    let scope = ready.origin_scope();
    #[cfg(all(test, debug_assertions))]
    if fault::paused(scope.root) {
        deferred(report, 7);
        return Ok(None);
    }
    let metadata = match boxed_metadata(target, &scope, memory) {
        Ok(p) => p,
        Err(_) => {
            deferred(report, 7);
            return Ok(None);
        }
    };
    if let Some(update) = super::maintenance_admission::prepare_metadata_recovery(&metadata, memory)
        .map_err(admission)?
    {
        deferred(report, 7);
        return Ok(Some(Work::Metadata {
            prepared: metadata,
            update: MemoryBox::new(update, memory).map_err(|_| CandidateError::ResourceLimit)?,
            append: None,
            reset: None,
        }));
    }
    prepare_artifact_work(
        target, domain, expected, category, memory, cancel, report, ready, metadata,
    )
}

// Keep admission's fixed frames outside payload traversal and replay frames.
#[inline(never)]
// Keep independent borrowed authorities and caller-owned scratch explicit.
#[allow(clippy::too_many_arguments)]
fn prepare_artifact_work(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    category: Option<super::maintenance::WorkCategory>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
    ready: MemoryBox<super::maintenance_admission::PreparedCanonical>,
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
) -> Result<Option<Work>, CandidateError> {
    let scope = ready.origin_scope();
    let control = Control::decode(
        metadata.payload().ok_or(CandidateError::InvalidEvidence)?,
        scope,
    )?;
    let root =
        RootedDirectory::open_readonly_root(domain.artifact_root(), scope.root).map_err(io)?;
    #[cfg(all(test, debug_assertions))]
    fault::preparation_pause(scope.root, cancel)?;
    if let Some(leaf) = control.leaf {
        prepare_artifact_resume(
            target, domain, expected, memory, cancel, report, ready, metadata, root, control, leaf,
        )
    } else {
        drop(ready);
        prepare_artifact_select(
            target, domain, expected, category, memory, cancel, report, metadata, root, control,
        )
    }
}

#[inline(never)]
// Keep independent borrowed authorities and caller-owned scratch explicit.
#[allow(clippy::too_many_arguments)]
fn prepare_artifact_resume(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
    ready: MemoryBox<super::maintenance_admission::PreparedCanonical>,
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    root: RootedDirectory,
    mut control: Control,
    mut leaf: super::maintenance_journal::RetirementLeaf,
) -> Result<Option<Work>, CandidateError> {
    let scope = ready.origin_scope();
    let work = {
        use super::maintenance_journal::RetirementStage as S;
        report.pending = true;
        report.reasons |= 1 << 7;
        match leaf.stage {
            S::AppendIntent => {
                let frame = leaf.encode();
                next_stage(&mut leaf, S::FrameReady)?;
                control.leaf = Some(leaf);
                metadata_work(metadata, &control, memory, Some(frame))?
            }
            S::FrameReady | S::Detached | S::PayloadGone => {
                let stream_stamp = verify_stream(&root, &leaf)?;
                let directory = match source_directory(&root, &leaf) {
                    Ok(directory) => directory,
                    Err(CandidateError::Io(std::io::ErrorKind::NotFound))
                        if matches!(leaf.stage, S::PayloadGone) =>
                    {
                        match root.open_readonly_file(&leaf.tombstone()) {
                            Err(error) if absent(&error) => {}
                            Err(error) => return Err(io(error)),
                            Ok(_) => return Err(CandidateError::Stale),
                        }
                        next_stage(&mut leaf, S::Done)?;
                        control.leaf = Some(leaf);
                        let work = metadata_work(metadata, &control, memory, None)?;
                        return Ok(Some(work));
                    }
                    Err(error) => return Err(error),
                };
                if directory.identity() != leaf.directory {
                    return Err(CandidateError::Stale);
                }
                let private = root.open_readonly_file(&leaf.tombstone());
                let public = directory.open_readonly_file(maintenance_origin::PAYLOAD_NAME);
                match leaf.stage {
                    S::FrameReady => match private {
                        Ok(payload) => {
                            if locks::file_identity(&payload).map_err(evidence)? != leaf.payload {
                                return Err(CandidateError::Stale);
                            }
                            if matches!(public,Ok(ref p)if locks::file_identity(p).ok()==Some(leaf.payload))
                            {
                                return Err(CandidateError::InvalidEvidence);
                            }
                            let (origin, receipt_hash) = validate_origin(
                                &root,
                                &directory,
                                &payload,
                                leaf.key,
                                memory,
                                OriginValidation {
                                    incomplete: leaf.source_kind == 1,
                                    detached: true,
                                    cancel,
                                },
                            )?
                            .ok_or(CandidateError::InvalidEvidence)?;
                            if receipt_hash != leaf.receipt_digest {
                                return Err(CandidateError::Stale);
                            }
                            if origin.payload_sha256 != leaf.digest
                                || origin.byte_len != leaf.byte_len
                            {
                                return Err(CandidateError::Stale);
                            }
                            next_stage(&mut leaf, S::Detached)?;
                            control.leaf = Some(leaf);
                            metadata_work(metadata, &control, memory, None)?
                        }
                        Err(error) if absent(&error) => {
                            if expected.counter() != leaf.source_epoch {
                                // Own epoch advancement without a rename cannot
                                // become Detached. Finish only the inert log and
                                // reset it before selecting a fresh source epoch.
                                leaf.stage = S::Done;
                                leaf.stage_generation = 4;
                                control.leaf = Some(leaf);
                                metadata_work(metadata, &control, memory, None)?
                            } else {
                                let payload = public.map_err(io)?;
                                let (origin, receipt_hash) = validate_origin(
                                    &root,
                                    &directory,
                                    &payload,
                                    leaf.key,
                                    memory,
                                    OriginValidation {
                                        incomplete: leaf.source_kind == 1,
                                        detached: false,
                                        cancel,
                                    },
                                )?
                                .ok_or(CandidateError::InvalidEvidence)?;
                                if receipt_hash != leaf.receipt_digest {
                                    return Err(CandidateError::Stale);
                                }
                                if origin.payload != leaf.payload
                                    || origin.payload_sha256 != leaf.digest
                                    || origin.byte_len != leaf.byte_len
                                {
                                    return Err(CandidateError::Stale);
                                }
                                let refs =
                                    super::maintenance_references::prepare_artifact_references(
                                        target, domain, expected, leaf.key, memory, cancel,
                                    )
                                    .map_err(reference)?;
                                if !refs.complete || refs.referenced {
                                    deferred(report, 1);
                                    return Ok(None);
                                }
                                let stamp =
                                    FixedFileStamp::capture(&payload, FixedControlKind::File)
                                        .map_err(admission)?;
                                Work::Leaf {
                                    ready,
                                    leaf,
                                    stamp,
                                    stream_stamp,
                                    held_file: Some(payload),
                                    payload_nonce: Some(origin.payload_nonce),
                                    post_rename: None,
                                    action: LeafAction::Detach,
                                }
                            }
                        }
                        Err(error) => return Err(io(error)),
                    },
                    S::Detached => match private {
                        Ok(payload) => {
                            let (origin, receipt_hash) = validate_origin(
                                &root,
                                &directory,
                                &payload,
                                leaf.key,
                                memory,
                                OriginValidation {
                                    incomplete: leaf.source_kind == 1,
                                    detached: true,
                                    cancel,
                                },
                            )?
                            .ok_or(CandidateError::InvalidEvidence)?;
                            if receipt_hash != leaf.receipt_digest {
                                return Err(CandidateError::Stale);
                            }
                            if origin.payload != leaf.payload
                                || origin.payload_sha256 != leaf.digest
                                || origin.byte_len != leaf.byte_len
                            {
                                return Err(CandidateError::Stale);
                            }
                            let refs = super::maintenance_references::prepare_artifact_references(
                                target, domain, expected, leaf.key, memory, cancel,
                            )
                            .map_err(reference)?;
                            if !refs.complete || refs.referenced {
                                deferred(report, 1);
                                return Ok(None);
                            }
                            let stamp = FixedFileStamp::capture(&payload, FixedControlKind::File)
                                .map_err(admission)?;
                            Work::Leaf {
                                ready,
                                leaf,
                                stamp,
                                stream_stamp,
                                held_file: Some(payload),
                                payload_nonce: Some(origin.payload_nonce),
                                post_rename: None,
                                action: LeafAction::UnlinkPayload,
                            }
                        }
                        Err(error) if absent(&error) => {
                            next_stage(&mut leaf, S::PayloadGone)?;
                            control.leaf = Some(leaf);
                            metadata_work(metadata, &control, memory, None)?
                        }
                        Err(error) => return Err(io(error)),
                    },
                    S::PayloadGone => {
                        // A new public incarnation is never addressed by an old journal.
                        match (public, private) {
                            (Err(a), Err(b)) if absent(&a) && absent(&b) => {}
                            (Err(error), _) if !absent(&error) => return Err(io(error)),
                            (_, Err(error)) if !absent(&error) => return Err(io(error)),
                            _ => {
                                deferred(report, 5);
                                return Ok(None);
                            }
                        }
                        match directory.open_readonly_file(maintenance_origin::ORIGIN_NAME) {
                            Ok(receipt) => {
                                if locks::file_identity(&receipt).map_err(evidence)? != leaf.receipt
                                {
                                    return Err(CandidateError::Stale);
                                }
                                let (digest, stamp) = receipt_digest(&directory)?;
                                if digest != leaf.receipt_digest {
                                    return Err(CandidateError::Stale);
                                }
                                Work::Leaf {
                                    ready,
                                    leaf,
                                    stamp,
                                    stream_stamp,
                                    held_file: Some(receipt),
                                    payload_nonce: None,
                                    post_rename: None,
                                    action: LeafAction::UnlinkReceipt,
                                }
                            }
                            Err(error) if absent(&error) => {
                                next_stage(&mut leaf, S::Done)?;
                                control.leaf = Some(leaf);
                                metadata_work(metadata, &control, memory, None)?
                            }
                            Err(error) => return Err(io(error)),
                        }
                    }
                    _ => unreachable!(),
                }
            }
            S::Done => {
                next_stage(&mut leaf, S::ResetIntent)?;
                control.leaf = Some(leaf);
                metadata_work(metadata, &control, memory, None)?
            }
            S::ResetIntent => {
                let len = metadata.stream_len();
                let end = leaf
                    .stream_offset
                    .checked_add(512)
                    .ok_or(CandidateError::InvalidEvidence)?;
                if len != 0 && len != end {
                    return Err(CandidateError::InvalidEvidence);
                }
                if len != 0 {
                    let stream = root
                        .open_readonly_file(b".publication.stream")
                        .map_err(io)?;
                    let stamp = FixedFileStamp::capture(&stream, FixedControlKind::File)
                        .map_err(admission)?;
                    let mut offset = 0;
                    let mut frame = [0; 512];
                    while offset < len {
                        if cancel() {
                            return Err(CandidateError::Cancelled);
                        }
                        locks::read_fixed_range(&stream, offset, &mut frame).map_err(io)?;
                        let prior = super::maintenance_journal::RetirementLeaf::decode(&frame)
                            .map_err(|_| CandidateError::InvalidEvidence)?;
                        if prior.scope != scope
                            || prior.stream_offset != offset
                            || prior.stage != S::AppendIntent
                            || prior.stage_generation != 0
                        {
                            return Err(CandidateError::InvalidEvidence);
                        }
                        offset = offset
                            .checked_add(512)
                            .ok_or(CandidateError::InvalidEvidence)?;
                    }
                    stamp.recheck(&stream).map_err(admission)?;
                    if frame != stream_frame(&leaf) {
                        return Err(CandidateError::InvalidEvidence);
                    }
                }
                control.leaf = None;
                let mut work = metadata_work(metadata, &control, memory, None)?;
                if let Work::Metadata { reset, .. } = &mut work {
                    if len != 0 {
                        *reset = Some(len);
                    }
                }
                work
            }
        }
    };
    if cancel() {
        return Err(CandidateError::Cancelled);
    }
    Ok(Some(work))
}

#[inline(never)]
// Keep independent borrowed authorities and caller-owned scratch explicit.
#[allow(clippy::too_many_arguments)]
fn prepare_artifact_select(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    category: Option<super::maintenance::WorkCategory>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    root: RootedDirectory,
    mut control: Control,
) -> Result<Option<Work>, CandidateError> {
    let scope = metadata.origin_scope();
    let mut scratch = [0; 4096];
    let mut child_scratch = [0; 4096];
    let work = {
        let mut selection = control.cursor.selection();
        root.visit_entries(&mut scratch, &mut |prefix| {
            if cancel() {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if let Some(address) = temp_address(prefix) {
                if category.is_none_or(|c| c == super::maintenance::WorkCategory::TemporaryOrBackup)
                {
                    selection.observe(super::maintenance::WorkCategory::TemporaryOrBackup, address);
                }
                report.inspected_entries = report.inspected_entries.saturating_add(1);
                return Ok(());
            }
            if prefix.len() != 2 || !lower_hex(prefix) {
                return Ok(());
            }
            let directory = match root.open_directory_readonly(prefix) {
                Ok(d) => d,
                Err(_) => {
                    report.unknown = true;
                    report.reasons |= 1;
                    return Ok(());
                }
            };
            directory.visit_entries(&mut child_scratch, &mut |name| {
                if cancel() {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                report.inspected_entries = report.inspected_entries.saturating_add(1);
                if name.len() == 64 && lower_hex(name) && &name[..2] == prefix {
                    if category.is_none_or(|c| c == super::maintenance::WorkCategory::Canonical) {
                        selection.observe(
                            super::maintenance::WorkCategory::Canonical,
                            super::maintenance::CursorAddress::from_short(name.try_into().unwrap()),
                        );
                    }
                } else {
                    report.unknown = true;
                    report.reasons |= 1;
                }
                Ok(())
            })
        })
        .map_err(|e| {
            if cancel() {
                CandidateError::Cancelled
            } else {
                io(e)
            }
        })?;
        report.checked_categories |= 3;
        let Some((category, address)) = selection.selected() else {
            return Ok(None);
        };
        let previous = control.cursor.after(category);
        let wrapped = previous.is_some_and(|previous| address <= previous);
        control.cursor.inspected(category, address);
        report.pending = !wrapped;
        let source_kind = if category == super::maintenance::WorkCategory::Canonical {
            0
        } else {
            1
        };
        let source_name = if source_kind == 1 {
            temp_name(address)?
        } else {
            [0; 134]
        };
        let name = if source_kind == 0 {
            address.as_short().ok_or(CandidateError::InvalidEvidence)?
        } else {
            encode_key(address.0[..32].try_into().unwrap())
        };
        let key = decode_key(&name);
        let directory = if source_kind == 0 {
            root.open_directory_readonly(&name[..2])
                .and_then(|prefix| prefix.open_directory_readonly(&name))
        } else {
            root.open_directory_readonly(&source_name)
        }
        .map_err(io)?;
        let payload = directory.open_readonly_file(maintenance_origin::PAYLOAD_NAME);
        let origin = match payload {
            Ok(ref payload) => validate_origin(
                &root,
                &directory,
                payload,
                key,
                memory,
                OriginValidation {
                    incomplete: source_kind == 1,
                    detached: false,
                    cancel,
                },
            )?,
            Err(_) => None,
        };
        if let Some((origin, receipt_hash)) = origin {
            if origin.scope.root != scope.root
                || origin.scope.coordination != scope.coordination
                || origin.scope.cohort != scope.cohort
                || origin.scope.publication_binding != scope.publication_binding
                || origin.scope.incarnation != scope.incarnation
            {
                return Err(CandidateError::Stale);
            }
            let refs = super::maintenance_references::prepare_artifact_references(
                target, domain, expected, key, memory, cancel,
            )
            .map_err(reference)?;
            if !refs.complete {
                deferred(report, 1);
            } else if refs.referenced {
                report.reasons |= 1 << 1;
            } else {
                let mut operation = [0; 32];
                getrandom::fill(&mut operation).map_err(|_| CandidateError::InvalidEvidence)?;
                control.leaf = Some(super::maintenance_journal::RetirementLeaf {
                    stage: super::maintenance_journal::RetirementStage::AppendIntent,
                    source_epoch: expected.counter(),
                    stage_generation: 0,
                    stream_offset: metadata.stream_len(),
                    key,
                    directory: origin.directory,
                    payload: origin.payload,
                    receipt: origin.receipt,
                    scope,
                    operation,
                    byte_len: origin.byte_len,
                    digest: origin.payload_sha256,
                    directory_created: origin.directory_created,
                    receipt_digest: receipt_hash,
                    source_kind,
                    source_name,
                });
                report.pending = true;
                report.reasons |= 1 << 7;
            }
        } else {
            report.unknown = true;
            report.reasons |= 1;
        }
        metadata_work(metadata, &control, memory, None)?
    };
    if cancel() {
        return Err(CandidateError::Cancelled);
    }
    Ok(Some(work))
}

pub(crate) fn advance_candidate(
    hot: CandidateHotFrame<'_>,
    grant: &mut MutationGrant,
    confirmation: Option<&super::maintenance_scheduler::LegacyQuiescenceConfirmation<'_>>,
) -> Result<CandidateAdvance, CandidateError> {
    let prepared = hot.prepared;
    let mut report = prepared.report;
    if grant.check_time().is_err() {
        deferred(&mut report, 2);
        return Ok(CandidateAdvance {
            status: CandidateStatus::Deferred,
            report,
        });
    }
    let mut needs_disposal = false;
    let mut work = prepared
        .work
        .try_borrow_mut()
        .map_err(|_| CandidateError::InvalidEvidence)?;
    if let Work::Scheduled {
        metadata, update, ..
    } = &mut **work
    {
        let result = super::maintenance_admission::with_private_metadata(
            metadata,
            grant,
            update.effects(),
            &mut |mutation, grant| mutation.publish_control(update, grant),
        );
        if let Err(error) = result {
            match error {
                locks::FixedControlError::Resource(_) => return Err(CandidateError::ResourceLimit),
                locks::FixedControlError::Io(_) => deferred(&mut report, 12),
                locks::FixedControlError::Grant(_)
                | locks::FixedControlError::Invalid(locks::FixedControlProblem::Busy) => {
                    deferred(&mut report, 2)
                }
                _ => deferred(&mut report, 5),
            }
            return Ok(CandidateAdvance {
                status: CandidateStatus::Deferred,
                report,
            });
        }
    }
    let selected = match &mut **work {
        Work::Scheduled { inner, .. } => &mut **inner,
        other => other,
    };
    let result = match selected {
        Work::Scheduled { .. } => Err(locks::FixedControlError::Invalid(
            locks::FixedControlProblem::InvalidRecord,
        )),
        Work::Legacy(legacy) => legacy.advance(grant, confirmation).map(|actual| {
            report = actual;
        }),
        Work::Generation(generation) => generation.advance(grant, confirmation).map(|actual| {
            report = actual;
        }),
        Work::Backup(backup) => backup.advance(grant, confirmation).map(|actual| {
            report = actual;
        }),
        Work::Terminal(terminal) => terminal.advance(grant, &mut report),
        Work::WorkspaceRoot { terminal, removal } => {
            use super::maintenance_workspace_gate as gate;
            let progress = if removal.is_initial() {
                gate::begin_workspace_root_removal(
                    terminal.as_mut().ok_or(CandidateError::InvalidEvidence)?,
                    removal,
                    grant,
                )
            } else {
                gate::advance_workspace_root_removal(removal, grant, 0, &mut |outcome, _grant| {
                    // The actual outcome is consumed before the gate's durable
                    // Idle completion. It cannot be synthesized by discovery.
                    let _closed = (
                        outcome.root_identity(),
                        outcome.incarnation(),
                        outcome.source_epoch(),
                    );
                    Ok(())
                })
            };
            progress.map(|progress| {
                if progress == gate::WorkspaceRemovalProgress::Complete {
                    report.mutated_entries = report.mutated_entries.saturating_add(1);
                    report.reclaimed_entries = report.reclaimed_entries.saturating_add(1);
                }
            })
        }
        Work::Metadata {
            prepared,
            update,
            append,
            reset,
        } => {
            let effects = update.effects()
                + if append.is_some() { 3 } else { 0 }
                + if reset.is_some() { 2 } else { 0 };
            super::maintenance_admission::with_private_metadata(
                prepared,
                grant,
                effects,
                &mut |mutation, grant| {
                    if let Some(append) = append {
                        mutation.append_frame(append.offset, &append.frame, grant)?;
                    }
                    if let Some(len) = reset {
                        mutation.reset_stream(*len, grant)?;
                    }
                    mutation.publish_control(update, grant)
                },
            )
        }
        Work::Leaf {
            ready,
            leaf,
            stamp,
            stream_stamp,
            held_file: _,
            payload_nonce,
            post_rename,
            action,
        } => {
            let mut changed = false;
            let result = super::maintenance_admission::with_prepared_canonical(
                ready,
                grant,
                28,
                &mut |mutation, grant| {
                    let invalid =
                        || locks::FixedControlError::Invalid(locks::FixedControlProblem::Changed);
                    if mutation.origin_scope() != leaf.scope {
                        return Err(invalid());
                    }
                    let root = mutation.directory();
                    grant.check_time()?;
                    let stream = root.open_readonly_file(b".publication.stream")?;
                    stream_stamp.recheck(&stream)?;
                    let mut frame = [0; 512];
                    grant.check_time()?;
                    locks::read_fixed_range(&stream, leaf.stream_offset, &mut frame)?;
                    if frame != stream_frame(leaf) {
                        return Err(invalid());
                    }
                    let name = encode_key(&leaf.key);
                    grant.check_time()?;
                    let prefix = if leaf.source_kind == 0 {
                        Some(root.open_directory(&name[..2])?)
                    } else {
                        None
                    };
                    let parent = prefix.as_ref().unwrap_or(root);
                    let source_name: &[u8] = if leaf.source_kind == 0 {
                        &name
                    } else {
                        &leaf.source_name
                    };
                    grant.check_time()?;
                    let directory = parent.open_directory(source_name)?;
                    if directory.identity() != leaf.directory {
                        return Err(invalid());
                    }
                    match action {
                        LeafAction::Detach => {
                            if mutation.epoch().before != leaf.source_epoch {
                                return Err(invalid());
                            }
                            grant.check_time()?;
                            let payload =
                                directory.open_existing(maintenance_origin::PAYLOAD_NAME)?;
                            stamp.recheck(&payload)?;
                            if locks::created_file_nonce(&payload)? != *payload_nonce {
                                return Err(invalid());
                            }
                            grant.check_time()?;
                            directory.rename_file_no_replace(
                                maintenance_origin::PAYLOAD_NAME,
                                root,
                                &leaf.tombstone(),
                                leaf.payload,
                            )?;
                            changed = true;
                            grant.check_time()?;
                            let moved = root.open_readonly_file(&leaf.tombstone())?;
                            let current = FixedFileStamp::capture(&moved, FixedControlKind::File)?;
                            let mut before = [0; 80];
                            let mut after = [0; 80];
                            stamp.encode_checkpoint(&mut before);
                            current.encode_checkpoint(&mut after);
                            // Only this successfully observed own rename may
                            // establish its new ctime. Identity/kind/length/link
                            // and mtime plus immutable EA must still match.
                            if before[..64] != after[..64]
                                || locks::created_file_nonce(&moved)? != *payload_nonce
                            {
                                return Err(invalid());
                            }
                            *post_rename = Some(current);
                            grant.check_time()?;
                            directory.sync_namespace()?;
                            grant.check_time()?;
                            root.sync_namespace()?;
                            #[cfg(all(test, debug_assertions))]
                            fault::reach(leaf.scope.root, CandidateFault::AfterDetach)?;
                        }
                        LeafAction::UnlinkPayload => {
                            grant.check_time()?;
                            let payload = root.open_existing(&leaf.tombstone())?;
                            stamp.recheck(&payload)?;
                            if locks::created_file_nonce(&payload)? != *payload_nonce {
                                return Err(invalid());
                            }
                            grant.check_time()?;
                            root.unlink_file(&leaf.tombstone(), leaf.payload)?;
                            changed = true;
                            grant.check_time()?;
                            root.sync_namespace()?;
                            #[cfg(all(test, debug_assertions))]
                            fault::reach(leaf.scope.root, CandidateFault::AfterPayloadUnlink)?;
                        }
                        LeafAction::UnlinkReceipt => {
                            grant.check_time()?;
                            match directory.open_readonly_file(maintenance_origin::PAYLOAD_NAME) {
                                Err(error) if absent(&error) => {}
                                Err(error) => return Err(error.into()),
                                Ok(_) => return Err(invalid()),
                            }
                            grant.check_time()?;
                            match root.open_readonly_file(&leaf.tombstone()) {
                                Err(error) if absent(&error) => {}
                                Err(error) => return Err(error.into()),
                                Ok(_) => return Err(invalid()),
                            }
                            grant.check_time()?;
                            let receipt =
                                directory.open_existing(maintenance_origin::ORIGIN_NAME)?;
                            stamp.recheck(&receipt)?;
                            grant.check_time()?;
                            directory.unlink_file(maintenance_origin::ORIGIN_NAME, leaf.receipt)?;
                            changed = true;
                            grant.check_time()?;
                            directory.sync_namespace()?;
                            // Atomic-empty only; unknown siblings keep their public directory.
                            if leaf.directory_created {
                                grant.check_time()?;
                                match parent.remove_empty_directory(source_name, leaf.directory) {
                                    Ok(()) => {
                                        grant.check_time()?;
                                        parent.sync_namespace()?;
                                    }
                                    Err(error) if super::locks::is_directory_not_empty(&error) => {
                                        report.unknown = true;
                                        report.reasons |= 1;
                                    }
                                    Err(error) => return Err(error.into()),
                                }
                            }
                        }
                    }
                    Ok(())
                },
            );
            if changed {
                report.mutated_entries = report.mutated_entries.saturating_add(1);
                if matches!(action, LeafAction::UnlinkPayload) {
                    needs_disposal = true;
                    report.reclaimed_entries = report.reclaimed_entries.saturating_add(1);
                    report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(leaf.byte_len);
                }
            }
            result.map(|_| ())
        }
    };
    match result {
        Ok(()) => {}
        Err(locks::FixedControlError::Grant(_))
        | Err(locks::FixedControlError::Invalid(locks::FixedControlProblem::Busy)) => {
            deferred(&mut report, 2)
        }
        Err(locks::FixedControlError::Invalid(_)) => deferred(&mut report, 5),
        Err(locks::FixedControlError::Resource(_)) => return Err(CandidateError::ResourceLimit),
        Err(locks::FixedControlError::Io(error)) => {
            deferred(
                &mut report,
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    8
                } else {
                    12
                },
            );
        }
    }
    let status = if needs_disposal {
        CandidateStatus::NeedsDisposal
    } else if report.deferred {
        CandidateStatus::Deferred
    } else if report.pending {
        CandidateStatus::Pending
    } else {
        CandidateStatus::Complete
    };
    Ok(CandidateAdvance { status, report })
}
pub(crate) fn finish_candidate(
    prepared: &mut PreparedCandidate,
    advance: CandidateAdvance,
    state: &mut CandidatePreparationState,
    cancel: &dyn Fn() -> bool,
) -> Result<CandidateReport, CandidateError> {
    if cancel() {
        return Err(CandidateError::Cancelled);
    }
    let selected = match &mut **prepared.work.get_mut() {
        Work::Scheduled { inner, .. } => &mut **inner,
        other => other,
    };
    if let Work::Legacy(legacy) = &mut *selected {
        super::maintenance_legacy_engine::finish_work(legacy, &mut state.legacy_jobs)
            .map_err(admission)?;
    }
    if let Work::Generation(generation) = &mut *selected {
        super::maintenance_generation_engine::finish_work(generation, &mut state.generations);
    }
    if let Work::Backup(backup) = &mut *selected {
        super::maintenance_backup::finish_work(backup, &mut state.backup).map_err(admission)?;
    }
    // Last close of an unlinked payload may perform expensive disposal. Keep
    // the preparation handle through mutation and close it only offpermit.
    if let Work::Leaf {
        held_file,
        post_rename,
        ..
    } = &mut *selected
    {
        let _observed_own_transition = post_rename.take();
        drop(held_file.take());
    }
    if let Work::Terminal(terminal) = &mut *selected {
        drop(terminal.held_payload.take());
    }
    Ok(advance.report)
}

#[cfg(all(test, debug_assertions))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateFault {
    AfterDetach,
    AfterPayloadUnlink,
    DuringPreparation,
}
#[cfg(all(test, debug_assertions))]
pub(crate) use fault::{arm_candidate_fault, arm_preparation_pause, CandidateFaultGuard};
#[cfg(all(test, debug_assertions))]
mod fault {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    };
    #[derive(Clone, Copy)]
    struct Armed {
        root: locks::FileIdentity,
        point: CandidateFault,
        nonce: u64,
        hit: bool,
        cancelled: bool,
    }
    static ARMED: Mutex<[Option<Armed>; 8]> = Mutex::new([None; 8]);
    static NEXT: AtomicU64 = AtomicU64::new(1);
    pub(crate) struct CandidateFaultGuard {
        slot: usize,
        nonce: u64,
    }
    impl CandidateFaultGuard {
        pub(crate) fn cancelled(&self) -> bool {
            ARMED
                .lock()
                .ok()
                .and_then(|a| a[self.slot])
                .is_some_and(|a| a.nonce == self.nonce && a.cancelled)
        }
        pub(crate) fn reached(&self) -> bool {
            ARMED
                .lock()
                .ok()
                .and_then(|a| a[self.slot])
                .is_some_and(|a| a.nonce == self.nonce && a.hit)
        }
    }
    impl Drop for CandidateFaultGuard {
        fn drop(&mut self) {
            if let Ok(mut slots) = ARMED.lock() {
                if slots[self.slot].is_some_and(|a| a.nonce == self.nonce) {
                    slots[self.slot] = None;
                }
            }
        }
    }
    pub(crate) fn arm_candidate_fault(
        root: &std::path::Path,
        point: CandidateFault,
    ) -> Result<CandidateFaultGuard, CandidateError> {
        let root = locks::path_identity(root).map_err(evidence)?;
        let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut slots = ARMED.lock().map_err(|_| CandidateError::InvalidEvidence)?;
        if slots.iter().flatten().any(|a| a.root == root) {
            return Err(CandidateError::InvalidEvidence);
        }
        let slot = slots
            .iter()
            .position(Option::is_none)
            .ok_or(CandidateError::ResourceLimit)?;
        slots[slot] = Some(Armed {
            root,
            point,
            nonce,
            hit: false,
            cancelled: false,
        });
        Ok(CandidateFaultGuard { slot, nonce })
    }
    pub(crate) fn arm_preparation_pause(
        root: &std::path::Path,
    ) -> Result<CandidateFaultGuard, CandidateError> {
        arm_candidate_fault(root, CandidateFault::DuringPreparation)
    }
    pub(super) fn preparation_pause(
        root: locks::FileIdentity,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), CandidateError> {
        loop {
            let stopped = cancel();
            {
                let mut slots = ARMED.lock().map_err(|_| CandidateError::InvalidEvidence)?;
                let Some(armed) = slots
                    .iter_mut()
                    .flatten()
                    .find(|a| a.root == root && a.point == CandidateFault::DuringPreparation)
                else {
                    return Ok(());
                };
                armed.hit = true;
                if stopped {
                    armed.cancelled = true;
                    return Err(CandidateError::Cancelled);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    pub(super) fn paused(root: locks::FileIdentity) -> bool {
        ARMED.lock().ok().is_some_and(|slots| {
            slots
                .iter()
                .flatten()
                .any(|a| a.root == root && a.hit && a.point != CandidateFault::DuringPreparation)
        })
    }
    pub(super) fn reach(root: locks::FileIdentity, point: CandidateFault) -> std::io::Result<()> {
        let mut slots = ARMED.lock().map_err(|_| std::io::ErrorKind::Other)?;
        if let Some(armed) = slots
            .iter_mut()
            .flatten()
            .find(|a| a.root == root && a.point == point && !a.hit)
        {
            armed.hit = true;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_candidate_crossings_keep_large_state_indirect() {
        assert!(std::mem::size_of::<PreparedCandidate>() < 2048);
        assert!(std::mem::size_of::<Work>() < 4096);
        eprintln!(
            "S3 requested layout: PreparedCandidate={} Work={} Canonical={} Metadata={}",
            std::mem::size_of::<PreparedCandidate>(),
            std::mem::size_of::<Work>(),
            std::mem::size_of::<super::super::maintenance_admission::PreparedCanonical>(),
            std::mem::size_of::<super::super::maintenance_admission::PreparedPrivateMetadata>()
        );
    }
    #[test]
    fn candidate_reason_codes_are_fixed_and_reports_do_not_invent_measurements() {
        let r = CandidateReport::default();
        assert!(!r.observations_complete);
        assert_eq!(r.reclaimed_allocated_bytes, None);
        assert!((0..=12).all(|bit| candidate_reason(bit).is_some()));
        assert_eq!(candidate_reason(13), None);
    }
    #[test]
    fn retirement_control_roundtrip_does_not_certify_whole_pass() {
        let id = locks::FileIdentity::from_fixed_words(1, 2);
        let scope = super::super::maintenance_admission::RootOriginBinding {
            root: id,
            coordination: id,
            cohort: id,
            publication_binding: id,
            incarnation: [3; 32],
        };
        let control = Control::decode(b"CGBUILD1", scope).unwrap();
        let encoded = control.encode();
        let decoded = Control::decode(&encoded, scope).unwrap();
        assert!(decoded.leaf.is_none());
        let mut corrupt = encoded;
        corrupt[8] = 1;
        assert!(Control::decode(&corrupt, scope).is_err());
    }
}

/// Produced only after the already-prepared completion control was durably
/// published through the actual private-metadata mutation facade. A decoded
/// completion record is deliberately a different, non-authorizing fact.
pub(crate) struct GenerationCohortDone {
    managed: locks::FileIdentity,
    removed: locks::FileIdentity,
    cohort_digest: [u8; 32],
}
impl GenerationCohortDone {
    pub(crate) fn matches(
        &self,
        managed: locks::FileIdentity,
        removed: locks::FileIdentity,
        cohort_digest: [u8; 32],
    ) -> bool {
        self.managed == managed && self.removed == removed && self.cohort_digest == cohort_digest
    }
}
/// Both the control image and cursor are constructed and charged off authority.
/// The root-removal callback may borrow this owner; it never moves or drops it.
pub(crate) struct PreparedGenerationCompletion {
    managed: locks::FileIdentity,
    removed: locks::FileIdentity,
    cohort_digest: [u8; 32],
    update: MemoryBox<super::maintenance_admission::PreparedControlUpdate>,
}
impl PreparedGenerationCompletion {
    pub(crate) fn effects(&self) -> usize {
        self.update.effects()
    }
    pub(crate) fn publish(
        &self,
        mutation: &mut super::maintenance_admission::PrivateMetadataMutation<'_>,
        grant: &mut MutationGrant,
    ) -> Result<GenerationCohortDone, locks::FixedControlError> {
        mutation.publish_control(&self.update, grant)?;
        Ok(GenerationCohortDone {
            managed: self.managed,
            removed: self.removed,
            cohort_digest: self.cohort_digest,
        })
    }
}
pub(crate) fn prepare_generation_completion(
    scope: &super::maintenance_admission::RootOriginBinding,
    before: super::maintenance_controls::CommitFact,
    managed: locks::FileIdentity,
    removed: locks::FileIdentity,
    cohort_digest: [u8; 32],
    cursor: &super::maintenance::MaintenanceCursor,
    memory: &CleanupMemory,
) -> Result<PreparedGenerationCompletion, CandidateError> {
    const SIZE: usize = 72 + super::maintenance::CURSOR_BYTES;
    let _scratch = memory
        .reserve(SIZE)
        .map_err(|_| CandidateError::ResourceLimit)?;
    let mut bytes = [0; SIZE];
    bytes[..8].copy_from_slice(b"CGGENDON");
    for (offset, id) in [(8, managed), (24, removed)] {
        let (a, b) = id.fixed_words();
        bytes[offset..offset + 8].copy_from_slice(&a.to_le_bytes());
        bytes[offset + 8..offset + 16].copy_from_slice(&b.to_le_bytes());
    }
    bytes[40..72].copy_from_slice(&cohort_digest);
    bytes[72..].copy_from_slice(&cursor.encode());
    let update =
        super::maintenance_admission::prepare_metadata_control(scope, before, &bytes, memory)
            .map_err(admission)?;
    Ok(PreparedGenerationCompletion {
        managed,
        removed,
        cohort_digest,
        update: MemoryBox::new(update, memory).map_err(|_| CandidateError::ResourceLimit)?,
    })
}

// A workspace output's genuine origin record is its prebound, same-root
// retirement journal. No new namespace or directory is adopted as owned.
const WORKSPACE_RETIRE_BYTES: usize = 512;
const WORKSPACE_PATH_DEPTH: usize = 16;
#[derive(Clone, Copy)]
struct WorkspaceParentPath {
    names: [[u8; 255]; WORKSPACE_PATH_DEPTH],
    lengths: [u8; WORKSPACE_PATH_DEPTH],
    identities: [Option<locks::FileIdentity>; WORKSPACE_PATH_DEPTH],
    depth: usize,
}
impl Default for WorkspaceParentPath {
    fn default() -> Self {
        Self {
            names: [[0; 255]; WORKSPACE_PATH_DEPTH],
            lengths: [0; WORKSPACE_PATH_DEPTH],
            identities: [None; WORKSPACE_PATH_DEPTH],
            depth: 0,
        }
    }
}
struct TerminalWork {
    ready: MemoryBox<super::maintenance_controls::PreparedTerminalWorkspace>,
    scope: super::maintenance_controls::WorkspaceOriginScope,
    parent: MemoryBox<WorkspaceParentPath>,
    origin: maintenance_origin::WorkspaceOriginFacts,
    receipt_name: [u8; 64],
    source_name: [u8; 255],
    source_name_len: u8,
    receipt_only: bool,
    root_metadata_receipt: bool,
    receipt_stamp: FixedFileStamp,
    receipt_nonce: [u8; 32],
    receipt_length: u64,
    append_offset: u64,
    prefix_length: usize,
    prefix_digest: [u8; 32],
    payload_stamp: Option<FixedFileStamp>,
    payload_digest: [u8; 32],
    payload_length: u64,
    stage: u8,
    frame: [u8; WORKSPACE_RETIRE_BYTES],
    held_payload: Option<std::fs::File>,
}
fn workspace_scope_matches(
    f: &maintenance_origin::WorkspaceOriginFacts,
    s: super::maintenance_controls::WorkspaceOriginScope,
) -> bool {
    let b = f.binding;
    b.root == s.root
        && b.coordination == s.coordination
        && b.registration == s.registration
        && b.publication_binding == s.publication_binding
        && b.incarnation == s.incarnation
        && b.registry == s.registry
}
fn workspace_retire_frame(
    work: &TerminalWork,
    stage: u8,
    source: u64,
    admission_epoch: u64,
) -> [u8; WORKSPACE_RETIRE_BYTES] {
    let mut b = [0; WORKSPACE_RETIRE_BYTES];
    b[..8].copy_from_slice(b"CGWRET01");
    b[8] = stage;
    b[16..24].copy_from_slice(&source.to_le_bytes());
    b[24..32].copy_from_slice(&admission_epoch.to_le_bytes());
    b[32..40].copy_from_slice(&(work.prefix_length as u64).to_le_bytes());
    b[40..72].copy_from_slice(&work.prefix_digest);
    if let Some(stamp) = work.payload_stamp {
        stamp.encode_checkpoint((&mut b[72..152]).try_into().unwrap());
    }
    b[152..160].copy_from_slice(&work.payload_length.to_le_bytes());
    b[160..192].copy_from_slice(&work.payload_digest);
    b[192..224].copy_from_slice(&work.origin.binding.operation);
    for (at, id) in [(224, work.scope.root), (240, work.origin.receipt)] {
        let (a, c) = id.fixed_words();
        b[at..at + 8].copy_from_slice(&a.to_le_bytes());
        b[at + 8..at + 16].copy_from_slice(&c.to_le_bytes());
    }
    b[256] = u8::from(work.origin.directory);
    let sha = Sha256::digest(&b[..480]);
    b[480..].copy_from_slice(&sha);
    b
}
fn workspace_frame_valid(b: &[u8; 512], work: &TerminalWork, stage: u8) -> bool {
    &b[..8] == b"CGWRET01"
        && b[8] == stage
        && b[9..16].iter().all(|x| *x == 0)
        && b[32..72] == workspace_retire_frame(work, stage, 0, 0)[32..72]
        && b[192..257] == workspace_retire_frame(work, stage, 0, 0)[192..257]
        && b[257..480].iter().all(|x| *x == 0)
        && b[480..] == Sha256::digest(&b[..480])[..]
}
fn workspace_parent(
    root: &RootedDirectory,
    path: &WorkspaceParentPath,
) -> std::io::Result<Option<RootedDirectory>> {
    let mut current = None;
    for i in 0..path.depth {
        let parent = current.as_ref().unwrap_or(root);
        let next = parent.open_directory(&path.names[i][..path.lengths[i] as usize])?;
        if Some(next.identity()) != path.identities[i] {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        current = Some(next);
    }
    Ok(current)
}
// Iterative fixed-depth discovery owns no exclusion. Every retained directory
// and all caller scratch are charged before construction. Unknown depth is
// preserved; this visitor never recursively deletes a directory.
#[inline(never)]
fn new_workspace_parent_path(
    memory: &CleanupMemory,
) -> Result<MemoryBox<WorkspaceParentPath>, CandidateError> {
    MemoryBox::new(WorkspaceParentPath::default(), memory)
        .map_err(|_| CandidateError::ResourceLimit)
}
#[inline(never)]
fn find_workspace_parent(
    root: &RootedDirectory,
    wanted: locks::FileIdentity,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<WorkspaceParentPath>>, CandidateError> {
    struct Walk {
        dirs: [Option<RootedDirectory>; WORKSPACE_PATH_DEPTH],
        last: [[u8; 255]; WORKSPACE_PATH_DEPTH],
        lens: [u8; WORKSPACE_PATH_DEPTH],
        path: Option<MemoryBox<WorkspaceParentPath>>,
        scratch: [u8; 4096],
    }
    let mut w = MemoryBox::new(
        Walk {
            dirs: std::array::from_fn(|_| None),
            last: [[0; 255]; WORKSPACE_PATH_DEPTH],
            lens: [0; WORKSPACE_PATH_DEPTH],
            path: Some(new_workspace_parent_path(memory)?),
            scratch: [0; 4096],
        },
        memory,
    )
    .map_err(|_| CandidateError::ResourceLimit)?;
    if root.identity() == wanted {
        return Ok(w.path.take());
    }
    let mut depth = 0;
    loop {
        if cancel() {
            return Err(CandidateError::Cancelled);
        }
        let mut chosen = [0; 255];
        let mut chosen_len = 0usize;
        let previous = w.last[depth];
        let previous_len = w.lens[depth] as usize;
        let Walk { dirs, scratch, .. } = &mut *w;
        let directory = if depth == 0 {
            root
        } else {
            dirs[depth - 1].as_ref().unwrap()
        };
        directory
            .visit_directories(scratch, &mut |name| {
                if name.is_empty()
                    || name.len() > 255
                    || !name.is_ascii()
                    || name == b"."
                    || name == b".."
                    || name <= &previous[..previous_len]
                    || (chosen_len > 0 && name >= &chosen[..chosen_len])
                {
                    return Ok(());
                }
                if directory.open_directory_readonly(name).is_ok() {
                    chosen[..name.len()].copy_from_slice(name);
                    chosen_len = name.len();
                }
                Ok(())
            })
            .map_err(io)?;
        if chosen_len == 0 {
            if depth == 0 {
                return Ok(None);
            }
            depth -= 1;
            w.dirs[depth] = None;
            w.path.as_mut().unwrap().depth = depth;
            continue;
        }
        w.last[depth] = chosen;
        w.lens[depth] = chosen_len as u8;
        let parent = if depth == 0 {
            root
        } else {
            w.dirs[depth - 1].as_ref().unwrap()
        };
        let child = parent
            .open_directory_readonly(&chosen[..chosen_len])
            .map_err(io)?;
        w.path.as_mut().unwrap().names[depth] = chosen;
        w.path.as_mut().unwrap().lengths[depth] = chosen_len as u8;
        w.path.as_mut().unwrap().identities[depth] = Some(child.identity());
        w.path.as_mut().unwrap().depth = depth + 1;
        if child.identity() == wanted {
            return Ok(w.path.take());
        }
        if depth + 1 == WORKSPACE_PATH_DEPTH {
            continue;
        }
        w.dirs[depth] = Some(child);
        depth += 1;
        w.last[depth] = [0; 255];
        w.lens[depth] = 0;
    }
}
fn terminal_name_address(kind: u8, name: &[u8]) -> Option<super::maintenance::CursorAddress> {
    if name.is_empty() || name.len() > 127 {
        return None;
    }
    let mut address = [0; 128];
    address[0] = kind;
    address[1..1 + name.len()].copy_from_slice(name);
    Some(super::maintenance::CursorAddress(address))
}
fn terminal_path_address(
    path: &std::path::Path,
    domain: &ReferenceDomain,
    target: &MaintenanceTarget,
) -> Option<(
    super::maintenance::WorkCategory,
    super::maintenance::CursorAddress,
)> {
    for kind in 1..=3 {
        let base = domain.workspace_registry(kind, Some(&target.state_root))?;
        if path.parent() == Some(base.as_path()) {
            return Some((
                if kind == 3 {
                    super::maintenance::WorkCategory::Phase
                } else {
                    super::maintenance::WorkCategory::Run
                },
                terminal_name_address(kind, path.file_name()?.as_encoded_bytes())?,
            ));
        }
    }
    None
}
fn terminal_work_address(
    work: &Work,
    domain: &ReferenceDomain,
    target: &MaintenanceTarget,
) -> Option<super::maintenance::CursorAddress> {
    let root = match work {
        Work::Terminal(work) => work.ready.root(),
        Work::WorkspaceRoot { removal, .. } => removal.root(),
        _ => return None,
    };
    terminal_path_address(root, domain, target).map(|(_, address)| address)
}
#[inline(never)]
fn boxed_pending_workspace_removal(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
) -> Result<
    Option<MemoryBox<super::maintenance_workspace_gate::PreparedWorkspaceRemoval>>,
    CandidateError,
> {
    super::maintenance_workspace_gate::prepare_pending_workspace_root_removal_for_target(
        target, domain, memory,
    )
    .map_err(admission)
}
#[inline(never)]
fn boxed_workspace_removal(
    path: &std::path::Path,
    domain: &ReferenceDomain,
    terminal: &super::maintenance_controls::PreparedTerminalWorkspace,
    memory: &CleanupMemory,
) -> Result<MemoryBox<super::maintenance_workspace_gate::PreparedWorkspaceRemoval>, CandidateError>
{
    super::maintenance_workspace_gate::prepare_workspace_root_removal_boxed(
        path,
        domain,
        Some(terminal),
        memory,
    )
    .map_err(admission)
}
#[inline(never)]
fn prepare_terminal_candidate(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
    category: super::maintenance::WorkCategory,
    after: Option<super::maintenance::CursorAddress>,
) -> Result<Option<Work>, CandidateError> {
    if let Some(removal) = boxed_pending_workspace_removal(target, domain, memory)? {
        if terminal_path_address(removal.root(), domain, target).is_some_and(|(c, _)| c == category)
        {
            return Ok(Some(Work::WorkspaceRoot {
                terminal: None,
                removal,
            }));
        }
    }
    for pass in 0..2 {
        if pass == 1 && after.is_none() {
            break;
        }
        for (kind, base) in [
            (1, domain.workspace_registry(1, Some(&target.state_root))),
            (2, domain.workspace_registry(2, Some(&target.state_root))),
            (3, domain.workspace_registry(3, Some(&target.state_root))),
        ] {
            if (category == super::maintenance::WorkCategory::Phase) != (kind == 3) {
                continue;
            }
            let Some(base) = base else {
                continue;
            };
            let id = match locks::path_identity(&base) {
                Ok(id) => id,
                Err(_) => continue,
            };
            let root = RootedDirectory::open_readonly_root(&base, id).map_err(io)?;
            let mut next = [0; 255];
            let mut next_len = 0;
            let mut scratch = [0; 4096];
            // Each root is examined independently. Foreign roots are never mutated.
            root.visit_entries(&mut scratch, &mut |name| {
                if cancel() {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let selected_pass = terminal_name_address(kind, name).is_some_and(|address| {
                    after.is_none_or(|previous| {
                        if pass == 0 {
                            address > previous
                        } else {
                            address <= previous
                        }
                    })
                });
                if selected_pass
                    && name.len() <= 255
                    && !name.is_empty()
                    && name.is_ascii()
                    && name != b"."
                    && name != b".."
                    && (next_len == 0 || name < &next[..next_len])
                    && root.open_directory_readonly(name).is_ok()
                {
                    next = [0; 255];
                    next[..name.len()].copy_from_slice(name);
                    next_len = name.len();
                }
                Ok(())
            })
            .map_err(io)?;
            while next_len > 0 {
                if cancel() {
                    return Err(CandidateError::Cancelled);
                }
                let name = std::str::from_utf8(&next[..next_len])
                    .map_err(|_| CandidateError::InvalidEvidence)?;
                let path = base.join(name);
                match super::maintenance_controls::prepare_terminal_workspace_boxed(
                    &path, domain, memory,
                ) {
                    Ok(ready) => {
                        if let Some(work) = prepare_terminal_output(ready, memory, cancel, report)?
                        {
                            #[cfg(test)]
                            if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                                eprintln!("terminal candidate selected output: {}", path.display());
                            }
                            return Ok(Some(Work::Terminal(work)));
                        }
                        #[cfg(test)]
                        if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                            eprintln!("terminal candidate has no output: {}", path.display());
                        }
                        let terminal =
                            super::maintenance_controls::prepare_terminal_workspace_boxed(
                                &path, domain, memory,
                            )
                            .map_err(admission)?;
                        match boxed_workspace_removal(&path, domain, &terminal, memory) {
                            Ok(removal) => {
                                report.pending = true;
                                report.reasons |= 1 << 7;
                                return Ok(Some(Work::WorkspaceRoot {
                                    terminal: Some(terminal),
                                    removal,
                                }));
                            }
                            Err(error) => {
                                #[cfg(test)]
                                if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                                    eprintln!(
                                        "terminal candidate root removal rejected: {}: {error:?}",
                                        path.display()
                                    );
                                }
                                let _ = error;
                            }
                        }
                    }
                    Err(error) => {
                        #[cfg(test)]
                        if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
                            eprintln!(
                                "terminal candidate workspace rejected: {}: {error:?}",
                                path.display()
                            );
                        }
                        let _ = error;
                        deferred(report, 2);
                    }
                }
                let previous = next;
                let previous_len = next_len;
                next = [0; 255];
                next_len = 0;
                root.visit_entries(&mut scratch, &mut |name| {
                    if cancel() {
                        return Err(std::io::ErrorKind::Interrupted.into());
                    }
                    let selected_pass = terminal_name_address(kind, name).is_some_and(|address| {
                        after.is_none_or(|previous| {
                            if pass == 0 {
                                address > previous
                            } else {
                                address <= previous
                            }
                        })
                    });
                    if selected_pass
                        && name.len() <= 255
                        && !name.is_empty()
                        && name.is_ascii()
                        && name > &previous[..previous_len]
                        && (next_len == 0 || name < &next[..next_len])
                        && root.open_directory_readonly(name).is_ok()
                    {
                        next[..name.len()].copy_from_slice(name);
                        next_len = name.len();
                    }
                    Ok(())
                })
                .map_err(io)?;
            }
        }
    }
    Ok(None)
}
#[inline(never)]
fn workspace_has_child_receipts(
    registry: &RootedDirectory,
    parent: locks::FileIdentity,
    scope: super::maintenance_controls::WorkspaceOriginScope,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<bool, CandidateError> {
    let mut found = false;
    let mut scratch = [0; 4096];
    registry
        .visit_entries(&mut scratch, &mut |entry| {
            if cancel() {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if found || entry.len() != 64 || !lower_hex(entry) {
                return Ok(());
            }
            let Ok(file) = registry.open_readonly_file(entry) else {
                found = true;
                return Ok(());
            };
            let Ok(stamp) = FixedFileStamp::capture(&file, FixedControlKind::File) else {
                found = true;
                return Ok(());
            };
            let mut bytes = [0; 1024];
            if locks::read_fixed_range(&file, 0, &mut bytes).is_err() {
                found = true;
                return Ok(());
            }
            found = match maintenance_origin::decode_workspace_output_origin(
                &bytes,
                stamp.identity(),
                None,
                memory,
            ) {
                Ok(fact) => {
                    // This is only a conservative veto. It grants no authority
                    // over a child, including one whose payload is already gone.
                    workspace_scope_matches(&fact, scope) && fact.binding.parent == parent
                }
                Err(_) => true,
            };
            Ok(())
        })
        .map_err(io)?;
    Ok(found)
}

fn prepare_terminal_output(
    ready: MemoryBox<super::maintenance_controls::PreparedTerminalWorkspace>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    report: &mut CandidateReport,
) -> Result<Option<MemoryBox<TerminalWork>>, CandidateError> {
    let scope = ready.origin_scope();
    let root = RootedDirectory::open_readonly_root(ready.root(), scope.root).map_err(io)?;
    let registry = root
        .open_directory_readonly(b".workspace-origins")
        .map_err(io)?;
    if registry.identity() != scope.registry {
        return Err(CandidateError::Stale);
    }
    let mut name = [0; 64];
    let mut found = false;
    let mut scratch = [0; 4096];
    for want_directory in [false, true] {
        registry
            .visit_entries(&mut scratch, &mut |entry| {
                if cancel() {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                if entry.len() == 64 && lower_hex(entry) && (!found || entry < &name) {
                    let Ok(file) = registry.open_readonly_file(entry) else {
                        return Ok(());
                    };
                    let Ok(stamp) = FixedFileStamp::capture(&file, FixedControlKind::File) else {
                        return Ok(());
                    };
                    let mut bound = [0; 1024];
                    if locks::read_fixed_range(&file, 0, &mut bound).is_err() {
                        return Ok(());
                    }
                    let Ok(fact) = maintenance_origin::decode_workspace_output_origin(
                        &bound,
                        stamp.identity(),
                        None,
                        memory,
                    ) else {
                        return Ok(());
                    };
                    let receipt_nonce = super::maintenance_controls::workspace_receipt_nonce(
                        fact.binding.incarnation,
                        fact.binding.parent,
                        fact.binding.output,
                        fact.binding.operation,
                    );
                    if locks::created_file_nonce(&file)? != Some(receipt_nonce) {
                        report.unknown = true;
                        return Ok(());
                    }
                    if fact.directory != want_directory
                        || !workspace_scope_matches(&fact, scope)
                        || super::maintenance_controls::workspace_origin_name(
                            &fact.binding.incarnation,
                            fact.binding.parent,
                            fact.binding.output,
                            &fact.binding.operation,
                        ) != entry
                    {
                        return Ok(());
                    }
                    if fact.directory {
                        let Ok(Some(path)) =
                            find_workspace_parent(&root, fact.binding.parent, memory, cancel)
                        else {
                            return Ok(());
                        };
                        let Ok(parent) = workspace_parent(&root, &path) else {
                            return Ok(());
                        };
                        let p = parent.as_ref().unwrap_or(&root);
                        match p.open_directory_readonly(
                            &fact.binding.name[..fact.binding.name_len as usize],
                        ) {
                            Ok(directory) => {
                                if directory.identity() != fact.binding.output {
                                    return Ok(());
                                }
                                // Children keep their parent until their own
                                // retirement receipts have also been consumed.
                                // Otherwise a restart cannot resolve the parent
                                // identity of a child already removed from disk.
                                if workspace_has_child_receipts(
                                    &registry,
                                    directory.identity(),
                                    scope,
                                    memory,
                                    cancel,
                                )
                                .map_err(|_| std::io::ErrorKind::InvalidData)?
                                {
                                    return Ok(());
                                }
                                let mut contents = [0; 4096];
                                let mut empty = true;
                                directory.visit_entries(&mut contents, &mut |name| {
                                    if cancel() {
                                        return Err(std::io::ErrorKind::Interrupted.into());
                                    }
                                    if name != b"." && name != b".." {
                                        empty = false
                                    }
                                    Ok(())
                                })?;
                                if !empty {
                                    return Ok(());
                                }
                            }
                            Err(e) if absent(&e) => {
                                if file.metadata()?.len() == 1024 {
                                    return Ok(());
                                }
                            }
                            Err(_) => return Ok(()),
                        }
                    }
                    name.copy_from_slice(entry);
                    found = true;
                }
                Ok(())
            })
            .map_err(io)?;
        if found {
            break;
        }
    }
    if !found {
        return Ok(None);
    }
    let mut receipt = registry.open_readonly_file(&name).map_err(io)?;
    let receipt_stamp =
        FixedFileStamp::capture(&receipt, FixedControlKind::File).map_err(admission)?;
    let _scratch = memory
        .reserve(4096 + std::mem::size_of::<TerminalWork>())
        .map_err(|_| CandidateError::ResourceLimit)?;
    let mut bytes = [0; 4096];
    let n = receipt.read(&mut bytes).map_err(io)?;
    if n != receipt.metadata().map_err(io)?.len() as usize || n < 1024 {
        return Ok(None);
    }
    let prefix_len = if n >= 2048 && &bytes[1024..1032] == b"CGWOUT01" {
        2048
    } else {
        1024
    };
    let origin = match maintenance_origin::decode_workspace_output_origin(
        &bytes[..1024],
        receipt_stamp.identity(),
        None,
        memory,
    ) {
        Ok(f) if workspace_scope_matches(&f, scope) => f,
        _ => {
            report.unknown = true;
            return Ok(None);
        }
    };
    let receipt_nonce = super::maintenance_controls::workspace_receipt_nonce(
        origin.binding.incarnation,
        origin.binding.parent,
        origin.binding.output,
        origin.binding.operation,
    );
    if locks::created_file_nonce(&receipt).map_err(io)? != Some(receipt_nonce) {
        report.unknown = true;
        return Ok(None);
    }
    let Some(parent) = find_workspace_parent(&root, origin.binding.parent, memory, cancel)? else {
        return Ok(None);
    };
    let parent_dir = workspace_parent(&root, &parent).map_err(io)?;
    let parent_ref = parent_dir.as_ref().unwrap_or(&root);
    let mut source_name = origin.binding.name;
    let mut source_name_len = origin.binding.name_len;
    let mut receipt_only = false;
    let root_metadata_receipt = workspace_registered_metadata(&origin, &ready);
    if origin.control {
        let mut matched = false;
        for name in [
            &origin.binding.name[..origin.binding.name_len as usize],
            &origin.intended_name[..origin.intended_name_len as usize],
        ] {
            if workspace_file_matches(
                parent_ref,
                name,
                origin.binding.output,
                origin.binding.operation,
            )
            .map_err(io)?
            {
                source_name = [0; 255];
                source_name[..name.len()].copy_from_slice(name);
                source_name_len = name.len() as u8;
                matched = true;
                break;
            }
        }
        // A detached control has neither public alias. Its existing intent
        // still governs the private payload and subsequent removal frames.
        // Only a never-started, already-consumed output is receipt-only.
        receipt_only = (!matched && n == prefix_len) || root_metadata_receipt;
    }
    let output_name = &source_name[..source_name_len as usize];
    let tombstone = workspace_tombstone(origin.binding.operation);
    let mut held = if receipt_only {
        None
    } else if origin.fifo {
        #[cfg(unix)]
        {
            let pin = super::maintenance_controls::workspace_fifo_pin_name(&origin.binding);
            match registry
                .open_pinned_fifo(&pin, origin.binding.output, 2)
                .or_else(|_| registry.open_pinned_fifo(&pin, origin.binding.output, 1))
            {
                Ok(file) => Some(file),
                Err(error) if absent(&error) => None,
                Err(error) => return Err(io(error)),
            }
        }
        #[cfg(not(unix))]
        {
            return Ok(None);
        }
    } else if origin.directory {
        match parent_ref.open_directory_readonly(output_name) {
            Ok(dir) => Some(dir.pin_handle().map_err(io)?),
            Err(e) if absent(&e) => None,
            Err(e) => return Err(io(e)),
        }
    } else {
        match root.open_readonly_file(&tombstone) {
            Ok(f) => {
                // A replacement at the original name is foreign to this retired
                // incarnation. Preserve it; it cannot replace the private proof.
                Some(f)
            }
            Err(e) if absent(&e) => match parent_ref.open_readonly_file(output_name) {
                Ok(f) => Some(f),
                Err(e) if absent(&e) => None,
                Err(e) => return Err(io(e)),
            },
            Err(e) => return Err(io(e)),
        }
    };
    if !origin.directory && !origin.fifo && !origin.control {
        if let Some(file) = &held {
            if locks::file_identity(file).map_err(evidence)? != origin.binding.output
                || locks::created_file_nonce(file).map_err(io)? != Some(origin.binding.operation)
            {
                // An unrelated replacement does not become this receipt's
                // payload. Retain that file and dispose only our old receipt.
                report.unknown = true;
                held = None;
            }
        }
    }
    let (stamp, length, digest) = if origin.fifo {
        (None, 0, [0; 32])
    } else if let Some(file) = &mut held {
        let stamp = FixedFileStamp::capture(
            file,
            if origin.directory {
                FixedControlKind::Directory
            } else {
                FixedControlKind::File
            },
        )
        .map_err(admission)?;
        if stamp.identity() != origin.binding.output
            || locks::created_file_nonce(file).map_err(io)? != Some(origin.binding.operation)
        {
            return Ok(None);
        }
        let mut hash = Sha256::new();
        let mut length = 0u64;
        let mut buffer = [0; 8192];
        if !origin.directory {
            loop {
                if cancel() {
                    return Err(CandidateError::Cancelled);
                }
                let n = file.read(&mut buffer).map_err(io)?;
                if n == 0 {
                    break;
                }
                length = length
                    .checked_add(n as u64)
                    .ok_or(CandidateError::InvalidEvidence)?;
                hash.update(&buffer[..n]);
            }
        }
        stamp.recheck(file).map_err(admission)?;
        (Some(stamp), length, hash.finalize().into())
    } else {
        (None, 0, [0; 32])
    };
    // Completed origin always checks the selected actual payload. If it is
    // already gone, the intact retirement journal below supplies exact facts.
    let full_origin = if held.is_some() && !origin.directory && !origin.fifo {
        maintenance_origin::decode_workspace_output_origin(
            &bytes[..prefix_len],
            receipt_stamp.identity(),
            Some((length, digest)),
            memory,
        )
        .map_err(evidence)?
    } else {
        origin
    };
    let mut work = MemoryBox::new(
        TerminalWork {
            ready,
            scope,
            parent,
            origin: full_origin,
            receipt_name: name,
            source_name,
            source_name_len,
            receipt_only,
            root_metadata_receipt,
            receipt_stamp,
            receipt_nonce,
            receipt_length: n as u64,
            append_offset: n as u64,
            prefix_length: prefix_len,
            prefix_digest: Sha256::digest(&bytes[..prefix_len]).into(),
            payload_stamp: stamp,
            payload_digest: digest,
            payload_length: length,
            stage: 0,
            frame: [0; 512],
            held_payload: held,
        },
        memory,
    )
    .map_err(|_| CandidateError::ResourceLimit)?;
    let tail = &bytes[prefix_len..n];
    if tail.len() > 1536 {
        report.unknown = true;
        return Ok(None);
    }
    for (i, frame) in tail.chunks_exact(512).enumerate() {
        let frame: &[u8; 512] = frame.try_into().unwrap();
        if !workspace_frame_valid(frame, &work, (i + 1) as u8)
            || u64::from_le_bytes(frame[16..24].try_into().unwrap()) != work.ready.source_epoch()
            || u64::from_le_bytes(frame[24..32].try_into().unwrap()) != work.ready.admission_epoch()
        {
            report.unknown = true;
            return Ok(None);
        }
        if i == 0 {
            let journal_length = u64::from_le_bytes(frame[152..160].try_into().unwrap());
            let journal_digest: [u8; 32] = frame[160..192].try_into().unwrap();
            if work.held_payload.is_some() && (length != journal_length || digest != journal_digest)
            {
                return Ok(None);
            }
            work.payload_length = journal_length;
            work.payload_digest = journal_digest;
        }
        if work.held_payload.is_none() && frame[72..152].iter().any(|x| *x != 0) {
            let stamp = FixedFileStamp::decode_checkpoint(frame[72..152].try_into().unwrap())
                .map_err(admission)?;
            if stamp.identity() != work.origin.binding.output {
                return Ok(None);
            }
            work.payload_stamp = Some(stamp);
        }
        if i > 0
            && (frame[152..160] != work.payload_length.to_le_bytes()
                || frame[160..192] != work.payload_digest)
        {
            return Ok(None);
        }
        work.stage = (i + 1) as u8;
    }
    if work.stage == 0 && work.held_payload.is_none() && !work.receipt_only {
        if !work.origin.directory && !work.origin.fifo {
            // A producer may already have consumed its temporary file. The
            // genuine nonce-bound receipt remains our metadata; no payload is
            // adopted or removed by this receipt-only transition.
            work.receipt_only = true;
        } else {
            report.unknown = true;
            return Ok(None);
        }
    }
    if work.receipt_only {
        if !tail.is_empty() {
            report.unknown = true;
            return Ok(None);
        }
        work.stage = 4;
    }
    work.frame = workspace_retire_frame(
        &work,
        work.stage + 1,
        work.ready.source_epoch(),
        work.ready.admission_epoch(),
    );
    let partial = &tail[(tail.len() / 512) * 512..];
    if partial != &work.frame[..partial.len()] {
        report.unknown = true;
        return Ok(None);
    }
    work.append_offset = (prefix_len + (tail.len() / 512) * 512) as u64;
    report.pending = true;
    report.reasons |= 1 << 7;

    Ok(Some(work))
}
fn workspace_tombstone(operation: [u8; 32]) -> [u8; 73] {
    let mut name = [0; 73];
    name[..9].copy_from_slice(b".retired-");
    name[9..].copy_from_slice(&encode_key(&operation));
    name
}
impl TerminalWork {
    fn advance(
        &mut self,
        grant: &mut MutationGrant,
        report: &mut CandidateReport,
    ) -> Result<(), locks::FixedControlError> {
        let stage = self.stage;
        let scope = self.scope;
        let origin = self.origin;
        let source_name = self.source_name;
        let source_name_len = self.source_name_len;
        let receipt_only = self.receipt_only;
        let root_metadata_receipt = self.root_metadata_receipt;
        let name = self.receipt_name;
        let payload_length = self.payload_length;
        let receipt_stamp = self.receipt_stamp;
        let receipt_nonce = self.receipt_nonce;
        let receipt_length = self.receipt_length;
        let append_offset = self.append_offset;
        let payload_stamp = self.payload_stamp;
        let frame = self.frame;
        let parent = &self.parent;
        let effects = parent.depth + 25;
        super::maintenance_controls::with_terminal_workspace(
            &mut self.ready,
            grant,
            effects,
            &mut |lease, grant| {
                let invalid =
                    || locks::FixedControlError::Invalid(locks::FixedControlProblem::Changed);
                if lease.origin_scope() != scope {
                    return Err(invalid());
                }
                let root = lease.directory();
                let registry = root.open_directory(b".workspace-origins")?;
                if registry.identity() != scope.registry {
                    return Err(invalid());
                }
                let mut receipt = registry.open_existing(&name)?;
                receipt_stamp.recheck(&receipt)?;
                if receipt.metadata()?.len() != receipt_length
                    || locks::created_file_nonce(&receipt)? != Some(receipt_nonce)
                {
                    return Err(invalid());
                }
                let parent_dir = workspace_parent(root, parent)?;
                let parent = parent_dir.as_ref().unwrap_or(root);
                let source = &source_name[..source_name_len as usize];
                let private = workspace_tombstone(origin.binding.operation);
                let append = |receipt: &mut std::fs::File,
                              grant: &mut MutationGrant|
                 -> Result<(), locks::FixedControlError> {
                    use std::io::{Seek, SeekFrom, Write};
                    grant.check_time()?;
                    receipt.seek(SeekFrom::Start(append_offset))?;
                    receipt.write_all(&frame)?;
                    receipt.sync_all()?;
                    let mut actual = [0; 512];
                    locks::read_fixed_range(receipt, append_offset, &mut actual)?;
                    if actual != frame {
                        return Err(invalid());
                    }
                    Ok(())
                };
                if receipt_only {
                    // Terminal admission rechecks the root metadata seal. Its
                    // creator receipt can be disposed while the exact sealed
                    // root control stays for the fixed-name final removal.
                    if !root_metadata_receipt {
                        let present = if origin.control {
                            workspace_control_present(parent, &origin)?
                        } else {
                            workspace_file_matches(
                                parent,
                                source,
                                origin.binding.output,
                                origin.binding.operation,
                            )? || workspace_file_matches(
                                root,
                                &private,
                                origin.binding.output,
                                origin.binding.operation,
                            )?
                        };
                        if present {
                            return Err(invalid());
                        }
                    }
                    grant.check_time()?;
                    registry.unlink_file(&name, origin.receipt)?;
                    registry.sync_namespace()?;
                    report.mutated_entries += 1;
                    report.reclaimed_entries += 1;
                    report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(receipt_length);
                    return Ok(());
                }
                if origin.fifo {
                    #[cfg(unix)]
                    {
                        let pin =
                            super::maintenance_controls::workspace_fifo_pin_name(&origin.binding);
                        match stage {
                            0 => {
                                append(&mut receipt, grant)?;
                            }
                            1 => {
                                // A public replacement is retained. Only the actual
                                // creator identity can consume the public FIFO link.
                                match parent.open_pinned_fifo(source, origin.binding.output, 2) {
                                    Ok(_held) => {
                                        let _pin = registry.open_pinned_fifo(
                                            &pin,
                                            origin.binding.output,
                                            2,
                                        )?;
                                        grant.check_time()?;
                                        parent.unlink_pinned_fifo(
                                            source,
                                            origin.binding.output,
                                            2,
                                        )?;
                                        parent.sync_namespace()?;
                                        report.mutated_entries += 1;
                                        report.reclaimed_entries += 1;
                                    }
                                    Err(_) => {
                                        let _pin = registry.open_pinned_fifo(
                                            &pin,
                                            origin.binding.output,
                                            1,
                                        )?;
                                    }
                                }
                                append(&mut receipt, grant)?;
                            }
                            2 => {
                                match registry.open_pinned_fifo(&pin, origin.binding.output, 1) {
                                    Ok(_held) => {
                                        grant.check_time()?;
                                        registry.unlink_pinned_fifo(
                                            &pin,
                                            origin.binding.output,
                                            1,
                                        )?;
                                        registry.sync_namespace()?;
                                        report.mutated_entries += 1;
                                        report.reclaimed_entries += 1;
                                    }
                                    Err(error) if absent(&error) => {}
                                    Err(error) => return Err(error.into()),
                                }
                                append(&mut receipt, grant)?;
                            }
                            3 => {
                                match registry.open_pinned_fifo(&pin, origin.binding.output, 1) {
                                    Err(error) if absent(&error) => {}
                                    _ => return Err(invalid()),
                                }
                                grant.check_time()?;
                                registry.unlink_file(&name, origin.receipt)?;
                                registry.sync_namespace()?;
                                report.mutated_entries += 1;
                                report.reclaimed_entries += 1;
                                report.reclaimed_bytes =
                                    report.reclaimed_bytes.saturating_add(receipt_length);
                            }
                            _ => return Err(invalid()),
                        }
                        return Ok(());
                    }
                    #[cfg(not(unix))]
                    {
                        return Err(invalid());
                    }
                }
                if origin.directory {
                    match stage {
                        0 => append(&mut receipt, grant)?,
                        1 => {
                            match parent.open_directory(source) {
                                Ok(directory) => {
                                    if directory.identity() != origin.binding.output {
                                        return Err(invalid());
                                    }
                                    let file = directory.pin_handle()?;
                                    payload_stamp.ok_or_else(invalid)?.recheck(&file)?;
                                    if locks::created_file_nonce(&file)?
                                        != Some(origin.binding.operation)
                                    {
                                        return Err(invalid());
                                    }
                                    grant.check_time()?;
                                    // Atomic emptiness is checked by the OS;
                                    // unknown concurrent children stay public.
                                    parent.remove_empty_directory(source, origin.binding.output)?;
                                    parent.sync_namespace()?;
                                    report.mutated_entries += 1;
                                    report.reclaimed_entries += 1;
                                }
                                Err(e) if absent(&e) => {}
                                Err(e) => return Err(e.into()),
                            }
                            append(&mut receipt, grant)?;
                        }
                        2 => {
                            match parent.open_directory_readonly(source) {
                                Err(e) if absent(&e) => {}
                                _ => return Err(invalid()),
                            }
                            append(&mut receipt, grant)?;
                        }
                        3 => {
                            match parent.open_directory_readonly(source) {
                                Err(e) if absent(&e) => {}
                                _ => return Err(invalid()),
                            }
                            grant.check_time()?;
                            registry.unlink_file(&name, origin.receipt)?;
                            registry.sync_namespace()?;
                            report.mutated_entries += 1;
                            report.reclaimed_entries += 1;
                            report.reclaimed_bytes =
                                report.reclaimed_bytes.saturating_add(receipt_length);
                        }
                        _ => return Err(invalid()),
                    }
                    return Ok(());
                }
                match stage {
                    0 => append(&mut receipt, grant)?,
                    1 => {
                        match root.open_existing(&private) {
                            Ok(file) => {
                                if Some(FixedFileStamp::capture(&file, FixedControlKind::File)?)
                                    != payload_stamp
                                    || locks::created_file_nonce(&file)?
                                        != Some(origin.binding.operation)
                                {
                                    return Err(invalid());
                                }
                            }
                            Err(e) if absent(&e) => {
                                let file = parent.open_existing(source)?;
                                payload_stamp.ok_or_else(invalid)?.recheck(&file)?;
                                if locks::created_file_nonce(&file)?
                                    != Some(origin.binding.operation)
                                {
                                    return Err(invalid());
                                }
                                grant.check_time()?;
                                parent.rename_file_no_replace(
                                    source,
                                    root,
                                    &private,
                                    origin.binding.output,
                                )?;
                                parent.sync_namespace()?;
                                root.sync_namespace()?;
                                let moved = root.open_readonly_file(&private)?;
                                let actual =
                                    FixedFileStamp::capture(&moved, FixedControlKind::File)?;
                                let mut before = [0; 80];
                                let mut after = [0; 80];
                                payload_stamp
                                    .ok_or_else(invalid)?
                                    .encode_checkpoint(&mut before);
                                actual.encode_checkpoint(&mut after);
                                if before[..64] != after[..64]
                                    || locks::created_file_nonce(&moved)?
                                        != Some(origin.binding.operation)
                                {
                                    return Err(invalid());
                                }
                                report.mutated_entries += 1;
                                // Preparation must observe the actual post-own
                                // stamp outside authority before sealing Detached.
                                return Ok(());
                            }
                            Err(e) => return Err(e.into()),
                        }
                        append(&mut receipt, grant)?;
                    }
                    2 => {
                        match root.open_existing(&private) {
                            Ok(file) => {
                                payload_stamp.ok_or_else(invalid)?.recheck(&file)?;
                                if locks::created_file_nonce(&file)?
                                    != Some(origin.binding.operation)
                                {
                                    return Err(invalid());
                                }
                                grant.check_time()?;
                                root.unlink_file(&private, origin.binding.output)?;
                                root.sync_namespace()?;
                                report.mutated_entries += 1;
                                report.reclaimed_entries += 1;
                                report.reclaimed_bytes =
                                    report.reclaimed_bytes.saturating_add(payload_length);
                            }
                            Err(e) if absent(&e) => {}
                            Err(e) => return Err(e.into()),
                        }
                        append(&mut receipt, grant)?;
                    }
                    3 => {
                        if workspace_file_matches(
                            root,
                            &private,
                            origin.binding.output,
                            origin.binding.operation,
                        )? || workspace_file_matches(
                            parent,
                            source,
                            origin.binding.output,
                            origin.binding.operation,
                        )? {
                            return Err(invalid());
                        }
                        grant.check_time()?;
                        registry.unlink_file(&name, origin.receipt)?;
                        registry.sync_namespace()?;
                        report.mutated_entries += 1;
                        report.reclaimed_entries += 1;
                        report.reclaimed_bytes =
                            report.reclaimed_bytes.saturating_add(receipt_length);
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
        )
        .map(|_| ())
    }
}

/// Membership in a durably completed rooted R4 cohort. Fields stay private to
/// the actual stream publisher/selector; decoding an association or certificate
/// independently cannot construct this token.
pub(crate) use super::maintenance_legacy_engine::LegacyStagingCohortLeaf;

fn workspace_file_matches(
    parent: &RootedDirectory,
    name: &[u8],
    id: locks::FileIdentity,
    nonce: [u8; 32],
) -> std::io::Result<bool> {
    match parent.open_readonly_file(name) {
        Ok(file) => Ok(locks::file_identity(&file).map_err(|error| match error {
            crate::error::NativeError::Io(error) => error,
            _ => std::io::Error::from(std::io::ErrorKind::InvalidData),
        })? == id
            && locks::created_file_nonce(&file)? == Some(nonce)),
        Err(e) if absent(&e) => Ok(false),
        Err(e) => Err(e),
    }
}
fn workspace_control_present(
    parent: &RootedDirectory,
    origin: &maintenance_origin::WorkspaceOriginFacts,
) -> std::io::Result<bool> {
    Ok(workspace_file_matches(
        parent,
        &origin.binding.name[..origin.binding.name_len as usize],
        origin.binding.output,
        origin.binding.operation,
    )? || workspace_file_matches(
        parent,
        &origin.intended_name[..origin.intended_name_len as usize],
        origin.binding.output,
        origin.binding.operation,
    )?)
}

fn workspace_registered_metadata(
    origin: &maintenance_origin::WorkspaceOriginFacts,
    ready: &super::maintenance_controls::PreparedTerminalWorkspace,
) -> bool {
    origin.control
        && origin.binding.parent == origin.binding.root
        && ready.metadata_proofs().iter().flatten().any(|proof| {
            proof.identity == origin.binding.output
                && proof.operation == origin.binding.operation
                && proof.receipt == Some(origin.receipt)
        })
}
