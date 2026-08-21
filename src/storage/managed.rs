use crate::error::NativeError;
use crate::storage::atomic::{create_new_file, sync_dir, write_json_atomically};
use crate::storage::layout::{
    direct_bundle_paths, managed_generation_id, validate_generation_id, CandidateGenerationLayout,
    GenerationPaths, ManagedLayout,
};
use crate::storage::locks::{
    open_locked, try_open_locked, LockMode, LockedFile, StateLease, WriterLease,
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
    store: ManagedStore,
    generation_id: String,
}

impl GenerationLease {
    fn new(lock: LockedFile, store: ManagedStore, generation_id: String) -> Self {
        Self {
            lock: Some(lock),
            store,
            generation_id,
        }
    }
}

impl Drop for GenerationLease {
    fn drop(&mut self) {
        drop(self.lock.take());
        if self.store.layout.storage_root().exists() {
            self.store
                .best_effort_cleanup_generation(&self.generation_id);
        }
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
    pub retired_deleted: usize,
    pub retired_pending: usize,
    pub retired_generations_deleted: usize,
    pub retired_generations_pending: usize,
}

#[derive(Debug)]
pub(crate) struct ManagedReadSnapshot {
    pub generation_id: String,
    pub db_path: PathBuf,
    pub manifest_path: PathBuf,
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
}

impl ManagedWriteSession {
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

    pub(crate) fn publish_with_stats<S: GenerationStats>(
        &mut self,
        graph_summary: &S,
    ) -> Result<String, NativeError> {
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
        match self
            .store
            .publish_candidate_inner(&workspace, &self.candidate)
        {
            Ok(generation_id) => {
                match workspace.mark_phase(RunPhase::Published, Some(generation_id.clone()), None) {
                    Ok(()) => match workspace.finish() {
                        Ok(()) => Ok(generation_id),
                        Err(error) => {
                            self.finished = true;
                            drop(self.writer_lease.take());
                            Err(error)
                        }
                    },
                    Err(error) => {
                        self.finished = true;
                        let error = abort_workspace_after_error(workspace, error);
                        drop(self.writer_lease.take());
                        Err(error)
                    }
                }
            }
            Err(error) => {
                self.finished = true;
                let error = abort_workspace_after_error(workspace, error);
                drop(self.writer_lease.take());
                Err(error)
            }
        }
    }

    pub(crate) fn finish(mut self) {
        self.finished = true;
        drop(self.writer_lease.take());
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
        self.ensure_layout()?;
        let writer_lease = self.open_writer_lock()?;
        let base_generation = self.read_active_with_shared_lock()?;
        let workspace = RunWorkspace::create(
            self.layout.runs_root(),
            base_generation
                .as_ref()
                .map(|value| value.generation_id.clone()),
        )?;

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
        let _state_lease = self.open_state_lock(LockMode::Shared)?;
        let Some(active) = self.read_active_generation()? else {
            return Ok(None);
        };
        let generation = self.layout.generation(&active.generation_id)?;
        validate_ready_generation(&generation)?;
        let metadata = read_generation_metadata(&generation)?;
        let lock = open_locked(generation.lease_path(), LockMode::Shared)?;
        let lease = Arc::new(GenerationLease::new(
            lock,
            self.clone(),
            active.generation_id.clone(),
        ));
        Ok(Some(ManagedReadSnapshot {
            generation_id: active.generation_id,
            db_path: generation.db_path(),
            manifest_path: generation.manifest_path(),
            _lease: lease,
            logical_size_bytes: metadata.logical_size_bytes,
            physical_size_bytes: metadata.physical_size_bytes,
        }))
    }

    pub(crate) fn recover_and_gc(&self) -> Result<ManagedCleanupReport, NativeError> {
        self.cleanup()
    }

    pub(crate) fn cleanup(&self) -> Result<ManagedCleanupReport, NativeError> {
        self.ensure_layout()?;
        let _state_lease = self.open_state_lock(LockMode::Exclusive)?;
        let mut report = ManagedCleanupReport::default();
        let mut active = self.read_active_generation()?;
        report.run_recovery = self.recover_runs_locked(&mut active)?;
        self.collect_retired_generations_locked(
            active.as_ref().map(|value| value.generation_id.as_str()),
            &mut report,
        )?;
        report.retired_generations_deleted = report.retired_deleted;
        report.retired_generations_pending = report.retired_pending;
        Ok(report)
    }

