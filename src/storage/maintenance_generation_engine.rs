//! Durable orchestration for genuinely published, superseded generations.
//! The private control selects work; the generation role reacquires every
//! actual writer, reader and native-family exclusion before changing a name.
use super::inventory::MaintenanceTarget;
use super::locks::{self, FixedControlError, FixedControlProblem, RootedDirectory};
use super::maintenance::{CursorAddress, MaintenanceCursor, WorkCategory, CURSOR_BYTES};
use super::maintenance_admission::{
    self as admission, PreparedControlUpdate, PreparedPrivateMetadata, PreparedTargetBinding,
    RootOriginBinding,
};
use super::maintenance_budget::{CleanupMemory, MemoryBox, MutationGrant};
use super::maintenance_candidate::{
    self as candidate, CandidateReport, PreparedGenerationCompletion,
};
use super::maintenance_generation_gate::{self as gate, Origin};
use super::maintenance_generations::{
    self as generations, GenerationError, PreparedGeneration, PreparedGenerationRootRemoval,
    GENERATION_PROOF_BYTES,
};
use super::maintenance_ownership::{ReferenceDomain, ReferenceEpoch};

const MAGIC: &[u8; 8] = b"CGGENE01";
const CONTROL_BYTES: usize = 224 + CURSOR_BYTES;
const PROOF_INTENT: u8 = 0;
const LEAF_INTENT: u8 = 1;
const DETACHED: u8 = 2;
const RESET_INTENT: u8 = 3;
const PAYLOAD_NAMES: [&[u8]; 12] = [
    b"graph.ldb",
    b"manifest.json",
    b"metadata.json",
    b"READY",
    b"graph.ldb.wal",
    b"graph.ldb.tmp",
    b"graph.ldb.search.meta.json",
    b"graph.ldb.search.lexicon.bin",
    b"graph.ldb.search.postings.bin",
    b"graph.ldb.search.doc_lengths.bin",
    b"graph.ldb.search.documents.bin",
    b"graph.ldb.search.document_offsets.bin",
];

fn changed() -> FixedControlError {
    FixedControlError::Invalid(FixedControlProblem::Changed)
}
fn generation_error(error: GenerationError) -> FixedControlError {
    match error {
        GenerationError::Io(kind) => std::io::Error::from(kind).into(),
        GenerationError::ResourceLimit => {
            FixedControlError::Resource(super::maintenance_budget::ResourceLimit::CeilingExceeded)
        }
        GenerationError::Busy => FixedControlError::Invalid(FixedControlProblem::Busy),
        _ => changed(),
    }
}
fn candidate_error(error: candidate::CandidateError) -> FixedControlError {
    match error {
        candidate::CandidateError::Io(kind) => std::io::Error::from(kind).into(),
        candidate::CandidateError::ResourceLimit => {
            FixedControlError::Resource(super::maintenance_budget::ResourceLimit::CeilingExceeded)
        }
        _ => changed(),
    }
}

