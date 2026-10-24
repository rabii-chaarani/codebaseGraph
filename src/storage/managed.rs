use crate::error::NativeError;
use crate::storage::atomic::{create_new_file, sync_dir, write_json_atomically};
use crate::storage::containment::{collect_registry, ensure_registry_empty, probe_registry};
use crate::storage::layout::{
    direct_bundle_paths, managed_generation_id, validate_generation_id, CandidateGenerationLayout,
    GenerationPaths, ManagedLayout,
};
use crate::storage::locks::{
    open_locked, try_open_existing_locked, try_open_locked, LockMode, LockedFile, StateLease,
    WriterLease,
};
use crate::storage::run_workspace::{
    remove_run_root_confined, RunJournal, RunPhase, RunWorkspace, RunWorkspaceRecovery,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const MANAGED_SCHEMA_VERSION: u64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActiveGeneration {
    #[serde(default = "managed_schema_version")]
    pub schema_version: u64,
    pub generation_id: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub activated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GenerationMetadata {
    #[serde(default = "managed_schema_version")]
    pub schema_version: u64,
    pub generation_id: String,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub published_at_ms: u64,
    #[serde(default)]
    pub base_generation_id: Option<String>,
    #[serde(default)]
    pub logical_size_bytes: u64,
    #[serde(default)]
    pub physical_size_bytes: u64,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default)]
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GenerationRetirement {
    #[serde(default = "managed_schema_version")]
    pub schema_version: u64,
    pub retired_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct SearchManifestEnvelope {
    #[serde(default)]
    search_backend: Option<SearchBackendEnvelope>,
}

#[derive(Debug, Deserialize)]
struct SearchBackendEnvelope {
    #[serde(default)]
    files: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(crate) struct GenerationLease {
    lock: Option<LockedFile>,
    _database_use: [super::maintenance_ownership::DatabaseUseLease; 2],
}
impl GenerationLease {
    fn new(
        lock: Option<LockedFile>,
        database_use: [super::maintenance_ownership::DatabaseUseLease; 2],
    ) -> Self {
        Self {
            lock,
            _database_use: database_use,
        }
    }
}
impl Drop for GenerationLease {
    fn drop(&mut self) {
        // Reader release is only lease release. The joined controller observes
        // the completion opportunity; no unbounded janitor runs in a destructor.
        drop(self.lock.take());
    }
}

#[derive(Debug, Clone)]
pub(crate) enum StorageMode {
    Direct,
    LegacyManagedV1,
    ManagedV2,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ManagedCleanupReport {
    pub run_recovery: RunWorkspaceRecovery,
    #[cfg(test)]
    pub retired_deleted: usize,
    pub retired_pending: usize,
    #[cfg(test)]
    pub retired_generations_deleted: usize,
    pub retired_generations_pending: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedJanitorStage {
    PreviousRetirementMarker,
    PublishedRunJournal,
    TerminalRunRemoval,
    #[cfg(test)]
    GenerationEnumeration,
    RetiredGenerationRemoval,
    GenerationDisposition,
}
impl ManagedJanitorStage {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::PreviousRetirementMarker => "managed_previous_retirement_pending",
            Self::PublishedRunJournal => "managed_run_journal_pending",
            Self::TerminalRunRemoval => "managed_run_cleanup_pending",
            #[cfg(test)]
            Self::GenerationEnumeration => "managed_generation_observation_pending",
            Self::RetiredGenerationRemoval => "managed_generation_cleanup_pending",
            Self::GenerationDisposition => "managed_generation_disposition_pending",
        }
    }
}
#[derive(Debug)]
pub(crate) struct ManagedJanitorIssue {
    pub stage: ManagedJanitorStage,
    pub source: NativeError,
}
#[derive(Debug, Default)]
pub(crate) struct ManagedJanitorReport {
    pub legacy: ManagedCleanupReport,
    pub pending: bool,
    pub observation_complete: bool,
    pub issue_count: usize,
    pub first_issue: Option<ManagedJanitorIssue>,
    pub deferred_reason: Option<&'static str>,
}
impl ManagedJanitorReport {
    fn complete() -> Self {
        Self {
            observation_complete: true,
            ..Self::default()
        }
    }
    fn issue(&mut self, stage: ManagedJanitorStage, source: NativeError, pending: bool) {
        self.issue_count = self.issue_count.saturating_add(1);
        if self.first_issue.is_none() {
            self.first_issue = Some(ManagedJanitorIssue { stage, source });
        }
        self.pending |= pending;
    }
    pub(crate) fn record_generation_disposition_failure(&mut self, error: NativeError) {
        self.issue(ManagedJanitorStage::GenerationDisposition, error, true);
    }

    pub(crate) fn merge_latest(&mut self, mut latest: Self) {
        self.issue_count = self.issue_count.saturating_add(latest.issue_count);
        if self.first_issue.is_none() {
            self.first_issue = latest.first_issue.take();
        }
        if latest.observation_complete {
            self.pending = latest.pending;
            self.legacy = latest.legacy;
        } else {
            self.pending |= latest.pending;
            self.legacy.run_recovery.skipped_locked = self
                .legacy
                .run_recovery
                .skipped_locked
                .max(latest.legacy.run_recovery.skipped_locked);
            self.legacy.retired_pending = self
                .legacy
                .retired_pending
                .max(latest.legacy.retired_pending);
            self.legacy.retired_generations_pending = self.legacy.retired_pending;
        }
        self.observation_complete &= latest.observation_complete;
        self.deferred_reason = latest.deferred_reason.or(self.deferred_reason);
    }
    #[cfg(test)]
    fn into_legacy(mut self) -> Result<ManagedCleanupReport, NativeError> {
        match self.first_issue.take() {
            Some(issue) => Err(issue.source),
            None => Ok(self.legacy),
        }
    }
}
#[derive(Debug)]
pub(crate) struct ManagedPublication {
    pub generation_id: String,
    pub janitor: ManagedJanitorReport,
}
#[derive(Debug)]
pub(crate) struct ManagedReconciliation {
    pub recovery: ManagedRecoveryReport,
    pub janitor: ManagedJanitorReport,
}
#[derive(Debug, Default)]
pub(crate) struct ManagedRecoveryReport {
    pub publishing_recovered: usize,
    pub skipped_locked: usize,
    pub observation_complete: bool,
}
impl ManagedReconciliation {
    fn new(recovery: ManagedRecoveryReport, mut janitor: ManagedJanitorReport) -> Self {
        janitor.legacy.run_recovery.publishing_recovered = recovery.publishing_recovered;
        janitor.legacy.run_recovery.skipped_locked = janitor
            .legacy
            .run_recovery
            .skipped_locked
            .max(recovery.skipped_locked);
        Self { recovery, janitor }
    }
}
#[derive(Debug)]
struct PublicationFailure {
    source: NativeError,
    may_have_effects: bool,
}
impl PublicationFailure {
    fn before(source: NativeError) -> Self {
        Self {
            source,
            may_have_effects: false,
        }
    }
    fn ambiguous(source: NativeError) -> Self {
        Self {
            source,
            may_have_effects: true,
        }
    }
}
#[derive(Debug)]
enum ReadyProbe {
    Absent,
    Valid,
    Invalid(NativeError),
}
#[derive(Debug)]
enum RunResolution {
    Committed { previous: Option<String> },
    Obsolete { published: GenerationPaths },
}

#[derive(Debug)]
pub(crate) struct ManagedReadSnapshot {
    pub generation_id: String,
    pub db_path: PathBuf,
    pub manifest_path: PathBuf,
    pub published_at_ms: u64,
    _lease: Arc<GenerationLease>,
    pub logical_size_bytes: u64,
    pub physical_size_bytes: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedCandidate {
    pub generation_id: String,
    pub paths: GenerationPaths,
}

impl ManagedCandidate {
    pub(crate) fn paths(&self) -> &GenerationPaths {
        &self.paths
    }
}

#[derive(Debug)]
pub(crate) struct ManagedWriteSession {
    store: ManagedStore,
    workspace: Option<RunWorkspace>,
    writer_lease: Option<WriterLease>,
    pub candidate: ManagedCandidate,
    pub base_generation: Option<ActiveGeneration>,
    ready: bool,
    finished: bool,
    #[cfg(test)]
    publication_report: Option<ManagedJanitorReport>,
}

impl ManagedWriteSession {
    pub(crate) fn writer_lease(&self) -> Result<&WriterLease, NativeError> {
        self.writer_lease
            .as_ref()
            .ok_or_else(|| NativeError::InvalidInput("write session is finished".into()))
    }

    pub(crate) fn phase_workspace_root(&self) -> PathBuf {
        self.store.layout.phase_workspace_root()
    }
    pub(crate) fn staging_root(&self) -> Option<PathBuf> {
        self.workspace.as_ref().map(RunWorkspace::staging_root)
    }

    pub(crate) fn candidate_db_path(&self) -> PathBuf {
        self.candidate.paths.db_path()
    }

    pub(crate) fn candidate_manifest_path(&self) -> PathBuf {
        self.candidate.paths.manifest_path()
    }

    pub(crate) fn base_manifest_path(&self) -> Result<Option<PathBuf>, NativeError> {
        self.base_generation
            .as_ref()
            .map(|generation| {
                self.store
                    .layout
                    .generation(&generation.generation_id)
                    .map(|paths| paths.manifest_path())
            })
            .transpose()
    }

    pub(crate) fn mark_ready_with_stats<S: GenerationStats>(
        &mut self,
        graph_summary: &S,
    ) -> Result<(), NativeError> {
        self.store.prepare_candidate(
            self.workspace.as_ref().ok_or_else(|| {
                NativeError::InvalidInput("managed write session already finished".to_string())
            })?,
            &self.candidate,
            graph_summary,
        )?;
        self.ready = true;
        Ok(())
    }

    pub(crate) fn abort(mut self, error: Option<String>) -> Result<(), NativeError> {
        self.finished = true;
        let result = match self.workspace.take() {
            Some(workspace) => workspace.abort(error),
            None => Ok(()),
        };
        drop(self.writer_lease.take());
        result
    }

    #[cfg(test)]
    pub(crate) fn publish_with_stats<S: GenerationStats>(
        &mut self,
        graph_summary: &S,
    ) -> Result<String, NativeError> {
        let mut publication = self.publish_with_stats_typed(graph_summary)?;
        // Explicit historical adapter only: production publication waits for
        // native verification before genuine generation disposition is sealed.
        if let Err(error) = self.store.finalize_previous_retirement(
            self.base_generation
                .as_ref()
                .map(|active| active.generation_id.as_str()),
        ) {
            publication
                .janitor
                .issue(ManagedJanitorStage::PreviousRetirementMarker, error, true);
        }
        self.publication_report = Some(publication.janitor);
        Ok(publication.generation_id)
    }

    pub(crate) fn publish_with_stats_typed<S: GenerationStats>(
        &mut self,
        graph_summary: &S,
    ) -> Result<ManagedPublication, NativeError> {
        if !self.ready {
            if let Err(error) = self.mark_ready_with_stats(graph_summary) {
                return Err(self.abort_after_error(error));
            }
        }
        let workspace = self.take_workspace()?;
        if let Err(error) = workspace.mark_phase(RunPhase::Publishing, None, None) {
            self.finished = true;
            let error = abort_workspace_after_error(workspace, error);
            drop(self.writer_lease.take());
            return Err(error);
        }
        let mut janitor = match self
            .store
            .publish_candidate_inner_typed(&workspace, &self.candidate)
        {
            Ok(report) => report,
            Err(failure) => {
                self.finished = true;
                let error = if failure.may_have_effects {
                    workspace.retain_for_recovery();
                    failure.source
                } else {
                    abort_workspace_after_error(workspace, failure.source)
                };
                drop(self.writer_lease.take());
                return Err(error);
            }
        };
        let generation_id = self.candidate.generation_id.clone();
        if self.base_generation.is_some() {
            janitor.pending = true;
            janitor.legacy.retired_pending = 1;
            janitor.legacy.retired_generations_pending = 1;
            janitor.deferred_reason = Some("managed_generation_collection_deferred");
        }
        #[cfg(test)]
        MANAGED_POST_PUBLICATION.with(|hook| {
            if let Some(hook) = hook.take() {
                hook(workspace.root());
            }
        });
        // These are the same resolved publication's independent optional
        // finalization steps. A failed journal update does not invent an extant
        // run if actual disposal subsequently succeeds.
        if let Err(error) =
            workspace.mark_phase(RunPhase::Published, Some(generation_id.clone()), None)
        {
            janitor.issue(ManagedJanitorStage::PublishedRunJournal, error, false);
        }
        if let Err(error) = workspace.finish() {
            janitor.issue(ManagedJanitorStage::TerminalRunRemoval, error, true);
            janitor.legacy.run_recovery.skipped_locked = 1;
        }
        Ok(ManagedPublication {
            generation_id,
            janitor,
        })
    }

    pub(crate) fn phase_owner_scope(&self) -> Option<super::run_workspace::ParentRunScope> {
        self.workspace.as_ref()?.phase_owner_scope()
    }

    pub(crate) fn reconcile_typed(&self) -> Result<ManagedReconciliation, NativeError> {
        let writer = self.writer_lease()?;
        self.store.recover_required_with_owned_writer(writer)
    }
    #[cfg(test)]
    pub(crate) fn recover_and_gc(&self) -> Result<ManagedCleanupReport, NativeError> {
        self.store
            .cleanup_with_owned_writer_typed(self.writer_lease()?)?
            .janitor
            .into_legacy()
    }
    pub(crate) fn check_active_generation(&self, expected: &str) -> Result<(), NativeError> {
        let active = self.store.read_active_with_shared_lock()?;
        if active.as_ref().map(|a| a.generation_id.as_str()) != Some(expected) {
            return Err(NativeError::InvalidInput(format!(
                "managed publish activated {}, but reopened generation was {:?}",
                expected,
                active.map(|a| a.generation_id)
            )));
        }
        validate_ready_generation(&self.store.layout.generation(expected)?)
    }
    pub(crate) fn finish_without_cleanup(mut self) {
        self.finished = true;
        drop(self.writer_lease.take());
    }

    #[cfg(test)]
    pub(crate) fn finish(mut self) {
        let _legacy_report = self.publication_report.take();
        self.finished = true;
        drop(self.writer_lease.take());
        // A final old-generation reader may have released its lease while this
        // writer still held publication ownership. Retry once after releasing it.
        let _ = self.store.cleanup();
    }

    fn abort_after_error(&mut self, error: NativeError) -> NativeError {
        self.finished = true;
        let error = match self.workspace.take() {
            Some(workspace) => abort_workspace_after_error(workspace, error),
            None => error,
        };
        drop(self.writer_lease.take());
        error
    }

    fn take_workspace(&mut self) -> Result<RunWorkspace, NativeError> {
        self.workspace.take().ok_or_else(|| {
            NativeError::InvalidInput("managed write session has already been finished".to_string())
        })
    }
}

impl Drop for ManagedWriteSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Some(workspace) = self.workspace.take() {
            let _ = workspace.abort(Some(
                "managed write session dropped without publish".to_string(),
            ));
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedFault {
    None,
    PointerAfterRename,
    MandatorySync,
    GenerationScan,
}
#[cfg(test)]
thread_local! { static MANAGED_FAULT: std::cell::Cell<ManagedFault> = const { std::cell::Cell::new(ManagedFault::None) }; }
#[cfg(test)]
thread_local! {
    static MANAGED_POST_PUBLICATION: std::cell::Cell<Option<fn(&Path)>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn set_managed_cleanup_hook(hook: fn(&Path)) {
    MANAGED_POST_PUBLICATION.with(|value| value.set(Some(hook)));
}
#[cfg(test)]
static SCOPED_MANAGED_SYNC_FAULT: std::sync::Mutex<Option<super::locks::FileIdentity>> =
    std::sync::Mutex::new(None);
#[cfg(test)]
pub(crate) struct ScopedManagedSyncFault(Option<super::locks::FileIdentity>);
#[cfg(test)]
impl Drop for ScopedManagedSyncFault {
    fn drop(&mut self) {
        *SCOPED_MANAGED_SYNC_FAULT
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = self.0;
    }
}
#[cfg(test)]
pub(crate) fn inject_scoped_managed_sync_fault(
    root: &Path,
) -> Result<ScopedManagedSyncFault, NativeError> {
    let identity = super::locks::path_identity(root)?;
    let mut current = SCOPED_MANAGED_SYNC_FAULT
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = current.replace(identity);
    Ok(ScopedManagedSyncFault(previous))
}

#[cfg(test)]
pub(crate) struct ManagedFaultReset(ManagedFault);
#[cfg(test)]
impl Drop for ManagedFaultReset {
    fn drop(&mut self) {
        MANAGED_FAULT.with(|f| f.set(self.0));
    }
}
#[cfg(test)]
pub(crate) fn inject_managed_fault(fault: ManagedFault) -> ManagedFaultReset {
    ManagedFaultReset(MANAGED_FAULT.with(|f| f.replace(fault)))
}
#[cfg(test)]
fn managed_fault(fault: ManagedFault) -> Result<(), NativeError> {
    if MANAGED_FAULT.with(|f| f.get()) == fault {
        Err(std::io::Error::other(match fault {
            ManagedFault::MandatorySync => "injected mandatory managed sync failure",
            _ => "injected generation-only scan failure",
        })
        .into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedStore {
    layout: ManagedLayout,
}

#[derive(Debug)]
pub(crate) enum ManagedValidationError {
    MissingReady(PathBuf),
    StaleBaseGeneration {
        expected: Option<String>,
        found: Option<String>,
    },
}

impl std::fmt::Display for ManagedValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingReady(path) => {
                write!(
                    formatter,
                    "candidate generation is not ready: {}",
                    path.display()
                )
            }
            Self::StaleBaseGeneration { expected, found } => write!(
                formatter,
                "stale generation base; expected active {:?}, found {:?}",
                expected, found
            ),
        }
    }
}

pub(crate) trait GenerationStats {
    fn node_count(&self) -> usize;
    fn edge_count(&self) -> usize;
}

impl GenerationStats for crate::protocol::GraphSummary {
    fn node_count(&self) -> usize {
        self.node_count
    }

    fn edge_count(&self) -> usize {
        self.edge_count
    }
}

impl ManagedStore {
    pub(crate) fn new(layout: ManagedLayout) -> Self {
        Self { layout }
    }

    pub(crate) fn layout(&self) -> &ManagedLayout {
        &self.layout
    }

    pub(crate) fn ensure_layout(&self) -> Result<(), NativeError> {
        self.layout.ensure_roots()
    }

    pub(crate) fn begin_write(&self) -> Result<ManagedWriteSession, NativeError> {
        let fresh = super::layout::create_fresh_reference_root(self.layout.storage_root())?;
        self.ensure_layout()?;
        let mut writer_lease = self.open_writer_lock()?;
        let reference_domain =
            super::maintenance_ownership::ReferenceDomain::managed(&self.layout)?;
        if let Some(fresh) = &fresh {
            super::maintenance_ownership::initialize_fresh_reference_domain(
                &reference_domain,
                fresh,
                &writer_lease,
            )?;
        }
        // Legacy roots may acquire their first ordinary use lock here. Observe
        // the real scope again after that producer-only coordination step, so
        // upgraded run associations never retain a stale missing-lock stamp.
        let mut artifact_use = super::maintenance_ownership::ArtifactUseLease::acquire_shared(
            &self.layout.artifacts_root(),
        )?;
        let reference_domain =
            super::maintenance_ownership::ReferenceDomain::managed(&self.layout)?;
        artifact_use.bind_reference_domain(&reference_domain)?;
        writer_lease.retain(artifact_use.into_lock());
        ensure_registry_empty(&self.layout.phase_workspace_root())
            .map_err(NativeError::InvalidInput)?;
        collect_registry(&self.layout.phase_workspace_root()).map_err(NativeError::InvalidInput)?;
        let base_generation = self.read_active_with_shared_lock()?;
        let mut workspace = RunWorkspace::create_scoped(
            self.layout.runs_root(),
            &reference_domain,
            base_generation
                .as_ref()
                .map(|value| value.generation_id.clone()),
        )?;

        workspace.set_cleanup_barrier(self.layout.phase_workspace_root())?;
        let generation_id = managed_generation_id();
        let candidate_layout =
            CandidateGenerationLayout::new(workspace.candidate_root(), generation_id.clone());
        let paths = candidate_layout.generation_paths();
        paths.ensure_root()?;
        workspace.register_candidate(generation_id.clone())?;

        Ok(ManagedWriteSession {
            store: self.clone(),
            workspace: Some(workspace),
            writer_lease: Some(writer_lease),
            candidate: ManagedCandidate {
                generation_id,
                paths,
            },
            base_generation,
            ready: false,
            finished: false,
            #[cfg(test)]
            publication_report: None,
        })
    }

    pub(crate) fn open_read(&self) -> Result<ManagedReadSnapshot, NativeError> {
        self.resolve_active_read()?.ok_or_else(|| {
            NativeError::InvalidInput(
                "managed storage does not have an active generation".to_string(),
            )
        })
    }

    pub(crate) fn resolve_active_read(&self) -> Result<Option<ManagedReadSnapshot>, NativeError> {
        // A read probe before first publication must not create the storage
        // root/state lock: doing so would erase the later genuine producer's
        // exclusive root-creation event and leave its outputs untracked.
        match fs::symlink_metadata(self.layout.active_pointer_path()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let _state_lease = self.open_state_lock(LockMode::Shared)?;
        let Some(active) = self.read_active_generation()? else {
            return Ok(None);
        };
        let generation = self.layout.generation(&active.generation_id)?;
        let root = fs::symlink_metadata(generation.root())?;
        if !root.is_dir() || root.file_type().is_symlink() {
            return Err(NativeError::InvalidInput(
                "managed generation root is not an owned directory".into(),
            ));
        }
        let lock = super::locks::open_shared_coordination(&generation.lease_path()).ok();
        // Missing/unwritable optional generation coordination must not deny an
        // otherwise committed read. Actual physical DB/manifest family guards
        // protect this snapshot even in that legacy/unknown metadata case.
        let database_use = [
            super::maintenance_ownership::DatabaseUseLease::acquire_shared(&generation.db_path())?,
            super::maintenance_ownership::DatabaseUseLease::acquire_shared(
                &generation.manifest_path(),
            )?,
        ];
        // Fixed, noncreating optional metadata observation while the actual
        // generation lease is held. Only a verified FINAL blocks late reads;
        // untracked/unknown optional records cannot deny a committed read.
        if let Ok(memory) = super::maintenance_budget::CleanupMemory::new(64 * 1024) {
            if matches!(
                super::maintenance_generation_gate::observe_generation_disposition(
                    &self.layout,
                    &generation,
                    &memory
                ),
                Ok(super::maintenance_generation_gate::GenerationDisposition::Final)
            ) {
                return Err(NativeError::InvalidInput(
                    "managed graph generation was retired; rebuild required".into(),
                ));
            }
        }
        validate_ready_generation(&generation)?;
        let metadata = read_generation_metadata(&generation)?;
        let lease = Arc::new(GenerationLease::new(lock, database_use));
        Ok(Some(ManagedReadSnapshot {
            generation_id: active.generation_id,
            db_path: generation.db_path(),
            manifest_path: generation.manifest_path(),
            published_at_ms: metadata.published_at_ms,
            _lease: lease,
            logical_size_bytes: metadata.logical_size_bytes,
            physical_size_bytes: metadata.physical_size_bytes,
        }))
    }

    /// Committed readers resolve only required transactions. Optional generation
    /// metadata and janitor traversal never gate opening an existing graph.
    fn has_existing_run_work(&self) -> Result<bool, NativeError> {
        match fs::symlink_metadata(self.layout.runs_root()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
            Ok(_) => self.has_reference_run_work(),
        }
    }
    fn deferred_required_report(
        reason: &'static str,
        pending: bool,
        recovery_complete: bool,
    ) -> ManagedReconciliation {
        ManagedReconciliation::new(
            ManagedRecoveryReport {
                observation_complete: recovery_complete,
                ..ManagedRecoveryReport::default()
            },
            ManagedJanitorReport {
                deferred_reason: Some(reason),
                pending,
                ..ManagedJanitorReport::default()
            },
        )
    }
    pub(crate) fn recover_for_read(&self) -> Result<ManagedReconciliation, NativeError> {
        if !self.has_existing_run_work()? {
            return Ok(Self::deferred_required_report(
                "managed_janitor_deferred",
                false,
                true,
            ));
        }
        let Some(writer) = try_open_locked(self.layout.writer_lock_path(), LockMode::Exclusive)?
        else {
            return Ok(Self::deferred_required_report(
                "managed_writer_busy",
                true,
                false,
            ));
        };
        self.recover_required_with_owned_writer(&writer)
    }
    fn recover_required_with_owned_writer(
        &self,
        _writer: &WriterLease,
    ) -> Result<ManagedReconciliation, NativeError> {
        let mut result = Self::deferred_required_report("managed_janitor_deferred", false, true);
        if !self.has_existing_run_work()? {
            return Ok(result);
        }
        result.janitor.pending = true;
        if !matches!(
            probe_registry(&self.layout.phase_workspace_root()),
            Ok(true)
        ) {
            return Ok(Self::deferred_required_report(
                "managed_containment_unresolved",
                true,
                false,
            ));
        }
        let domain = super::maintenance_ownership::ReferenceDomain::managed(&self.layout)?;
        let _mutation =
            match super::maintenance_ownership::ReferenceMutation::try_begin_in_domain(&domain) {
                Ok(Some(mutation)) => mutation,
                Ok(None) | Err(_) => {
                    return Ok(Self::deferred_required_report(
                        "managed_reference_control_unavailable",
                        true,
                        false,
                    ))
                }
            };
        let _state = self.open_state_lock(LockMode::Exclusive)?;
        let mut active = self.read_active_generation()?;
        self.recover_runs_locked_typed(
            &mut active,
            &mut result.janitor,
            &mut result.recovery,
            false,
            &_mutation,
        )?;
        Ok(ManagedReconciliation::new(result.recovery, result.janitor))
    }

    #[cfg(test)]
    pub(crate) fn cleanup(&self) -> Result<ManagedCleanupReport, NativeError> {
        self.cleanup_typed()?.janitor.into_legacy()
    }
    #[cfg(test)]
    pub(crate) fn cleanup_typed(&self) -> Result<ManagedReconciliation, NativeError> {
        self.ensure_layout()?;
        let Some(writer) = try_open_locked(self.layout.writer_lock_path(), LockMode::Exclusive)?
        else {
            return self.pending_reconciliation("managed_writer_busy");
        };
        self.cleanup_with_owned_writer_typed(&writer)
    }
    #[cfg(test)]
    fn cleanup_with_owned_writer_typed(
        &self,
        _writer: &WriterLease,
    ) -> Result<ManagedReconciliation, NativeError> {
        let run_work = self.has_reference_run_work()?;
        let active_before = self.read_active_with_shared_lock()?;
        let mut report = ManagedJanitorReport::complete();
        let generation_work = match self
            .has_generation_cleanup_work(active_before.as_ref().map(|a| a.generation_id.as_str()))
        {
            Ok(work) => work,
            Err(error) => {
                report.issue(ManagedJanitorStage::GenerationEnumeration, error, true);
                report.observation_complete = false;
                false
            }
        };
        if !run_work && !generation_work {
            let observed = self.pending_reconciliation("managed_cleanup_observation")?;
            report.merge_latest(observed.janitor);
            return Ok(ManagedReconciliation::new(observed.recovery, report));
        }
        if !matches!(
            probe_registry(&self.layout.phase_workspace_root()),
            Ok(true)
        ) {
            let mut deferred = self.pending_reconciliation("managed_containment_unresolved")?;
            report.merge_latest(deferred.janitor);
            report.observation_complete = false;
            deferred = ManagedReconciliation::new(deferred.recovery, report);
            return Ok(deferred);
        }
        let domain = match super::maintenance_ownership::ReferenceDomain::managed(&self.layout) {
            Ok(domain) => domain,
            Err(_) => {
                return self.pending_reconciliation_with_issue(
                    "managed_reference_control_unavailable",
                    report,
                )
            }
        };
        let _mutation =
            match super::maintenance_ownership::ReferenceMutation::try_begin_in_domain(&domain) {
                Ok(Some(mutation)) => mutation,
                Ok(None) | Err(_) => {
                    return self.pending_reconciliation_with_issue(
                        "managed_reference_control_unavailable",
                        report,
                    )
                }
            };
        let _state = self.open_state_lock(LockMode::Exclusive)?;
        let mut active = self.read_active_generation()?;
        let mut recovery = ManagedRecoveryReport {
            observation_complete: true,
            ..ManagedRecoveryReport::default()
        };
        self.recover_runs_locked_typed(&mut active, &mut report, &mut recovery, true, &_mutation)
            .map_err(|primary| append_janitor_secondary(primary, &mut report))?;
        if report.first_issue.is_none() {
            if let Err(error) = self.collect_retired_generations_locked(
                active.as_ref().map(|a| a.generation_id.as_str()),
                &mut report.legacy,
            ) {
                report.issue(ManagedJanitorStage::RetiredGenerationRemoval, error, true);
                report.observation_complete = false;
            }
        } else {
            report.pending = true;
            report.observation_complete = false;
        }
        report.legacy.retired_generations_deleted = report.legacy.retired_deleted;
        report.legacy.retired_generations_pending = report.legacy.retired_pending;
        report.pending |=
            report.legacy.retired_pending > 0 || report.legacy.run_recovery.skipped_locked > 0;
        Ok(ManagedReconciliation::new(recovery, report))
    }
    #[cfg(test)]
    fn pending_reconciliation_with_issue(
        &self,
        reason: &'static str,
        mut issue: ManagedJanitorReport,
    ) -> Result<ManagedReconciliation, NativeError> {
        let observed = self.pending_reconciliation(reason)?;
        issue.merge_latest(observed.janitor);
        issue.observation_complete = false;
        Ok(ManagedReconciliation::new(observed.recovery, issue))
    }
    fn has_reference_run_work(&self) -> Result<bool, NativeError> {
        for entry in fs::read_dir(self.layout.runs_root())? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && entry.file_name().to_string_lossy().starts_with("run-")
                && RunWorkspace::reference_domain(&entry.path())?.is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[cfg(test)]
    fn has_generation_cleanup_work(&self, active: Option<&str>) -> Result<bool, NativeError> {
        #[cfg(test)]
        managed_fault(ManagedFault::GenerationScan)?;
        for entry in fs::read_dir(self.layout.generations_root())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || active.is_some_and(|id| entry.file_name() == format!("gen-{id}").as_str())
            {
                continue;
            }
            let retired = match fs::symlink_metadata(entry.path().join("retired.json")) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e.into()),
            };
            let ready = match fs::symlink_metadata(entry.path().join("READY")) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e.into()),
            };
            if retired || !ready {
                return Ok(true);
            }
        }
        Ok(false)
    }
    #[cfg(test)]
    fn pending_reconciliation(
        &self,
        reason: &'static str,
    ) -> Result<ManagedReconciliation, NativeError> {
        let mut report = ManagedJanitorReport::complete();
        report.deferred_reason = Some(reason);
        for entry in fs::read_dir(self.layout.runs_root())? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            match entry.file_type() {
                Ok(kind)
                    if kind.is_dir() && entry.file_name().to_string_lossy().starts_with("run-") =>
                {
                    report.legacy.run_recovery.skipped_locked += 1
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        let active = self.read_active_generation()?;
        let generation_observation = (|| -> Result<(), NativeError> {
            #[cfg(test)]
            managed_fault(ManagedFault::GenerationScan)?;
            for entry in fs::read_dir(self.layout.generations_root())? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                if active.as_ref().is_some_and(|a| {
                    entry.file_name() == format!("gen-{}", a.generation_id).as_str()
                }) {
                    continue;
                }
                match fs::symlink_metadata(entry.path().join("retired.json")) {
                    Ok(meta) if meta.is_file() => report.legacy.retired_pending += 1,
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(())
        })();
        if let Err(error) = generation_observation {
            report.issue(ManagedJanitorStage::GenerationEnumeration, error, true);
            report.observation_complete = false;
        }
        report.legacy.retired_generations_pending = report.legacy.retired_pending;
        report.pending |=
            report.legacy.run_recovery.skipped_locked > 0 || report.legacy.retired_pending > 0;
        let recovery = ManagedRecoveryReport {
            publishing_recovered: 0,
            skipped_locked: report.legacy.run_recovery.skipped_locked,
            observation_complete: reason == "managed_cleanup_observation"
                && report.legacy.run_recovery.skipped_locked == 0,
        };
        if reason != "managed_cleanup_observation" {
            report.observation_complete = false;
        }
        Ok(ManagedReconciliation::new(recovery, report))
    }

    pub(crate) fn read_active_generation(&self) -> Result<Option<ActiveGeneration>, NativeError> {
        let path = self.layout.active_pointer_path();
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(NativeError::InvalidInput(format!(
                    "active generation pointer is not a regular file: {}",
                    path.display()
                )))
            }
            Ok(_) => {}
        }
        let text = fs::read_to_string(path)?;
        let active: ActiveGeneration = serde_json::from_str(&text)?;
        if active.schema_version != MANAGED_SCHEMA_VERSION {
            return Err(NativeError::InvalidInput(format!(
                "active generation pointer has schema_version {}, expected {}",
                active.schema_version, MANAGED_SCHEMA_VERSION
            )));
        }
        validate_generation_id(&active.generation_id)?;
        Ok(Some(active))
    }

    pub(crate) fn prepare_candidate<S: GenerationStats>(
        &self,
        workspace: &RunWorkspace,
        candidate: &ManagedCandidate,
        graph_summary: &S,
    ) -> Result<(), NativeError> {
        ensure_registry_empty(&self.layout.phase_workspace_root())
            .map_err(NativeError::InvalidInput)?;
        validate_candidate_generation(&candidate.paths)?;
        write_generation_metadata_with_stats(
            &candidate.paths,
            workspace.journal()?.base_generation_id,
            graph_summary.node_count(),
            graph_summary.edge_count(),
        )?;
        let mut ready = create_new_file(&candidate.paths.ready_path())?;
        ready.write_all(b"prepared\n")?;
        ready.sync_all()?;
        sync_dir(candidate.paths.root())?;
        workspace.mark_candidate_ready()
    }

    fn publish_candidate_inner_typed(
        &self,
        workspace: &RunWorkspace,
        candidate: &ManagedCandidate,
    ) -> Result<ManagedJanitorReport, PublicationFailure> {
        ensure_registry_empty(&self.layout.phase_workspace_root())
            .map_err(|e| PublicationFailure::before(NativeError::InvalidInput(e)))?;
        let _state = self
            .open_state_lock(LockMode::Exclusive)
            .map_err(PublicationFailure::before)?;
        let mut active = self
            .read_active_generation()
            .map_err(PublicationFailure::before)?;
        let journal = workspace.journal().map_err(PublicationFailure::before)?;
        self.finish_run_transaction_locked(&journal, workspace.root(), &mut active, false)?;
        // No optional predecessor marker/removal occurs before the API proves
        // native coherence. Its NativeVerifiedGeneration capability is the
        // sole production entry to new Origin/FINAL disposition.
        let report = ManagedJanitorReport::complete();
        debug_assert_eq!(
            active.as_ref().map(|a| a.generation_id.as_str()),
            Some(candidate.generation_id.as_str())
        );
        Ok(report)
    }

    fn recover_runs_locked_typed(
        &self,
        active: &mut Option<ActiveGeneration>,
        report: &mut ManagedJanitorReport,
        recovery: &mut ManagedRecoveryReport,
        perform_janitor: bool,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
    ) -> Result<(), NativeError> {
        let runs_root = self.layout.runs_root();
        for entry in fs::read_dir(&runs_root)? {
            // A worker can finish and remove its run workspace after the
            // directory iterator yields an entry. Windows reports that
            // observation as ERROR_PATH_NOT_FOUND; only that race is benign.
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let path = entry.path();
            let is_directory = match entry.file_type() {
                Ok(file_type) => file_type.is_dir(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if !is_directory {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with("run-") {
                continue;
            }

            // Neither a vanished parent nor a lease removed by cooperative
            // retirement may be recreated by an ordinary recovery reader.
            let lease =
                match try_open_existing_locked(&path.join("lease.lock"), LockMode::Exclusive) {
                    Ok(Some(lease)) => lease,
                    Ok(None) => {
                        report.legacy.run_recovery.skipped_locked += 1;
                        recovery.skipped_locked += 1;
                        recovery.observation_complete = false;
                        continue;
                    }
                    Err(error) if is_not_found_error(&error) => continue,
                    Err(error) => return Err(error),
                };
            let expected_domain =
                super::maintenance_ownership::ReferenceDomain::managed(&self.layout)?;
            match RunWorkspace::reference_domain(&path)? {
                Some(domain) if domain.same_scope(&expected_domain) => {}
                _ => {
                    report.legacy.run_recovery.skipped_locked += 1;
                    report.observation_complete = false;
                    recovery.skipped_locked += 1;
                    recovery.observation_complete = false;
                    continue;
                }
            }
            // Optional registration damage must not preempt required journal
            // replay. A validated owner-terminal control, however, is the
            // actual resolution receipt and prevents replay on later reads.
            let registration = if !perform_janitor {
                match super::maintenance_controls::WorkspaceRegistration::recover_run(
                    &path,
                    &expected_domain,
                    &lease,
                    mutation,
                )
                .and_then(|registration| {
                    let terminal = registration
                        .as_ref()
                        .map(|registration| registration.terminal_complete())
                        .transpose()?
                        .unwrap_or(false);
                    Ok((registration, terminal))
                }) {
                    Ok((_, true)) => {
                        report.pending = true;
                        report.observation_complete = false;
                        continue;
                    }
                    Ok((registration, false)) => registration,
                    Err(error) => {
                        report.issue(ManagedJanitorStage::TerminalRunRemoval, error, true);
                        None
                    }
                }
            } else {
                None
            };
            let journal = match read_run_journal_or_default(&path, name) {
                Ok(journal) => journal,
                Err(error) if is_not_found_error(&error) => {
                    drop(lease);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let resolution = if matches!(journal.phase, RunPhase::Publishing | RunPhase::Published)
            {
                match self.finish_run_transaction_locked(&journal, &path, active, true) {
                    Ok(resolution) => {
                        report.legacy.run_recovery.publishing_recovered += 1;
                        recovery.publishing_recovered += 1;
                        Some(resolution)
                    }
                    Err(failure) => return Err(failure.source),
                }
            } else {
                None
            };
            if !perform_janitor {
                if let (Some(registration), Some(resolution)) = (&registration, &resolution) {
                    let disposition = match resolution {
                        RunResolution::Committed { .. } => {
                            super::maintenance_controls::WorkspaceDisposition::PublishedConsumed
                        }
                        RunResolution::Obsolete { .. } => {
                            super::maintenance_controls::WorkspaceDisposition::AbortedNotPublished
                        }
                    };
                    let closure = (|| -> Result<(), NativeError> {
                        registration.source_change(&lease, mutation, false, true)?;
                        // Publishing closes ordinary metadata startup before its
                        // old journal write. Recovery must produce its own resolved
                        // journal and creator token; observing that old inode is
                        // never metadata ownership or a terminal receipt.
                        let resolved = RunJournal {
                            run_id: journal.run_id.clone(),
                            phase: match resolution {
                                RunResolution::Committed { .. } => RunPhase::Published,
                                RunResolution::Obsolete { .. } => RunPhase::Failed,
                            },
                            base_generation_id: journal.base_generation_id.clone(),
                            candidate_generation_id: journal.candidate_generation_id.clone(),
                            active_generation_id: active.as_ref().map(|a| a.generation_id.clone()),
                            last_error: None,
                        };
                        let publication = crate::process_lifecycle::write_private_json_recorded(
                            &path.join("journal.json"),
                            &resolved,
                        )
                        .map_err(NativeError::InvalidInput)?
                        .ok_or_else(|| {
                            NativeError::InvalidInput(
                                "recovered journal creation is unverified".into(),
                            )
                        })?;
                        registration.record_metadata(&lease, mutation, &publication)?;
                        sync_dir(&path)?;
                        registration.terminal(&lease, mutation, disposition)
                    })();
                    if let Err(error) = closure {
                        report.issue(ManagedJanitorStage::TerminalRunRemoval, error, true);
                    }
                }
                // This run was actually locked and its required transaction
                // was resolved. Retaining it for bounded optional collection
                // does not invent a skipped/live recovery lock.
                report.pending = true;
                report.observation_complete = false;
                continue;
            }
            drop(lease);
            if let Some(resolution) = resolution {
                self.finish_run_janitor_locked(
                    &path,
                    resolution,
                    active.as_ref().map(|a| a.generation_id.as_str()),
                    true,
                    report,
                );
            } else if report.first_issue.is_none() {
                match super::run_workspace::ensure_persisted_cleanup_allowed(&path)
                    .and_then(|()| remove_run_root_confined(&path, &runs_root))
                {
                    Ok(()) => report.legacy.run_recovery.deleted += 1,
                    Err(error) => {
                        report.issue(ManagedJanitorStage::TerminalRunRemoval, error, true);
                        report.legacy.run_recovery.skipped_locked += 1;
                    }
                }
            } else {
                report.legacy.run_recovery.skipped_locked += 1;
                report.pending = true;
            }
        }
        Ok(())
    }

    fn probe_ready(paths: &GenerationPaths) -> ReadyProbe {
        match fs::symlink_metadata(paths.root()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ReadyProbe::Absent,
            Err(error) => ReadyProbe::Invalid(error.into()),
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                ReadyProbe::Invalid(NativeError::InvalidInput(format!(
                    "invalid generation root {}",
                    paths.root().display()
                )))
            }
            Ok(_) => match validate_ready_generation(paths) {
                Ok(()) => ReadyProbe::Valid,
                Err(error) => ReadyProbe::Invalid(error),
            },
        }
    }
    fn require_ready(paths: &GenerationPaths) -> Result<(), NativeError> {
        match Self::probe_ready(paths) {
            ReadyProbe::Valid => Ok(()),
            ReadyProbe::Invalid(error) => Err(error),
            ReadyProbe::Absent => Err(NativeError::InvalidInput(format!(
                "required generation is absent: {}",
                paths.root().display()
            ))),
        }
    }
    fn finish_run_transaction_locked(
        &self,
        journal: &RunJournal,
        run_root: &Path,
        active: &mut Option<ActiveGeneration>,
        recovery: bool,
    ) -> Result<RunResolution, PublicationFailure> {
        let before = PublicationFailure::before;
        ensure_registry_empty(&self.layout.phase_workspace_root())
            .map_err(|e| before(NativeError::InvalidInput(e)))?;
        let candidate_id = journal.candidate_generation_id.as_ref().ok_or_else(|| {
            before(NativeError::InvalidInput(
                "publishing run is missing its candidate generation".into(),
            ))
        })?;
        validate_generation_id(candidate_id).map_err(before)?;
        if let Some(base) = journal.base_generation_id.as_deref() {
            validate_generation_id(base).map_err(before)?;
        }
        let published = self.layout.generation(candidate_id).map_err(before)?;
        let active_id = active.as_ref().map(|a| a.generation_id.clone());
        if active_id.as_deref() == Some(candidate_id.as_str()) {
            Self::require_ready(&published).map_err(PublicationFailure::ambiguous)?;
            // Complete interrupted mandatory barriers without rewriting identity,
            // timestamps, metadata or READY. Readability is not a commit ack.
            sync_parent_dir(published.root()).map_err(PublicationFailure::ambiguous)?;
            self.complete_root_sync()
                .map_err(PublicationFailure::ambiguous)?;
            return Ok(RunResolution::Committed {
                previous: journal.base_generation_id.clone(),
            });
        }
        if active_id != journal.base_generation_id {
            if !recovery {
                return Err(before(NativeError::InvalidInput(
                    ManagedValidationError::StaleBaseGeneration {
                        expected: journal.base_generation_id.clone(),
                        found: active_id,
                    }
                    .to_string(),
                )));
            }
            if let Some(current) = active.as_ref() {
                Self::require_ready(
                    &self
                        .layout
                        .generation(&current.generation_id)
                        .map_err(before)?,
                )
                .map_err(before)?;
            } else {
                return Err(before(NativeError::InvalidInput(
                    "cannot resolve stale publishing evidence without a valid active generation"
                        .into(),
                )));
            }
            return Ok(RunResolution::Obsolete { published });
        }
        match Self::probe_ready(&published) {
            ReadyProbe::Invalid(error) => return Err(before(error)),
            ReadyProbe::Valid => {}
            ReadyProbe::Absent => {
                let staged = CandidateGenerationLayout::new(
                    run_root.join("candidate"),
                    candidate_id.clone(),
                )
                .generation_paths();
                Self::require_ready(&staged).map_err(before)?;
                self.promote_candidate_generation_locked(&staged, &published)
                    .map_err(PublicationFailure::ambiguous)?;
            }
        }
        // Recovered published-only state may still need promotion's directory
        // barriers; complete them before making its pointer authoritative.
        sync_parent_dir(published.root()).map_err(PublicationFailure::ambiguous)?;
        let activated = self
            .activate_published_generation_locked(&published)
            .map_err(PublicationFailure::ambiguous)?;
        *active = Some(activated);
        Ok(RunResolution::Committed {
            previous: journal.base_generation_id.clone(),
        })
    }

    fn finish_run_janitor_locked(
        &self,
        run_root: &Path,
        resolution: RunResolution,
        active_id: Option<&str>,
        cleanup_root: bool,
        report: &mut ManagedJanitorReport,
    ) {
        if report.first_issue.is_some() {
            if cleanup_root {
                report.legacy.run_recovery.skipped_locked += 1;
            }
            report.pending = true;
            return;
        }
        if cleanup_root {
            if let Err(error) = super::run_workspace::ensure_persisted_cleanup_allowed(run_root) {
                report.issue(ManagedJanitorStage::TerminalRunRemoval, error, true);
                report.legacy.run_recovery.skipped_locked += 1;
                return;
            }
        }
        let action = match resolution {
            RunResolution::Committed { previous } => self
                .finalize_previous_retirement(previous.as_deref())
                .map_err(|e| (ManagedJanitorStage::PreviousRetirementMarker, e)),
            RunResolution::Obsolete { published } => (|| -> Result<(), NativeError> {
                if published.root().exists() {
                    write_json_atomically(
                        &published.retired_path(),
                        &GenerationRetirement {
                            schema_version: MANAGED_SCHEMA_VERSION,
                            retired_at_ms: unix_time_ms(),
                        },
                    )?;
                    self.retire_or_delete_generation_locked(&published, active_id)?;
                }
                Ok(())
            })()
            .map_err(|e| (ManagedJanitorStage::RetiredGenerationRemoval, e)),
        };
        if let Err((stage, error)) = action {
            report.issue(stage, error, true);
            if cleanup_root {
                report.legacy.run_recovery.skipped_locked += 1;
            }
            return;
        }
        if cleanup_root {
            match remove_run_root_confined(run_root, &self.layout.runs_root()) {
                Ok(()) => report.legacy.run_recovery.deleted += 1,
                Err(error) => {
                    report.issue(ManagedJanitorStage::TerminalRunRemoval, error, true);
                    report.legacy.run_recovery.skipped_locked += 1;
                }
            }
        }
    }

    fn promote_candidate_generation_locked(
        &self,
        staged: &GenerationPaths,
        published: &GenerationPaths,
    ) -> Result<(), NativeError> {
        validate_ready_generation(staged)?;
        if published.root().exists() {
            return Err(NativeError::InvalidInput(format!(
                "generation destination already exists: {}",
                published.root().display()
            )));
        }
        if let Some(parent) = published.root().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(staged.root(), published.root())?;
        sync_parent_dir(published.root())?;
        Ok(())
    }

    fn activate_published_generation_locked(
        &self,
        published: &GenerationPaths,
    ) -> Result<ActiveGeneration, NativeError> {
        validate_ready_generation(published)?;
        let mut metadata = read_generation_metadata(published)?;
        let now_ms = unix_time_ms();
        metadata.published_at_ms = now_ms;
        write_json_atomically(&published.metadata_path(), &metadata)?;
        fs::write(published.ready_path(), format!("{now_ms}\n"))?;
        sync_dir(published.root())?;

        let active = ActiveGeneration {
            schema_version: MANAGED_SCHEMA_VERSION,
            generation_id: published.generation_id().to_string(),
            published_at: format!("unix:{now_ms}"),
            activated_at_ms: now_ms,
        };
        self.write_active_pointer(&active)?;
        self.complete_root_sync()?;
        Ok(active)
    }

    fn write_active_pointer(&self, active: &ActiveGeneration) -> Result<(), NativeError> {
        #[cfg(test)]
        if MANAGED_FAULT.with(|f| f.get()) == ManagedFault::PointerAfterRename {
            return crate::storage::atomic::write_json_atomically_with_fault(
                &self.layout.active_pointer_path(),
                active,
                crate::storage::atomic::AtomicWriteFailure::AfterRename,
            );
        }
        write_json_atomically(&self.layout.active_pointer_path(), active)
    }
    fn complete_root_sync(&self) -> Result<(), NativeError> {
        #[cfg(test)]
        if SCOPED_MANAGED_SYNC_FAULT
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some_and(|identity| {
                super::locks::path_identity(self.layout.storage_root()).ok() == Some(identity)
            })
        {
            return Err(std::io::Error::other("injected mandatory managed sync failure").into());
        }
        #[cfg(test)]
        managed_fault(ManagedFault::MandatorySync)?;
        sync_dir(self.layout.storage_root())
    }
    fn finalize_previous_retirement(
        &self,
        previous_active_id: Option<&str>,
    ) -> Result<(), NativeError> {
        if let Some(previous_id) = previous_active_id {
            let retired = self.layout.generation(previous_id)?;
            if retired.root().exists() {
                write_json_atomically(
                    &retired.retired_path(),
                    &GenerationRetirement {
                        schema_version: MANAGED_SCHEMA_VERSION,
                        retired_at_ms: unix_time_ms(),
                    },
                )?;
                sync_dir(retired.root())?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn collect_retired_generations_locked(
        &self,
        active_id: Option<&str>,
        report: &mut ManagedCleanupReport,
    ) -> Result<(), NativeError> {
        #[cfg(test)]
        managed_fault(ManagedFault::GenerationScan)?;
        let generations_root = self.layout.generations_root();
        if !generations_root.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(generations_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with("gen-") {
                continue;
            }
            let generation_id = name.trim_start_matches("gen-");
            if active_id == Some(generation_id) {
                continue;
            }
            let generation = self.layout.generation(generation_id)?;
            self.retire_or_delete_generation_locked(&generation, active_id)?;
            if generation.root().exists() {
                report.retired_pending += 1;
            } else {
                report.retired_deleted += 1;
            }
        }
        Ok(())
    }

    fn retire_or_delete_generation_locked(
        &self,
        generation: &GenerationPaths,
        active_id: Option<&str>,
    ) -> Result<(), NativeError> {
        if active_id == Some(generation.generation_id()) || !generation.root().exists() {
            return Ok(());
        }
        let should_collect =
            generation.retired_path().exists() || !generation.ready_path().exists();
        if !should_collect {
            return Ok(());
        }
        if let Some(lease) = try_open_locked(generation.lease_path(), LockMode::Exclusive)? {
            drop(lease);
            ensure_directory_without_symlinks(generation.root())?;
            let removal = remove_path_without_symlinks(generation.root());
            // Windows can report ACCESS_DENIED (5) or SHARING_VIOLATION (32)
            // while a reader still has a database file open after releasing
            // its generation lease. Retry collection on the next cleanup.
            #[cfg(windows)]
            if matches!(&removal, Err(NativeError::Io(io)) if matches!(io.raw_os_error(), Some(5 | 32)))
            {
                // The recursive delete may already have removed the
                // retirement marker. Restore it so a later cleanup will
                // retry even if the ready marker survived.
                write_json_atomically(
                    &generation.retired_path(),
                    &GenerationRetirement {
                        schema_version: MANAGED_SCHEMA_VERSION,
                        retired_at_ms: unix_time_ms(),
                    },
                )?;
                return Ok(());
            }
            removal?;
        }
        Ok(())
    }

    fn open_state_lock(&self, mode: LockMode) -> Result<StateLease, NativeError> {
        open_locked(self.layout.state_lock_path(), mode)
    }

    fn open_writer_lock(&self) -> Result<WriterLease, NativeError> {
        open_locked(self.layout.writer_lock_path(), LockMode::Exclusive)
    }

    fn read_active_with_shared_lock(&self) -> Result<Option<ActiveGeneration>, NativeError> {
        let _lease = self.open_state_lock(LockMode::Shared)?;
        self.read_active_generation()
    }
}

pub(crate) struct GraphStorage;

impl GraphStorage {
    pub(crate) fn managed(storage_root: impl Into<PathBuf>) -> ManagedStore {
        ManagedStore::new(ManagedLayout::new(storage_root))
    }
}

fn write_generation_metadata_with_stats(
    paths: &GenerationPaths,
    base_generation_id: Option<String>,
    node_count: usize,
    edge_count: usize,
) -> Result<GenerationMetadata, NativeError> {
    validate_candidate_generation(paths)?;
    let sizes = generation_sizes(paths)?;
    let metadata = GenerationMetadata {
        schema_version: MANAGED_SCHEMA_VERSION,
        generation_id: paths.generation_id().to_string(),
        created_at_ms: unix_time_ms(),
        published_at_ms: 0,
        base_generation_id,
        logical_size_bytes: sizes.logical_size_bytes,
        physical_size_bytes: sizes.physical_size_bytes,
        node_count,
        edge_count,
    };
    write_json_atomically(&paths.metadata_path(), &metadata)?;
    sync_dir(paths.root())?;
    Ok(metadata)
}

fn read_generation_metadata(paths: &GenerationPaths) -> Result<GenerationMetadata, NativeError> {
    let text = fs::read_to_string(paths.metadata_path())?;
    let metadata: GenerationMetadata = serde_json::from_str(&text)?;
    if metadata.schema_version != MANAGED_SCHEMA_VERSION
        || metadata.generation_id != paths.generation_id()
    {
        return Err(NativeError::InvalidInput(format!(
            "generation metadata does not match {}",
            paths.root().display()
        )));
    }
    Ok(metadata)
}

fn validate_candidate_generation(paths: &GenerationPaths) -> Result<(), NativeError> {
    ensure_regular_file(&paths.db_path())?;
    ensure_regular_file(&paths.manifest_path())?;
    ensure_directory_without_symlinks(paths.root())?;
    let manifest: SearchManifestEnvelope =
        serde_json::from_str(&fs::read_to_string(paths.manifest_path())?)?;
    if let Some(search_backend) = manifest.search_backend.as_ref() {
        validate_search_sidecar_checksums(&paths.db_path(), &search_backend.files)?;
    }
    Ok(())
}

fn validate_search_sidecar_checksums(
    db_path: &Path,
    expected: &BTreeMap<String, String>,
) -> Result<(), NativeError> {
    let search_suffixes = crate::storage::layout::DIRECT_DB_SIDECAR_SUFFIXES
        .iter()
        .copied()
        .filter(|suffix| suffix.starts_with("search."))
        .collect::<Vec<_>>();
    if expected.len() != search_suffixes.len()
        || search_suffixes
            .iter()
            .any(|suffix| !expected.contains_key(*suffix))
    {
        return Err(NativeError::InvalidInput(
            "managed search sidecar metadata has an incomplete file set".to_string(),
        ));
    }
    for suffix in search_suffixes {
        let path = PathBuf::from(format!("{}.{}", db_path.display(), suffix));
        ensure_regular_file(&path)?;
        if sha256_path(&path)? != expected[suffix] {
            return Err(NativeError::InvalidInput(format!(
                "managed search sidecar checksum mismatch: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn sha256_path(path: &Path) -> Result<String, NativeError> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_ready_generation(paths: &GenerationPaths) -> Result<(), NativeError> {
    validate_candidate_generation(paths)?;
    if !paths.ready_path().exists() {
        return Err(NativeError::InvalidInput(
            ManagedValidationError::MissingReady(paths.ready_path()).to_string(),
        ));
    }
    ensure_regular_file(&paths.ready_path())?;
    read_generation_metadata(paths)?;
    Ok(())
}

fn generation_sizes(paths: &GenerationPaths) -> Result<FileSizeSummary, NativeError> {
    let mut logical = 0_u64;
    let mut physical = 0_u64;
    for path in direct_bundle_paths(&paths.db_path()) {
        if path.exists() {
            let metadata = fs::metadata(&path)?;
            logical += metadata.len();
            physical += allocated_size(&metadata);
        }
    }
    let manifest = paths.manifest_path();
    if manifest.exists() {
        let metadata = fs::metadata(&manifest)?;
        logical += metadata.len();
        physical += allocated_size(&metadata);
    }
    Ok(FileSizeSummary {
        logical_size_bytes: logical,
        physical_size_bytes: physical,
    })
}

fn ensure_regular_file(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to use symlinked file {}",
            path.display()
        )));
    }
    if metadata.is_file() {
        Ok(())
    } else {
        Err(NativeError::InvalidInput(format!(
            "expected regular file {}",
            path.display()
        )))
    }
}

fn ensure_directory_without_symlinks(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to use symlinked path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            let child_metadata = fs::symlink_metadata(&child)?;
            if child_metadata.file_type().is_symlink() {
                return Err(NativeError::InvalidInput(format!(
                    "refusing to use symlinked path {}",
                    child.display()
                )));
            }
            if child_metadata.is_dir() {
                ensure_directory_without_symlinks(&child)?;
            }
        }
    }
    Ok(())
}

fn remove_path_without_symlinks(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            remove_path_without_symlinks(&entry?.path())?;
        }
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn read_run_journal_or_default(path: &Path, name: &str) -> Result<RunJournal, NativeError> {
    let journal_path = path.join("journal.json");
    let text = match fs::read_to_string(&journal_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RunJournal {
                run_id: name.trim_start_matches("run-").to_string(),
                phase: RunPhase::CleanupPending,
                base_generation_id: None,
                candidate_generation_id: None,
                active_generation_id: None,
                last_error: Some("missing journal".to_string()),
            });
        }
        Err(error) => return Err(error.into()),
    };
    Ok(serde_json::from_str(&text)?)
}

fn is_not_found_error(error: &NativeError) -> bool {
    matches!(error, NativeError::Io(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn sync_parent_dir(path: &Path) -> Result<(), NativeError> {
    let parent = path.parent().ok_or_else(|| {
        NativeError::InvalidInput(format!("path {} has no parent", path.display()))
    })?;
    sync_dir(path)?;
    sync_dir(parent)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct FileSizeSummary {
    logical_size_bytes: u64,
    physical_size_bytes: u64,
}

#[cfg(unix)]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn managed_schema_version() -> u64 {
    MANAGED_SCHEMA_VERSION
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn abort_workspace_after_error(workspace: RunWorkspace, primary: NativeError) -> NativeError {
    match workspace.abort(Some(primary.to_string())) {
        Ok(()) => primary,
        Err(cleanup) => NativeError::InvalidInput(format!("{primary}; cleanup failed: {cleanup}")),
    }
}

#[cfg(test)]
fn append_janitor_secondary(
    primary: NativeError,
    report: &mut ManagedJanitorReport,
) -> NativeError {
    match report.first_issue.take() {
        None => primary,
        Some(issue) => {
            NativeError::InvalidInput(format!("{primary}; cleanup failed: {}", issue.source))
        }
    }
}

#[cfg(test)]
mod transaction_janitor_tests {
    use super::*;
    use crate::protocol::GraphSummary;

    fn store(label: &str) -> (PathBuf, ManagedStore) {
        let root = std::env::temp_dir().join(format!(
            "managed-role-{label}-{}-{}",
            std::process::id(),
            unix_time_ms()
        ));
        fs::create_dir(&root).unwrap();
        (
            root.clone(),
            ManagedStore::new(ManagedLayout::new(root.join("storage"))),
        )
    }
    #[test]
    fn managed_absent_read_probe_preserves_genuine_first_producer_root_creation() {
        let (root, store) = store("noncreating-absent-read");
        assert!(!store.layout.storage_root().exists());
        assert!(store.resolve_active_read().unwrap().is_none());
        assert!(
            !store.layout.storage_root().exists(),
            "read probe created producer namespace"
        );
        let session = store.begin_write().unwrap();
        session.abort(None).unwrap();
        assert!(
            matches!(
                super::super::maintenance_ownership::observe_reference_epoch(
                    &super::super::maintenance_ownership::ReferenceDomain::managed(store.layout())
                        .unwrap()
                )
                .unwrap(),
                super::super::maintenance_ownership::EpochObservation::Verified(_)
            ),
            "actual first producer did not initialize its fresh reference domain"
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn prepared(store: &ManagedStore) -> ManagedWriteSession {
        let mut session = store.begin_write().unwrap();
        fs::write(session.candidate_db_path(), b"storage ABI fixture").unwrap();
        fs::write(session.candidate_manifest_path(), b"{}").unwrap();
        session
            .mark_ready_with_stats(&GraphSummary::default())
            .unwrap();
        session
    }
    fn leave_publishing(mut session: ManagedWriteSession, promoted: bool) -> PathBuf {
        let workspace = session.workspace.take().unwrap();
        workspace
            .mark_phase(RunPhase::Publishing, None, None)
            .unwrap();
        if promoted {
            let _state = session.store.open_state_lock(LockMode::Exclusive).unwrap();
            let published = session
                .store
                .layout
                .generation(&session.candidate.generation_id)
                .unwrap();
            session
                .store
                .promote_candidate_generation_locked(&session.candidate.paths, &published)
                .unwrap();
        }
        let root = workspace.root().to_path_buf();
        workspace.retain_for_recovery();
        session.finish_without_cleanup();
        root
    }

    #[test]
    fn managed_read_recovery_does_not_recreate_a_partially_retired_run_lease() {
        use crate::storage::maintenance_budget::{CleanupMemory, MutationGrant};
        use crate::storage::{
            maintenance_controls as controls, maintenance_workspace_gate as gate,
        };
        use std::time::Duration;
        let (root, store) = store("noncreating-partial-retirement");
        // A genuine still-Publishing sibling forces required read recovery;
        // it must remain recoverable while another closed run is being removed.
        let sibling = leave_publishing(prepared(&store), false);
        let domain =
            super::super::maintenance_ownership::ReferenceDomain::managed(store.layout()).unwrap();
        let mut run = RunWorkspace::create_scoped(store.layout.runs_root(), &domain, None).unwrap();
        run.set_cleanup_barrier(store.layout.phase_workspace_root())
            .unwrap();
        let retired = run.retire_for_maintenance().unwrap();
        // Isolate fixed root transport after receipt collection. Every receipt
        // here was created by this fixture, as in the root-transport unit tests.
        for entry in fs::read_dir(retired.join(".workspace-origins")).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
        let mut terminal =
            controls::prepare_terminal_workspace_boxed(&retired, &domain, &memory).unwrap();
        let mut initial =
            gate::prepare_workspace_root_removal(&retired, &domain, Some(&terminal), &memory)
                .unwrap();
        let mut grant = MutationGrant::for_test(100, Duration::from_millis(100));
        gate::begin_workspace_root_removal(&mut terminal, &mut initial, &mut grant).unwrap();
        drop(initial);
        drop(terminal);
        let mut observed_gap = false;
        for _ in 0..16 {
            let mut next = gate::prepare_pending_workspace_root_removal(&domain, &memory)
                .unwrap()
                .unwrap();
            let mut grant = MutationGrant::for_test(100, Duration::from_millis(100));
            let progress =
                gate::advance_workspace_root_removal(&mut next, &mut grant, 0, &mut |_, _| {
                    panic!("root should not complete before the lease/rmdir grant seam")
                })
                .unwrap();
            if retired.exists() && !retired.join("lease.lock").exists() {
                assert_eq!(progress, gate::WorkspaceRemovalProgress::Pending);
                observed_gap = true;
                break;
            }
        }
        assert!(
            observed_gap,
            "did not reach actual cooperative lease-removal seam"
        );
        let recovered = store.recover_for_read().unwrap();
        assert_eq!(recovered.recovery.publishing_recovered, 1);
        assert!(recovered.janitor.first_issue.is_none(), "{recovered:?}");
        assert!(retired.exists());
        assert!(
            !retired.join("lease.lock").exists(),
            "recovery recreated a removed lease inode"
        );
        assert!(
            sibling.exists(),
            "required read recovery removed retained sibling payload"
        );
        let mut complete = false;
        for _ in 0..8 {
            let mut next = gate::prepare_pending_workspace_root_removal(&domain, &memory)
                .unwrap()
                .unwrap();
            let existed = retired.exists();
            let mut grant = MutationGrant::for_test(100, Duration::from_millis(100));
            let progress = gate::advance_workspace_root_removal(
                &mut next,
                &mut grant,
                0,
                &mut |outcome, _| {
                    match outcome {
                        gate::WorkspaceRootOutcome::Removed(_) => assert!(existed),
                        gate::WorkspaceRootOutcome::RecoveredMissing(_) => assert!(!existed),
                    }
                    assert!(!retired.exists());
                    Ok(())
                },
            )
            .unwrap();
            if progress == gate::WorkspaceRemovalProgress::Complete {
                complete = true;
                break;
            }
        }
        assert!(complete);
        assert!(!retired.exists());
        assert!(
            gate::prepare_pending_workspace_root_removal(&domain, &memory)
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_read_recovery_closes_registered_run_without_replaying() {
        use crate::storage::maintenance_budget::CleanupMemory;
        for promoted in [false, true] {
            let (root, store) = store(if promoted {
                "read-published"
            } else {
                "read-staged"
            });
            let session = prepared(&store);
            let id = session.candidate.generation_id.clone();
            let run = leave_publishing(session, promoted);
            assert!(
                run.join(".workspace.binding").is_file(),
                "fixture must have genuine producer registration"
            );
            let domain =
                super::super::maintenance_ownership::ReferenceDomain::managed(store.layout())
                    .unwrap();
            let memory = CleanupMemory::new(8 * 1024 * 1024).unwrap();
            assert!(
                super::super::maintenance_controls::prepare_terminal_workspace_boxed(
                    &run, &domain, &memory
                )
                .is_err()
            );
            let recovered = store.recover_for_read().unwrap();
            assert_eq!(recovered.recovery.publishing_recovered, 1);
            assert!(recovered.janitor.first_issue.is_none(), "{recovered:?}");
            assert!(
                run.exists(),
                "read recovery traversed/removed retained payload"
            );
            assert_eq!(
                store
                    .read_active_generation()
                    .unwrap()
                    .unwrap()
                    .generation_id,
                id
            );
            let terminal = super::super::maintenance_controls::prepare_terminal_workspace_boxed(
                &run, &domain, &memory,
            )
            .unwrap();
            drop(terminal);
            let pointer = fs::read(store.layout.active_pointer_path()).unwrap();
            let repeated = store.recover_for_read().unwrap();
            assert_eq!(
                repeated.recovery.publishing_recovered, 0,
                "owner-terminal recovered publication replayed on next read"
            );
            assert_eq!(
                fs::read(store.layout.active_pointer_path()).unwrap(),
                pointer
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn managed_typed_recovery_accepts_staged_only_and_published_only_seams() {
        for promoted in [false, true] {
            let (root, store) = store(if promoted { "published" } else { "staged" });
            let session = prepared(&store);
            let id = session.candidate.generation_id.clone();
            let run = leave_publishing(session, promoted);
            let result = store.cleanup_typed().unwrap();
            assert_eq!(result.recovery.publishing_recovered, 1);
            assert!(result.recovery.observation_complete);
            assert!(result.janitor.first_issue.is_none());
            assert_eq!(
                store
                    .read_active_generation()
                    .unwrap()
                    .unwrap()
                    .generation_id,
                id
            );
            assert!(!run.exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn managed_typed_ambiguous_pointer_error_retains_bytes_and_requires_durability_recovery() {
        let (root, store) = store("ambiguous");
        let mut session = prepared(&store);
        let id = session.candidate.generation_id.clone();
        let run = session.workspace.as_ref().unwrap().root().to_path_buf();
        let fault = inject_managed_fault(ManagedFault::PointerAfterRename);
        let error = session
            .publish_with_stats_typed(&GraphSummary::default())
            .unwrap_err();
        assert!(error.to_string().contains("after rename"));
        drop(fault);
        drop(session);
        let generation = store.layout.generation(&id).unwrap();
        assert!(
            !generation.retired_path().exists(),
            "uncertain pointer failure marked candidate retired"
        );
        let evidence = || {
            [
                fs::read(run.join("journal.json")).unwrap(),
                fs::read(store.layout.active_pointer_path()).unwrap(),
                fs::read(generation.metadata_path()).unwrap(),
                fs::read(generation.ready_path()).unwrap(),
            ]
        };
        let before = evidence();
        assert!(
            try_open_locked(store.layout.writer_lock_path(), LockMode::Exclusive)
                .unwrap()
                .is_some()
        );
        let fault = inject_managed_fault(ManagedFault::MandatorySync);
        assert!(store
            .cleanup_typed()
            .unwrap_err()
            .to_string()
            .contains("mandatory managed sync"));
        assert_eq!(evidence(), before);
        drop(fault);
        let result = store.cleanup_typed().unwrap();
        assert_eq!(result.recovery.publishing_recovered, 1);
        assert_eq!(
            fs::read(store.layout.active_pointer_path()).unwrap(),
            before[1]
        );
        assert_eq!(fs::read(generation.metadata_path()).unwrap(), before[2]);
        assert_eq!(fs::read(generation.ready_path()).unwrap(), before[3]);
        assert!(!run.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_typed_required_corrupt_or_missing_evidence_is_not_an_absent_alternative() {
        for corrupt_present in [false, true] {
            let (root, store) = store(if corrupt_present {
                "corrupt"
            } else {
                "missing"
            });
            let session = prepared(&store);
            let staged = session.candidate.paths.root().to_path_buf();
            let published = store
                .layout
                .generation(&session.candidate.generation_id)
                .unwrap();
            let run = leave_publishing(session, false);
            if corrupt_present {
                fs::create_dir_all(published.root()).unwrap();
                fs::write(published.db_path(), b"invalid present evidence").unwrap();
            } else {
                fs::remove_dir_all(&staged).unwrap();
            }
            let journal = fs::read(run.join("journal.json")).unwrap();
            assert!(store.cleanup_typed().is_err());
            assert_eq!(fs::read(run.join("journal.json")).unwrap(), journal);
            if corrupt_present {
                assert!(
                    staged.exists(),
                    "silently preferred/replaced staged alternative"
                );
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn managed_typed_generation_issue_never_preempts_required_replay_or_its_error() {
        for corrupt_run in [false, true] {
            let (root, store) = store(if corrupt_run {
                "later-error"
            } else {
                "later-success"
            });
            let session = prepared(&store);
            let id = session.candidate.generation_id.clone();
            let run = leave_publishing(session, false);
            if corrupt_run {
                let mut journal: RunJournal =
                    serde_json::from_slice(&fs::read(run.join("journal.json")).unwrap()).unwrap();
                journal.candidate_generation_id = Some("../invalid".into());
                write_json_atomically(&run.join("journal.json"), &journal).unwrap();
            }
            let fault = inject_managed_fault(ManagedFault::GenerationScan);
            if corrupt_run {
                let error = store.cleanup_typed().unwrap_err().to_string();
                assert!(
                    error.starts_with("invalid managed generation id"),
                    "{error}"
                );
                assert!(
                    error.contains("cleanup failed: injected generation-only"),
                    "{error}"
                );
            } else {
                let result = store.cleanup_typed().unwrap();
                assert_eq!(result.recovery.publishing_recovered, 1);
                assert!(result.recovery.observation_complete);
                assert!(result.janitor.first_issue.is_some());
                assert_eq!(
                    store
                        .read_active_generation()
                        .unwrap()
                        .unwrap()
                        .generation_id,
                    id
                );
                assert!(run.exists(), "first optional issue should defer disposal");
                assert_eq!(result.janitor.legacy.run_recovery.skipped_locked, 1);
            }
            drop(fault);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn managed_typed_recovered_publication_retains_run_until_persisted_barrier_resolves() {
        let (root, store) = store("persisted-barrier");
        let session = prepared(&store);
        let id = session.candidate.generation_id.clone();
        let run = leave_publishing(session, false);
        fs::write(run.join("cleanup-barrier.json"), b"invalid cleanup barrier").unwrap();
        let journal = fs::read(run.join("journal.json")).unwrap();
        let report = store.cleanup_typed().unwrap();
        assert_eq!(report.recovery.publishing_recovered, 1);
        assert!(report.recovery.observation_complete);
        assert!(report.janitor.pending);
        assert_eq!(report.janitor.legacy.run_recovery.skipped_locked, 1);
        assert!(report
            .janitor
            .first_issue
            .as_ref()
            .unwrap()
            .source
            .to_string()
            .contains("invalid run cleanup barrier"));
        assert_eq!(
            store
                .read_active_generation()
                .unwrap()
                .unwrap()
                .generation_id,
            id
        );
        assert_eq!(fs::read(run.join("journal.json")).unwrap(), journal);
        // Strict compatibility still returns the original optional source and
        // never bypasses the persisted containment gate on its next attempt.
        assert!(store
            .cleanup()
            .unwrap_err()
            .to_string()
            .contains("invalid run cleanup barrier"));
        assert!(run.exists());
        fs::remove_file(run.join("cleanup-barrier.json")).unwrap();
        store.cleanup().unwrap();
        assert!(!run.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_typed_merge_preserves_first_issue_with_a_complete_latest_report() {
        // Pure report algebra: this synthetic complete observation makes no
        // filesystem-disposal claim. Actual removal is tested by bounded GC.
        let mut first = ManagedJanitorReport::complete();
        first.issue(
            ManagedJanitorStage::PublishedRunJournal,
            NativeError::InvalidInput("first journal diagnostic".into()),
            true,
        );
        first.legacy.run_recovery.skipped_locked = 1;
        let mut latest = ManagedJanitorReport::complete();
        latest.issue(
            ManagedJanitorStage::TerminalRunRemoval,
            NativeError::InvalidInput("second diagnostic".into()),
            false,
        );
        first.merge_latest(latest);
        assert_eq!(first.issue_count, 2);
        assert!(!first.pending);
        assert_eq!(first.legacy.run_recovery.skipped_locked, 0);
        assert_eq!(
            first.first_issue.as_ref().unwrap().source.to_string(),
            "first journal diagnostic"
        );
        assert_eq!(
            first.into_legacy().unwrap_err().to_string(),
            "first journal diagnostic"
        );
    }

    #[test]
    fn managed_registered_owner_finish_retains_terminal_work_for_bounded_collection() {
        let (root, store) = store("terminal-retention");
        let mut session = prepared(&store);
        // Strict recovery may report the genuinely live owner without deleting
        // its workspace or turning that pending state into a primary error.
        assert_eq!(
            session
                .recover_and_gc()
                .unwrap()
                .run_recovery
                .skipped_locked,
            1
        );
        let candidate = session.candidate_db_path();
        let bytes = fs::read(&candidate).unwrap();
        let workspace = session.workspace.take().unwrap();
        let run = workspace.root().to_path_buf();
        workspace.finish().unwrap();
        assert!(run.exists());
        assert_eq!(fs::read(&candidate).unwrap(), bytes);
        session.finish_without_cleanup();
        let domain =
            super::super::maintenance_ownership::ReferenceDomain::managed(store.layout()).unwrap();
        let memory = super::super::maintenance_budget::CleanupMemory::new(8 * 1024 * 1024).unwrap();
        super::super::maintenance_controls::prepare_terminal_workspace_boxed(
            &run, &domain, &memory,
        )
        .unwrap();
        let latest = store.recover_for_read().unwrap();
        assert!(
            latest.janitor.pending,
            "terminal retention was reported disposed"
        );
        assert_eq!(latest.janitor.legacy.run_recovery.skipped_locked, 0);
        assert!(latest.janitor.first_issue.is_none());
        assert!(run.exists());
        assert_eq!(fs::read(candidate).unwrap(), bytes);
        fs::remove_dir_all(root).unwrap();
    }
}
