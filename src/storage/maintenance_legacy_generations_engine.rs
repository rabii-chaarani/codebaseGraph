//! Historical native cohorts. A sealed full association is retained verbatim
//! until actual source completion; no decoded bytes construct producer origin.
use super::inventory::MaintenanceTarget;
use super::locks::{
    self, FileIdentity, FixedControlError, FixedControlKind, FixedControlProblem, FixedFileStamp,
    FixedLockedFile, LockMode, RootedDirectory,
};
use super::maintenance_admission::{
    self as admission, PreparedControlUpdate, PreparedPrivateMetadata, PreparedTargetBinding,
};
use super::maintenance_budget::{CleanupMemory, MemoryBox, MutationGrant, ReservedBytes};
use super::maintenance_candidate::CandidateReport;
use super::maintenance_legacy_association::{
    LegacyAssociation, LegacyCacheCertificate, ReplayLegacyCertificates,
    SelectedBackupAssociationState,
};
use super::maintenance_legacy_bootstrap::{
    self as source, AcceptedLegacyRootCohort, LegacyClosedVersion, LegacyScope, LegacySourceOwner,
    PreparedLegacyScope,
};
use super::maintenance_legacy_guard::LegacyPhase;
use super::maintenance_legacy_stream::{
    self as stream, AssociationSourceProgress, CommittedLegacyStreamStep, LegacyFrameKind,
    LegacyStreamAction, LegacyStreamFrame, LegacyStreamHeader, LegacyStreamReplay,
    PreparedLegacyStreamStep,
};
use super::maintenance_ownership::{ReferenceDomain, ReferenceEpoch};
use super::maintenance_scheduler::LegacyQuiescenceConfirmation;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const MAGIC: &[u8; 8] = b"CGLGEN01";
const CONTROL: usize = 2048;
const INSPECT: u8 = 0;
const UNLINK: u8 = 1;
const ROOT: u8 = 2;
const FINISH: u8 = 3;
const RESET: u8 = 4;
const CACHE: u16 = 100;
const NAMES: [&[u8]; 14] = [
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
    b"retired.json",
    b".generation-legacy-final",
];
fn invalid() -> FixedControlError {
    FixedControlError::Invalid(FixedControlProblem::Changed)
}
fn boxed<T>(value: T, memory: &CleanupMemory) -> Result<MemoryBox<T>, FixedControlError> {
    MemoryBox::new(value, memory).map_err(FixedControlError::Resource)
}
fn prepare_transition(
    job: &mut Job,
    next: LegacyPhase,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    job.scope
        .prepare_transition(job.owner.as_ref().ok_or_else(invalid)?, next, memory)
}
fn pending() -> CandidateReport {
    CandidateReport {
        pending: true,
        ..CandidateReport::default()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyNativeScope {
    ManagedGeneration,
    DefaultDirect,
}
#[derive(Default)]
pub(crate) struct LegacyNativePreparationState {
    job: Option<MemoryBox<Job>>,
    association: SelectedBackupAssociationState,
}
impl LegacyNativePreparationState {
    pub(crate) fn clear(&mut self) {
        self.association.clear();
        drop(self.job.take());
    }
    pub(crate) fn has_parked_payloads(&self) -> bool {
        !self.association.needs_source_handles()
            || self.job.as_ref().is_some_and(|j| j.proof.is_some())
    }
}
struct Job {
    mode: LegacyNativeScope,
    scope: MemoryBox<PreparedLegacyScope>,
    owner: Option<LegacySourceOwner>,
    cohort: Option<MemoryBox<AcceptedLegacyRootCohort>>,
    automatic: Option<MemoryBox<super::maintenance_ownership::PreparedAutomaticDirect>>,
    member: PathBuf,
    storage: PathBuf,
    member_name: [u8; 128],
    member_len: u8,
    proof: Option<(LegacyAssociation, File, File)>,
    current: Option<FixedFileStamp>,
}
struct Control {
    mode: LegacyNativeScope,
    stage: u8,
    cache_started: bool,
    index: u16,
    source: LegacyClosedVersion,
    member: [u8; 128],
    member_len: u8,
    offset: u64,
    frame: [u8; 32],
    key: [u8; 32],
    stamp: Option<FixedFileStamp>,
    digest: [u8; 32],
    parent: Option<FileIdentity>,
    directory: Option<FileIdentity>,
    cursor: [u8; stream::CURSOR_BYTES],
}
impl Control {
    fn encode(&self) -> [u8; CONTROL] {
        let mut b = [0; CONTROL];
        b[..8].copy_from_slice(MAGIC);
        b[8] = self.mode as u8;
        b[9] = self.stage;
        b[10] = self.member_len;
        b[11] = u8::from(self.cache_started);
        b[12..14].copy_from_slice(&self.index.to_le_bytes());
        self.source
            .encode_checkpoint((&mut b[16..528]).try_into().unwrap());
        b[528..656].copy_from_slice(&self.member);
        b[656..664].copy_from_slice(&self.offset.to_le_bytes());
        b[664..696].copy_from_slice(&self.frame);
        b[696..728].copy_from_slice(&self.key);
        if let Some(stamp) = self.stamp {
            stamp.encode_checkpoint((&mut b[728..808]).try_into().unwrap());
        }
        b[808..840].copy_from_slice(&self.digest);
        for (at, id) in [(840, self.parent), (856, self.directory)] {
            if let Some(id) = id {
                let (a, c) = id.fixed_words();
                b[at..at + 8].copy_from_slice(&a.to_le_bytes());
                b[at + 8..at + 16].copy_from_slice(&c.to_le_bytes());
            }
        }
        b[872..872 + stream::CURSOR_BYTES].copy_from_slice(&self.cursor);
        let digest = Sha256::digest(&b[..CONTROL - 32]);
        b[CONTROL - 32..].copy_from_slice(&digest);
        b
    }
    fn decode(b: &[u8]) -> Result<Self, FixedControlError> {
        if b.len() != CONTROL
            || &b[..8] != MAGIC
            || Sha256::digest(&b[..CONTROL - 32])[..] != b[CONTROL - 32..]
            || b[8] > 1
            || b[9] > RESET
            || b[10] > 128
            || b[11] > 1
            || b[14..16]
                .iter()
                .chain(b[872 + stream::CURSOR_BYTES..CONTROL - 32].iter())
                .any(|v| *v != 0)
        {
            return Err(invalid());
        }
        let id = |at: usize| {
            let a = u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
            let c = u64::from_le_bytes(b[at + 8..at + 16].try_into().unwrap());
            if a == 0 && c == 0 {
                None
            } else {
                Some(FileIdentity::from_fixed_words(a, c))
            }
        };
        let result = Self {
            mode: if b[8] == 0 {
                LegacyNativeScope::ManagedGeneration
            } else {
                LegacyNativeScope::DefaultDirect
            },
            stage: b[9],
            cache_started: b[11] != 0,
            member_len: b[10],
            index: u16::from_le_bytes(b[12..14].try_into().unwrap()),
            source: LegacyClosedVersion::decode_checkpoint(b[16..528].try_into().unwrap())?,
            member: b[528..656].try_into().unwrap(),
            offset: u64::from_le_bytes(b[656..664].try_into().unwrap()),
            frame: b[664..696].try_into().unwrap(),
            key: b[696..728].try_into().unwrap(),
            stamp: if b[728..808].iter().all(|x| *x == 0) {
                None
            } else {
                Some(FixedFileStamp::decode_checkpoint(
                    b[728..808].try_into().unwrap(),
                )?)
            },
            digest: b[808..840].try_into().unwrap(),
            parent: id(840),
            directory: id(856),
            cursor: b[872..872 + stream::CURSOR_BYTES].try_into().unwrap(),
        };
        if result.member[usize::from(result.member_len)..]
            .iter()
            .any(|b| *b != 0)
            || result.mode == LegacyNativeScope::ManagedGeneration
                && !super::maintenance_generation_gate::component(
                    &result.member[..usize::from(result.member_len)],
                )
        {
            return Err(invalid());
        }
        Ok(result)
    }
}
pub(crate) fn is_native_payload(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}
pub(crate) fn native_payload_mode(bytes: &[u8]) -> Result<LegacyNativeScope, FixedControlError> {
    Control::decode(bytes).map(|control| control.mode)
}
enum Action {
    Recover,
    Enroll,
    Association(MemoryBox<PreparedLegacyStreamStep>),
    Retire,
    Custody(MemoryBox<PreparedLegacyStreamStep>),
    Metadata(MemoryBox<MetadataAction>),
    Leaf(MemoryBox<LeafAction>),
    Root(MemoryBox<RootAction>),
    Finish,
    Complete,
}
struct MetadataAction {
    metadata: MemoryBox<PreparedPrivateMetadata>,
    update: MemoryBox<PreparedControlUpdate>,
    reset: Option<u64>,
}
struct LeafAction {
    metadata: MemoryBox<PreparedPrivateMetadata>,
    update: MemoryBox<PreparedControlUpdate>,
    control: Control,
    name: Vec<u8>,
    file: Option<File>,
    invalidation: Option<MemoryBox<admission::PreparedScopedInvalidation>>,
}
struct RootAction {
    metadata: MemoryBox<PreparedPrivateMetadata>,
    update: MemoryBox<PreparedControlUpdate>,
    controls: [Option<ControlLeaf>; 5],
    held: [Option<FixedLockedFile>; 5],
}
pub(crate) struct LegacyNativeWork {
    job: Option<MemoryBox<Job>>,
    action: Action,
    memory: CleanupMemory,
    committed: Option<CommittedLegacyStreamStep>,
    report: CandidateReport,
    done: bool,
}
impl LegacyNativeWork {
    pub(crate) fn requires_legacy_confirmation(&self) -> bool {
        false
    }
}
fn work(
    job: MemoryBox<Job>,
    action: Action,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    boxed(
        LegacyNativeWork {
            job: Some(job),
            action,
            memory: memory.clone(),
            committed: None,
            report: pending(),
            done: false,
        },
        memory,
    )
    .map(Some)
}
#[inline(never)]
fn scope_box(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    mode: LegacyNativeScope,
    storage: &std::path::Path,
    member: &std::path::Path,
    artifact: &std::path::Path,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedLegacyScope>, FixedControlError> {
    let prepared = match mode {
        LegacyNativeScope::ManagedGeneration => source::prepare_legacy_scope(
            target,
            binding,
            LegacyScope::ManagedGeneration {
                storage_root: storage,
                generation_root: member,
            },
            false,
            memory,
        )?,
        LegacyNativeScope::DefaultDirect => source::prepare_legacy_scope(
            target,
            binding,
            LegacyScope::DefaultDirect {
                pair: &super::inventory::default_direct(target),
                artifact_root: artifact,
            },
            true,
            memory,
        )?,
    };
    Ok(prepared)
}
// Presence is an advisory early exit only. Existing bytes still go through
// the complete rooted cohort observer before they can supply any permission.
#[inline(never)]
fn root_cohort_control_exists(domain: &ReferenceDomain) -> Result<bool, FixedControlError> {
    let path = domain.artifact_root().parent().ok_or_else(invalid)?;
    let root = RootedDirectory::open_readonly_root(
        path,
        locks::path_identity(path).map_err(|_| invalid())?,
    )?;
    match root.open_readonly_file(super::maintenance_legacy_guard::ADMIN_STATE) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
#[inline(never)]
pub(crate) fn prepare_root_cohort_boxed(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<AcceptedLegacyRootCohort>>, FixedControlError> {
    source::observe_accepted_root_cohort_boxed(target, binding, memory)
}
#[inline(never)]
fn select_job(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    mode: LegacyNativeScope,
    metadata: &PreparedPrivateMetadata,
    cohort: Option<MemoryBox<AcceptedLegacyRootCohort>>,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<Job>>, FixedControlError> {
    if cohort.is_none() && mode == LegacyNativeScope::ManagedGeneration {
        return Ok(None);
    }
    let config =
        super::maintenance_projection::project_binding(target, memory).map_err(|_| invalid())?;
    let mut name = [0; 128];
    let mut len = 0usize;
    let active_control = metadata
        .payload()
        .filter(|p| is_native_payload(p))
        .map(Control::decode)
        .transpose()?;
    if active_control.as_ref().is_some_and(|c| c.mode != mode) {
        return Ok(None);
    }
    let active_source = if let Some(c) = active_control.as_ref() {
        name = c.member;
        len = usize::from(c.member_len);
        Some(c.source)
    } else {
        metadata
            .payload()
            .and_then(|p| LegacyStreamHeader::decode(p).ok())
            .and_then(|h| h.source_version())
    };
    let (storage, mut member, mut artifact) = match mode {
        LegacyNativeScope::ManagedGeneration => {
            if config.direct.is_some() {
                return Ok(None);
            }
            let storage = config.storage;
            let root = RootedDirectory::open_readonly_root(
                &storage,
                locks::path_identity(&storage).map_err(|_| invalid())?,
            )?;
            let current =
                super::maintenance_generation_gate::read_current(&root).map_err(|_| invalid())?;
            // Unsettled run journals can still protect their base/candidate.
            // Start only at an actually empty registry; terminal runs get their
            // own cursor turn before this source is closed.
            if active_source.is_none() && !managed_runs_empty(&root)? {
                return Ok(None);
            }
            let generations = root.open_directory_readonly(b"generations")?;
            if len == 0 {
                let mut scratch = [0; 4096];
                let mut count = 0;
                generations.visit_entries(&mut scratch, &mut |entry| {
                    count += 1;
                    if count > 100 {
                        return Err(std::io::ErrorKind::WouldBlock.into());
                    }
                    if !entry.starts_with(b"gen-")
                        || entry.len() > 128
                        || !super::maintenance_generation_gate::component(entry)
                    {
                        return Ok(());
                    }
                    if entry[4..] == current.0[..usize::from(current.1)] {
                        return Ok(());
                    }
                    let dir = match generations.open_directory_readonly(entry) {
                        Ok(d) => d,
                        Err(_) => return Ok(()),
                    };
                    if active_source.is_some_and(|s| {
                        s.root_identity() != root.identity()
                            || s.member_identity() != dir.identity()
                    }) {
                        return Ok(());
                    }
                    match dir.open_readonly_file(b".generation-origin") {
                        Ok(_) => return Ok(()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                    if len == 0 || entry < &name[..len] {
                        name.fill(0);
                        name[..entry.len()].copy_from_slice(entry);
                        len = entry.len();
                    }
                    Ok(())
                })?;
            }
            if len == 0 || name[4..len] == current.0[..usize::from(current.1)] {
                return Ok(None);
            }
            let member = storage
                .join("generations")
                .join(std::str::from_utf8(&name[..len]).map_err(|_| invalid())?);
            let artifact = storage.join("artifacts");
            (storage, member, artifact)
        }
        LegacyNativeScope::DefaultDirect => {
            // The configured Direct graph is current; this role only handles an
            // old default pair after a genuine Managed administrative cohort.
            if config.direct.is_some() {
                return Ok(None);
            }
            let state_id = locks::path_identity(&target.state_root).map_err(|_| invalid())?;
            if active_source.is_some_and(|s| s.root_identity() != state_id) {
                return Ok(None);
            }
            let pair = super::inventory::default_direct(target);
            if active_source.is_none() && (!pair.database.is_file() || !pair.manifest.is_file()) {
                return Ok(None);
            }
            let conventional = target.state_root.join("artifacts");
            let keyed = super::layout::DirectLayout::new(&pair.database, &pair.manifest)
                .artifact_root_path();
            let artifact = if cohort.is_some() && conventional.is_dir() {
                conventional
            } else {
                keyed
            };
            (
                target.state_root.clone(),
                target.state_root.clone(),
                artifact,
            )
        }
    };
    let mut scope = scope_box(target, binding, mode, &storage, &member, &artifact, memory)?;
    let mut recovering = scope.refresh_publication_recovery(memory, None)?;
    let mut version = if recovering {
        None
    } else {
        source::observe_legacy_source_version(&mut scope, memory)?
    };
    if mode == LegacyNativeScope::DefaultDirect {
        if let Some(version) = version.or(active_source) {
            if locks::path_identity(&artifact).ok() != Some(version.cache_identity()) {
                let pair = super::inventory::default_direct(target);
                let keyed = super::layout::DirectLayout::new(&pair.database, &pair.manifest)
                    .artifact_root_path();
                let conventional = target.state_root.join("artifacts");
                if let Some(actual) = [keyed, conventional]
                    .into_iter()
                    .find(|p| locks::path_identity(p).ok() == Some(version.cache_identity()))
                {
                    artifact = actual;
                    scope = scope_box(target, binding, mode, &storage, &member, &artifact, memory)?;
                    recovering = scope.refresh_publication_recovery(memory, None)?;
                }
            }
        }
    }

    if mode == LegacyNativeScope::ManagedGeneration
        && active_source.is_none()
        && version.is_some_and(|v| v.phase() != LegacyPhase::Removed)
    {
        if let Some((actual, actual_len)) = source::observe_legacy_member_name(&mut scope, memory)?
        {
            if actual != name || usize::from(actual_len) != len {
                name = actual;
                len = usize::from(actual_len);
                member = storage
                    .join("generations")
                    .join(std::str::from_utf8(&name[..len]).map_err(|_| invalid())?);
                scope = scope_box(target, binding, mode, &storage, &member, &artifact, memory)?;
                version = source::observe_legacy_source_version(&mut scope, memory)?;
            }
        }
    }
    if active_source.is_none()
        && mode == LegacyNativeScope::ManagedGeneration
        && version.is_some_and(|v| v.phase() == LegacyPhase::Removed)
    {
        version = None;
    }
    if mode == LegacyNativeScope::DefaultDirect
        && version.is_none()
        && !recovering
        && (!scope.database_path().is_file() || !scope.manifest_path().is_file())
    {
        return Ok(None);
    }
    if !recovering && active_source.is_some_and(|s| version.is_none_or(|v| !v.same_job_source(&s)))
    {
        return Ok(None);
    }
    let owner = version
        .map(|v| source::reopen_legacy_source_owner(&mut scope, &v, memory))
        .transpose()?;
    let automatic =
        if mode == LegacyNativeScope::DefaultDirect && cohort.is_none() && owner.is_none() {
            let pair = super::inventory::default_direct(target);
            let layout = super::layout::DirectLayout::new(&pair.database, &pair.manifest);
            if artifact != layout.artifact_root_path() {
                return Ok(None);
            }
            let Some(cap) =
                super::maintenance_ownership::prepare_automatic_direct(&layout, memory)?
            else {
                return Ok(None);
            };
            Some(cap)
        } else {
            None
        };

    if owner.is_none() && !recovering {
        if let Some(cap) = cohort.as_ref() {
            scope.prepare_enrollment_in_cohort(cap, memory)?;
        } else if automatic.is_some() {
            scope.prepare_enrollment_automatic(memory)?;
        }
    }
    boxed(
        Job {
            mode,
            scope,
            owner,
            cohort,
            automatic,
            member,
            storage,
            member_name: name,
            member_len: len as u8,
            proof: None,
            current: None,
        },
        memory,
    )
    .map(Some)
}
/// Preparation and all retained payload disposal run on the joined worker,
/// outside the execution permit. Each returned work owns at most one payload
/// validation, or the association job's actual native pair.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
pub(crate) fn prepare_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    mode: LegacyNativeScope,
    state: &mut LegacyNativePreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    if cancel() {
        return Err(invalid());
    }
    let origin =
        super::maintenance_legacy_engine::staging_origin(target, domain, expected, memory)?;
    let metadata = super::maintenance_legacy_engine::staging_metadata(target, &origin, memory)?;
    let job = if let Some(job) = state.job.take() {
        if job.mode != mode {
            state.job = Some(job);
            return Ok(None);
        }
        job
    } else {
        let cohort = if root_cohort_control_exists(domain)? {
            prepare_root_cohort_boxed(target, binding, memory)?
        } else {
            None
        };
        let Some(job) = select_job(target, binding, mode, &metadata, cohort, memory)? else {
            return Ok(None);
        };
        job
    };
    prepare_job(
        target, domain, expected, state, job, metadata, memory, cancel,
    )
}
#[inline(never)]
// Keep prepared scope, metadata and immutable source proofs separate across the bounded off-permit stage.
#[allow(clippy::too_many_arguments)]
fn prepare_job(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut LegacyNativePreparationState,
    mut job: MemoryBox<Job>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    let recovering = {
        let job = &mut *job;
        job.scope
            .refresh_publication_recovery(memory, job.owner.as_ref())?
    };
    if recovering {
        return work(job, Action::Recover, memory);
    }
    if job.owner.is_none() {
        if job.mode == LegacyNativeScope::ManagedGeneration {
            let root = RootedDirectory::open_readonly_root(
                &job.storage,
                locks::path_identity(&job.storage).map_err(|_| invalid())?,
            )?;
            let current =
                super::maintenance_generation_gate::read_current(&root).map_err(|_| invalid())?;
            if !managed_runs_empty(&root)? {
                state.job = Some(job);
                return Ok(None);
            }
            if job.member_name[4..usize::from(job.member_len)]
                == current.0[..usize::from(current.1)]
            {
                return Err(invalid());
            }
            job.current = Some(current.2);
        }
        job.scope.refresh_enrollment_preparation(memory)?;
        return work(job, Action::Enroll, memory);
    }
    refresh_current(&mut job)?;
    let version = job.owner.as_ref().unwrap().closed_version();
    if let Some(recovery) = admission::prepare_metadata_recovery(&metadata, memory)? {
        if !metadata.payload().is_some_and(is_native_payload) {
            state.job = Some(job);
            return Ok(None);
        }
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update: boxed(recovery, memory)?,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    }
    if metadata.payload().is_some_and(is_native_payload) {
        return prepare_control(target, domain, job, metadata, memory, cancel);
    }
    if version.phase() == LegacyPhase::Removed && job.mode == LegacyNativeScope::DefaultDirect {
        return work(job, Action::Complete, memory);
    }
    if version.phase() == LegacyPhase::Preparing {
        drop(metadata);
        return prepare_association_job(target, domain, expected, state, job, memory, cancel);
    }
    prepare_retiring_job(target, job, metadata, version, memory)
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn prepare_association_job(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut LegacyNativePreparationState,
    mut job: MemoryBox<Job>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    let version = job.owner.as_ref().ok_or_else(invalid)?.closed_version();
    if job.proof.is_some() {
        prepare_transition(&mut job, LegacyPhase::Retiring, memory)?;
        return work(job, Action::Retire, memory);
    }
    let progress = if state.association.needs_source_handles() {
        let root = RootedDirectory::open_readonly_root(
            job.scope.artifact_root(),
            version.cache_identity(),
        )?;
        stream::prepare_association_source(
            target,
            domain,
            expected,
            File::open(job.scope.database_path())?,
            File::open(job.scope.manifest_path())?,
            root,
            job.scope.artifact_root(),
            &target.repository_root,
            job.owner.as_ref(),
            None,
            &mut state.association,
            memory,
            cancel,
        )?
    } else {
        stream::prepare_parked_association_source(
            target,
            domain,
            expected,
            job.scope.artifact_root(),
            &target.repository_root,
            job.owner.as_ref(),
            None,
            &mut state.association,
            memory,
            cancel,
        )?
    };
    match progress {
        AssociationSourceProgress::Pending => {
            state.job = Some(job);
            Ok(None)
        }
        AssociationSourceProgress::Step(step) => work(job, Action::Association(step), memory),
        AssociationSourceProgress::Complete {
            association,
            database,
            manifest,
        } => {
            job.proof = Some((association, database, manifest));
            prepare_transition(&mut job, LegacyPhase::Retiring, memory)?;
            work(job, Action::Retire, memory)
        }
    }
}

#[inline(never)]
fn prepare_retiring_job(
    target: &MaintenanceTarget,
    job: MemoryBox<Job>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    version: LegacyClosedVersion,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    let header = metadata
        .payload()
        .and_then(|p| LegacyStreamHeader::decode(p).ok());
    let header = header.ok_or_else(invalid)?;
    let original = if header.done() || header.pending() {
        header.before_native_cohort()?
    } else {
        let replay = LegacyStreamReplay::new(
            &metadata,
            &header.source_version().ok_or_else(invalid)?,
            header.context_digest(),
            memory,
        )?;
        let cp = replay.checkpoint()?;
        if cp.as_slice().len() != 3968 || &cp.as_slice()[..8] != b"CGLGNCP1" {
            return Err(invalid());
        }
        let original = LegacyStreamHeader::decode(&cp.as_slice()[8..2056])?;
        validate_native_payload(cp.as_slice(), &job, version, &original)?;
        return install_control(job, metadata, original, header.chain_digest(), memory);
    };
    if !version.same_job_source(&original.source_version().ok_or_else(invalid)?) {
        return Err(invalid());
    }
    let _replay = LegacyStreamReplay::from_header(&metadata, original, memory)?;
    let payload = native_payload(&job, version, original, memory)?;
    let frame = LegacyStreamFrame::new(
        LegacyFrameKind::NativeCohort,
        original.context_digest(),
        original.sequence(),
        original.chain_digest(),
        payload.as_slice(),
        memory,
    )?;
    let action = if header.pending() {
        LegacyStreamAction::FinishAppend(frame)
    } else {
        LegacyStreamAction::BeginAppend(frame)
    };
    let step =
        stream::prepare_legacy_stream_step(target, &metadata.origin_scope(), action, memory)?;
    work(job, Action::Custody(step), memory)
}

#[inline(never)]
fn native_payload(
    job: &Job,
    version: LegacyClosedVersion,
    original: LegacyStreamHeader,
    memory: &CleanupMemory,
) -> Result<ReservedBytes, FixedControlError> {
    let mut payload = memory
        .reserve(3968)
        .map_err(FixedControlError::Resource)?
        .bytes(3968)
        .map_err(FixedControlError::Resource)?;
    let b = payload.as_mut_slice();
    b.fill(0);
    b[..8].copy_from_slice(b"CGLGNCP1");
    original.encode((&mut b[8..2056]).try_into().unwrap());
    version.encode_checkpoint((&mut b[2056..2568]).try_into().unwrap());
    b[2568] = job.mode as u8;
    b[2569] = job.member_len;
    b[2576..2704].copy_from_slice(&job.member_name);
    let digest = Sha256::digest(&b[..3936]);
    b[3936..].copy_from_slice(&digest);
    Ok(payload)
}
fn validate_native_payload(
    b: &[u8],
    job: &Job,
    version: LegacyClosedVersion,
    original: &LegacyStreamHeader,
) -> Result<(), FixedControlError> {
    if b.len() != 3968
        || &b[..8] != b"CGLGNCP1"
        || b[2568] != job.mode as u8
        || b[2569] != job.member_len
        || b[2576..2704] != job.member_name
        || Sha256::digest(&b[..3936])[..] != b[3936..]
        || !version.same_job_source(&LegacyClosedVersion::decode_checkpoint(
            b[2056..2568].try_into().unwrap(),
        )?)
        || !version.same_job_source(&original.source_version().ok_or_else(invalid)?)
    {
        return Err(invalid());
    }
    Ok(())
}
fn control_update(
    metadata: &PreparedPrivateMetadata,
    control: &Control,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedControlUpdate>, FixedControlError> {
    boxed(
        admission::prepare_metadata_control(
            &metadata.origin_scope(),
            metadata.committed().ok_or_else(invalid)?,
            &control.encode(),
            memory,
        )?,
        memory,
    )
}
fn completed_cursor(
    job: &Job,
    original: &LegacyStreamHeader,
    installation: [u8; 32],
) -> Result<[u8; stream::CURSOR_BYTES], FixedControlError> {
    let mut cursor = super::maintenance::MaintenanceCursor::decode(original.cursor(), installation)
        .map_err(|_| invalid())?;
    let mut address = job.member_name;
    let category = if job.mode == LegacyNativeScope::ManagedGeneration {
        super::maintenance::WorkCategory::LegacyNative
    } else {
        let name = job
            .scope
            .database_path()
            .file_name()
            .ok_or_else(invalid)?
            .as_encoded_bytes();
        if name.len() > 128 {
            return Err(invalid());
        }
        address.fill(0);
        address[..name.len()].copy_from_slice(name);
        super::maintenance::WorkCategory::DefaultDirect
    };
    cursor.inspected(category, super::maintenance::CursorAddress(address));
    Ok(cursor.encode())
}
fn install_control(
    job: MemoryBox<Job>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    original: LegacyStreamHeader,
    frame: [u8; 32],
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    let control = Control {
        mode: job.mode,
        stage: INSPECT,
        cache_started: false,
        index: 0,
        source: job.owner.as_ref().unwrap().closed_version(),
        member: job.member_name,
        member_len: job.member_len,
        offset: original.committed_end(),
        frame,
        key: [0; 32],
        stamp: None,
        digest: [0; 32],
        parent: None,
        directory: None,
        cursor: completed_cursor(&job, &original, metadata.origin_scope().incarnation)?,
    };
    let update = control_update(&metadata, &control, memory)?;
    work(
        job,
        Action::Metadata(boxed(
            MetadataAction {
                metadata,
                update,
                reset: None,
            },
            memory,
        )?),
        memory,
    )
}
/// The only accepted tail is the exact single committed NativeCohort frame.
/// Extra bytes are never truncated, including on apparently completed jobs.
#[inline(never)]
fn read_custody(
    metadata: &PreparedPrivateMetadata,
    control: &Control,
    job: &Job,
    memory: &CleanupMemory,
) -> Result<LegacyStreamHeader, FixedControlError> {
    if metadata.stream_len() != control.offset.checked_add(4096).ok_or_else(invalid)? {
        return Err(invalid());
    }
    let mut bytes = memory
        .reserve(4096)
        .map_err(FixedControlError::Resource)?
        .bytes(4096)
        .map_err(FixedControlError::Resource)?;
    metadata.read_stream_exact(control.offset, bytes.as_mut_slice())?;
    let b = bytes.as_slice();
    if &b[..8] != b"CGLGFR01"
        || b[8] != 7
        || b[9..16].iter().any(|x| *x != 0)
        || u64::from_le_bytes(b[56..64].try_into().unwrap()) != 3968
        || Sha256::digest(&b[..4064])[..] != b[4064..]
        || b[4064..] != control.frame
    {
        return Err(invalid());
    }
    let original = LegacyStreamHeader::decode(&b[104..2152])?;
    validate_native_payload(
        &b[96..4064],
        job,
        job.owner.as_ref().unwrap().closed_version(),
        &original,
    )?;
    if original.committed_end() != control.offset
        || b[16..48] != original.context_digest()
        || b[64..96] != original.chain_digest()
        || u64::from_le_bytes(b[48..56].try_into().unwrap()) != original.sequence()
    {
        return Err(invalid());
    }
    LegacyStreamReplay::from_header(metadata, original, memory)?;
    Ok(original)
}
fn leaf_name(job: &Job, index: u16) -> Result<Vec<u8>, FixedControlError> {
    let index = usize::from(index);
    if index >= NAMES.len() {
        return Err(invalid());
    }
    if job.mode == LegacyNativeScope::ManagedGeneration || index == 1 || index >= 12 {
        return Ok(NAMES[index].to_vec());
    }
    let base = job
        .scope
        .database_path()
        .file_name()
        .ok_or_else(invalid)?
        .as_encoded_bytes();
    let mut name = base.to_vec();
    if index != 0 {
        name.extend_from_slice(&NAMES[index][b"graph.ldb".len()..]);
    }
    Ok(name)
}
fn optional_file(root: &RootedDirectory, name: &[u8]) -> Result<Option<File>, FixedControlError> {
    match root.open_readonly_file(name) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn managed_runs_empty(root: &RootedDirectory) -> Result<bool, FixedControlError> {
    let runs = match root.open_directory_readonly(b"runs") {
        Ok(runs) => runs,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let mut empty = true;
    let mut scratch = [0; 4096];
    match runs.visit_entries(&mut scratch, &mut |_| {
        empty = false;
        Err(std::io::ErrorKind::WouldBlock.into())
    }) {
        Ok(()) => Ok(empty),
        Err(error) if !empty && error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn refresh_current(job: &mut Job) -> Result<(), FixedControlError> {
    if job.mode == LegacyNativeScope::DefaultDirect {
        return Ok(());
    }
    let root = RootedDirectory::open_readonly_root(
        &job.storage,
        job.owner.as_ref().unwrap().closed_version().root_identity(),
    )?;
    let current = super::maintenance_generation_gate::read_current(&root).map_err(|_| invalid())?;
    if job.member_name[4..usize::from(job.member_len)] == current.0[..usize::from(current.1)] {
        return Err(invalid());
    }
    job.current = Some(current.2);
    Ok(())
}

fn lease(
    member: &RootedDirectory,
    mode: LegacyNativeScope,
) -> Result<Option<FixedLockedFile>, FixedControlError> {
    if mode == LegacyNativeScope::DefaultDirect {
        return Ok(None);
    }
    FixedLockedFile::try_lock(member.open_existing(b"lease.lock")?, LockMode::Exclusive)?
        .map(Some)
        .ok_or(FixedControlError::Invalid(FixedControlProblem::Busy))
}
#[inline(never)]
fn prepare_control(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    mut job: MemoryBox<Job>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    let mut control = Control::decode(metadata.payload().ok_or_else(invalid)?)?;
    let version = job.owner.as_ref().unwrap().closed_version();
    if control.mode != job.mode
        || !version.same_job_source(&control.source)
        || control.member != job.member_name
        || control.member_len != job.member_len
    {
        return Err(invalid());
    }
    let already_reset = control.stage == RESET
        && version.phase() == LegacyPhase::Removed
        && metadata.stream_len() == 0;
    let original = if already_reset {
        None
    } else {
        Some(read_custody(&metadata, &control, &job, memory)?)
    };
    if let Some(original) = original.as_ref() {
        if completed_cursor(&job, original, metadata.origin_scope().incarnation)? != control.cursor
        {
            return Err(invalid());
        }
    }
    super::maintenance::MaintenanceCursor::decode(
        &control.cursor,
        metadata.origin_scope().incarnation,
    )
    .map_err(|_| invalid())?;
    refresh_current(&mut job)?;
    if control.stage == RESET {
        if version.phase() != LegacyPhase::Removed {
            return Err(invalid());
        }
        let cursor = &control.cursor;
        let update = boxed(
            admission::prepare_metadata_control(
                &metadata.origin_scope(),
                metadata.committed().ok_or_else(invalid)?,
                cursor,
                memory,
            )?,
            memory,
        )?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: Some(if already_reset {
                        0
                    } else {
                        control.offset + 4096
                    }),
                },
                memory,
            )?),
            memory,
        );
    }
    if version.phase() == LegacyPhase::Removed {
        control.stage = RESET;
        let update = control_update(&metadata, &control, memory)?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    }
    if version.phase() != LegacyPhase::Retiring {
        return Err(invalid());
    }
    if control.stage == FINISH {
        prepare_transition(&mut job, LegacyPhase::Removed, memory)?;
        return work(job, Action::Finish, memory);
    }
    if control.stage == ROOT {
        return prepare_root(job, metadata, control, memory);
    }
    if control.index == CACHE {
        return prepare_cache(
            target,
            domain,
            job,
            metadata,
            control,
            original.ok_or_else(invalid)?,
            memory,
            cancel,
        );
    }
    if usize::from(control.index) >= NAMES.len() {
        control.stage = if job.mode == LegacyNativeScope::ManagedGeneration {
            ROOT
        } else {
            INSPECT
        };
        if job.mode == LegacyNativeScope::DefaultDirect {
            control.index = CACHE;
            control.key = [0; 32];
            control.cache_started = false;
        }
        let update = control_update(&metadata, &control, memory)?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    }
    if job.mode == LegacyNativeScope::DefaultDirect && matches!(control.index, 2 | 3 | 12 | 13) {
        control.index += 1;
        let update = control_update(&metadata, &control, memory)?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    }
    let member = RootedDirectory::open_readonly_root(&job.member, version.member_identity())?;
    let name = leaf_name(&job, control.index)?;
    let mut file = optional_file(&member, &name)?;
    if control.stage == INSPECT {
        if let Some(file) = file.as_mut() {
            let stamp = FixedFileStamp::capture(file, FixedControlKind::File)?;
            recognize_leaf(&job, control.index, file, stamp, memory)?;
            control.digest = hash(file, stamp, memory, cancel)?;
            control.stamp = Some(stamp);
            control.stage = UNLINK;
        } else {
            control.index += 1;
        }
        let update = control_update(&metadata, &control, memory)?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    }
    if let Some(file) = file.as_mut() {
        let stamp = control.stamp.ok_or_else(invalid)?;
        stamp.recheck(file)?;
        if hash(file, stamp, memory, cancel)? != control.digest {
            return Err(invalid());
        }
    }
    let mut next = Control::decode(&control.encode())?;
    next.stage = INSPECT;
    next.index += 1;
    next.stamp = None;
    next.digest = [0; 32];
    let update = control_update(&metadata, &next, memory)?;
    let invalidation = if job.mode == LegacyNativeScope::ManagedGeneration {
        Some(boxed(
            admission::prepare_scoped_invalidation(domain, memory)?,
            memory,
        )?)
    } else {
        None
    };
    work(
        job,
        Action::Leaf(boxed(
            LeafAction {
                metadata,
                update,
                control,
                name,
                file,
                invalidation,
            },
            memory,
        )?),
        memory,
    )
}
#[inline(never)]
fn hash(
    file: &mut File,
    stamp: FixedFileStamp,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<[u8; 32], FixedControlError> {
    let mut bytes = memory
        .reserve(4096)
        .map_err(FixedControlError::Resource)?
        .bytes(4096)
        .map_err(FixedControlError::Resource)?;
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut remaining = stamp.len();
    while remaining != 0 {
        if cancel() {
            return Err(invalid());
        }
        let size = remaining.min(4096) as usize;
        file.read_exact(&mut bytes.as_mut_slice()[..size])?;
        digest.update(&bytes.as_slice()[..size]);
        remaining -= size as u64;
    }
    stamp.recheck(file)?;
    Ok(digest.finalize().into())
}
/// Native DB and manifest membership comes from the full opaque association.
/// Ancillary files additionally require the actual producer's bounded format.
#[inline(never)]
fn recognize_leaf(
    job: &Job,
    index: u16,
    file: &mut File,
    stamp: FixedFileStamp,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    match index {
        0 => {
            if Some(stamp)
                != job
                    .owner
                    .as_ref()
                    .unwrap()
                    .closed_version()
                    .database_stamp()
            {
                return Err(invalid());
            }
        }
        1 => {
            if Some(stamp)
                != job
                    .owner
                    .as_ref()
                    .unwrap()
                    .closed_version()
                    .manifest_stamp()
            {
                return Err(invalid());
            }
        }
        2 | 6 | 12 => {
            if stamp.len() > 16384 {
                return Err(invalid());
            }
            let mut bytes = memory
                .reserve(stamp.len() as usize + 32768)
                .map_err(FixedControlError::Resource)?
                .bytes(stamp.len() as usize)
                .map_err(FixedControlError::Resource)?;
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(bytes.as_mut_slice())?;
            if index == 2 {
                let meta: super::managed::GenerationMetadata =
                    serde_json::from_slice(bytes.as_slice()).map_err(|_| invalid())?;
                if !matches!(meta.schema_version, 1 | 2)
                    || meta.generation_id.as_bytes()
                        != &job.member_name[4..usize::from(job.member_len)]
                {
                    return Err(invalid());
                }
            }
            if index == 6 {
                #[derive(serde::Deserialize)]
                struct Meta {
                    backend: String,
                    format_version: u64,
                }
                let meta: Meta = serde_json::from_slice(bytes.as_slice()).map_err(|_| invalid())?;
                if meta.backend != "disk_bm25_v1" || meta.format_version != 1 {
                    return Err(invalid());
                }
            }
            if index == 12 {
                let _: super::managed::GenerationRetirement =
                    serde_json::from_slice(bytes.as_slice()).map_err(|_| invalid())?;
            }
        }
        3 => {
            let mut bytes = [0; 64];
            if stamp.len() == 0 || stamp.len() > 64 {
                return Err(invalid());
            }
            file.seek(SeekFrom::Start(0))?;
            file.read_exact(&mut bytes[..stamp.len() as usize])?;
            let b = &bytes[..stamp.len() as usize];
            if b != b"prepared\n"
                && (!b.ends_with(b"\n") || !b[..b.len() - 1].iter().all(u8::is_ascii_digit))
            {
                return Err(invalid());
            }
        }
        // A non-empty WAL/tmp needs native recovery and is intentionally retained.
        4 | 5 => {
            if stamp.len() != 0 {
                return Err(invalid());
            }
        }
        7..=11 => recognize_search(index, file, stamp, memory)?,
        13 => {
            let directory = RootedDirectory::open_readonly_root(
                &job.member,
                job.owner
                    .as_ref()
                    .unwrap()
                    .closed_version()
                    .member_identity(),
            )?;
            let (_, _, actual) = super::maintenance_generation_gate::read_legacy_final(
                &directory,
                job.owner.as_ref().unwrap().closed_version().root_identity(),
            )
            .map_err(|_| invalid())?;
            if actual != stamp {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    stamp.recheck(file)?;
    Ok(())
}
#[inline(never)]
fn recognize_search(
    index: u16,
    file: &mut File,
    stamp: FixedFileStamp,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    let magic = match index {
        7 => b"CBGLX001",
        8 => b"CBGPS001",
        9 => b"CBGDL001",
        10 => b"CBGDC001",
        11 => b"CBGDO001",
        _ => return Err(invalid()),
    };
    file.seek(SeekFrom::Start(0))?;
    let mut actual = [0; 8];
    file.read_exact(&mut actual)?;
    if &actual != magic {
        return Err(invalid());
    }
    let mut remaining = stamp.len().checked_sub(8).ok_or_else(invalid)?;
    if index == 8 || index == 9 {
        if remaining % 12 != 0 {
            return Err(invalid());
        }
        let mut record = [0; 12];
        while remaining != 0 {
            file.read_exact(&mut record)?;
            remaining -= 12;
            if index == 8 {
                if u32::from_le_bytes(record[8..12].try_into().unwrap()) == 0 {
                    return Err(invalid());
                }
            } else if record[8] > 1 || record[9..12].iter().any(|v| *v != 0) {
                return Err(invalid());
            }
        }
        return Ok(());
    }
    if index == 11 {
        if remaining % 8 != 0 {
            return Err(invalid());
        }
        let mut prior = None;
        let mut record = [0; 8];
        while remaining != 0 {
            file.read_exact(&mut record)?;
            remaining -= 8;
            let offset = u64::from_le_bytes(record);
            if prior.is_none() && offset != 8 || prior.is_some_and(|old| offset <= old) {
                return Err(invalid());
            }
            prior = Some(offset);
        }
        return Ok(());
    }
    let mut previous = memory
        .reserve(65536)
        .map_err(FixedControlError::Resource)?
        .bytes(65536)
        .map_err(FixedControlError::Resource)?;
    let mut previous_len = 0usize;
    let mut next_postings = 8u64;
    let mut scratch = memory
        .reserve(65536)
        .map_err(FixedControlError::Resource)?
        .bytes(65536)
        .map_err(FixedControlError::Resource)?;
    while remaining != 0 {
        if remaining < 4 {
            return Err(invalid());
        }
        let mut size = [0; 4];
        file.read_exact(&mut size)?;
        remaining -= 4;
        let len = u32::from_le_bytes(size) as usize;
        if len == 0 || len > 65536 || len as u64 > remaining {
            return Err(invalid());
        }
        file.read_exact(&mut scratch.as_mut_slice()[..len])?;
        remaining -= len as u64;
        if index == 7 {
            std::str::from_utf8(&scratch.as_slice()[..len]).map_err(|_| invalid())?;
            if previous_len != 0 && previous.as_slice()[..previous_len] >= scratch.as_slice()[..len]
            {
                return Err(invalid());
            }
            previous.as_mut_slice()[..len].copy_from_slice(&scratch.as_slice()[..len]);
            previous_len = len;
            if remaining < 12 {
                return Err(invalid());
            }
            let mut tail = [0; 12];
            file.read_exact(&mut tail)?;
            remaining -= 12;
            let offset = u64::from_le_bytes(tail[..8].try_into().unwrap());
            let frequency = u32::from_le_bytes(tail[8..].try_into().unwrap());
            if offset != next_postings || frequency == 0 {
                return Err(invalid());
            }
            next_postings = next_postings
                .checked_add(u64::from(frequency) * 12)
                .ok_or_else(invalid)?;
        } else {
            #[derive(serde::Deserialize)]
            struct Document {
                id: String,
                node_type: String,
                index_order: u64,
                layer: String,
            }
            let _floor = memory
                .reserve(131072)
                .map_err(FixedControlError::Resource)?;
            let doc: Document =
                serde_json::from_slice(&scratch.as_slice()[..len]).map_err(|_| invalid())?;
            let _ = (&doc.id, &doc.node_type, doc.index_order);
            if !matches!(doc.layer.as_str(), "semantic" | "syntax") {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
fn key_name(key: [u8; 32]) -> [u8; 64] {
    let mut out = [0; 64];
    for (i, b) in key.iter().enumerate() {
        out[i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
        out[i * 2 + 1] = b"0123456789abcdef"[(b & 15) as usize];
    }
    out
}
fn cache_directory(
    job: &Job,
    control: &Control,
) -> Result<(RootedDirectory, RootedDirectory), FixedControlError> {
    let root = RootedDirectory::open_readonly_root(
        job.scope.artifact_root(),
        job.owner
            .as_ref()
            .unwrap()
            .closed_version()
            .cache_identity(),
    )?;
    let name = key_name(control.key);
    let shard = root.open_directory_readonly(&name[..2])?;
    let directory = shard.open_directory_readonly(&name)?;
    if control.parent.is_some_and(|id| id != shard.identity())
        || control
            .directory
            .is_some_and(|id| id != directory.identity())
    {
        return Err(invalid());
    }
    Ok((shard, directory))
}
#[inline(never)]
// Keep prepared scope, metadata and immutable source proofs separate across the bounded off-permit stage.
#[allow(clippy::too_many_arguments)]
fn prepare_cache(
    _target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    job: MemoryBox<Job>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: Control,
    original: LegacyStreamHeader,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    if job.mode != LegacyNativeScope::DefaultDirect {
        return Err(invalid());
    }
    // A shared modern cache needs the modern reference retirement protocol.
    // This role never converts old source membership into current reachability.
    if locks::path_identity(domain.artifact_root()).map_err(|_| invalid())?
        == job
            .owner
            .as_ref()
            .unwrap()
            .closed_version()
            .cache_identity()
    {
        control.stage = FINISH;
        let update = control_update(&metadata, &control, memory)?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    }
    if control.stage == UNLINK {
        let file = match cache_directory(&job, &control) {
            Ok((_, directory)) => optional_file(&directory, b"partition.json")?,
            Err(FixedControlError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let mut file = file;
        if let Some(f) = file.as_mut() {
            let stamp = control.stamp.ok_or_else(invalid)?;
            stamp.recheck(f)?;
            if hash(f, stamp, memory, cancel)? != control.digest {
                return Err(invalid());
            }
        }
        let mut next = Control::decode(&control.encode())?;
        next.stage = INSPECT;
        next.stamp = None;
        next.parent = None;
        next.directory = None;
        let update = control_update(&metadata, &next, memory)?;
        return work(
            job,
            Action::Leaf(boxed(
                LeafAction {
                    metadata,
                    update,
                    control,
                    name: b"partition.json".to_vec(),
                    file,
                    invalidation: None,
                },
                memory,
            )?),
            memory,
        );
    }
    let replay = LegacyStreamReplay::from_header(&metadata, original, memory)?;
    let mut selected: Option<LegacyCacheCertificate> = None;
    replay
        .replay(memory, &mut |cert| {
            if cert.loader_usable()
                && (!control.cache_started || cert.key() > control.key)
                && selected.is_none_or(|old| cert.key() < old.key())
            {
                selected = Some(cert)
            }
            Ok(())
        })
        .map_err(|_| invalid())?;
    let Some(cert) = selected else {
        control.stage = FINISH;
        let update = control_update(&metadata, &control, memory)?;
        return work(
            job,
            Action::Metadata(boxed(
                MetadataAction {
                    metadata,
                    update,
                    reset: None,
                },
                memory,
            )?),
            memory,
        );
    };
    control.key = cert.key();
    control.cache_started = true;
    let actual = match cache_directory(&job, &control) {
        Ok(pair) => Some(pair),
        Err(FixedControlError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some((shard, directory)) = actual {
        if let Some(mut file) = optional_file(&directory, b"partition.json")? {
            cert.stamp().recheck(&file)?;
            if hash(&mut file, cert.stamp(), memory, cancel)? != cert.payload_digest() {
                return Err(invalid());
            }
            control.stamp = Some(cert.stamp());
            control.digest = cert.payload_digest();
            control.parent = Some(shard.identity());
            control.directory = Some(directory.identity());
            control.stage = UNLINK;
        }
    }
    let update = control_update(&metadata, &control, memory)?;
    work(
        job,
        Action::Metadata(boxed(
            MetadataAction {
                metadata,
                update,
                reset: None,
            },
            memory,
        )?),
        memory,
    )
}
#[inline(never)]
fn prepare_root(
    mut job: MemoryBox<Job>,
    metadata: MemoryBox<PreparedPrivateMetadata>,
    mut control: Control,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<LegacyNativeWork>>, FixedControlError> {
    if job.mode != LegacyNativeScope::ManagedGeneration {
        return Err(invalid());
    }
    let directory =
        match RootedDirectory::open_readonly_root(&job.member, control.source.member_identity()) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                prepare_transition(&mut job, LegacyPhase::Removed, memory)?;
                return work(job, Action::Finish, memory);
            }
            Err(e) => return Err(e.into()),
        };
    let resource = super::maintenance_generation_gate::resource_name_for(b"graph.ldb");
    let manifest_resource = super::maintenance_generation_gate::resource_name_for(b"manifest.json");
    let names: [&[u8]; 5] = [
        b"lease.lock",
        b".database-use-family.lock",
        &resource,
        &manifest_resource,
        b"graph.ldb.lock",
    ];
    let mut controls: [Option<ControlLeaf>; 5] = std::array::from_fn(|_| None);
    for (index, name) in names.iter().enumerate() {
        if let Some(file) = optional_file(&directory, name)? {
            controls[index] = Some(ControlLeaf {
                name: name.to_vec(),
                stamp: FixedFileStamp::capture(&file, FixedControlKind::File)?,
                file: Some(file),
            });
        }
    }
    let mut scratch = [0; 4096];
    let mut count = 0;
    directory.visit_entries(&mut scratch, &mut |entry| {
        count += 1;
        if count > 100 || !names.contains(&entry) {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(())
    })?;
    control.stage = FINISH;
    let update = control_update(&metadata, &control, memory)?;
    work(
        job,
        Action::Root(boxed(
            RootAction {
                metadata,
                update,
                controls,
                held: std::array::from_fn(|_| None),
            },
            memory,
        )?),
        memory,
    )
}
struct ControlLeaf {
    name: Vec<u8>,
    stamp: FixedFileStamp,
    file: Option<File>,
}
fn check_current_facts(
    mode: LegacyNativeScope,
    current: Option<FixedFileStamp>,
    root: &RootedDirectory,
) -> Result<(), FixedControlError> {
    if mode == LegacyNativeScope::ManagedGeneration {
        current
            .ok_or_else(invalid)?
            .recheck(&root.open_readonly_file(b"active.json")?)?;
    }
    Ok(())
}
fn publish_owned(
    action: &mut MetadataAction,
    root_use: &FixedLockedFile,
    grant: &mut MutationGrant,
) -> Result<(), FixedControlError> {
    admission::with_reserved_private_metadata_owned_root(
        &mut action.metadata,
        root_use,
        grant,
        action.update.effects(),
        &mut |mutation, grant| {
            if let Some(len) = action.reset {
                mutation.reset_stream(len, grant)?;
            }
            mutation.publish_control(&action.update, grant)
        },
    )?;
    Ok(())
}
impl LegacyNativeWork {
    #[inline(never)]
    pub(crate) fn advance(
        &mut self,
        grant: &mut MutationGrant,
        _confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    ) -> Result<CandidateReport, FixedControlError> {
        let job: &mut Job = self.job.as_mut().ok_or_else(invalid)?;
        let memory = &self.memory;
        let mode = job.mode;
        let current = job.current;
        match &mut self.action {
            Action::Recover => {
                if !job
                    .scope
                    .recover_publication(job.owner.as_mut(), grant, memory)?
                {
                    return Err(invalid());
                }
                self.report.mutated_entries = 1;
            }
            Action::Complete => {
                grant.check()?;
                if !job.owner.as_ref().unwrap().is_current(memory)? {
                    return Err(invalid());
                }
                self.done = true;
                self.report.pending = false;
            }
            Action::Enroll => {
                let mut apply = |authority: &source::LegacyEnrollmentAuthority<'_>,
                                 _grant: &mut MutationGrant| {
                    check_current_facts(mode, current, authority.directory())?;
                    let _lease = lease(authority.member_directory(), mode)?;
                    Ok(())
                };
                let outcome = if let Some(cap) = job.cohort.as_ref() {
                    source::with_legacy_enrollment_in_cohort(
                        &mut job.scope,
                        cap,
                        grant,
                        memory,
                        &mut apply,
                    )
                } else {
                    source::with_legacy_enrollment_automatic(
                        &mut job.scope,
                        job.automatic.as_mut().ok_or_else(invalid)?,
                        grant,
                        memory,
                        &mut apply,
                    )
                };
                match outcome {
                    Ok((_, owner)) => job.owner = Some(owner),
                    Err(FixedControlError::Invalid(FixedControlProblem::Busy))
                        if job.scope.enrollment_pending() => {}
                    Err(error) => return Err(error),
                }
                self.report.mutated_entries = 1;
            }
            Action::Association(step) => {
                self.committed = Some(step.advance(grant)?);
                self.report.mutated_entries = 1;
            }
            Action::Retire => {
                let (association, _, _) = job.proof.as_ref().ok_or_else(invalid)?;
                // The owner and full association pin the actual pair. Transition
                // is durable before any native-cohort intent can be selected.
                source::begin_legacy_retirement(
                    &mut job.scope,
                    job.owner.as_mut().ok_or_else(invalid)?,
                    association,
                    grant,
                    memory,
                )?;
                self.report.mutated_entries = 1;
            }
            Action::Custody(step) => {
                let owned = mode == LegacyNativeScope::ManagedGeneration;
                if owned {
                    grant.take_entries(step.effects())?;
                }
                self.committed = Some(source::with_closed_source(
                    &mut job.scope,
                    job.owner.as_ref().ok_or_else(invalid)?,
                    grant,
                    memory,
                    &mut |authority, grant| {
                        check_current_facts(mode, current, authority.directory())?;
                        let _lease = lease(authority.member_directory(), mode)?;
                        if owned {
                            step.advance_with_owned_root(authority.artifact_use_guard(), grant)
                        } else {
                            step.advance(grant)
                        }
                    },
                )?);
                self.report.mutated_entries = 1;
            }
            Action::Metadata(action) => {
                let action: &mut MetadataAction = action;
                if job.owner.as_ref().unwrap().closed_version().phase() == LegacyPhase::Removed {
                    if !job.owner.as_ref().unwrap().is_current(memory)? {
                        return Err(invalid());
                    }
                    let effects = action.update.effects() + usize::from(action.reset.is_some()) * 2;
                    admission::with_private_metadata(
                        &mut action.metadata,
                        grant,
                        effects,
                        &mut |mutation, grant| {
                            if let Some(len) = action.reset {
                                mutation.reset_stream(len, grant)?;
                            }
                            mutation.publish_control(&action.update, grant)
                        },
                    )?;
                } else {
                    let same_root = action.metadata.origin_scope().root
                        == job
                            .owner
                            .as_ref()
                            .unwrap()
                            .closed_version()
                            .cache_identity();
                    if same_root {
                        grant.take_entries(action.update.effects())?;
                    }
                    source::with_closed_source(
                        &mut job.scope,
                        job.owner.as_ref().unwrap(),
                        grant,
                        memory,
                        &mut |authority, grant| {
                            check_current_facts(mode, current, authority.directory())?;
                            let _lease = lease(authority.member_directory(), mode)?;
                            if same_root {
                                publish_owned(action, authority.artifact_use_guard(), grant)
                            } else {
                                admission::with_private_metadata(
                                    &mut action.metadata,
                                    grant,
                                    action.update.effects(),
                                    &mut |mutation, grant| {
                                        mutation.publish_control(&action.update, grant)
                                    },
                                )?;
                                Ok(())
                            }
                        },
                    )?;
                }
                self.report.mutated_entries = 1;
                if action.reset.is_some() {
                    self.done = true;
                    self.report.pending = false;
                }
            }
            Action::Leaf(action) => {
                advance_leaf(job, action, grant, memory, &mut self.report)?;
            }
            Action::Root(action) => {
                advance_root(job, action, grant, memory, &mut self.report)?;
            }
            Action::Finish => {
                source::finish_legacy_retirement(
                    &mut job.scope,
                    job.owner.as_mut().unwrap(),
                    grant,
                    memory,
                )?;
                self.report.mutated_entries = 1;
            }
        }
        Ok(self.report)
    }
}
fn open_cache_under_guard(
    root: &RootedDirectory,
    key: &[u8; 64],
    control: &Control,
) -> Result<Option<(RootedDirectory, RootedDirectory)>, FixedControlError> {
    let shard = match root.open_directory(&key[..2]) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if Some(shard.identity()) != control.parent {
        return Err(invalid());
    }
    let directory = match shard.open_directory(key) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if Some(directory.identity()) != control.directory {
        return Err(invalid());
    }
    Ok(Some((shard, directory)))
}
#[inline(never)]
fn advance_leaf(
    job: &mut Job,
    action: &mut LeafAction,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    report: &mut CandidateReport,
) -> Result<(), FixedControlError> {
    let mode = job.mode;
    let current = job.current;
    let control = &action.control;
    let name = &action.name;
    let key = key_name(control.key);
    let same_root = action.metadata.origin_scope().root
        == job
            .owner
            .as_ref()
            .unwrap()
            .closed_version()
            .cache_identity();
    // Reserve the atomic effect unit before admitting the actual source kernel.
    let effects = action.update.effects() + 5;
    if same_root {
        grant.take_entries(effects)?;
    }
    source::with_closed_source(
        &mut job.scope,
        job.owner.as_ref().unwrap(),
        grant,
        memory,
        &mut |authority, grant| {
            check_current_facts(mode, current, authority.directory())?;
            let _lease = lease(authority.member_directory(), mode)?;
            let cache = if control.index == CACHE {
                open_cache_under_guard(authority.artifact_directory(), &key, control)?
            } else {
                None
            };
            let parent = cache
                .as_ref()
                .map(|(_, d)| d)
                .unwrap_or(authority.member_directory());
            let mutate = |grant: &mut MutationGrant| -> Result<(), FixedControlError> {
                if let Some(file) = action.file.as_ref() {
                    let stamp = control.stamp.ok_or_else(invalid)?;
                    stamp.recheck(file)?;
                    stamp.recheck(&parent.open_readonly_file(name)?)?;
                    grant.check_time()?;
                    parent.unlink_file(name, stamp.identity())?;
                    parent.sync_namespace()?;
                } else if (control.index != CACHE || cache.is_some())
                    && optional_file(parent, name)?.is_some()
                {
                    return Err(invalid());
                }
                if let Some((shard, directory)) = &cache {
                    // Empty-only disposal preserves every unrecognized neighbor.
                    grant.check_time()?;
                    match shard.remove_empty_directory(&key, directory.identity()) {
                        Ok(()) => shard.sync_namespace()?,
                        Err(e) if locks::is_directory_not_empty(&e) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                Ok(())
            };
            if let Some(invalidation) = action.invalidation.as_mut() {
                admission::with_scoped_reference_invalidation_owned_root(
                    invalidation,
                    authority.artifact_use_guard(),
                    grant,
                    0,
                    &mut |_, _| Ok(()),
                    &mut |_, grant| {
                        mutate(grant)?;
                        admission::with_reserved_private_metadata_owned_root(
                            &mut action.metadata,
                            authority.artifact_use_guard(),
                            grant,
                            action.update.effects(),
                            &mut |metadata, grant| metadata.publish_control(&action.update, grant),
                        )?;
                        Ok(())
                    },
                )?;
            } else if same_root {
                mutate(grant)?;
                admission::with_reserved_private_metadata_owned_root(
                    &mut action.metadata,
                    authority.artifact_use_guard(),
                    grant,
                    action.update.effects(),
                    &mut |metadata, grant| metadata.publish_control(&action.update, grant),
                )?;
            } else {
                grant.take_entries(5)?;
                mutate(grant)?;
                admission::with_private_metadata(
                    &mut action.metadata,
                    grant,
                    action.update.effects(),
                    &mut |metadata, grant| metadata.publish_control(&action.update, grant),
                )?;
            }
            Ok(())
        },
    )?;
    report.mutated_entries = 1;
    if action.file.is_some() {
        report.reclaimed_entries = 1;
        report.reclaimed_bytes = control.stamp.ok_or_else(invalid)?.len();
    }
    Ok(())
}
#[inline(never)]
fn advance_root(
    job: &mut Job,
    action: &mut RootAction,
    grant: &mut MutationGrant,
    memory: &CleanupMemory,
    report: &mut CandidateReport,
) -> Result<(), FixedControlError> {
    let mode = job.mode;
    let current = job.current;
    let member = job
        .owner
        .as_ref()
        .unwrap()
        .closed_version()
        .member_identity();
    grant.take_entries(action.update.effects() + 12)?;
    source::with_legacy_root_completion(
        &mut job.scope,
        job.owner.as_ref().unwrap(),
        grant,
        memory,
        &mut |authority, grant| {
            check_current_facts(mode, current, authority.directory())?;
            let directory = authority
                .member_parent()
                .open_directory(authority.member_name())?;
            if directory.identity() != member {
                return Err(invalid());
            }
            for (index, leaf) in action.controls.iter_mut().enumerate() {
                if let Some(leaf) = leaf {
                    leaf.stamp
                        .recheck(&directory.open_readonly_file(&leaf.name)?)?;
                    let guard = FixedLockedFile::try_lock(
                        leaf.file.take().ok_or_else(invalid)?,
                        LockMode::Exclusive,
                    )?
                    .ok_or(FixedControlError::Invalid(FixedControlProblem::Busy))?;
                    action.held[index] = Some(guard);
                }
            }
            let mut scratch = [0; 4096];
            let mut count = 0;
            directory.visit_entries(&mut scratch, &mut |entry| {
                count += 1;
                if count > 5 || !action.controls.iter().flatten().any(|c| c.name == entry) {
                    return Err(std::io::ErrorKind::InvalidData.into());
                }
                Ok(())
            })?;
            for leaf in action.controls.iter().flatten() {
                grant.check_time()?;
                directory.unlink_file(&leaf.name, leaf.stamp.identity())?;
            }
            directory.sync_namespace()?;
            grant.check_time()?;
            authority
                .member_parent()
                .remove_empty_directory(authority.member_name(), member)?;
            authority.member_parent().sync_namespace()?;
            admission::with_reserved_private_metadata_owned_root(
                &mut action.metadata,
                authority.artifact_use_guard().ok_or_else(invalid)?,
                grant,
                action.update.effects(),
                &mut |metadata, grant| metadata.publish_control(&action.update, grant),
            )?;
            Ok(())
        },
    )?;
    report.mutated_entries = 1;
    report.reclaimed_entries = 1;
    Ok(())
}
/// ACK and disposal are deliberately outside both execution and role permits.
pub(crate) fn finish_work(
    work: &mut LegacyNativeWork,
    state: &mut LegacyNativePreparationState,
) -> Result<(), FixedControlError> {
    if matches!(work.action, Action::Association(_)) {
        if let Some(committed) = work.committed.take() {
            stream::finish_association_source_step(&mut state.association, committed)?;
        }
    } else {
        let _ = work.committed.take();
    }
    let mut job = work.job.take().ok_or_else(invalid)?;
    if matches!(work.action, Action::Recover) && job.owner.is_none() {
        let version = source::observe_legacy_source_version(&mut job.scope, &work.memory)?
            .ok_or_else(invalid)?;
        job.owner = Some(source::reopen_legacy_source_owner(
            &mut job.scope,
            &version,
            &work.memory,
        )?);
    }
    if matches!(work.action, Action::Retire)
        || (matches!(work.action, Action::Recover)
            && job.owner.as_ref().is_some_and(|owner| {
                matches!(
                    owner.closed_version().phase(),
                    LegacyPhase::Retiring | LegacyPhase::Removed
                )
            }))
    {
        drop(job.proof.take());
        state.association.clear();
    }
    if work.done {
        state.association.clear();
        drop(job);
    } else {
        state.job = Some(job);
    }
    Ok(())
}