/// A checksum is an integrity fact only. Authority comes from fresh role
/// admission plus the original complete producer-bound cohort in the stream.
struct Control {
    stage: u8,
    selected: u8,
    source_epoch: u64,
    operation: [u8; 32],
    proof_digest: [u8; 32],
    generation: [u8; 128],
    generation_len: u8,
    cursor: MaintenanceCursor,
}
impl Control {
    fn encode(&self) -> [u8; CONTROL_BYTES] {
        let mut b = [0; CONTROL_BYTES];
        b[..8].copy_from_slice(MAGIC);
        b[8] = self.stage;
        b[9] = self.selected;
        b[10] = self.generation_len;
        b[16..24].copy_from_slice(&self.source_epoch.to_le_bytes());
        b[24..56].copy_from_slice(&self.operation);
        b[56..88].copy_from_slice(&self.proof_digest);
        b[88..216].copy_from_slice(&self.generation);
        b[224..].copy_from_slice(&self.cursor.encode());
        b
    }
    fn decode(b: &[u8], scope: RootOriginBinding) -> Result<Self, FixedControlError> {
        if b.len() != CONTROL_BYTES
            || &b[..8] != MAGIC
            || b[8] > RESET_INTENT
            || b[9] > 12
            || b[11..16].iter().chain(b[216..224].iter()).any(|v| *v != 0)
            || b[24..56].iter().all(|v| *v == 0)
            || b[56..88].iter().all(|v| *v == 0)
        {
            return Err(changed());
        }
        let generation: [u8; 128] = b[88..216].try_into().unwrap();
        let len = usize::from(b[10]);
        if len == 0
            || len > 128
            || !gate::component(&generation[..len])
            || generation[len..].iter().any(|v| *v != 0)
        {
            return Err(changed());
        }
        Ok(Self {
            stage: b[8],
            selected: b[9],
            generation_len: b[10],
            source_epoch: u64::from_le_bytes(b[16..24].try_into().unwrap()),
            operation: b[24..56].try_into().unwrap(),
            proof_digest: b[56..88].try_into().unwrap(),
            generation,
            cursor: MaintenanceCursor::decode(b[224..].try_into().unwrap(), scope.incarnation)
                .map_err(|_| changed())?,
        })
    }
    fn tombstone(&self) -> [u8; 73] {
        let mut b = [0; 73];
        b[..9].copy_from_slice(b".retired-");
        let mut operation = self.operation;
        operation[31] ^= self.selected;
        for (i, value) in operation.iter().enumerate() {
            b[9 + i * 2] = b"0123456789abcdef"[usize::from(value >> 4)];
            b[10 + i * 2] = b"0123456789abcdef"[usize::from(value & 15)];
        }
        b
    }
}

pub(crate) fn is_generation_payload(payload: &[u8]) -> bool {
    payload.starts_with(MAGIC) || payload.starts_with(b"CGGENDON")
}
#[derive(Default)]
pub(crate) struct GenerationPreparationState {
    observed: Option<CandidateReport>,
}
impl GenerationPreparationState {
    pub(crate) fn clear(&mut self) {
        self.observed = None;
    }
    pub(crate) fn take_observation(&mut self) -> Option<CandidateReport> {
        self.observed.take()
    }
    pub(crate) fn has_parked_payloads(&self) -> bool {
        false
    }
}

enum Action {
    Metadata {
        metadata: MemoryBox<PreparedPrivateMetadata>,
        update: MemoryBox<PreparedControlUpdate>,
        append: Option<MemoryBox<[u8; GENERATION_PROOF_BYTES]>>,
        reset: bool,
    },
    Leaf {
        prepared: MemoryBox<PreparedGeneration>,
        metadata: MemoryBox<PreparedPrivateMetadata>,
        update: MemoryBox<PreparedControlUpdate>,
        tombstone: [u8; 73],
        detached: bool,
    },
    Root {
        prepared: MemoryBox<PreparedGenerationRootRemoval>,
        metadata: MemoryBox<PreparedPrivateMetadata>,
        completion: PreparedGenerationCompletion,
    },
}
pub(crate) struct GenerationWork {
    action: Action,
    report: CandidateReport,
}

