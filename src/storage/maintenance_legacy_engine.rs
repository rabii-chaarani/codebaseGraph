//! Joined-worker legacy orchestration. Only prebound short work enters the real
//! execution permit; source/profile preparation and disposal remain offpermit.
use super::inventory::MaintenanceTarget;
use super::locks::{
    self, FixedControlError, FixedControlKind, FixedControlProblem, FixedFileStamp, RootedDirectory,
};
use super::maintenance_admission::{prepare_canonical, PreparedTargetBinding};
use super::maintenance_budget::{CleanupMemory, MemoryBox, MemoryReservation, MutationGrant};
use super::maintenance_candidate::CandidateReport;
use super::maintenance_legacy_bootstrap::{
    self as source, LegacyClosedVersion, LegacyScope, LegacySourceOwner, PreparedLegacyScope,
};
use super::maintenance_ownership::{ReferenceDomain, ReferenceEpoch};
use super::maintenance_scheduler::LegacyQuiescenceConfirmation;
use std::io::Read;
use std::path::PathBuf;
#[track_caller]
fn invalid() -> FixedControlError {
    #[cfg(test)]
    if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
        eprintln!(
            "legacy preparation invalid at {}",
            std::panic::Location::caller()
        );
    }
    FixedControlError::Invalid(FixedControlProblem::InvalidRecord)
}
#[derive(Default)]
pub(crate) struct LegacyPreparationState {
    interest: bool,
    operator_required: bool,
    request_metadata: Option<(
        u64,
        Option<super::maintenance_admission::LegacyConfirmationSource>,
    )>,
    staging: Option<MemoryBox<StagingJob>>,
    native: super::maintenance_legacy_generations_engine::LegacyNativePreparationState,
}
impl LegacyPreparationState {
    pub(crate) fn clear(&mut self) {
        self.interest = false;
        self.operator_required = false;
        self.request_metadata = None;
        drop(self.staging.take());
        self.native.clear();
    }
    pub(crate) fn needs_operator(&self) -> bool {
        self.operator_required
    }
    pub(crate) fn set_request_metadata(
        &mut self,
        metadata: Option<(
            u64,
            Option<super::maintenance_admission::LegacyConfirmationSource>,
        )>,
    ) {
        self.request_metadata = metadata;
    }
    pub(crate) fn has_parked_payloads(&self) -> bool {
        self.native.has_parked_payloads()
    }
    pub(crate) fn set_legacy_interest(&mut self, interest: bool) {
        self.interest |= interest;
    }
}
enum Action {
    Native(MemoryBox<super::maintenance_legacy_generations_engine::LegacyNativeWork>),
    Pending,
    Staging(MemoryBox<StagingWork>),
    Enrollment {
        scope: MemoryBox<PreparedLegacyScope>,
        owner: Option<LegacySourceOwner>,
        needs_confirmation: bool,
    },
    Administration {
        scope: MemoryBox<PreparedLegacyScope>,
        coverage: Option<MemoryBox<super::maintenance_ownership::PreparedLegacyCoverage>>,
        needs_confirmation: bool,
        owner: Option<LegacySourceOwner>,
        covered: bool,
    },
}
pub(crate) struct LegacyWork {
    action: Action,
    memory: CleanupMemory,
    report: CandidateReport,
    _floor: MemoryReservation,
}
impl LegacyWork {
    pub(crate) fn requires_legacy_confirmation(&self) -> bool {
        match &self.action {
            Action::Native(work) => work.requires_legacy_confirmation(),
            Action::Pending => false,
            Action::Staging(_) => false,
            Action::Enrollment {
                needs_confirmation, ..
            } => *needs_confirmation,
            Action::Administration {
                needs_confirmation, ..
            } => *needs_confirmation,
        }
    }
    pub(crate) fn advance(
        &mut self,
        grant: &mut MutationGrant,
        confirmation: Option<&LegacyQuiescenceConfirmation<'_>>,
    ) -> Result<CandidateReport, FixedControlError> {
        match &mut self.action {
            Action::Native(work) => work.advance(grant, confirmation),
            Action::Pending => Ok(self.report),
            Action::Staging(work) => {
                work.advance(&self.memory, grant)?;
                self.report.pending = true;
                self.report.mutated_entries = 1;
                Ok(self.report)
            }
            Action::Enrollment {
                scope,
                owner,
                needs_confirmation,
            } => {
                if *needs_confirmation && confirmation.is_none() {
                    return Err(invalid());
                }
                let outcome = source::with_legacy_enrollment(
                    scope,
                    confirmation,
                    grant,
                    &self.memory,
                    &mut |_, _| Ok(()),
                );
                match outcome {
                    Ok((_, actual_owner)) => *owner = Some(actual_owner),
                    Err(FixedControlError::Invalid(FixedControlProblem::Busy))
                        if scope.enrollment_pending() => {}
                    Err(e) => return Err(e),
                };
                self.report.pending = true;
                self.report.mutated_entries = 1;
                Ok(self.report)
            }
            Action::Administration {
                scope,
                coverage,
                needs_confirmation,
                owner,
                covered,
            } => {
                if *needs_confirmation && confirmation.is_none() {
                    return Err(invalid());
                }
                let outcome = source::with_legacy_enrollment(
                    scope,
                    confirmation,
                    grant,
                    &self.memory,
                    &mut |authority, grant| {
                        if let Some(coverage) = coverage.as_mut() {
                            coverage.advance(authority, grant)
                        } else {
                            Ok(false)
                        }
                    },
                );
                match outcome {
                    Ok((done, actual_owner)) => {
                        *owner = Some(actual_owner);
                        *covered = done;
                    }
                    Err(FixedControlError::Invalid(FixedControlProblem::Busy))
                        if scope.enrollment_pending() => {}
                    Err(e) => return Err(e),
                };
                self.report.pending = !*covered;
                self.report.mutated_entries = 1;
                Ok(self.report)
            }
        }
    }
}
#[inline(never)]
fn active_generation(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
) -> Result<Option<(PathBuf, PathBuf)>, FixedControlError> {
    let _floor = memory.reserve(32768).map_err(FixedControlError::Resource)?;
    let configured =
        super::maintenance_projection::project_binding(target, memory).map_err(|_| invalid())?;
    if configured.direct.is_some() {
        return Ok(None);
    }
    let storage = configured.storage;
    let root = RootedDirectory::open_readonly_root(
        &storage,
        locks::path_identity(&storage).map_err(|_| invalid())?,
    )?;
    let mut pointer = match root.open_readonly_file(b"active.json") {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let stamp = FixedFileStamp::capture(&pointer, FixedControlKind::File)?;
    if stamp.len() > 1024 {
        return Err(invalid());
    }
    let mut bytes = [0; 1024];
    pointer.read_exact(&mut bytes[..stamp.len() as usize])?;
    let active: super::managed::ActiveGeneration =
        serde_json::from_slice(&bytes[..stamp.len() as usize]).map_err(|_| invalid())?;
    if active.schema_version != 2 {
        return Err(invalid());
    }
    super::layout::validate_generation_id(&active.generation_id).map_err(|_| invalid())?;
    stamp.recheck(&pointer)?;
    stamp.recheck(&root.open_readonly_file(b"active.json")?)?;
    let generation = super::layout::ManagedLayout::new(&storage)
        .generation(&active.generation_id)
        .map_err(|_| invalid())?
        .root()
        .to_owned();
    Ok(Some((storage, generation)))
}
/// Runs before observing E: original stores may have no E/cohort controls yet.
/// Initial confirmation is consumed solely by this actual administrative role.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_bootstrap_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    state: &mut LegacyPreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
    legacy_interest: bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    if cancel() {
        return Ok(None);
    }
    state.interest |= legacy_interest;
    state.operator_required = discover_operator_interest(target, memory, cancel)?;
    if super::maintenance_legacy_generations_engine::prepare_root_cohort_boxed(
        target, binding, memory,
    )?
    .is_some()
    {
        state.operator_required = false;
        if let Some(work) = prepare_default_enrollment(target, binding, state, memory)? {
            return Ok(Some(work));
        }
        if !state.interest {
            return Ok(None);
        }
    }
    let Some((storage, generation)) = active_generation(target, memory)? else {
        return Ok(None);
    };
    let mut scope = source::prepare_legacy_scope(
        target,
        binding,
        LegacyScope::ManagedAdministration {
            storage_root: &storage,
            generation_root: &generation,
        },
        false,
        memory,
    )?;
    scope.prepare_enrollment_metadata(state.request_metadata, memory)?;
    let accepted = scope.has_accepted_job(memory)?;
    if accepted {
        state.operator_required = false;
    }
    if !state.interest && !accepted {
        return Ok(None);
    }
    let domain = ReferenceDomain::managed(&super::layout::ManagedLayout::new(&storage)).ok();
    let root_use = std::fs::File::open(storage.join("artifacts/.use.lock"));
    let has_header = match root_use {
        Ok(file) => locks::read_coordination_incarnation(&file)
            .map_err(|_| invalid())?
            .is_some(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    let coverage = if let Some(domain) = domain.as_ref().filter(|_| has_header) {
        Some(super::maintenance_ownership::prepare_legacy_coverage(
            domain, memory,
        )?)
    } else {
        None
    };
    let floor = memory
        .reserve(std::mem::size_of::<LegacyWork>() + 8192)
        .map_err(FixedControlError::Resource)?;
    MemoryBox::new(
        LegacyWork {
            action: Action::Administration {
                scope,
                coverage,
                needs_confirmation: state.interest || !accepted,
                owner: None,
                covered: false,
            },
            memory: memory.clone(),
            report: CandidateReport {
                pending: true,
                ..CandidateReport::default()
            },
            _floor: floor,
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
pub(crate) fn is_legacy_payload(payload: &[u8]) -> bool {
    payload.starts_with(b"CGLEG001") || payload.starts_with(b"CGLGEN01")
}
/// Shared slot dispatch entry. Dataset roles are prepared only after future-use
/// coverage and the accepted physical root cohort are both available.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut LegacyPreparationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    if cancel() {
        return Ok(None);
    }
    let cohort = super::maintenance_legacy_generations_engine::prepare_root_cohort_boxed(
        target, binding, memory,
    )?;
    let origin = staging_origin(target, domain, expected, memory)?;
    let metadata = staging_metadata(target, &origin, memory)?;
    let header = boxed_staging_header(&metadata, memory)?;
    let native_mode = metadata
        .payload()
        .filter(|p| super::maintenance_legacy_generations_engine::is_native_payload(p))
        .map(super::maintenance_legacy_generations_engine::native_payload_mode)
        .transpose()?;
    let native = metadata
        .payload()
        .is_some_and(super::maintenance_legacy_generations_engine::is_native_payload)
        || header
            .as_ref()
            .is_some_and(|h| h.kind() == stream::LegacyStreamKind::Association);
    drop(metadata);
    if native {
        let mode = if let Some(mode) = native_mode {
            mode
        } else if header
            .as_ref()
            .and_then(|h| h.source_version())
            .is_some_and(|s| {
                locks::path_identity(&target.state_root).ok() == Some(s.root_identity())
            })
        {
            super::maintenance_legacy_generations_engine::LegacyNativeScope::DefaultDirect
        } else {
            super::maintenance_legacy_generations_engine::LegacyNativeScope::ManagedGeneration
        };
        return prepare_native_work(
            target, binding, domain, expected, state, mode, memory, cancel,
        );
    }
    if let Some(cohort) = cohort {
        if let Some(work) = prepare_staging_work(
            target, binding, domain, expected, state, cohort, memory, cancel,
        )? {
            return Ok(Some(work));
        }
    }
    prepare_native_work(
        target,
        binding,
        domain,
        expected,
        state,
        super::maintenance_legacy_generations_engine::LegacyNativeScope::DefaultDirect,
        memory,
        cancel,
    )
}
/// Called only after the execution permit and claimed request pins are released.
pub(crate) fn finish_work(
    work: &mut LegacyWork,
    state: &mut LegacyPreparationState,
) -> Result<(), FixedControlError> {
    match &mut work.action {
        Action::Native(native) => {
            super::maintenance_legacy_generations_engine::finish_work(native, &mut state.native)?;
        }
        Action::Pending => {}
        Action::Staging(work) => work.finish(state)?,
        Action::Enrollment {
            owner,
            needs_confirmation,
            scope,
            ..
        } => {
            if owner.is_some() || scope.has_accepted_job(&work.memory)? {
                state.interest = false;
                *needs_confirmation = false;
            }
            drop(owner.take());
        }
        Action::Administration {
            owner,
            needs_confirmation,
            scope,
            ..
        } => {
            if owner.is_some() || scope.has_accepted_job(&work.memory)? {
                state.interest = false;
                *needs_confirmation = false;
            }
            drop(owner.take());
        }
    }
    Ok(())
}

/// This phase consumes request-time source pins before any native/cache proof
/// job is opened. Durable Preparing is the only continuation after the claim.
#[inline(never)]
fn prepare_default_enrollment(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    state: &mut LegacyPreparationState,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let pair = super::inventory::default_direct(target);
    if !pair.database.is_file() || !pair.manifest.is_file() {
        return Ok(None);
    }
    let conventional = target.state_root.join("artifacts");
    let keyed =
        super::layout::DirectLayout::new(&pair.database, &pair.manifest).artifact_root_path();
    let artifact = if conventional.is_dir() {
        conventional
    } else {
        keyed
    };
    if !artifact.is_dir() {
        return Ok(None);
    }
    let mut scope = source::prepare_legacy_scope(
        target,
        binding,
        LegacyScope::DefaultDirect {
            pair: &pair,
            artifact_root: &artifact,
        },
        true,
        memory,
    )?;
    scope.prepare_enrollment_metadata(state.request_metadata, memory)?;
    scope.has_accepted_job(memory)?;
    if !state.interest {
        return Ok(None);
    }
    let floor = memory
        .reserve(std::mem::size_of::<LegacyWork>() + 8192)
        .map_err(FixedControlError::Resource)?;
    MemoryBox::new(
        LegacyWork {
            action: Action::Enrollment {
                scope,
                owner: None,
                needs_confirmation: true,
            },
            memory: memory.clone(),
            report: CandidateReport {
                pending: true,
                ..CandidateReport::default()
            },
            _floor: floor,
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
pub(crate) struct LegacyStagingCohortLeaf {
    association: super::maintenance_legacy_staging::StagingAssociation,
    certificate: super::maintenance_legacy_staging::StagingPayloadCertificate,
    original_source: super::maintenance_legacy_bootstrap::LegacyClosedVersion,
}
impl LegacyStagingCohortLeaf {
    pub(crate) fn association(&self) -> &super::maintenance_legacy_staging::StagingAssociation {
        &self.association
    }
    pub(crate) fn certificate(
        &self,
    ) -> &super::maintenance_legacy_staging::StagingPayloadCertificate {
        &self.certificate
    }
    pub(crate) fn original_source(
        &self,
    ) -> &super::maintenance_legacy_bootstrap::LegacyClosedVersion {
        &self.original_source
    }
}

use super::maintenance_legacy_staging as staging;
use super::maintenance_legacy_stream as stream;
use staging::ReplayStagingCertificates;

struct StagingJob {
    scope: MemoryBox<source::PreparedLegacyStagingScope>,
    owner: Option<LegacySourceOwner>,
    root: RootedDirectory,
    original: Option<LegacyClosedVersion>,
    projection: Option<MemoryBox<staging::StagingProjection>>,
    pending: Option<MemoryBox<staging::StagingProjectionBatch>>,
    seal: Option<staging::StagingAssociation>,
    _floor: MemoryReservation,
}
// These fixed effect descriptors are retained in charged work and consumed without allocation under authority.
#[allow(clippy::large_enum_variant)]
enum StagingAction {
    Recover,
    Enroll(MemoryBox<source::AcceptedLegacyRootCohort>),
    Stream(MemoryBox<stream::PreparedLegacyStreamStep>),
    Retire(staging::StagingAssociation),
    Leaf(MemoryBox<staging::PreparedStagingLeaf>),
    Reopen(LegacyStagingCohortDone),
    Activate,
}
struct StagingWork {
    job: Option<MemoryBox<StagingJob>>,
    action: StagingAction,
    result: MemoryBox<Option<stream::CommittedLegacyStreamStep>>,
}
fn staging_error(error: staging::StagingIssue) -> FixedControlError {
    #[cfg(test)]
    if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
        eprintln!("legacy staging validation rejected: {error:?}");
    }
    match error {
        staging::StagingIssue::Resource(e) => FixedControlError::Resource(e),
        staging::StagingIssue::Io(kind) => FixedControlError::Io(std::io::Error::from(kind)),
        _ => invalid(),
    }
}
fn staging_trace(operation: &str, error: FixedControlError) -> FixedControlError {
    #[cfg(test)]
    if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
        eprintln!("legacy staging {operation}: {error:?}");
    }
    let _ = operation;
    error
}
impl StagingWork {
    fn advance(
        &mut self,
        memory: &CleanupMemory,
        grant: &mut MutationGrant,
    ) -> Result<(), FixedControlError> {
        let job: &mut StagingJob = self.job.as_mut().ok_or_else(invalid)?;
        match &mut self.action {
            StagingAction::Recover => {
                job.scope
                    .recover_publication(job.owner.as_mut(), grant, memory)?;
            }
            StagingAction::Enroll(cohort) => {
                match source::with_legacy_staging_enrollment_in_cohort(
                    &mut job.scope,
                    cohort,
                    grant,
                    memory,
                    &mut |_, _| Ok(()),
                ) {
                    Ok((_, owner)) => {
                        job.original = Some(owner.closed_version());
                        job.owner = Some(owner);
                    }
                    Err(FixedControlError::Invalid(FixedControlProblem::Busy))
                        if job.scope.enrollment_pending() => {}
                    Err(e) => return Err(e),
                }
            }
            StagingAction::Stream(step) => {
                *self.result = Some(step.advance(grant)?);
            }
            StagingAction::Retire(association) => {
                source::begin_legacy_staging_retirement(
                    &mut job.scope,
                    job.owner.as_mut().ok_or_else(invalid)?,
                    association,
                    grant,
                    memory,
                )?;
            }
            StagingAction::Leaf(leaf) => {
                leaf.with_admitted(
                    &mut job.scope,
                    job.owner.as_ref().ok_or_else(invalid)?,
                    grant,
                    3,
                    memory,
                    &mut |actual, grant| {
                        grant.check_time()?;
                        actual
                            .source_parent()
                            .unlink_file(actual.name(), actual.payload_identity())?;
                        grant.check_time()?;
                        actual.source_parent().sync_namespace()?;
                        Ok(())
                    },
                )?;
            }
            StagingAction::Activate => {
                source::reopen_completed_legacy_staging(
                    &mut job.scope,
                    job.owner.as_mut().ok_or_else(invalid)?,
                    grant,
                    memory,
                )?;
            }
            StagingAction::Reopen(done) => {
                source::finish_legacy_staging_cohort(
                    &mut job.scope,
                    job.owner.as_mut().ok_or_else(invalid)?,
                    done,
                    grant,
                    memory,
                )?;
            }
        }
        Ok(())
    }
    fn finish(&mut self, state: &mut LegacyPreparationState) -> Result<(), FixedControlError> {
        if let Some(stream::CommittedLegacyStreamStep::Checkpoint { ack, .. }) = self.result.take()
        {
            let job: &mut StagingJob = self.job.as_mut().ok_or_else(invalid)?;
            if let Some(batch) = job.pending.as_ref() {
                if batch.checkpoint.digest() != ack {
                    return Err(invalid());
                }
                job.projection
                    .as_mut()
                    .ok_or_else(invalid)?
                    .acknowledge(ack)
                    .map_err(staging_error)?;
                job.pending = None;
            }
        }
        if matches!(self.action, StagingAction::Activate)
            || matches!(self.action, StagingAction::Recover)
                && self
                    .job
                    .as_ref()
                    .and_then(|j| j.owner.as_ref())
                    .is_some_and(|o| {
                        o.closed_version().phase()
                            == super::maintenance_legacy_guard::LegacyPhase::Active
                    })
        {
            drop(self.job.take());
            state.staging = None;
        } else {
            state.staging = self.job.take();
        }
        Ok(())
    }
}
#[inline(never)]
fn staging_work(
    job: MemoryBox<StagingJob>,
    action: StagingAction,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let floor = memory
        .reserve(std::mem::size_of::<LegacyWork>() + 8192)
        .map_err(FixedControlError::Resource)?;
    let staging = MemoryBox::new(
        StagingWork {
            job: Some(job),
            action,
            result: MemoryBox::new(None, memory).map_err(FixedControlError::Resource)?,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)?;
    MemoryBox::new(
        LegacyWork {
            action: Action::Staging(staging),
            memory: memory.clone(),
            report: CandidateReport {
                pending: true,
                ..CandidateReport::default()
            },
            _floor: floor,
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn staging_step(
    target: &MaintenanceTarget,
    scope: &super::maintenance_admission::RootOriginBinding,
    action: stream::LegacyStreamAction,
    memory: &CleanupMemory,
) -> Result<MemoryBox<stream::PreparedLegacyStreamStep>, FixedControlError> {
    stream::prepare_legacy_stream_step(target, scope, action, memory)
}

#[inline(never)]
pub(crate) fn staging_origin(
    target: &MaintenanceTarget,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    memory: &CleanupMemory,
) -> Result<super::maintenance_admission::RootOriginBinding, FixedControlError> {
    let canonical = prepare_canonical(target, domain, expected, memory)?;
    Ok(canonical.origin_scope())
}
#[inline(never)]
pub(crate) fn staging_metadata(
    target: &MaintenanceTarget,
    scope: &super::maintenance_admission::RootOriginBinding,
    memory: &CleanupMemory,
) -> Result<MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>, FixedControlError> {
    super::maintenance_admission::prepare_private_metadata_boxed(target, scope, memory)
}

#[inline(never)]
fn boxed_staging_header(
    metadata: &super::maintenance_admission::PreparedPrivateMetadata,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<stream::LegacyStreamHeader>>, FixedControlError> {
    metadata
        .payload()
        .and_then(|payload| stream::LegacyStreamHeader::decode(payload).ok())
        .map(|header| MemoryBox::new(header, memory).map_err(FixedControlError::Resource))
        .transpose()
}
#[inline(never)]
fn boxed_staging_source_scope(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    path: &std::path::Path,
    memory: &CleanupMemory,
) -> Result<MemoryBox<source::PreparedLegacyStagingScope>, FixedControlError> {
    source::prepare_legacy_staging_scope_boxed(target, binding, path, memory)
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn prepare_staging_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut LegacyPreparationState,
    cohort: MemoryBox<source::AcceptedLegacyRootCohort>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let _temporary = memory.reserve(65536).map_err(FixedControlError::Resource)?;
    let scope = staging_origin(target, domain, expected, memory)?;
    let metadata = staging_metadata(target, &scope, memory)?;
    let header = boxed_staging_header(&metadata, memory)?;
    if header
        .as_ref()
        .is_some_and(|h| h.kind() != stream::LegacyStreamKind::Staging)
    {
        return Ok(None);
    }
    if state.staging.is_none() {
        state.staging = prepare_staging_job(target, binding, &cohort, header.as_deref(), memory)?;
        if state.staging.is_none() {
            return Ok(None);
        }
    }
    let job = state.staging.take().ok_or_else(invalid)?;
    continue_staging_work(target, job, cohort, scope, metadata, header, memory, cancel)
}

#[inline(never)]
fn prepare_staging_job(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    cohort: &source::AcceptedLegacyRootCohort,
    header: Option<&stream::LegacyStreamHeader>,
    memory: &CleanupMemory,
) -> Result<Option<MemoryBox<StagingJob>>, FixedControlError> {
    let path = target.state_root.join("native-staging");
    let identity = match locks::path_identity(&path) {
        Ok(id) => id,
        Err(_) if !path.exists() => return Ok(None),
        Err(_) => return Err(invalid()),
    };
    let root = RootedDirectory::open_readonly_root(&path, identity)?;
    let mut source_scope = boxed_staging_source_scope(target, binding, &path, memory)
        .map_err(|e| staging_trace("scope preparation", e))?;
    source_scope
        .prepare_enrollment_in_cohort(cohort, memory)
        .map_err(|e| staging_trace("cohort enrollment preparation", e))?;
    let actual = source::observe_legacy_staging_source_version(&mut source_scope, memory)
        .map_err(|e| staging_trace("source version observation", e))?;
    let owner = match actual {
        Some(_) if source_scope.publication_recovery_pending() => None,
        Some(actual) => Some(
            source::reopen_legacy_staging_owner(&mut source_scope, &actual, memory)
                .map_err(|e| staging_trace("owner reopen", e))?,
        ),
        None => None,
    };
    if owner.is_none() {
        let mut known = false;
        let mut scratch = [0; 4096];
        root.visit_entries(&mut scratch, &mut |name| {
            known |= staging::staging_profile(name).is_some();
            Ok(())
        })?;
        if !known {
            return Ok(None);
        }
    }
    let original = header.and_then(|h| h.source_version()).or(actual);
    if let (Some(original), Some(owner)) = (original, owner.as_ref()) {
        if !owner.closed_version().same_job_source(&original) {
            return Err(invalid());
        }
    }
    let job = MemoryBox::new(
        StagingJob {
            scope: source_scope,
            owner,
            root,
            original,
            projection: None,
            pending: None,
            seal: None,
            _floor: memory
                .reserve(std::mem::size_of::<StagingJob>() + 32768)
                .map_err(FixedControlError::Resource)?,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)?;
    Ok(Some(job))
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn continue_staging_work(
    target: &MaintenanceTarget,
    mut job: MemoryBox<StagingJob>,
    cohort: MemoryBox<source::AcceptedLegacyRootCohort>,
    scope: super::maintenance_admission::RootOriginBinding,
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    header: Option<MemoryBox<stream::LegacyStreamHeader>>,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let recovering = {
        let job = &mut *job;
        job.scope
            .refresh_publication_recovery(memory, job.owner.as_ref())
            .map_err(|e| staging_trace("publication recovery preparation", e))?
    };
    if recovering {
        return staging_work(job, StagingAction::Recover, memory);
    }
    if job.owner.is_none() {
        job.scope.refresh_enrollment_preparation(memory)?;
        return staging_work(job, StagingAction::Enroll(cohort), memory);
    }
    if !job.owner.as_ref().ok_or_else(invalid)?.is_current(memory)? {
        return Err(staging_trace("retained owner changed", invalid()));
    }
    let original = job.original.ok_or_else(invalid)?;
    if let Some(header) = header.as_ref() {
        if header
            .source_version()
            .is_none_or(|s| !s.same_job_source(&original))
            || header.context_digest() != original.context_digest()
        {
            return Err(invalid());
        }
    } else {
        if job
            .owner
            .as_ref()
            .ok_or_else(invalid)?
            .closed_version()
            .phase()
            == super::maintenance_legacy_guard::LegacyPhase::Removed
        {
            if super::maintenance_candidate::idle_control_cursor(
                metadata.payload().ok_or_else(invalid)?,
                scope,
            )
            .map_err(|_| invalid())?
            .is_none()
                || metadata.stream_len() != 0
            {
                return Err(invalid());
            }
            {
                let StagingJob { scope, owner, .. } = &mut *job;
                scope.prepare_transition(
                    owner.as_ref().ok_or_else(invalid)?,
                    super::maintenance_legacy_guard::LegacyPhase::Active,
                    memory,
                )?;
            }
            return staging_work(job, StagingAction::Activate, memory);
        }
        if metadata.stream_len() != 0 {
            return Err(invalid());
        }
        let cursor = super::maintenance_candidate::idle_control_cursor(
            metadata.payload().ok_or_else(invalid)?,
            scope,
        )
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?
        .encode();
        drop(metadata);
        let action = stream::LegacyStreamAction::Start {
            cursor,
            source: original,
            context: original.context_digest(),
            kind: stream::LegacyStreamKind::Staging,
        };
        return staging_work(
            job,
            StagingAction::Stream(staging_step(target, &scope, action, memory)?),
            memory,
        );
    }
    let header = header.unwrap();
    if job
        .owner
        .as_ref()
        .ok_or_else(invalid)?
        .closed_version()
        .phase()
        == super::maintenance_legacy_guard::LegacyPhase::Removed
    {
        let action = if header.resetting() {
            stream::LegacyStreamAction::FinishReset
        } else if header.done() {
            stream::LegacyStreamAction::BeginReset
        } else {
            return Err(invalid());
        };
        drop(metadata);
        return staging_work(
            job,
            StagingAction::Stream(staging_step(target, &scope, action, memory)?),
            memory,
        );
    }
    let seal = {
        let replay = stream::LegacyStreamReplay::new(
            &metadata,
            &original,
            original.context_digest(),
            memory,
        )?;
        replay.staging_seal()?
    };
    if let Some(seal) = seal {
        return prepare_sealed_staging_work(
            target, job, scope, metadata, header, original, seal, memory, cancel,
        );
    }
    prepare_staging_projection(
        target, job, scope, metadata, header, original, memory, cancel,
    )
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn prepare_sealed_staging_work(
    target: &MaintenanceTarget,
    mut job: MemoryBox<StagingJob>,
    scope: super::maintenance_admission::RootOriginBinding,
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    header: MemoryBox<stream::LegacyStreamHeader>,
    original: LegacyClosedVersion,
    seal: staging::StagingAssociation,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let replay =
        stream::LegacyStreamReplay::new(&metadata, &original, original.context_digest(), memory)?;
    authenticate_staging_membership(&replay, &seal, &original, &job.root, memory, cancel)
        .map_err(|e| staging_trace("sealed membership authentication", e))?;
    if !header.done() {
        drop(metadata);
        return staging_work(
            job,
            StagingAction::Stream(staging_step(
                target,
                &scope,
                stream::LegacyStreamAction::Done,
                memory,
            )?),
            memory,
        );
    }
    match job
        .owner
        .as_ref()
        .ok_or_else(invalid)?
        .closed_version()
        .phase()
    {
        super::maintenance_legacy_guard::LegacyPhase::Preparing => {
            {
                let StagingJob { scope, owner, .. } = &mut *job;
                scope.prepare_transition(
                    owner.as_ref().ok_or_else(invalid)?,
                    super::maintenance_legacy_guard::LegacyPhase::Retiring,
                    memory,
                )?;
            }
            staging_work(job, StagingAction::Retire(seal), memory)
        }
        super::maintenance_legacy_guard::LegacyPhase::Retiring => {
            let mut chosen = None;
            replay
                .replay(&mut |event| {
                    if cancel() {
                        return Err(staging::StagingIssue::Cancelled);
                    }
                    if let staging::StagingReplayEvent::Payload(certificate) = event {
                        let address = certificate.witness_address();
                        match job
                            .root
                            .open_readonly_file(&address.name[..usize::from(address.name_len)])
                        {
                            Ok(file) => {
                                address
                                    .stamp
                                    .recheck(&file)
                                    .map_err(|_| staging::StagingIssue::Changed)?;
                                if chosen.is_none() {
                                    chosen = Some(certificate);
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(e.into()),
                        }
                    }
                    Ok(())
                })
                .map_err(staging_error)?;
            if let Some(certificate) = chosen {
                let membership = LegacyStagingCohortLeaf {
                    association: seal,
                    certificate,
                    original_source: original,
                };
                let leaf = staging::prepare_staging_leaf(
                    &membership,
                    job.owner.as_ref().ok_or_else(invalid)?,
                    &job.root,
                    memory,
                    cancel,
                )
                .map_err(staging_error)?;
                return staging_work(job, StagingAction::Leaf(leaf), memory);
            }
            let stamp =
                FixedFileStamp::capture(&job.root.pin_handle()?, FixedControlKind::Directory)?;
            let done = LegacyStagingCohortDone {
                original,
                directory: stamp,
            };
            {
                let StagingJob { scope, owner, .. } = &mut *job;
                scope.prepare_transition(
                    owner.as_ref().ok_or_else(invalid)?,
                    super::maintenance_legacy_guard::LegacyPhase::Removed,
                    memory,
                )?;
            }
            staging_work(job, StagingAction::Reopen(done), memory)
        }
        _ => Err(invalid()),
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn prepare_staging_projection(
    target: &MaintenanceTarget,
    mut job: MemoryBox<StagingJob>,
    scope: super::maintenance_admission::RootOriginBinding,
    metadata: MemoryBox<super::maintenance_admission::PreparedPrivateMetadata>,
    header: MemoryBox<stream::LegacyStreamHeader>,
    original: LegacyClosedVersion,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let action = prepare_staging_projection_action(
        target, &mut job, &metadata, &header, original, memory, cancel,
    )?;
    // Keep the exact captured metadata that authenticated the projection.
    // The eventual guarded mutation rechecks these stamps and slot images;
    // no fresh metadata can relabel an action from an older source prefix.
    let step = stream::prepare_stream_step_from_metadata(metadata, &scope, action, memory)?;
    staging_work(job, StagingAction::Stream(step), memory)
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn prepare_staging_projection_action(
    target: &MaintenanceTarget,
    job: &mut StagingJob,
    metadata: &super::maintenance_admission::PreparedPrivateMetadata,
    header: &stream::LegacyStreamHeader,
    original: LegacyClosedVersion,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<stream::LegacyStreamAction, FixedControlError> {
    let replay =
        stream::LegacyStreamReplay::new(metadata, &original, original.context_digest(), memory)?;
    if job.projection.is_none() {
        if let Ok(bytes) = replay.checkpoint() {
            let checkpoint = staging::StagingProjectionCheckpoint::decode(
                bytes.as_slice().try_into().map_err(|_| invalid())?,
            )
            .map_err(staging_error)?;
            job.projection = Some(
                staging::StagingProjection::restore(
                    &job.root,
                    checkpoint,
                    original.context_digest(),
                    memory,
                )
                .map_err(staging_error)?,
            );
        }
    }
    let previous = job.projection.as_ref().map(|p| p.checkpoint());
    if job.projection.is_none() || previous.as_ref().is_some_and(|p| p.finished()) {
        let mut chosen = [0; 255];
        let mut chosen_len = 0usize;
        let mut scratch = [0; 4096];
        job.root.visit_entries(&mut scratch, &mut |name| {
            if staging::staging_profile(name).is_none()
                || previous.as_ref().is_some_and(|p| name <= p.payload_name())
                || chosen_len > 0 && name >= &chosen[..chosen_len]
            {
                return Ok(());
            }
            chosen[..name.len()].copy_from_slice(name);
            chosen_len = name.len();
            Ok(())
        })?;
        if chosen_len == 0 {
            let association = staging::associate_staging_bundle(
                &job.root,
                &target.repository_root,
                original.context_digest(),
                &replay,
                256,
                memory,
                cancel,
                &|| {
                    if job
                        .owner
                        .as_ref()
                        .ok_or(staging::StagingIssue::Changed)?
                        .is_current(memory)
                        .map_err(|_| staging::StagingIssue::Changed)?
                    {
                        Ok(())
                    } else {
                        Err(staging::StagingIssue::Changed)
                    }
                },
            )
            .map_err(staging_error)?;
            job.seal = Some(association);
        } else {
            job.projection = Some(
                staging::StagingProjection::new(
                    &job.root,
                    &chosen[..chosen_len],
                    original.context_digest(),
                    memory,
                )
                .map_err(staging_error)?,
            );
        }
    }
    if let Some(association) = job.seal {
        let mut bytes = [0; 512];
        association.encode(&mut bytes);
        let frame = stream::LegacyStreamFrame::new(
            stream::LegacyFrameKind::StagingSeal,
            original.context_digest(),
            header.sequence(),
            header.chain_digest(),
            &bytes,
            memory,
        )?;
        let action = if header.pending() {
            stream::LegacyStreamAction::FinishAppend(frame)
        } else {
            stream::LegacyStreamAction::BeginAppend(frame)
        };
        return Ok(action);
    }
    if job.pending.is_none() {
        let StagingJob {
            projection, root, ..
        } = job;
        let pending = projection
            .as_mut()
            .ok_or_else(invalid)?
            .advance(root, memory, cancel)
            .map_err(staging_error)?;
        job.pending = pending;
    }
    let batch = job.pending.as_ref().ok_or_else(invalid)?;
    let mut bytes = [0; 3072];
    let kind = header.pending_kind().unwrap_or_else(|| {
        if header.awaits_checkpoint() {
            stream::LegacyFrameKind::StagingCheckpoint
        } else if batch.certificate.is_some() {
            stream::LegacyFrameKind::StagingPayload
        } else if batch.rows[0].is_some() {
            stream::LegacyFrameKind::StagingRows
        } else {
            stream::LegacyFrameKind::StagingCheckpoint
        }
    });
    let len = match kind {
        stream::LegacyFrameKind::StagingRows => {
            let mut len = 0;
            for row in batch.rows.iter().flatten() {
                row.encode((&mut bytes[len..len + 768]).try_into().unwrap());
                len += 768;
            }
            len
        }
        stream::LegacyFrameKind::StagingPayload => {
            batch
                .certificate
                .ok_or_else(invalid)?
                .encode((&mut bytes[..768]).try_into().unwrap());
            768
        }
        stream::LegacyFrameKind::StagingCheckpoint => {
            batch
                .checkpoint
                .encode((&mut bytes[..768]).try_into().unwrap());
            768
        }
        _ => return Err(invalid()),
    };
    let frame = stream::LegacyStreamFrame::new(
        kind,
        original.context_digest(),
        header.sequence(),
        header.chain_digest(),
        &bytes[..len],
        memory,
    )?;
    let action = if header.pending() {
        stream::LegacyStreamAction::FinishAppend(frame)
    } else {
        stream::LegacyStreamAction::BeginAppend(frame)
    };
    Ok(action)
}

/// Authenticate the complete immutable certificate set and row-chain before
/// reconstructing membership from a controlled journal. Parsed facts alone
/// cannot reach this constructor: actual rooted source custody is also checked.
fn authenticate_staging_membership(
    replay: &stream::LegacyStreamReplay<'_>,
    association: &staging::StagingAssociation,
    original: &LegacyClosedVersion,
    root: &RootedDirectory,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<(), FixedControlError> {
    use sha2::{Digest, Sha256};
    let _floor = memory.reserve(16384).map_err(FixedControlError::Resource)?;
    if association.root_identity() != root.identity()
        || association.source_context() != original.context_digest()
    {
        return Err(invalid());
    }
    let mut digest = Sha256::new();
    let mut payloads = 0u64;
    let mut rows = 0u64;
    let mut current = None;
    let mut chain = [0u8; 32];
    let mut ordinal = 0u64;
    let mut previous = [0; 255];
    let mut previous_len = 0usize;
    replay
        .replay(&mut |event| {
            if cancel() {
                return Err(staging::StagingIssue::Cancelled);
            }
            match event {
                staging::StagingReplayEvent::Payload(certificate) => {
                    let address = certificate.witness_address();
                    let name = &address.name[..usize::from(address.name_len)];
                    if current.is_some()
                        || certificate.rows == 0
                        || certificate.source_context != association.source_context()
                        || address.parent != root.identity()
                        || previous_len > 0 && name <= &previous[..previous_len]
                    {
                        return Err(staging::StagingIssue::Unknown);
                    }
                    previous[..name.len()].copy_from_slice(name);
                    previous_len = name.len();
                    current = Some(certificate);
                    ordinal = 0;
                    chain = [0; 32];
                }
                staging::StagingReplayEvent::Row(row) => {
                    let certificate = current.as_ref().ok_or(staging::StagingIssue::Unknown)?;
                    let address = certificate.witness_address();
                    if row.ordinal != ordinal
                        || row.parent != address.parent
                        || row.name != address.name
                        || row.name_len != address.name_len
                        || row.stamp != address.stamp
                        || row.profile != address.profile
                    {
                        return Err(staging::StagingIssue::Unknown);
                    }
                    let mut bytes = [0; 768];
                    row.encode(&mut bytes);
                    let mut next = Sha256::new();
                    next.update(b"CGLegacyStagingRowChain01");
                    next.update(chain);
                    next.update(bytes);
                    chain = next.finalize().into();
                    ordinal = ordinal
                        .checked_add(1)
                        .ok_or(staging::StagingIssue::Unknown)?;
                }
                staging::StagingReplayEvent::EndPayload => {
                    let certificate = current.take().ok_or(staging::StagingIssue::Unknown)?;
                    if certificate.rows != ordinal || certificate.witness_chain != chain {
                        return Err(staging::StagingIssue::Unknown);
                    }
                    digest.update(certificate.certificate_digest());
                    rows = rows
                        .checked_add(ordinal)
                        .ok_or(staging::StagingIssue::Unknown)?;
                    payloads = payloads
                        .checked_add(1)
                        .ok_or(staging::StagingIssue::Unknown)?;
                }
            }
            Ok(())
        })
        .map_err(staging_error)?;
    if current.is_some()
        || payloads != association.payload_count()
        || rows != association.row_count()
        || <[u8; 32]>::from(digest.finalize()) != association.certificate_set_digest()
    {
        return Err(invalid());
    }
    Ok(())
}

/// Minted only after authenticating the completed exact private cohort and
/// observing every recorded payload absent under the unchanged closed source.
pub(crate) struct LegacyStagingCohortDone {
    original: LegacyClosedVersion,
    directory: FixedFileStamp,
}
impl LegacyStagingCohortDone {
    pub(crate) fn original_source(&self) -> &LegacyClosedVersion {
        &self.original
    }
    pub(crate) fn directory_stamp(&self) -> FixedFileStamp {
        self.directory
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
pub(crate) fn prepare_native_work(
    target: &MaintenanceTarget,
    binding: &PreparedTargetBinding,
    domain: &ReferenceDomain,
    expected: &ReferenceEpoch,
    state: &mut LegacyPreparationState,
    mode: super::maintenance_legacy_generations_engine::LegacyNativeScope,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<MemoryBox<LegacyWork>>, FixedControlError> {
    let work = super::maintenance_legacy_generations_engine::prepare_work(
        target,
        binding,
        domain,
        expected,
        mode,
        &mut state.native,
        memory,
        cancel,
    )?;
    let action = if let Some(work) = work {
        Action::Native(work)
    } else if state.native.has_parked_payloads() {
        Action::Pending
    } else {
        return Ok(None);
    };
    let floor = memory
        .reserve(std::mem::size_of::<LegacyWork>() + 8192)
        .map_err(FixedControlError::Resource)?;
    MemoryBox::new(
        LegacyWork {
            action,
            memory: memory.clone(),
            report: CandidateReport {
                pending: true,
                ..CandidateReport::default()
            },
            _floor: floor,
        },
        memory,
    )
    .map(Some)
    .map_err(FixedControlError::Resource)
}

/// Names/context are advisory discovery only. No bytes or scope flag here can
/// construct a source owner, origin fact, or an eligibility capability.
#[inline(never)]
fn discover_operator_interest(
    target: &MaintenanceTarget,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<bool, FixedControlError> {
    let _floor = memory.reserve(65536).map_err(FixedControlError::Resource)?;
    let configured =
        super::maintenance_projection::project_binding(target, memory).map_err(|_| invalid())?;
    if configured.direct.is_some() {
        return Ok(false);
    }
    let staging = target.state_root.join("native-staging");
    if let Ok(id) = locks::path_identity(&staging) {
        let root = RootedDirectory::open_readonly_root(&staging, id)?;
        let mut found = false;
        let mut scratch = [0; 4096];
        root.visit_entries(&mut scratch, &mut |name| {
            if !cancel()
                && staging::staging_profile(name).is_some()
                && root.open_readonly_file(name).is_ok()
            {
                found = true;
            }
            Ok(())
        })?;
        if found {
            return Ok(true);
        }
    }
    if cancel() {
        return Ok(false);
    }
    let pair = super::inventory::default_direct(target);
    if pair.database.is_file() && pair.manifest.is_file() {
        let layout = super::layout::DirectLayout::new(&pair.database, &pair.manifest);
        if super::maintenance_ownership::prepare_automatic_direct(&layout, memory)?.is_none() {
            return Ok(true);
        }
    }
    let generations = configured.storage.join("generations");
    let Some(identity) = locks::path_identity(&generations).ok() else {
        return Ok(false);
    };
    let root = RootedDirectory::open_readonly_root(&generations, identity)?;
    let mut found = false;
    let mut scratch = [0; 4096];
    root.visit_entries(&mut scratch, &mut |name| {
        if cancel() || !name.starts_with(b"gen-") || name.len() > 132 {
            return Ok(());
        }
        let Ok(text) = std::str::from_utf8(&name[4..]) else {
            return Ok(());
        };
        if super::layout::validate_generation_id(text).is_err() {
            return Ok(());
        }
        let Ok(member) = root.open_directory_readonly(name) else {
            return Ok(());
        };
        let database = member.open_readonly_file(b"graph.ldb").is_ok();
        let manifest = member.open_readonly_file(b"manifest.json").is_ok();
        if database
            && manifest
            && member
                .open_readonly_file(super::maintenance_generation_gate::ORIGIN_NAME)
                .is_err()
        {
            found = true;
        }
        Ok(())
    })?;
    Ok(found)
}