    pub(crate) fn read_active_generation(&self) -> Result<Option<ActiveGeneration>, NativeError> {
        let path = self.layout.active_pointer_path();
        if !path.exists() {
            return Ok(None);
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

    fn publish_candidate_inner(
        &self,
        workspace: &RunWorkspace,
        candidate: &ManagedCandidate,
    ) -> Result<String, NativeError> {
        let _state_lease = self.open_state_lock(LockMode::Exclusive)?;
        let mut active = self.read_active_generation()?;
        self.finish_run_locked(workspace.root(), &workspace.journal()?, &mut active, false)?;
        Ok(candidate.generation_id.clone())
    }

    fn recover_runs_locked(
        &self,
        active: &mut Option<ActiveGeneration>,
    ) -> Result<RunWorkspaceRecovery, NativeError> {
        let runs_root = self.layout.runs_root();
        if !runs_root.exists() {
            return Ok(RunWorkspaceRecovery::default());
        }
        let mut report = RunWorkspaceRecovery::default();
        for entry in fs::read_dir(&runs_root)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !name.starts_with("run-") {
                continue;
            }
            let lease = match try_open_locked(path.join("lease.lock"), LockMode::Exclusive)? {
                Some(lease) => lease,
                None => {
                    report.skipped_locked += 1;
                    continue;
                }
            };
            let journal = read_run_journal_or_default(&path, name)?;
            let recovered = match journal.phase {
                RunPhase::Publishing | RunPhase::Published => {
                    self.finish_run_locked(&path, &journal, active, true)?;
                    report.publishing_recovered += 1;
                    true
                }
                _ => false,
            };
            drop(lease);
            if !recovered {
                remove_run_root_confined(&path, &runs_root)?;
                report.deleted += 1;
            }
        }
        Ok(report)
    }

    fn finish_run_locked(
        &self,
        run_root: &Path,
        journal: &RunJournal,
        active: &mut Option<ActiveGeneration>,
        cleanup_run_root: bool,
    ) -> Result<(), NativeError> {
        let Some(candidate_id) = journal.candidate_generation_id.as_ref() else {
            if cleanup_run_root {
                remove_run_root_confined(run_root, &self.layout.runs_root())?;
            }
            return Ok(());
        };
        validate_generation_id(candidate_id)?;
        if let Some(base_generation_id) = journal.base_generation_id.as_deref() {
            validate_generation_id(base_generation_id)?;
        }
        let staged =
            CandidateGenerationLayout::new(run_root.join("candidate"), candidate_id.clone())
                .generation_paths();
        let published = self.layout.generation(candidate_id)?;
        let active_id = active.as_ref().map(|value| value.generation_id.clone());
        let base_matches = active_id == journal.base_generation_id;
        let published_valid = is_ready_generation_valid(&published);
        let staged_valid = is_ready_generation_valid(&staged);

        if active_id.as_deref() == Some(candidate_id.as_str()) && published_valid {
            self.finalize_previous_retirement(journal.base_generation_id.as_deref())?;
            if cleanup_run_root {
                remove_run_root_confined(run_root, &self.layout.runs_root())?;
            }
            return Ok(());
        }

        if !base_matches {
            if published.root().exists() {
                write_json_atomically(
                    &published.retired_path(),
                    &GenerationRetirement {
                        schema_version: MANAGED_SCHEMA_VERSION,
                        retired_at_ms: unix_time_ms(),
                    },
                )?;
                self.retire_or_delete_generation_locked(
                    &published,
                    active.as_ref().map(|value| value.generation_id.as_str()),
                )?;
            }
            if cleanup_run_root {
                remove_run_root_confined(run_root, &self.layout.runs_root())?;
                return Ok(());
            }
            return Err(NativeError::InvalidInput(
                ManagedValidationError::StaleBaseGeneration {
                    expected: journal.base_generation_id.clone(),
                    found: active_id,
                }
                .to_string(),
            ));
        }

        if published_valid && base_matches {
            let activated = self.activate_published_generation_locked(
                &published,
                journal.base_generation_id.as_deref(),
            )?;
            *active = Some(activated);
            if cleanup_run_root {
                remove_run_root_confined(run_root, &self.layout.runs_root())?;
            }
            return Ok(());
        }

        if staged_valid && base_matches {
            self.promote_candidate_generation_locked(&staged, &published)?;
            let activated = self.activate_published_generation_locked(
                &published,
                journal.base_generation_id.as_deref(),
            )?;
            *active = Some(activated);
            if cleanup_run_root {
                remove_run_root_confined(run_root, &self.layout.runs_root())?;
            }
            return Ok(());
        }

        if published.root().exists() {
            self.retire_or_delete_generation_locked(
                &published,
                active.as_ref().map(|value| value.generation_id.as_str()),
            )?;
        }
        if cleanup_run_root {
            remove_run_root_confined(run_root, &self.layout.runs_root())?;
        }
        Ok(())
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
        previous_active_id: Option<&str>,
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
        if let Err(error) = write_json_atomically(&self.layout.active_pointer_path(), &active) {
            let _ = write_json_atomically(
                &published.retired_path(),
                &GenerationRetirement {
                    schema_version: MANAGED_SCHEMA_VERSION,
                    retired_at_ms: unix_time_ms(),
                },
            );
            return Err(error);
        }
        sync_dir(self.layout.storage_root())?;
        self.finalize_previous_retirement(previous_active_id)?;
        Ok(active)
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

    fn collect_retired_generations_locked(
        &self,
        active_id: Option<&str>,
        report: &mut ManagedCleanupReport,
    ) -> Result<(), NativeError> {
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
            remove_path_without_symlinks(generation.root())?;
        }
        Ok(())
    }

    fn best_effort_cleanup_generation(&self, generation_id: &str) {
        let Ok(Some(_state_lock)) =
            try_open_locked(self.layout.state_lock_path(), LockMode::Exclusive)
        else {
            return;
        };
        let Ok(active) = self.read_active_generation() else {
            return;
        };
        if active.as_ref().map(|value| value.generation_id.as_str()) == Some(generation_id) {
            return;
        }
        let Ok(generation) = self.layout.generation(generation_id) else {
            return;
        };
        let _ = self.retire_or_delete_generation_locked(
            &generation,
            active.as_ref().map(|value| value.generation_id.as_str()),
        );
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

fn is_ready_generation_valid(paths: &GenerationPaths) -> bool {
    validate_ready_generation(paths).is_ok()
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
    for path in [paths.manifest_path()] {
        if path.exists() {
            let metadata = fs::metadata(&path)?;
            logical += metadata.len();
            physical += allocated_size(&metadata);
        }
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
    if !journal_path.exists() {
        return Ok(RunJournal {
            run_id: name.trim_start_matches("run-").to_string(),
            phase: RunPhase::CleanupPending,
            base_generation_id: None,
            candidate_generation_id: None,
            active_generation_id: None,
            last_error: Some("missing journal".to_string()),
        });
    }
    let text = fs::read_to_string(journal_path)?;
    Ok(serde_json::from_str(&text)?)
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