fn boxed<T>(value: T, memory: &CleanupMemory) -> Result<MemoryBox<T>, FixedControlError> {
    MemoryBox::new(value, memory).map_err(FixedControlError::Resource)
}
fn update(
    metadata: &PreparedPrivateMetadata,
    bytes: &[u8],
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedControlUpdate>, FixedControlError> {
    boxed(
        admission::prepare_metadata_control(
            &metadata.origin_scope(),
            metadata.committed().ok_or_else(changed)?,
            bytes,
            memory,
        )?,
        memory,
    )
}
fn metadata_work(
    metadata: MemoryBox<PreparedPrivateMetadata>,
    bytes: &[u8],
    append: Option<MemoryBox<[u8; GENERATION_PROOF_BYTES]>>,
    reset: bool,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    let update = update(&metadata, bytes, memory)?;
    boxed(
        GenerationWork {
            action: Action::Metadata {
                metadata,
                update,
                append,
                reset,
            },
            report: pending(),
        },
        memory,
    )
    .map(Some)
}
fn pending() -> CandidateReport {
    CandidateReport {
        pending: true,
        reasons: 1 << 7,
        ..CandidateReport::default()
    }
}

fn read_proof(
    metadata: &PreparedPrivateMetadata,
    digest: [u8; 32],
    memory: &CleanupMemory,
) -> Result<MemoryBox<[u8; GENERATION_PROOF_BYTES]>, FixedControlError> {
    if metadata.stream_len() != GENERATION_PROOF_BYTES as u64 {
        return Err(changed());
    }
    let mut proof = boxed([0; GENERATION_PROOF_BYTES], memory)?;
    metadata.read_stream_exact(0, &mut *proof)?;
    if gate::digest(&*proof) != digest
        || &proof[..8] != b"CGGPR001"
        || gate::digest(&proof[..4064]) != proof[4064..]
    {
        return Err(changed());
    }
    Ok(proof)
}
fn proof_origin(proof: &[u8; GENERATION_PROOF_BYTES]) -> Result<Origin, FixedControlError> {
    Origin::decode(proof[32..2080].try_into().unwrap()).ok_or_else(changed)
}

#[inline(never)]
fn prepare_scope(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
) -> Result<Option<RootOriginBinding>, FixedControlError> {
    let canonical = match admission::prepare_canonical(target, domain, expected, memory) {
        Ok(canonical) => canonical,
        Err(FixedControlError::Resource(error)) => return Err(FixedControlError::Resource(error)),
        Err(_) => return Ok(None),
    };
    let scope = canonical.origin_scope();
    drop(canonical);
    Ok(Some(scope))
}
pub(crate) fn prepare_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut GenerationPreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    state.observed = None;
    if cancel() {
        return Err(changed());
    }
    if binding.is_legacy() || target.direct_pair.is_some() {
        return Ok(None);
    }
    let Some(scope) = prepare_scope(target, domain, expected, memory)? else {
        return Ok(None);
    };
    let metadata = match admission::prepare_private_metadata(target, &scope, memory) {
        Ok(metadata) => boxed(metadata, memory)?,
        Err(FixedControlError::Resource(error)) => return Err(FixedControlError::Resource(error)),
        Err(_) => return Ok(None),
    };
    if let Some(recovery) = admission::prepare_metadata_recovery(&metadata, memory)? {
        // Recovery belongs to whichever role actually committed the intent.
        if !metadata.payload().is_some_and(is_generation_payload) {
            return Ok(None);
        }
        return boxed(
            GenerationWork {
                action: Action::Metadata {
                    metadata,
                    update: boxed(recovery, memory)?,
                    append: None,
                    reset: false,
                },
                report: pending(),
            },
            memory,
        )
        .map(Some);
    }
    let payload = metadata.payload().ok_or_else(changed)?;
    if payload.starts_with(b"CGGENDON") {
        return prepare_completed(target, domain, metadata, memory, cancel);
    }
    if !payload.starts_with(MAGIC) {
        return prepare_selection(
            target, domain, expected, state, metadata, scope, memory, cancel,
        );
    }
    let mut control = Control::decode(payload, scope)?;
    if control.stage == RESET_INTENT {
        return prepare_reset(metadata, control, memory);
    }
    if control.stage == PROOF_INTENT {
        return prepare_proof_intent(target, domain, expected, metadata, control, memory, cancel);
    }
    let proof = read_proof(&metadata, control.proof_digest, memory)?;
    let origin = proof_origin(&proof)?;
    if origin.generation != control.generation
        || origin.generation_len != control.generation_len
        || u64::from_le_bytes(proof[8..16].try_into().unwrap()) != control.source_epoch
    {
        return Err(changed());
    }
    while control.selected < 12 && origin.leaves[usize::from(control.selected)].is_none() {
        control.selected += 1;
    }
    if control.selected == 12 {
        return prepare_root(
            target, domain, metadata, scope, control, proof, origin, memory, cancel,
        );
    }
    prepare_leaf(
        target, domain, expected, metadata, control, proof, origin, memory, cancel,
    )
}
#[inline(never)]
// Keep immutable epoch, prepared metadata and source ownership explicit across this bounded off-permit stage.
#[allow(clippy::too_many_arguments)]
fn prepare_selection(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut GenerationPreparationState,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    scope: RootOriginBinding,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    let payload = metadata.payload().ok_or_else(changed)?;

    let Some(mut cursor) =
        candidate::idle_control_cursor(payload, scope).map_err(candidate_error)?
    else {
        return Ok(None);
    };
    if metadata.stream_len() != 0 {
        return Err(changed());
    }
    let mut selection = cursor.selection();
    let observed = generations::discover_generations(target, memory, cancel, &mut |id| {
        let mut address = [0; 128];
        address[..id.len()].copy_from_slice(id);
        selection.observe(WorkCategory::Generation, CursorAddress(address));
        Ok(())
    })
    .map_err(generation_error)?;
    state.observed = Some(CandidateReport {
        checked_categories: 1 << (WorkCategory::Generation as u8),
        inspected_entries: observed.inspected,
        unknown: observed.unknown != 0,
        reasons: u64::from(observed.unknown != 0),
        ..CandidateReport::default()
    });
    let Some((_, address)) = selection.selected() else {
        return Ok(None);
    };
    let len = address.0.iter().position(|v| *v == 0).unwrap_or(128);
    let prepared = generations::prepare_exact_generation(
        target,
        domain,
        expected,
        &address.0[..len],
        0,
        memory,
        cancel,
    )
    .map_err(generation_error)?;
    let proof = prepared.inventory().encode_proof();
    cursor.inspected(WorkCategory::Generation, address);
    let mut operation = [0; 32];
    getrandom::fill(&mut operation).map_err(|_| changed())?;
    if operation == [0; 32] {
        return Err(changed());
    }
    let control = Control {
        stage: PROOF_INTENT,
        selected: 0,
        source_epoch: expected.counter(),
        operation,
        proof_digest: gate::digest(&proof),
        generation: address.0,
        generation_len: len as u8,
        cursor,
    };
    let mut work = metadata_work(metadata, &control.encode(), None, false, memory)?.unwrap();
    work.report.inspected_entries = observed.inspected;
    work.report.unknown = observed.unknown != 0;
    Ok(Some(work))
}
#[inline(never)]
fn prepare_reset(
    metadata: MemoryBox<PreparedPrivateMetadata>,
    control: Control,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    // The reset intent is published only after real durable cohort Done.
    // Preserve any nonmatching/foreign tail rather than truncate it.
    if metadata.stream_len() == 0 {
        return metadata_work(metadata, &control.cursor.encode(), None, false, memory);
    }
    let _proof = read_proof(&metadata, control.proof_digest, memory)?;
    metadata_work(metadata, &control.cursor.encode(), None, true, memory)
}
#[inline(never)]
fn prepare_proof_intent(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: Control,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    let prepared = generations::prepare_exact_generation(
        target,
        domain,
        expected,
        &control.generation[..usize::from(control.generation_len)],
        0,
        memory,
        cancel,
    )
    .map_err(generation_error)?;
    let mut proof = boxed(prepared.inventory().encode_proof(), memory)?;
    // Current scoped admission may have advanced while the immutable old
    // bundle stayed closed. Reconstruct the exact *original* committed
    // SourceE image; equality to its durable digest is required below.
    proof[8..16].copy_from_slice(&control.source_epoch.to_le_bytes());
    let checksum = gate::digest(&proof[..4064]);
    proof[4064..].copy_from_slice(&checksum);
    if gate::digest(&*proof) != control.proof_digest
        || metadata.stream_len() > GENERATION_PROOF_BYTES as u64
    {
        return Err(changed());
    }
    control.stage = LEAF_INTENT;
    metadata_work(metadata, &control.encode(), Some(proof), false, memory)
}
#[inline(never)]
// Keep immutable epoch, prepared metadata and source ownership explicit across this bounded off-permit stage.
#[allow(clippy::too_many_arguments)]
fn prepare_root(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    scope: RootOriginBinding,
    control: Control,
    proof: MemoryBox<[u8; GENERATION_PROOF_BYTES]>,
    origin: Origin,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    let prepared =
        generations::prepare_generation_root_removal(target, domain, &proof, memory, cancel)
            .map_err(generation_error)?;
    let completion = candidate::prepare_generation_completion(
        &scope,
        metadata.committed().ok_or_else(changed)?,
        origin.managed,
        origin.directory,
        control.proof_digest,
        &control.cursor,
        memory,
    )
    .map_err(candidate_error)?;
    boxed(
        GenerationWork {
            action: Action::Root {
                prepared,
                metadata,
                completion,
            },
            report: pending(),
        },
        memory,
    )
    .map(Some)
}
#[inline(never)]
// Keep immutable epoch, prepared metadata and source ownership explicit across this bounded off-permit stage.
#[allow(clippy::too_many_arguments)]
fn prepare_leaf(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: Control,
    proof: MemoryBox<[u8; GENERATION_PROOF_BYTES]>,
    origin: Origin,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    let tombstone = control.tombstone();
    let root =
        super::maintenance_projection::project_binding(target, memory).map_err(|_| changed())?;
    let managed = RootedDirectory::open_readonly_root(&root.storage, origin.managed)?;
    let detached = match managed.open_readonly_file(&tombstone[..]) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    if control.stage == DETACHED && !detached {
        // A durable observation of our exact detached incarnation precedes
        // unlink. A crash after unlink can therefore advance only this item.
        let source = gate::open_generation(&managed, &origin.generation, origin.generation_len)
            .map_err(|_| changed())?;
        if source.identity() != origin.directory {
            return Err(changed());
        }
        match source.open_readonly_file(PAYLOAD_NAMES[usize::from(control.selected)]) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(changed()),
        }
        control.stage = LEAF_INTENT;
        control.selected += 1;
        return metadata_work(metadata, &control.encode(), None, false, memory);
    }
    let prepared = if detached {
        generations::prepare_generation_detached(
            target,
            domain,
            expected,
            &proof,
            usize::from(control.selected),
            tombstone,
            memory,
            cancel,
        )
    } else {
        generations::prepare_generation_resume(
            target,
            domain,
            expected,
            &proof,
            usize::from(control.selected),
            memory,
            cancel,
        )
    }
    .map_err(generation_error)?;
    if detached {
        if control.stage == LEAF_INTENT {
            // Recover the rename-before-control cut. Validate the one actual
            // tombstone before recording its disposition; do not unlink yet.
            control.stage = DETACHED;
            return metadata_work(metadata, &control.encode(), None, false, memory);
        }
        control.selected += 1;
        control.stage = LEAF_INTENT;
    } else {
        control.stage = DETACHED;
    }
    let next = update(&metadata, &control.encode(), memory)?;
    boxed(
        GenerationWork {
            action: Action::Leaf {
                prepared,
                metadata,
                update: next,
                tombstone,
                detached,
            },
            report: pending(),
        },
        memory,
    )
    .map(Some)
}

