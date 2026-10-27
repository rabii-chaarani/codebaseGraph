//! Superseded generation facts and role-scoped mutation admission. Genuine
//! publication and FINAL disposition are distinct from READY or file hashes.
use super::inventory::MaintenanceTarget;
use super::layout::ManagedLayout;
use super::locks::{
    self, FileIdentity, FixedFileStamp, FixedLockedFile, LockMode, RootedDirectory,
};
use super::maintenance_admission::{
    self, AdmissionError, EpochAdvance, PreparedScopedInvalidation, PreparedTargetBinding,
    RootOriginBinding,
};
use super::maintenance_budget::{CleanupMemory, MemoryBox, MemoryReservation, MutationGrant};
use super::maintenance_ownership::{ReferenceDomain, ReferenceEpoch, ReferenceMutation};
use crate::error::NativeError;
use std::io::Write;

#[cfg(test)]
use super::maintenance_generation_gate::gen_name;
use super::maintenance_generation_gate::{
    self as gate, component, digest, exact, fixed_id, invalid, open_generation, read_current,
    read_final, read_origin, recheck, resource_name, stamp, BoundLeaf, Final,
    GenerationOriginScope, Origin, FINAL_BYTES, FINAL_NAME, ORIGIN_BYTES, ORIGIN_NAME, PAYLOADS,
};
#[cfg(test)]
pub(crate) use super::maintenance_generation_gate::{
    observe_generation_disposition, GenerationDisposition,
};
impl From<RootOriginBinding> for GenerationOriginScope {
    fn from(s: RootOriginBinding) -> Self {
        Self {
            root: s.root,
            coordination: s.coordination,
            cohort: s.cohort,
            publication_binding: s.publication_binding,
            incarnation: s.incarnation,
        }
    }
}
impl From<GenerationOriginScope> for RootOriginBinding {
    fn from(s: GenerationOriginScope) -> Self {
        Self {
            root: s.root,
            coordination: s.coordination,
            cohort: s.cohort,
            publication_binding: s.publication_binding,
            incarnation: s.incarnation,
        }
    }
}