fn prepare_completed(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<GenerationWork>>, FixedControlError> {
    let scope = metadata.origin_scope();
    let bytes = metadata.payload().ok_or_else(changed)?;
    if bytes.len() != 72 + CURSOR_BYTES {
        return Err(changed());
    }
    let id = |offset| {
        locks::FileIdentity::from_fixed_words(
            u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()),
            u64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap()),
        )
    };
    let managed = id(8);
    let removed = id(24);
    let proof_digest = bytes[40..72].try_into().unwrap();
    let cursor = MaintenanceCursor::decode(bytes[72..].try_into().unwrap(), scope.incarnation)
        .map_err(|_| changed())?;
    let proof = read_proof(&metadata, proof_digest, memory)?;
    let origin = proof_origin(&proof)?;
    if origin.managed != managed
        || origin.directory != removed
        || RootOriginBinding::from(origin.scope) != scope
    {
        return Err(changed());
    }
    let prepared =
        generations::prepare_generation_root_removal(target, domain, &proof, memory, cancel)
            .map_err(generation_error)?;
    if prepared.observed_completed().map_err(generation_error)? {
        // A genuine durable completion and a completed outside-root gate are
        // both required before the own stream can be reset.
        let mut operation = [0; 32];
        getrandom::fill(&mut operation).map_err(|_| changed())?;
        if operation == [0; 32] {
            return Err(changed());
        }
        let control = Control {
            stage: RESET_INTENT,
            selected: 12,
            source_epoch: u64::from_le_bytes(proof[8..16].try_into().unwrap()),
            operation,
            proof_digest,
            generation: origin.generation,
            generation_len: origin.generation_len,
            cursor,
        };
        metadata_work(metadata, &control.encode(), None, false, memory)
    } else {
        // Callback publication can precede the gate's final Idle transport.
        // Resume it against the same complete cohort after interruption.
        let completion = candidate::prepare_generation_completion(
            &scope,
            metadata.committed().ok_or_else(changed)?,
            managed,
            removed,
            proof_digest,
            &cursor,
            memory,
        )
        .map_err(candidate_error)?;
        boxed(
            GenerationWork {
                action: Action::Root {
                    prepared,
                    metadata,
                    completion,
                },
                report: pending(),
            },
            memory,
        )
        .map(Some)
    }
}

impl GenerationWork {
    pub(crate) fn requires_legacy_confirmation(&self) -> bool {
        false
    }
    pub(crate) fn advance(
        &mut self,
        grant: &mut MutationGrant,
        _confirmation: Option<&super::maintenance_scheduler::LegacyQuiescenceConfirmation<'_>>,
    ) -> Result<CandidateReport, FixedControlError> {
        let mut report = self.report;
        match &mut self.action {
            Action::Metadata {
                metadata,
                update,
                append,
                reset,
            } => {
                let effects = update.effects()
                    + if append.is_some() { 3 } else { 0 }
                    + if *reset { 2 } else { 0 };
                admission::with_private_metadata(
                    metadata,
                    grant,
                    effects,
                    &mut |mutation, grant| {
                        if let Some(proof) = append {
                            mutation.append_frame(0, &**proof, grant)?;
                        }
                        if *reset {
                            mutation.reset_stream(GENERATION_PROOF_BYTES as u64, grant)?;
                        }
                        mutation.publish_control(update, grant)
                    },
                )?;
                report.mutated_entries += 1;
            }
            Action::Leaf {
                prepared,
                metadata,
                update,
                tombstone,
                detached,
            } => {
                let stamp = prepared.leaf().stamp();
                let effects = 32;
                #[cfg(test)]
                let fault_root = metadata.origin_scope().root;
                prepared.with_admitted(grant, effects, &mut |mutation, grant| {
                    let leaf = mutation.leaf();
                    let root = mutation.managed_root();
                    grant.check_time()?;
                    if *detached {
                        stamp.recheck(&root.open_readonly_file(&tombstone[..])?)?;
                        root.unlink_file(&tombstone[..], leaf.payload)?;
                        root.sync_namespace()?;
                        #[cfg(test)]
                        fault::reach(fault_root, fault::Cut::AfterUnlink);
                    } else {
                        stamp
                            .recheck(&mutation.source_parent().open_readonly_file(leaf.name())?)?;
                        mutation.source_parent().rename_file_no_replace(
                            leaf.name(),
                            root,
                            &tombstone[..],
                            leaf.payload,
                        )?;
                        mutation.source_parent().sync_namespace()?;
                        root.sync_namespace()?;
                        #[cfg(test)]
                        fault::reach(fault_root, fault::Cut::AfterDetach);
                    }
                    admission::with_reserved_private_metadata(
                        metadata,
                        grant,
                        update.effects(),
                        &mut |metadata, grant| metadata.publish_control(update, grant),
                    )
                })?;
                report.mutated_entries += 1;
                if *detached {
                    report.reclaimed_entries += 1;
                    report.reclaimed_bytes += prepared.leaf().byte_len;
                }
            }
            Action::Root {
                prepared,
                metadata,
                completion,
            } => {
                #[cfg(test)]
                let fault_root = metadata.origin_scope().root;
                prepared.advance(grant, completion.effects(), &mut |_outcome, grant| {
                    #[cfg(test)]
                    fault::reach(fault_root, fault::Cut::AfterRootRemoval);
                    let mut done = None;
                    admission::with_reserved_private_metadata(
                        metadata,
                        grant,
                        completion.effects(),
                        &mut |mutation, grant| {
                            done = Some(completion.publish(mutation, grant)?);
                            Ok(())
                        },
                    )?;
                    #[cfg(test)]
                    fault::reach(fault_root, fault::Cut::AfterDurableDone);
                    done.ok_or_else(changed)
                })?;
                report.mutated_entries += 1;
                report.reclaimed_entries += 1;
            }
        }
        Ok(report)
    }
}
pub(crate) fn finish_work(_work: &mut GenerationWork, _state: &mut GenerationPreparationState) {}