fn admission_invalid() -> AdmissionError {
    AdmissionError::Invalid(locks::FixedControlProblem::Changed)
}
pub(crate) const GENERATION_PROOF_BYTES: usize = 4096;
pub(crate) const MAX_GENERATION_LEAVES: usize = 14;
const NAMES: [&[u8]; MAX_GENERATION_LEAVES] = [
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
    ORIGIN_NAME,
    FINAL_NAME,
];
/// The API owns this unforgeable token's construction after full native
/// verification, required recovery and a fresh CURRENT check, under its actual
/// retained writer and actual new-generation read lease.
pub(crate) fn seal_verified_generation(
    verified: &crate::api::NativeVerifiedGeneration<'_>,
) -> Result<(), NativeError> {
    let layout = verified.layout();
    let writer = verified.writer();
    if !writer.verifies(&layout.writer_lock_path(), LockMode::Exclusive)? {
        return Err(invalid("generation_writer_changed"));
    }
    let domain = ReferenceDomain::managed(layout)?;
    let mutation = ReferenceMutation::begin_in_domain(&domain)?;
    let mut capability = mutation
        .artifact_root_producer_capability()?
        .ok_or_else(|| invalid("generation_producer_scope_untracked"))?;
    let lease = super::maintenance_controls::try_begin_root_publication(&mut capability)?
        .ok_or_else(|| invalid("generation_publication_scope_busy"))?;
    let root = RootedDirectory::open(
        layout.storage_root(),
        locks::path_identity(layout.storage_root())?,
    )?;
    let (id, len) = fixed_id(verified.generation_id())?;
    let directory = open_generation(&root, &id, len)?;
    let current = read_current(&root)?;
    if current.0 != id || current.1 != len {
        return Err(invalid("generation_current_changed"));
    }
    let origin = match read_origin(&directory, root.identity()) {
        Ok((origin, bytes, _)) => {
            let expected_scope = RootOriginBinding {
                root: lease.binding().root,
                coordination: lease.binding().coordination,
                cohort: lease.root_cohort_identity(),
                publication_binding: lease.publication_binding_identity(),
                incarnation: lease.binding().incarnation,
            };
            if origin.generation != id
                || origin.generation_len != len
                || origin.scope != expected_scope.into()
                || origin.writer != locks::path_identity(&layout.writer_lock_path())?
            {
                return Err(invalid("generation_existing_origin_changed"));
            }
            (origin, bytes)
        }
        Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            let record_file = directory.create_new(ORIGIN_NAME)?;
            let receipt = locks::file_identity(&record_file)?;
            let mut operation = [0; 32];
            getrandom::fill(&mut operation)
                .map_err(|_| invalid("generation_origin_entropy_unavailable"))?;
            let mut origin = Origin {
                generation: id,
                generation_len: len,
                scope: GenerationOriginScope {
                    root: lease.binding().root,
                    coordination: lease.binding().coordination,
                    cohort: lease.root_cohort_identity(),
                    publication_binding: lease.publication_binding_identity(),
                    incarnation: lease.binding().incarnation,
                },
                managed: root.identity(),
                writer: locks::path_identity(&layout.writer_lock_path())?,
                directory: directory.identity(),
                receipt,
                operation,
                lease: locks::file_identity(&directory.open_readonly_file(b"lease.lock")?)?,
                family: locks::file_identity(
                    &directory.open_readonly_file(b".database-use-family.lock")?,
                )?,
                resource: locks::file_identity(&directory.open_readonly_file(&resource_name())?)?,
                manifest_resource: locks::file_identity(&directory.open_readonly_file(
                    &super::maintenance_generation_gate::resource_name_for(b"manifest.json"),
                )?)?,
                native_lock: match directory.open_readonly_file(b"graph.ldb.lock") {
                    Ok(f) => Some(locks::file_identity(&f)?),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.into()),
                },
                leaves: [None; PAYLOADS],
            };
            for (i, name) in NAMES[..PAYLOADS].iter().enumerate() {
                let file = match directory.open_readonly_file(name) {
                    Ok(file) => file,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound && i >= 4 => {
                        continue
                    }
                    Err(error) => return Err(error.into()),
                };
                let before = stamp(&file)?;
                let (n, hash) = super::maintenance_origin::hash_payload(&file)?;
                recheck(before, &file)?;
                origin.leaves[i] = Some(BoundLeaf {
                    id: before.identity(),
                    len: n,
                    hash,
                    stamp_digest: before.binding_digest(),
                });
            }
            let bytes = origin.encode();
            if Origin::decode(&bytes).is_none() {
                return Err(invalid("generation_bundle_incomplete"));
            }
            let mut cursor = &record_file;
            cursor.write_all(&bytes)?;
            record_file.sync_all()?;
            directory.sync_namespace()?;
            (origin, bytes)
        }
        Err(error) => return Err(error),
    };
    // Only a prior genuine published receipt can acquire FINAL. An old READY
    // bundle without such evidence is preserved for controlled association.
    if let Some(previous) = verified.previous_generation_id() {
        let (old_id, old_len) = fixed_id(previous)?;
        if old_id == id {
            return Err(invalid("generation_self_retirement"));
        }
        let old = open_generation(&root, &old_id, old_len)?;
        match read_origin(&old, root.identity()) {
            Ok((prior, prior_bytes, _)) => {
                if prior.generation != old_id
                    || prior.generation_len != old_len
                    || prior.scope != origin.0.scope
                    || prior.writer != origin.0.writer
                {
                    return Err(invalid("generation_predecessor_changed"));
                }
                match old.open_readonly_file(FINAL_NAME) {
                    Ok(_) => {
                        read_final(&old, prior, &prior_bytes)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        let file = old.create_new(FINAL_NAME)?;
                        let record = Final {
                            generation: old_id,
                            generation_len: old_len,
                            managed: root.identity(),
                            directory: old.identity(),
                            origin: prior.receipt,
                            origin_digest: digest(&prior_bytes),
                            receipt: locks::file_identity(&file)?,
                            successor: origin.0.directory,
                            successor_origin: origin.0.receipt,
                            successor_id: id,
                            successor_len: len,
                            successor_digest: digest(&origin.1),
                        };
                        let mut cursor = &file;
                        cursor.write_all(&record.encode())?;
                        file.sync_all()?;
                        old.sync_namespace()?;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                match old.open_readonly_file(gate::LEGACY_FINAL_NAME) {
                    Ok(_) => {
                        let (closed, _, _) = gate::read_legacy_final(&old, root.identity())?;
                        if closed.scope != origin.0.scope
                            || closed.generation != old_id
                            || closed.generation_len != old_len
                        {
                            return Err(invalid("legacy_generation_final_scope_changed"));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        let database = stamp(&old.open_readonly_file(b"graph.ldb")?)?;
                        let manifest = stamp(&old.open_readonly_file(b"manifest.json")?)?;
                        let file = old.create_new(gate::LEGACY_FINAL_NAME)?;
                        let mut operation = [0; 32];
                        getrandom::fill(&mut operation)
                            .map_err(|_| invalid("legacy_final_entropy_unavailable"))?;
                        let closed = gate::LegacyFinal {
                            generation: old_id,
                            generation_len: old_len,
                            successor_id: id,
                            successor_len: len,
                            scope: origin.0.scope,
                            managed: root.identity(),
                            writer: origin.0.writer,
                            directory: old.identity(),
                            receipt: locks::file_identity(&file)?,
                            lease: locks::file_identity(&old.open_readonly_file(b"lease.lock")?)?,
                            family: locks::file_identity(
                                &old.open_readonly_file(b".database-use-family.lock")?,
                            )?,
                            resource: locks::file_identity(
                                &old.open_readonly_file(&resource_name())?,
                            )?,
                            manifest_resource: locks::file_identity(
                                &old.open_readonly_file(&gate::resource_name_for(
                                    b"manifest.json",
                                ))?,
                            )?,
                            database: gate::LegacyClosedFile {
                                id: database.identity(),
                                len: database.len(),
                                stamp_digest: database.binding_digest(),
                            },
                            manifest: gate::LegacyClosedFile {
                                id: manifest.identity(),
                                len: manifest.len(),
                                stamp_digest: manifest.binding_digest(),
                            },
                            successor: origin.0.directory,
                            successor_origin: origin.0.receipt,
                            successor_digest: digest(&origin.1),
                            operation,
                            native_lock: match old.open_readonly_file(b"graph.ldb.lock") {
                                Ok(f) => Some(locks::file_identity(&f)?),
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                                Err(e) => return Err(e.into()),
                            },
                        };
                        let mut cursor = &file;
                        cursor.write_all(&closed.encode())?;
                        file.sync_all()?;
                        old.sync_namespace()?;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(error) => return Err(error),
        }
    }
    if read_current(&root)?.2 != current.2 || !lease.revalidate_existing()? {
        return Err(invalid("generation_verified_publication_changed"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GenerationLeaf {
    pub(crate) index: u8,

    pub(crate) payload: FileIdentity,
    pub(crate) byte_len: u64,

    stamp: FixedFileStamp,
}
impl GenerationLeaf {
    pub(crate) fn stamp(&self) -> FixedFileStamp {
        self.stamp
    }
    pub(crate) fn name(&self) -> &'static [u8] {
        NAMES[usize::from(self.index)]
    }
}
#[derive(Clone, Copy)]
pub(crate) struct GenerationInventory {
    pub(crate) source_epoch: u64,
    pub(crate) managed: FileIdentity,
    pub(crate) directory: FileIdentity,
    pub(crate) generation_id: [u8; 128],
    pub(crate) generation_len: u8,
    pub(crate) leaves: [Option<GenerationLeaf>; MAX_GENERATION_LEAVES],
    origin: Origin,
    origin_image: [u8; ORIGIN_BYTES],
    final_image: [u8; FINAL_BYTES],
}
impl GenerationInventory {
    /// Facts to persist as a complete, digest-bound cohort prefix BEFORE the
    /// first leaf detaches. These bytes carry no mutation authority themselves.
    pub(crate) fn encode_proof(&self) -> [u8; GENERATION_PROOF_BYTES] {
        let mut bytes = [0; GENERATION_PROOF_BYTES];
        bytes[..8].copy_from_slice(b"CGGPR001");
        bytes[8..16].copy_from_slice(&self.source_epoch.to_le_bytes());
        bytes[32..32 + ORIGIN_BYTES].copy_from_slice(&self.origin_image);
        bytes[2080..2080 + FINAL_BYTES].copy_from_slice(&self.final_image);
        let hash = digest(&bytes[..GENERATION_PROOF_BYTES - 32]);
        bytes[GENERATION_PROOF_BYTES - 32..].copy_from_slice(&hash);
        bytes
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GenerationReport {
    pub(crate) inspected: u64,
    pub(crate) final_generations: u64,
    pub(crate) unknown: u64,
    pub(crate) active: u64,
    pub(crate) category_complete: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationError {
    Io(std::io::ErrorKind),
    Unknown,
    Changed,
    Busy,
    Cancelled,
    ResourceLimit,
}
fn generation_error(error: NativeError) -> GenerationError {
    match error {
        NativeError::Io(error) => GenerationError::Io(error.kind()),
        _ => GenerationError::Unknown,
    }
}
fn generation_admission(error: AdmissionError) -> GenerationError {
    match error {
        AdmissionError::Io(error) => GenerationError::Io(error.kind()),
        AdmissionError::Resource(_) => GenerationError::ResourceLimit,
        AdmissionError::Invalid(locks::FixedControlProblem::Busy) => GenerationError::Busy,
        _ => GenerationError::Changed,
    }
}
/// Advisory owners are Send; no directory/lock authority survives preparation.
pub(crate) struct PreparedGeneration {
    binding: MemoryBox<PreparedTargetBinding>,
    scoped: MemoryBox<PreparedScopedInvalidation>,
    layout: ManagedLayout,
    inventory: MemoryBox<GenerationInventory>,
    selected: usize,
    current: FixedFileStamp,
    origin_stamp: Option<FixedFileStamp>,
    final_stamp: Option<FixedFileStamp>,
    detached: Option<[u8; 73]>,
    _floor: MemoryReservation,
}
pub(crate) struct GenerationMutation<'a> {
    managed: &'a RootedDirectory,
    parent: &'a RootedDirectory,
    leaf: &'a GenerationLeaf,
}
impl GenerationMutation<'_> {
    pub(crate) fn managed_root(&self) -> &RootedDirectory {
        self.managed
    }
    pub(crate) fn source_parent(&self) -> &RootedDirectory {
        self.parent
    }
    pub(crate) fn leaf(&self) -> &GenerationLeaf {
        self.leaf
    }
}
fn observe_scope_matches(origin: Origin, scope: RootOriginBinding) -> bool {
    origin.scope == scope.into()
}
fn current_origin_scope(
    layout: &ManagedLayout,
    memory: &CleanupMemory,
) -> Result<RootOriginBinding, GenerationError> {
    let path = layout.artifacts_root();
    let root = RootedDirectory::open_readonly_root(
        &path,
        locks::path_identity(&path).map_err(generation_error)?,
    )
    .map_err(|e| GenerationError::Io(e.kind()))?;
    let scope = super::maintenance_origin::observe_artifact_root_scope(&root, memory)
        .map_err(generation_error)?
        .ok_or(GenerationError::Unknown)?;
    Ok(RootOriginBinding {
        root: scope.root,
        coordination: scope.coordination,
        cohort: scope.cohort,
        publication_binding: scope.publication_binding,
        incarnation: scope.incarnation,
    })
}
fn capture_leaf(
    directory: &RootedDirectory,
    index: usize,

    bound: BoundLeaf,
) -> Result<GenerationLeaf, GenerationError> {
    let file = directory
        .open_readonly_file(NAMES[index])
        .map_err(|e| GenerationError::Io(e.kind()))?;
    let before = stamp(&file).map_err(generation_error)?;
    if before.identity() != bound.id
        || before.len() != bound.len
        || before.binding_digest() != bound.stamp_digest
    {
        return Err(GenerationError::Changed);
    }
    Ok(GenerationLeaf {
        index: index as u8,
        payload: bound.id,
        byte_len: bound.len,
        stamp: before,
    })
}
fn hash_leaf(
    directory: &RootedDirectory,
    index: usize,

    bound: BoundLeaf,
) -> Result<GenerationLeaf, GenerationError> {
    let file = directory
        .open_readonly_file(NAMES[index])
        .map_err(|e| GenerationError::Io(e.kind()))?;
    let before = stamp(&file).map_err(generation_error)?;
    if before.identity() != bound.id
        || before.len() != bound.len
        || before.binding_digest() != bound.stamp_digest
    {
        return Err(GenerationError::Changed);
    }
    let (len, hash) = super::maintenance_origin::hash_payload(&file).map_err(generation_error)?;
    recheck(before, &file).map_err(generation_error)?;
    if len != bound.len || hash != bound.hash {
        return Err(GenerationError::Changed);
    }
    Ok(GenerationLeaf {
        index: index as u8,
        payload: bound.id,
        byte_len: len,
        stamp: before,
    })
}
fn control_leaf(
    directory: &RootedDirectory,
    index: usize,

    expected: &[u8],
) -> Result<GenerationLeaf, GenerationError> {
    let file = directory
        .open_readonly_file(NAMES[index])
        .map_err(|e| GenerationError::Io(e.kind()))?;
    let before = stamp(&file).map_err(generation_error)?;
    let (len, hash) = super::maintenance_origin::hash_payload(&file).map_err(generation_error)?;
    if len != expected.len() as u64 || hash != digest(expected) {
        return Err(GenerationError::Changed);
    }
    Ok(GenerationLeaf {
        index: index as u8,
        payload: before.identity(),
        byte_len: len,
        stamp: before,
    })
}
// Large prepared admission values finish construction in their own frame and
// remain charged on the heap through every callback. They carry no authority.
#[inline(never)]
fn boxed_generation_binding(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedTargetBinding>, GenerationError> {
    let binding = maintenance_admission::prepare_target_binding(target, memory)
        .map_err(generation_admission)?;
    MemoryBox::new(binding, memory).map_err(|_| GenerationError::ResourceLimit)
}
#[inline(never)]
fn boxed_generation_invalidation(
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedScopedInvalidation>, GenerationError> {
    let scoped = maintenance_admission::prepare_scoped_invalidation(domain, memory)
        .map_err(generation_admission)?;
    MemoryBox::new(scoped, memory).map_err(|_| GenerationError::ResourceLimit)
}
#[inline(never)]
fn common(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
) -> Result<
    (
        ManagedLayout,
        MemoryBox<PreparedTargetBinding>,
        MemoryReservation,
    ),
    GenerationError,
> {
    if target.direct_pair.is_some() {
        return Err(GenerationError::Unknown);
    }
    let projected = super::maintenance_projection::project_binding(target, memory)
        .map_err(|_| GenerationError::Changed)?;
    if projected.direct.is_some() {
        return Err(GenerationError::Unknown);
    }
    let storage = &projected.storage;
    #[cfg(windows)]
    const PATH_BYTES: usize = 65536;
    #[cfg(not(windows))]
    const PATH_BYTES: usize = 4096;
    let floor = memory
        .reserve(
            std::mem::size_of::<PreparedGeneration>()
                + std::mem::size_of::<GenerationInventory>() * 4
                + 8 * PATH_BYTES
                + 24576,
        )
        .map_err(|_| GenerationError::ResourceLimit)?;
    let layout = ManagedLayout::new(storage);
    let actual = ReferenceDomain::managed(&layout).map_err(generation_error)?;
    if !actual.same_scope(domain) || !domain.revalidate().map_err(generation_error)? {
        return Err(GenerationError::Changed);
    }
    let binding = boxed_generation_binding(target, memory)?;
    Ok((layout, binding, floor))
}
/// Metadata-only fair discovery. IDs are emitted only after exact genuine
/// Origin+FINAL records validate; emission is advisory and owns no authority.
/// No payload hash, native open or exclusive family/generation probe occurs.
pub(crate) fn discover_generations(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    emit: &mut impl FnMut(&[u8]) -> Result<(), GenerationError>,
) -> Result<GenerationReport, GenerationError> {
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    #[cfg(windows)]
    const DISCOVERY_PATH_BYTES: usize = 65536;
    #[cfg(not(windows))]
    const DISCOVERY_PATH_BYTES: usize = 4096;
    let _floor = memory
        .reserve(
            8 * DISCOVERY_PATH_BYTES
                + 65536
                + std::mem::size_of::<Origin>() * 3
                + ORIGIN_BYTES * 3
                + FINAL_BYTES * 3,
        )
        .map_err(|_| GenerationError::ResourceLimit)?;
    let projected = super::maintenance_projection::project_binding(target, memory)
        .map_err(|_| GenerationError::Changed)?;
    if projected.direct.is_some() {
        return Ok(GenerationReport {
            category_complete: true,
            ..GenerationReport::default()
        });
    }
    let layout = ManagedLayout::new(&projected.storage);
    let origin_scope = current_origin_scope(&layout, memory)?;
    let mut binding = maintenance_admission::prepare_target_binding(target, memory)
        .map_err(generation_admission)?;
    let root = RootedDirectory::open_readonly_root(
        layout.storage_root(),
        locks::path_identity(layout.storage_root()).map_err(generation_error)?,
    )
    .map_err(|e| GenerationError::Io(e.kind()))?;
    let current = read_current(&root).map_err(generation_error)?;
    let generations = root
        .open_directory_readonly(b"generations")
        .map_err(|e| GenerationError::Io(e.kind()))?;
    let mut report = GenerationReport {
        category_complete: true,
        ..GenerationReport::default()
    };
    let mut names = [0; 4096];
    let mut failure = None;
    let visited = generations.visit_entries(&mut names, &mut |name| {
        if cancel() {
            failure = Some(GenerationError::Cancelled);
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        report.inspected = report
            .inspected
            .checked_add(1)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        let Some(id) = name.strip_prefix(b"gen-") else {
            report.unknown = report.unknown.saturating_add(1);
            report.category_complete = false;
            return Ok(());
        };
        if !component(id) {
            report.unknown = report.unknown.saturating_add(1);
            report.category_complete = false;
            return Ok(());
        }
        let mut gen = [0; 128];
        gen[..id.len()].copy_from_slice(id);
        if gen == current.0 && id.len() == usize::from(current.1) {
            report.active = report.active.saturating_add(1);
            return Ok(());
        }
        let observed = (|| -> Result<(), GenerationError> {
            let directory = generations
                .open_directory_readonly(name)
                .map_err(|e| GenerationError::Io(e.kind()))?;
            let (origin, bytes, _) =
                read_origin(&directory, root.identity()).map_err(generation_error)?;
            if origin.generation != gen
                || usize::from(origin.generation_len) != id.len()
                || !observe_scope_matches(origin, origin_scope)
            {
                return Err(GenerationError::Changed);
            }
            read_final(&directory, origin, &bytes).map_err(generation_error)?;
            let mut child_names = [0; 4096];
            directory
                .visit_entries(&mut child_names, &mut |child| {
                    if !NAMES.contains(&child)
                        && child != b"lease.lock"
                        && child != b".database-use-family.lock"
                        && child != resource_name().as_slice()
                        && child != b"graph.ldb.lock"
                    {
                        report.unknown = report.unknown.saturating_add(1);
                        report.category_complete = false;
                    }
                    Ok(())
                })
                .map_err(|e| GenerationError::Io(e.kind()))?;
            report.final_generations = report.final_generations.saturating_add(1);
            emit(id)?;
            Ok(())
        })();
        if let Err(error) = observed {
            if matches!(
                error,
                GenerationError::Cancelled | GenerationError::ResourceLimit
            ) {
                failure = Some(error);
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            report.unknown = report.unknown.saturating_add(1);
            report.category_complete = false;
        }
        Ok(())
    });
    if let Some(error) = failure {
        return Err(error);
    }
    visited.map_err(|e| GenerationError::Io(e.kind()))?;
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    if read_current(&root).map_err(generation_error)?.2 != current.2
        || !binding.is_current().map_err(generation_admission)?
    {
        return Err(GenerationError::Changed);
    }
    Ok(report)
}
/// One charged inventory is initialized once and remains behind its owner
/// through every admitted callback. Keeping this construction in its own small
/// frame avoids debug builds multiplying whole-cohort copies on the worker.
#[inline(never)]
fn allocate_inventory(
    source_epoch: u64,
    origin: &Origin,
    origin_image: &[u8; ORIGIN_BYTES],
    final_image: &[u8; FINAL_BYTES],
    memory: &CleanupMemory,
) -> Result<MemoryBox<GenerationInventory>, GenerationError> {
    MemoryBox::new(
        GenerationInventory {
            source_epoch,
            managed: origin.managed,
            directory: origin.directory,
            generation_id: origin.generation,
            generation_len: origin.generation_len,
            leaves: [None; MAX_GENERATION_LEAVES],
            origin: *origin,
            origin_image: *origin_image,
            final_image: *final_image,
        },
        memory,
    )
    .map_err(|_| GenerationError::ResourceLimit)
}

/// Validate one exact selected payload, after fair metadata discovery selected
/// this category. Other cohort leaves get fixed producer-bound stamp checks.
#[inline(never)]
pub(crate) fn prepare_exact_generation(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    generation_id: &[u8],
    selected: u8,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<MemoryBox<PreparedGeneration>, GenerationError> {
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    if !component(generation_id) || usize::from(selected) >= PAYLOADS {
        return Err(GenerationError::Unknown);
    }
    let (layout, mut binding, floor) = common(target, domain, memory)?;
    let origin_scope = current_origin_scope(&layout, memory)?;
    let root = RootedDirectory::open_readonly_root(
        layout.storage_root(),
        locks::path_identity(layout.storage_root()).map_err(generation_error)?,
    )
    .map_err(|e| GenerationError::Io(e.kind()))?;
    let current = read_current(&root).map_err(generation_error)?;
    let mut id = [0; 128];
    id[..generation_id.len()].copy_from_slice(generation_id);
    let len = generation_id.len() as u8;
    if current.0 == id && current.1 == len {
        return Err(GenerationError::Busy);
    }
    let directory = open_generation(&root, &id, len).map_err(generation_error)?;
    let (origin, origin_image, _) =
        read_origin(&directory, root.identity()).map_err(generation_error)?;
    if origin.generation != id
        || origin.generation_len != len
        || !observe_scope_matches(origin, origin_scope)
    {
        return Err(GenerationError::Changed);
    }
    let (_, final_image, _) =
        read_final(&directory, origin, &origin_image).map_err(generation_error)?;
    let mut inventory = allocate_inventory(
        expected.counter(),
        &origin,
        &origin_image,
        &final_image,
        memory,
    )?;
    for i in 0..PAYLOADS {
        if cancel() {
            return Err(GenerationError::Cancelled);
        }
        if let Some(bound) = origin.leaves[i] {
            inventory.leaves[i] = Some(capture_leaf(&directory, i, bound)?);
        }
    }
    inventory.leaves[usize::from(selected)] = Some(hash_leaf(
        &directory,
        usize::from(selected),
        origin.leaves[usize::from(selected)].ok_or(GenerationError::Unknown)?,
    )?);
    inventory.leaves[12] = Some(control_leaf(&directory, 12, &origin_image)?);
    inventory.leaves[13] = Some(control_leaf(&directory, 13, &final_image)?);
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    if read_current(&root).map_err(generation_error)?.2 != current.2
        || !binding.is_current().map_err(generation_admission)?
    {
        return Err(GenerationError::Changed);
    }
    let origin_stamp = inventory.leaves[12].map(|l| l.stamp);
    let final_stamp = inventory.leaves[13].map(|l| l.stamp);
    let prepared = PreparedGeneration {
        binding,
        scoped: boxed_generation_invalidation(domain, memory)?,
        layout,
        inventory,
        selected: usize::from(selected),
        current: current.2,
        origin_stamp,
        final_stamp,
        detached: None,
        _floor: floor,
    };
    MemoryBox::new(prepared, memory).map_err(|_| GenerationError::ResourceLimit)
}
/// Resume only from the complete cohort prefix already authenticated by the
/// caller's admitted private stream. This consumes no native SDK or SQL and
/// never reconstructs origin from the remaining file names after DB removal.
pub(crate) fn prepare_generation_resume(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    _expected: &ReferenceEpoch,
    proof: &[u8; GENERATION_PROOF_BYTES],
    selected: usize,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<MemoryBox<PreparedGeneration>, GenerationError> {
    prepare_generation_resume_inner(
        target, domain, _expected, proof, selected, None, memory, cancel,
    )
}
// Keep source selection, immutable closure proof and the shared memory account explicit in this role adapter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_generation_detached(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    proof: &[u8; GENERATION_PROOF_BYTES],
    selected: usize,
    tombstone: [u8; 73],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<MemoryBox<PreparedGeneration>, GenerationError> {
    if &tombstone[..9] != b".retired-"
        || !tombstone[9..]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return Err(GenerationError::Unknown);
    }
    prepare_generation_resume_inner(
        target,
        domain,
        expected,
        proof,
        selected,
        Some(tombstone),
        memory,
        cancel,
    )
}
// Keep source selection, immutable closure proof and the shared memory account explicit in this role adapter.
#[allow(clippy::too_many_arguments)]
fn prepare_generation_resume_inner(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    _expected: &ReferenceEpoch,
    proof: &[u8; GENERATION_PROOF_BYTES],
    selected: usize,
    detached: Option<[u8; 73]>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<MemoryBox<PreparedGeneration>, GenerationError> {
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    if selected >= MAX_GENERATION_LEAVES
        || &proof[..8] != b"CGGPR001"
        || proof[16..32]
            .iter()
            .chain(proof[2592..GENERATION_PROOF_BYTES - 32].iter())
            .any(|v| *v != 0)
        || digest(&proof[..GENERATION_PROOF_BYTES - 32]) != proof[GENERATION_PROOF_BYTES - 32..]
    {
        return Err(GenerationError::Unknown);
    }
    let (layout, mut binding, floor) = common(target, domain, memory)?;
    let origin_scope = current_origin_scope(&layout, memory)?;
    let origin_image = proof[32..2080].try_into().unwrap();
    let final_image = proof[2080..2592].try_into().unwrap();
    let origin = Origin::decode(&origin_image).ok_or(GenerationError::Unknown)?;
    let final_record = Final::decode(&final_image).ok_or(GenerationError::Unknown)?;
    let original_epoch = u64::from_le_bytes(proof[8..16].try_into().unwrap());
    if final_record.origin_digest != digest(&origin_image)
        || final_record.directory != origin.directory
        || final_record.origin != origin.receipt
        || final_record.generation != origin.generation
        || !observe_scope_matches(origin, origin_scope)
    {
        return Err(GenerationError::Changed);
    }
    let root = RootedDirectory::open_readonly_root(layout.storage_root(), origin.managed)
        .map_err(|e| GenerationError::Io(e.kind()))?;
    let current = read_current(&root).map_err(generation_error)?;
    if current.0 == origin.generation && current.1 == origin.generation_len {
        return Err(GenerationError::Changed);
    }
    let directory = open_generation(&root, &origin.generation, origin.generation_len)
        .map_err(generation_error)?;
    if directory.identity() != origin.directory {
        return Err(GenerationError::Changed);
    }
    // Origin may be detached only after every payload. FINAL remains the last
    // public control and is matched to the durable original cohort bytes.
    let origin_stamp = match directory.open_readonly_file(ORIGIN_NAME) {
        Ok(file) => {
            let s = stamp(&file).map_err(generation_error)?;
            if exact::<ORIGIN_BYTES>(&file).map_err(generation_error)? != origin_image
                || s.identity() != origin.receipt
            {
                return Err(GenerationError::Changed);
            }
            Some(s)
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && (selected == 13 || (selected == 12 && detached.is_some())) =>
        {
            None
        }
        Err(error) => return Err(GenerationError::Io(error.kind())),
    };
    let final_stamp = match directory.open_readonly_file(FINAL_NAME) {
        Ok(file) => {
            let actual = stamp(&file).map_err(generation_error)?;
            if exact::<FINAL_BYTES>(&file).map_err(generation_error)? != final_image
                || actual.identity() != final_record.receipt
            {
                return Err(GenerationError::Changed);
            }
            Some(actual)
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && selected == 13
                && detached.is_some() =>
        {
            None
        }
        Err(error) => return Err(GenerationError::Io(error.kind())),
    };
    let leaf = if let Some(name) = detached {
        let file = root
            .open_readonly_file(&name)
            .map_err(|e| GenerationError::Io(e.kind()))?;
        let before = stamp(&file).map_err(generation_error)?;
        let (byte_len, hash) =
            super::maintenance_origin::hash_payload(&file).map_err(generation_error)?;
        let (wanted_id, wanted_len, wanted_hash) = if selected < PAYLOADS {
            let bound = origin.leaves[selected].ok_or(GenerationError::Unknown)?;
            (bound.id, bound.len, bound.hash)
        } else if selected == 12 {
            (origin.receipt, ORIGIN_BYTES as u64, digest(&origin_image))
        } else {
            (
                final_record.receipt,
                FINAL_BYTES as u64,
                digest(&final_image),
            )
        };
        if before.identity() != wanted_id || byte_len != wanted_len || hash != wanted_hash {
            return Err(GenerationError::Changed);
        }
        recheck(before, &file).map_err(generation_error)?;
        GenerationLeaf {
            index: selected as u8,
            payload: wanted_id,
            byte_len,

            stamp: before,
        }
    } else if selected < PAYLOADS {
        hash_leaf(
            &directory,
            selected,
            origin.leaves[selected].ok_or(GenerationError::Unknown)?,
        )?
    } else {
        control_leaf(
            &directory,
            selected,
            if selected == 12 {
                &origin_image[..]
            } else {
                &final_image[..]
            },
        )?
    };
    let mut inventory =
        allocate_inventory(original_epoch, &origin, &origin_image, &final_image, memory)?;
    inventory.leaves[selected] = Some(leaf);
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    if !binding.is_current().map_err(generation_admission)? {
        return Err(GenerationError::Changed);
    }
    MemoryBox::new(
        PreparedGeneration {
            binding,
            scoped: boxed_generation_invalidation(domain, memory)?,
            layout,
            inventory,
            selected,
            current: current.2,
            origin_stamp,
            final_stamp,
            detached,
            _floor: floor,
        },
        memory,
    )
    .map_err(|_| GenerationError::ResourceLimit)
}
impl PreparedGeneration {
    pub(crate) fn inventory(&self) -> &GenerationInventory {
        &self.inventory
    }
    pub(crate) fn leaf(&self) -> &GenerationLeaf {
        self.inventory.leaves[self.selected].as_ref().unwrap()
    }
    #[cfg(test)]
    pub(crate) fn source_epoch_counter(&self) -> u64 {
        self.inventory.source_epoch
    }
    /// Local validation/opens are charged separately before E; caller supplies
    /// the complete bounded journal/rename/unlink transition effect count.
    pub(crate) fn effect_entries(&self) -> usize {
        24
    }
    pub(crate) fn with_admitted(
        &mut self,
        grant: &mut MutationGrant,
        effect_entries: usize,
        apply: &mut impl FnMut(
            &GenerationMutation<'_>,
            &mut MutationGrant,
        ) -> Result<(), AdmissionError>,
    ) -> Result<EpochAdvance, AdmissionError> {
        grant.take_entries(self.effect_entries())?;
        let root = RootedDirectory::open(self.layout.storage_root(), self.inventory.managed)?;
        grant.check_time()?;
        let writer_file = root.open_readonly_file(b"writer.lock")?;
        if locks::file_identity(&writer_file).map_err(|_| admission_invalid())?
            != self.inventory.origin.writer
        {
            return Err(admission_invalid());
        }
        let _writer = FixedLockedFile::try_lock(writer_file, LockMode::Exclusive)?
            .ok_or(AdmissionError::Invalid(locks::FixedControlProblem::Busy))?;
        grant.check_time()?;
        let directory = open_generation(
            &root,
            &self.inventory.generation_id,
            self.inventory.generation_len,
        )
        .map_err(|_| admission_invalid())?;
        if directory.identity() != self.inventory.directory {
            return Err(admission_invalid());
        }
        let mut held: [Option<FixedLockedFile>; 3] = std::array::from_fn(|_| None);
        for (i, name, id) in [
            (0, b"lease.lock".as_slice(), self.inventory.origin.lease),
            (
                1,
                b".database-use-family.lock".as_slice(),
                self.inventory.origin.family,
            ),
            (
                2,
                resource_name().as_slice(),
                self.inventory.origin.resource,
            ),
        ] {
            grant.check_time()?;
            let file = directory.open_readonly_file(name)?;
            if locks::file_identity(&file).map_err(|_| admission_invalid())? != id {
                return Err(admission_invalid());
            }
            held[i] = Some(
                FixedLockedFile::try_lock(file, LockMode::Exclusive)?
                    .ok_or(AdmissionError::Invalid(locks::FixedControlProblem::Busy))?,
            );
        }
        let leaf = *self.leaf();
        let binding = &mut self.binding;
        let current = self.current;
        let origin_stamp = self.origin_stamp;
        let final_stamp = self.final_stamp;
        let detached = self.detached;
        let inventory = &self.inventory;
        maintenance_admission::with_scoped_reference_invalidation(
            &mut self.scoped,
            grant,
            effect_entries,
            &mut |_, grant| {
                binding.recheck(grant)?;
                grant.check_time()?;
                if open_generation(&root, &inventory.generation_id, inventory.generation_len)
                    .map_err(|_| admission_invalid())?
                    .identity()
                    != inventory.directory
                {
                    return Err(admission_invalid());
                }
                grant.check_time()?;
                let file = root.open_readonly_file(b"active.json")?;
                current.recheck(&file)?;
                for (name, expected) in [(ORIGIN_NAME, origin_stamp), (FINAL_NAME, final_stamp)] {
                    if let Some(expected) = expected {
                        grant.check_time()?;
                        let file = directory.open_readonly_file(name)?;
                        expected.recheck(&file)?;
                    } else if directory.open_readonly_file(name).is_ok() {
                        return Err(admission_invalid());
                    }
                }
                grant.check_time()?;
                let file = if let Some(name) = detached {
                    root.open_readonly_file(&name)?
                } else {
                    directory.open_readonly_file(leaf.name())?
                };
                leaf.stamp.recheck(&file)?;
                for (i, name) in [
                    b"lease.lock".as_slice(),
                    b".database-use-family.lock".as_slice(),
                    resource_name().as_slice(),
                ]
                .into_iter()
                .enumerate()
                {
                    grant.check_time()?;
                    let file = directory.open_readonly_file(name)?;
                    if locks::file_identity(&file).map_err(|_| admission_invalid())?
                        != locks::file_identity(held[i].as_ref().unwrap().file())
                            .map_err(|_| admission_invalid())?
                    {
                        return Err(admission_invalid());
                    }
                }
                Ok(())
            },
            &mut |_epoch, grant| {
                apply(
                    &GenerationMutation {
                        managed: &root,
                        parent: &directory,
                        leaf: &leaf,
                    },
                    grant,
                )
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{execute_materialization, MaterializeOptions};
    fn epoch(domain: &ReferenceDomain) -> ReferenceEpoch {
        match super::super::maintenance_ownership::observe_reference_epoch(domain).unwrap() {
            super::super::maintenance_ownership::EpochObservation::Verified(epoch) => epoch,
            other => panic!("actual reference epoch unavailable: {other:?}"),
        }
    }
    #[test]
    fn generation_preparation_keeps_admission_images_out_of_fixed_owner_layout() {
        // The worker remains 256 KiB on every platform. These fixed owner
        // envelopes must carry charged handles, not embed/copy large admission
        // scratch images above configuration, inventory and payload validation.
        assert!(std::mem::size_of::<PreparedGeneration>() <= 1024);
        assert!(std::mem::size_of::<PreparedGenerationRootRemoval>() <= 4096);
        assert!(
            std::mem::size_of::<PreparedGeneration>()
                < std::mem::size_of::<PreparedTargetBinding>()
        );
        assert!(
            std::mem::size_of::<PreparedGeneration>()
                < std::mem::size_of::<PreparedScopedInvalidation>()
        );
    }

    #[test]
    fn dc_generation_actual_publication_closed_role_lease_stale_and_resume_after_db_detach() {
        let base = std::env::temp_dir().join(format!(
            "dc-gen-public-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(base.join("src")).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        std::fs::write(base.join("src/lib.rs"), "pub fn first_version() {}\n").unwrap();
        let storage = base.join(".codebaseGraph/storage");
        let options = MaterializeOptions {
            source_root: Some(base.clone()),
            storage_root: Some(storage.clone()),
            mode: "full".into(),
            parallel: false,
            use_git: false,
            ..MaterializeOptions::default()
        };
        let (_, first) =
            execute_materialization(&options).expect("actual first managed public producer");
        let first_id = first.active_generation.unwrap();
        let layout = ManagedLayout::new(&storage);
        let store = super::super::managed::ManagedStore::new(layout.clone());
        let old_reader = store.open_read().unwrap();
        assert_eq!(old_reader.generation_id, first_id);
        let memory = CleanupMemory::new(16 * 1024 * 1024).unwrap();
        let first_paths = layout.generation(&first_id).unwrap();
        assert_eq!(
            observe_generation_disposition(&layout, &first_paths, &memory).unwrap(),
            GenerationDisposition::Published
        );
        std::fs::write(
            base.join("src/lib.rs"),
            "pub fn second_version() { let _value = 2; }\n",
        )
        .unwrap();
        let (_, second) = execute_materialization(&options)
            .expect("actual verified successor managed public producer");
        let second_id = second.active_generation.unwrap();
        assert_ne!(first_id, second_id);
        assert_eq!(
            observe_generation_disposition(&layout, &first_paths, &memory).unwrap(),
            GenerationDisposition::Final
        );
        std::fs::write(
            base.join(".codebaseGraph/config.json"),
            serde_json::to_vec(&serde_json::json!({"schema_version":3,"storage_root":storage}))
                .unwrap(),
        )
        .unwrap();
        let target = MaintenanceTarget {
            repository_root: base.clone(),
            state_root: base.join(".codebaseGraph"),
            storage_root: Some(storage),
            direct_pair: None,
            config_path: None,
        };
        let domain = ReferenceDomain::managed(&layout).unwrap();
        let original = epoch(&domain);
        let mut discovered = Vec::new();
        let report = discover_generations(&target, &memory, &|| false, &mut |id| {
            discovered.push(id.to_vec());
            Ok(())
        })
        .unwrap();
        assert_eq!(report.final_generations, 1);
        assert_eq!(discovered, vec![first_id.as_bytes().to_vec()]);
        let mut prepared = prepare_exact_generation(
            &target,
            &domain,
            &original,
            first_id.as_bytes(),
            0,
            &memory,
            &|| false,
        )
        .unwrap();
        let mut zero = MutationGrant::for_test(0, std::time::Duration::from_secs(1));
        let mut ran = false;
        assert!(prepared
            .with_admitted(&mut zero, 2, &mut |_, _| {
                ran = true;
                Ok(())
            })
            .is_err());
        assert!(!ran);
        assert_eq!(epoch(&domain), original);
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        assert!(matches!(
            prepared.with_admitted(&mut grant, 2, &mut |_, _| {
                ran = true;
                Ok(())
            }),
            Err(AdmissionError::Invalid(locks::FixedControlProblem::Busy))
        ));
        assert!(!ran);
        assert_eq!(epoch(&domain), original);
        drop(old_reader);
        drop(prepared);
        std::fs::write(
            first_paths.root().join("foreign-neighbor"),
            b"foreign remains at its public address",
        )
        .unwrap();
        let neighbor = locks::path_identity(&first_paths.root().join("foreign-neighbor")).unwrap();
        let pointer = std::fs::read(layout.active_pointer_path()).unwrap();
        let mut stale = prepare_exact_generation(
            &target,
            &domain,
            &original,
            first_id.as_bytes(),
            0,
            &memory,
            &|| false,
        )
        .unwrap();
        std::fs::write(layout.active_pointer_path(), &pointer).unwrap();
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        assert!(stale
            .with_admitted(&mut grant, 2, &mut |_, _| panic!(
                "stale CURRENT must not admit"
            ))
            .is_err());
        assert_eq!(epoch(&domain), original);
        drop(stale);
        let before_memory = memory.used();
        let mut prepared = std::thread::Builder::new()
            .name("generation-preparation-proof".into())
            .stack_size(256 * 1024)
            .spawn({
                let target = target.clone();
                let domain = domain.clone();
                let original = original.clone();
                let memory = memory.clone();
                let first_id = first_id.clone();
                move || {
                    prepare_exact_generation(
                        &target,
                        &domain,
                        &original,
                        first_id.as_bytes(),
                        0,
                        &memory,
                        &|| false,
                    )
                    .unwrap()
                }
            })
            .unwrap()
            .join()
            .unwrap();
        let proof = prepared.inventory().encode_proof();
        let selected = *prepared.leaf();
        // The production candidate owns journal staging. This unit exercises the
        // admitted exact leaf operation and restoration-free resume boundary;
        // its cohort comes from the actual producer, never a fabricated record.
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        let advance = prepared
            .with_admitted(&mut grant, 3, &mut |mutation, grant| {
                grant.check_time()?;
                mutation.source_parent().rename_file_no_replace(
                    mutation.leaf().name(),
                    mutation.source_parent(),
                    b".test-detached-database",
                    mutation.leaf().payload,
                )?;
                grant.check_time()?;
                mutation.source_parent().sync_namespace()?;
                Ok(())
            })
            .unwrap();
        assert!(advance.after > advance.before);
        assert_eq!(prepared.source_epoch_counter(), original.counter());
        assert!(!first_paths.db_path().exists());
        assert_eq!(
            locks::path_identity(&first_paths.root().join(".test-detached-database")).unwrap(),
            selected.payload
        );
        drop(prepared);
        let current_epoch = epoch(&domain);
        let resumed = prepare_generation_resume(
            &target,
            &domain,
            &current_epoch,
            &proof,
            1,
            &memory,
            &|| false,
        )
        .expect("full recorded cohort resumes without reopening missing native DB");
        assert_eq!(resumed.source_epoch_counter(), original.counter());
        assert_eq!(resumed.leaf().name(), b"manifest.json");
        drop(resumed);
        assert_eq!(
            locks::path_identity(&first_paths.root().join("foreign-neighbor")).unwrap(),
            neighbor
        );
        assert_eq!(
            std::fs::read(first_paths.root().join("foreign-neighbor")).unwrap(),
            b"foreign remains at its public address"
        );
        assert_eq!(store.open_read().unwrap().generation_id, second_id);
        assert!(
            super::super::maintenance_ownership::DatabaseUseLease::acquire_shared(
                &first_paths.db_path()
            )
            .is_err(),
            "actual FINAL blocks late native birth"
        );
        // Dispose every recorded remaining payload through the same admitted
        // role. The independent foreign names are kept at their public address.
        let old_dir = RootedDirectory::open(
            first_paths.root(),
            locks::path_identity(first_paths.root()).unwrap(),
        )
        .unwrap();
        old_dir
            .unlink_file(b".test-detached-database", selected.payload)
            .unwrap();
        for (index, control_name) in NAMES.iter().enumerate().take(PAYLOADS).skip(1) {
            if old_dir.open_readonly_file(control_name).is_err() {
                continue;
            }
            let e = epoch(&domain);
            let mut leaf =
                prepare_generation_resume(&target, &domain, &e, &proof, index, &memory, &|| false)
                    .unwrap();
            let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
            leaf.with_admitted(&mut grant, 2, &mut |m, g| {
                g.check_time()?;
                m.source_parent()
                    .unlink_file(m.leaf().name(), m.leaf().payload)?;
                m.source_parent().sync_namespace()?;
                Ok(())
            })
            .unwrap();
        }
        let mut close =
            prepare_generation_root_removal(&target, &domain, &proof, &memory, &|| false).unwrap();
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        assert!(close
            .advance(&mut grant, 0, &mut |_, _| panic!(
                "foreign sibling must prevent completion"
            ))
            .is_err());
        assert!(first_paths.root().join(".generation-origin").exists());
        assert_eq!(
            locks::path_identity(&first_paths.root().join("foreign-neighbor")).unwrap(),
            neighbor
        );
        drop(close);
        old_dir.unlink_file(b"foreign-neighbor", neighbor).unwrap();
        drop(old_dir);
        let mut interrupted =
            prepare_generation_root_removal(&target, &domain, &proof, &memory, &|| false).unwrap();
        interrupted.scratch.fail_after_transport_step(1);
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        assert!(interrupted
            .advance(&mut grant, 0, &mut |_, _| panic!(
                "partial Intent must not claim root removal"
            ))
            .is_err());
        assert!(first_paths.root().join(".generation-origin").exists());
        // The complete durable Intent excludes only the named old root while
        // current native startup remains usable during required recovery.
        let current_paths = layout.generation(&second_id).unwrap();
        drop(
            super::super::maintenance_ownership::DatabaseUseLease::acquire_shared(
                &current_paths.db_path(),
            )
            .unwrap(),
        );
        drop(interrupted);
        let mut close =
            prepare_generation_root_removal(&target, &domain, &proof, &memory, &|| false).unwrap();
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        let mut actual_removed = false;
        assert!(close
            .advance(&mut grant, 0, &mut |outcome, _| {
                actual_removed = matches!(outcome, GenerationRootOutcome::Removed { .. });
                Err(admission_invalid())
            })
            .is_err());
        assert!(
            actual_removed,
            "injected completion fault occurs after actual atomic empty rmdir"
        );
        assert!(!first_paths.root().exists());
        drop(close);
        let mut resume =
            prepare_generation_root_removal(&target, &domain, &proof, &memory, &|| false).unwrap();
        let mut grant = MutationGrant::for_test(100, std::time::Duration::from_secs(2));
        let mut recovered_missing = false;
        assert!(resume
            .advance(&mut grant, 0, &mut |outcome, _| {
                recovered_missing = matches!(outcome, GenerationRootOutcome::RecoveredMissing);
                Err(admission_invalid())
            })
            .is_err());
        assert!(recovered_missing,"missing-root recovery remains distinct from actual rmdir; Removing is retained until a real durable completion token");
        drop(resume);
        assert!(
            super::super::maintenance_ownership::DatabaseUseLease::acquire_shared(
                &first_paths.db_path()
            )
            .is_err()
        );
        assert!(
            !first_paths.root().exists(),
            "late native use cannot recreate removed managed parent"
        );
        assert_eq!(store.open_read().unwrap().generation_id, second_id);
        assert!(memory.used() <= before_memory);
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_generation_fixed_records_reject_corruption_and_invalid_ids_without_authority() {
        let id = FileIdentity::from_fixed_words(1, 2);
        let scope = GenerationOriginScope {
            root: id,
            coordination: id,
            cohort: id,
            publication_binding: id,
            incarnation: [7; 32],
        };
        let (mut name, len) = fixed_id("producer-1").unwrap();
        let leaf = BoundLeaf {
            id,
            len: 1,
            hash: [3; 32],
            stamp_digest: [4; 32],
        };
        let mut leaves = [None; PAYLOADS];
        leaves[..4].fill(Some(leaf));
        let origin = Origin {
            generation: name,
            generation_len: len,
            scope,
            managed: id,
            writer: id,
            directory: id,
            receipt: id,
            operation: [8; 32],
            lease: id,
            family: id,
            resource: id,
            manifest_resource: id,
            native_lock: None,
            leaves,
        };
        let image = origin.encode();
        assert!(Origin::decode(&image).is_some());
        for offset in [0, 8, 16, 32, 160, 256, 304, 320, 352, 400, 2016, 2047] {
            let mut bad = image;
            bad[offset] ^= 0x80;
            assert!(Origin::decode(&bad).is_none());
        }
        name[0] = b'/';
        assert!(gen_name(&name, len).is_err());
        assert!(gen_name(&[0; 128], 255).is_err());
        assert!(fixed_id(&"x".repeat(129)).is_err());
    }
    #[test]
    fn dc_generation_preparation_cancellation_has_no_creating_effects() {
        let memory = CleanupMemory::new(128 * 1024).unwrap();
        let path = std::path::PathBuf::from("/does-not-exist-generation-cancel");
        let target = MaintenanceTarget {
            repository_root: path.clone(),
            state_root: path.clone(),
            storage_root: Some(path),
            direct_pair: None,
            config_path: None,
        };
        assert!(matches!(
            discover_generations(&target, &memory, &|| true, &mut |_| panic!(
                "cancelled discovery must not emit"
            )),
            Err(GenerationError::Cancelled)
        ));
    }
}

/// Final cohort operation. Payload leaves have already been retired through the
/// caller's durable cohort journal; receipt/lock/root removal is indivisible in
/// authority and restartable through the outside-generation closed intent.
pub(crate) struct PreparedGenerationRootRemoval {
    binding: MemoryBox<PreparedTargetBinding>,
    scoped: MemoryBox<PreparedScopedInvalidation>,
    layout: ManagedLayout,
    origin: Origin,
    final_record: Final,
    current: FixedFileStamp,
    proof_digest: [u8; 32],
    scratch: MemoryBox<gate::RemovalScratch>,
    _floor: MemoryReservation,
}
pub(crate) fn prepare_generation_root_removal(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    proof: &[u8; GENERATION_PROOF_BYTES],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<MemoryBox<PreparedGenerationRootRemoval>, GenerationError> {
    if cancel() {
        return Err(GenerationError::Cancelled);
    }
    if &proof[..8] != b"CGGPR001"
        || proof[16..32]
            .iter()
            .chain(proof[2592..4064].iter())
            .any(|v| *v != 0)
        || digest(&proof[..4064]) != proof[4064..]
    {
        return Err(GenerationError::Unknown);
    }
    let origin_image: &[u8; 2048] = proof[32..2080].try_into().unwrap();
    let final_image: &[u8; 512] = proof[2080..2592].try_into().unwrap();
    let origin = Origin::decode(origin_image).ok_or(GenerationError::Unknown)?;
    let final_record = Final::decode(final_image).ok_or(GenerationError::Unknown)?;
    if final_record.origin_digest != digest(origin_image)
        || final_record.origin != origin.receipt
        || final_record.directory != origin.directory
        || final_record.generation != origin.generation
    {
        return Err(GenerationError::Changed);
    }
    let (layout, mut binding, floor) = common(target, domain, memory)?;
    if !observe_scope_matches(origin, current_origin_scope(&layout, memory)?) {
        return Err(GenerationError::Changed);
    }
    let root = RootedDirectory::open_readonly_root(layout.storage_root(), origin.managed)
        .map_err(|e| GenerationError::Io(e.kind()))?;
    let current = read_current(&root).map_err(generation_error)?;
    if current.0 == origin.generation && current.1 == origin.generation_len {
        return Err(GenerationError::Changed);
    }
    let directory = match open_generation(&root, &origin.generation, origin.generation_len) {
        Ok(directory) => Some(directory),
        Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(generation_error(e)),
    };
    if let Some(directory) = directory {
        if directory.identity() != origin.directory {
            return Err(GenerationError::Changed);
        }
        // No payload reads or hashing: every known payload must already be absent.
        for name in &NAMES[..PAYLOADS] {
            match directory.open_readonly_file(name) {
                Ok(_) => return Err(GenerationError::Busy),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(GenerationError::Io(e.kind())),
            }
        }
    }
    if !binding.is_current().map_err(generation_admission)? || cancel() {
        return Err(GenerationError::Changed);
    }
    MemoryBox::new(
        PreparedGenerationRootRemoval {
            binding,
            scoped: boxed_generation_invalidation(domain, memory)?,
            layout,
            origin,
            final_record,
            current: current.2,
            proof_digest: digest(proof),
            scratch: gate::RemovalScratch::new(&root, origin, final_record, digest(proof), memory)
                .map_err(generation_error)?,
            _floor: floor,
        },
        memory,
    )
    .map_err(|_| GenerationError::ResourceLimit)
}
pub(crate) enum GenerationRootOutcome {
    Removed { _removed: gate::RemovedGeneration },
    RecoveredMissing,
}
impl PreparedGenerationRootRemoval {
    /// Completion-record replay observation; not a removed-generation token.
    /// The caller must also match its durable completion record to this proof.
    pub(crate) fn observed_completed(&self) -> Result<bool, GenerationError> {
        if !self.scratch.observed_committed_idle() {
            return Ok(false);
        }
        let root =
            RootedDirectory::open_readonly_root(self.layout.storage_root(), self.origin.managed)
                .map_err(|e| GenerationError::Io(e.kind()))?;
        match open_generation(&root, &self.origin.generation, self.origin.generation_len) {
            Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(generation_error(e)),
        }
    }
    /// Includes fixed admission, complete bounded control transport, lock/name
    /// validation, exact unlink, namespace sync and final atomic empty removal.
    pub(crate) fn effect_entries(&self) -> usize {
        100
    }
    pub(crate) fn advance(
        &mut self,
        grant: &mut MutationGrant,
        caller_callback_entries: usize,
        completed: &mut impl FnMut(
            &GenerationRootOutcome,
            &mut MutationGrant,
        ) -> Result<
            super::maintenance_candidate::GenerationCohortDone,
            AdmissionError,
        >,
    ) -> Result<GenerationRootOutcome, AdmissionError> {
        // Reserve the entire job before acquiring any exclusion. The nested
        // transport consumes the reserved allowance through this child grant.
        if grant.remaining_entries() < self.effect_entries() {
            grant.take_entries(self.effect_entries())?;
        }
        grant.check_time()?;
        grant.take_entries(6)?;
        let root = RootedDirectory::open(self.layout.storage_root(), self.origin.managed)?;
        let writer = root.open_readonly_file(b"writer.lock")?;
        if locks::file_identity(&writer).map_err(|_| admission_invalid())? != self.origin.writer {
            return Err(admission_invalid());
        }
        let _writer = FixedLockedFile::try_lock(writer, LockMode::Exclusive)?
            .ok_or(AdmissionError::Invalid(locks::FixedControlProblem::Busy))?;
        let gate = gate::admit_retirement_gate(&root, self.origin, &self.scratch, grant)
            .map_err(|_| admission_invalid())?;
        let directory =
            match open_generation(&root, &self.origin.generation, self.origin.generation_len) {
                Ok(directory) => Some(directory),
                Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err(admission_invalid()),
            };
        if directory
            .as_ref()
            .is_some_and(|directory| directory.identity() != self.origin.directory)
        {
            return Err(admission_invalid());
        }
        let current = self.current;
        let origin = self.origin;
        let final_record = self.final_record;
        let proof = self.proof_digest;
        let binding = &mut self.binding;
        let scratch = &mut self.scratch;
        let mut removed = None;
        maintenance_admission::with_scoped_reference_invalidation(
            &mut self.scoped,
            grant,
            48usize
                .checked_add(caller_callback_entries)
                .ok_or_else(admission_invalid)?,
            &mut |_, grant| {
                binding.recheck(grant)?;
                grant.check_time()?;
                current.recheck(&root.open_readonly_file(b"active.json")?)?;
                match (
                    directory.as_ref(),
                    open_generation(&root, &origin.generation, origin.generation_len),
                ) {
                    (Some(_), Ok(current)) if current.identity() == origin.directory => {}
                    (None, Err(NativeError::Io(e))) if e.kind() == std::io::ErrorKind::NotFound => {
                    }
                    _ => return Err(admission_invalid()),
                }
                Ok(())
            },
            &mut |_, grant| {
                removed = Some(if let Some(directory) = directory.as_ref() {
                    GenerationRootOutcome::Removed {
                        _removed: gate
                            .remove_closed_root(
                                &root,
                                directory,
                                origin,
                                final_record,
                                proof,
                                scratch,
                                grant,
                            )
                            .map_err(|_| admission_invalid())?,
                    }
                } else {
                    gate.complete_missing(&root, origin, final_record, proof, scratch, grant)
                        .map_err(|_| admission_invalid())?;
                    GenerationRootOutcome::RecoveredMissing
                });
                let completed = completed(removed.as_ref().unwrap(), grant)?;
                if !completed.matches(root.identity(), origin.directory, proof) {
                    return Err(admission_invalid());
                }
                gate.finish_removed(&root, origin, final_record, proof, scratch, grant)
                    .map_err(|_| admission_invalid())?;
                Ok(())
            },
        )?;
        removed.ok_or_else(admission_invalid)
    }
}

#[cfg(test)]
mod legacy_final_tests {
    use super::*;
    use crate::api::{execute_materialization, MaterializeOptions};
    #[test]
    fn dc_generation_actual_verified_successor_closes_unreceipted_predecessor_without_minting_origin(
    ) {
        let base = std::env::temp_dir().join(format!(
            "dc-legacy-final-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(base.join("src")).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        std::fs::write(base.join("src/lib.rs"), "pub fn old() {}\n").unwrap();
        let storage = base.join(".codebaseGraph/storage");
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
        let layout = ManagedLayout::new(&storage);
        let paths = layout.generation(&first).unwrap();
        // Model a real pre-receipt producer directory by removing only the
        // fixture's exact optional receipt. The successor and FINAL capability
        // are still constructed solely through the actual public API pipeline.
        let directory =
            RootedDirectory::open(paths.root(), locks::path_identity(paths.root()).unwrap())
                .unwrap();
        let origin = directory.open_readonly_file(ORIGIN_NAME).unwrap();
        directory
            .unlink_file(ORIGIN_NAME, locks::file_identity(&origin).unwrap())
            .unwrap();
        drop(origin);
        drop(directory);
        std::fs::write(base.join("src/lib.rs"), "pub fn successor() {let _v=7;}\n").unwrap();
        let (_, second) = execute_materialization(&options).unwrap();
        assert_ne!(second.active_generation.as_ref().unwrap(), &first);
        let root =
            RootedDirectory::open_readonly_root(&storage, locks::path_identity(&storage).unwrap())
                .unwrap();
        let directory = open_generation(
            &root,
            &fixed_id(&first).unwrap().0,
            fixed_id(&first).unwrap().1,
        )
        .unwrap();
        let (closed, bytes, _) = gate::read_legacy_final(&directory, root.identity()).unwrap();
        assert_eq!(closed.generation, fixed_id(&first).unwrap().0);
        assert_eq!(
            closed.database.stamp_digest,
            stamp(&directory.open_readonly_file(b"graph.ldb").unwrap())
                .unwrap()
                .binding_digest()
        );
        assert!(gate::LegacyFinal::decode(&bytes).is_some());
        assert!(!paths.root().join(".generation-origin").exists());
        assert!(!paths.root().join(".generation-final").exists());
        let memory = CleanupMemory::new(128 * 1024).unwrap();
        assert_eq!(
            observe_generation_disposition(&layout, &paths, &memory).unwrap(),
            GenerationDisposition::Final
        );
        assert!(
            super::super::maintenance_ownership::DatabaseUseLease::acquire_shared(&paths.db_path())
                .is_err()
        );
        assert!(super::super::managed::ManagedStore::new(layout)
            .open_read()
            .is_ok());
        std::fs::remove_dir_all(base).unwrap();
    }
}