#[cfg(test)]
mod fault {
    use super::locks::FileIdentity;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    // Fault names identify exact post-effect interruption boundaries.
    #[allow(clippy::enum_variant_names)]
    pub(super) enum Cut {
        AfterDetach,
        AfterUnlink,
        AfterRootRemoval,
        AfterDurableDone,
    }
    thread_local! { static CUT: std::cell::Cell<Option<(FileIdentity, Cut)>> = const { std::cell::Cell::new(None) }; }
    pub(super) struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            CUT.with(|slot| slot.set(None));
        }
    }
    pub(super) fn arm(root: FileIdentity, cut: Cut) -> Guard {
        CUT.with(|slot| slot.set(Some((root, cut))));
        Guard
    }
    pub(super) fn reach(root: FileIdentity, cut: Cut) {
        CUT.with(|slot| {
            if slot.get() == Some((root, cut)) {
                slot.set(None);
                panic!("actual generation durable cut: {cut:?}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observed_epoch(domain: &ReferenceDomain) -> ReferenceEpoch {
        match super::super::maintenance_ownership::observe_reference_epoch(domain).unwrap() {
            super::super::maintenance_ownership::EpochObservation::Verified(epoch) => epoch,
            other => panic!("actual epoch unavailable: {other:?}"),
        }
    }
    #[test]
    fn generation_role_control_rejects_foreign_or_noncanonical_fields() {
        let scope = RootOriginBinding {
            root: locks::FileIdentity::from_fixed_words(1, 2),
            coordination: locks::FileIdentity::from_fixed_words(3, 4),
            cohort: locks::FileIdentity::from_fixed_words(5, 6),
            publication_binding: locks::FileIdentity::from_fixed_words(7, 8),
            incarnation: [9; 32],
        };
        let mut generation = [0; 128];
        generation[..3].copy_from_slice(b"old");
        let control = Control {
            stage: LEAF_INTENT,
            selected: 0,
            source_epoch: 3,
            operation: [1; 32],
            proof_digest: [2; 32],
            generation,
            generation_len: 3,
            cursor: MaintenanceCursor::new(scope.incarnation),
        };
        let wire = control.encode();
        assert!(Control::decode(&wire, scope).is_ok());
        let mut corrupt = wire;
        corrupt[9] = 13;
        assert!(Control::decode(&corrupt, scope).is_err());
        let mut corrupt = wire;
        corrupt[91] = 1;
        assert!(Control::decode(&corrupt, scope).is_err());
        let first = control.tombstone();
        assert_eq!(&first[..9], b".retired-");
        assert!(first[9..].iter().all(|b| b.is_ascii_hexdigit()));
        let mut next = Control::decode(&wire, scope).unwrap();
        next.selected = 1;
        assert_ne!(first, next.tombstone());
    }

    #[test]
    fn dc_generation_engine_actual_cold_progress_preserves_both_reader_generations() {
        use crate::api::{execute_materialization, MaterializeOptions};
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let base = std::env::temp_dir().join(format!(
            "dc-generation-engine-{:032x}",
            u128::from_le_bytes(nonce)
        ));
        std::fs::create_dir_all(base.join("src")).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        let storage = base.join(".codebaseGraph/storage");
        std::fs::write(base.join("src/lib.rs"), "pub fn original_generation() {}\n").unwrap();
        let options = MaterializeOptions {
            source_root: Some(base.clone()),
            storage_root: Some(storage.clone()),
            mode: "full".into(),
            parallel: false,
            use_git: false,
            ..MaterializeOptions::default()
        };
        let (_, first) = execute_materialization(&options).unwrap();
        let first = first.active_generation.unwrap();
        let layout = super::super::layout::ManagedLayout::new(&storage);
        let store = super::super::managed::ManagedStore::new(layout.clone());
        let old_reader = store.open_read().unwrap();
        std::fs::write(
            base.join("src/lib.rs"),
            "pub fn replacement_generation() { let _value = 2; }\n",
        )
        .unwrap();
        let (_, second) = execute_materialization(&options).unwrap();
        let second = second.active_generation.unwrap();
        assert_ne!(first, second);
        std::fs::write(
            base.join(".codebaseGraph/config.json"),
            serde_json::to_vec(&serde_json::json!({"schema_version":3,"storage_root":storage}))
                .unwrap(),
        )
        .unwrap();
        let target = MaintenanceTarget {
            repository_root: base.clone(),
            state_root: base.join(".codebaseGraph"),
            storage_root: Some(storage.clone()),
            direct_pair: None,
            config_path: None,
        };
        let domain = ReferenceDomain::managed(&layout).unwrap();
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let mut blocked = false;
        for _ in 0..8 {
            let binding = admission::prepare_target_binding(&target, &memory).unwrap();
            let epoch = observed_epoch(&domain);
            let mut state = GenerationPreparationState::default();
            let mut work = prepare_work(
                &target,
                &binding,
                &domain,
                &epoch,
                &mut state,
                &memory,
                &|| false,
            )
            .unwrap()
            .expect("actual final generation work");
            match work.advance(&mut MutationGrant::start(), None) {
                Err(FixedControlError::Invalid(FixedControlProblem::Busy)) => {
                    blocked = true;
                    break;
                }
                Ok(_) => finish_work(&mut work, &mut state),
                Err(error) => {
                    panic!("generation preparation failed before reader exclusion: {error:?}")
                }
            }
        }
        assert!(blocked, "old reader must block the actual role detach");
        assert!(layout.generation(&first).unwrap().db_path().exists());
        drop(old_reader);
        // The protected current generation remains actually open throughout
        // all old-generation effects, including native-family lock retirement.
        let current_reader = store.open_read().unwrap();
        assert_eq!(current_reader.generation_id, second);
        let mut completed = false;
        let cuts = [
            fault::Cut::AfterDetach,
            fault::Cut::AfterUnlink,
            fault::Cut::AfterRootRemoval,
            fault::Cut::AfterDurableDone,
        ];
        let fault_root = locks::path_identity(domain.artifact_root()).unwrap();
        let mut cut_index = 0;
        for _ in 0..128 {
            let binding = admission::prepare_target_binding(&target, &memory).unwrap();
            let epoch = observed_epoch(&domain);
            // Recreate every warm owner on purpose: only actual durable
            // control+cohort facts and reacquired role guards can make progress.
            let mut state = GenerationPreparationState::default();
            let Some(mut work) = prepare_work(
                &target,
                &binding,
                &domain,
                &epoch,
                &mut state,
                &memory,
                &|| false,
            )
            .unwrap() else {
                completed = true;
                break;
            };
            let arm = cuts.get(cut_index).map(|cut| fault::arm(fault_root, *cut));
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                work.advance(&mut MutationGrant::start(), None)
            })) {
                Ok(Ok(_)) => finish_work(&mut work, &mut state),
                Ok(Err(FixedControlError::Grant(_))) => {}
                Ok(Err(error)) => panic!("cold generation progress failed: {error:?}"),
                Err(payload) => {
                    let is_cut = payload.downcast_ref::<String>().is_some_and(|message| {
                        message.starts_with("actual generation durable cut:")
                    });
                    if !is_cut {
                        std::panic::resume_unwind(payload);
                    }
                    cut_index += 1;
                }
            }
            drop(arm);
        }
        assert!(
            completed,
            "bounded cold attempts must complete the original cohort and reset only its stream"
        );
        assert_eq!(
            cut_index,
            cuts.len(),
            "all actual durable generation cut points must be replayed"
        );
        assert!(!layout.generation(&first).unwrap().root().exists());
        assert!(layout.generation(&second).unwrap().db_path().exists());
        assert_eq!(store.open_read().unwrap().generation_id, second);
        assert!(memory.peak() <= 8 * 1024 * 1024);
        drop(current_reader);
        drop(store);
        std::fs::remove_dir_all(base).unwrap();
    }
}
