use crate::error::NativeError;
use crate::storage::atomic::write_json_atomically;
use crate::storage::layout::managed_generation_id;
use crate::storage::locks::RunLease;
#[cfg(test)]
use crate::storage::locks::{try_open_locked_in_existing_parent, LockMode};
use crate::storage::maintenance_ownership::{ReferenceDomain, ReferenceMutation};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunPhase {
    Created,
    Staged,
    CandidateReady,
    Publishing,
    Published,
    Failed,
    CleanupPending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunJournal {
    pub run_id: String,
    pub phase: RunPhase,
    #[serde(default)]
    pub base_generation_id: Option<String>,
    #[serde(default)]
    pub candidate_generation_id: Option<String>,
    #[serde(default)]
    pub active_generation_id: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RunWorkspaceRecovery {
    pub deleted: usize,
    pub skipped_locked: usize,
    pub publishing_recovered: usize,
}

#[derive(Debug)]
pub(crate) struct RunWorkspace {
    root: PathBuf,
    run_id: String,
    lease: Option<std::sync::Arc<RunLease>>,
    finished: bool,
    cleanup_barrier: Option<PathBuf>,
    _reference_mutation: Option<ReferenceMutation<'static>>,
    workspace_registration:
        Option<std::sync::Arc<crate::storage::maintenance_controls::WorkspaceRegistration>>,
}

/// Retains the real parent owner lease through phase creation/execution. This
/// cannot be fabricated from an addressed run directory or serialized facts.
#[derive(Debug)]
pub(crate) struct ParentRunScope {
    owner: std::sync::Arc<RunLease>,
    registration: std::sync::Arc<crate::storage::maintenance_controls::WorkspaceRegistration>,
}
impl ParentRunScope {
    pub(crate) fn retain(&self) -> Self {
        Self {
            owner: std::sync::Arc::clone(&self.owner),
            registration: std::sync::Arc::clone(&self.registration),
        }
    }
    pub(crate) fn domain(&self) -> &ReferenceDomain {
        self.registration.domain()
    }
    pub(super) fn owner(&self) -> &RunLease {
        &self.owner
    }
    pub(super) fn registration(
        &self,
    ) -> &crate::storage::maintenance_controls::WorkspaceRegistration {
        &self.registration
    }
}

impl RunWorkspace {
    // Only legacy/test fixtures may create a workspace without an association;
    // production callers must supply their actual reference domain.
    #[cfg(test)]
    pub(crate) fn create(
        runs_root: impl AsRef<Path>,
        base_generation_id: Option<String>,
    ) -> Result<Self, NativeError> {
        Self::create_inner(runs_root.as_ref(), None, base_generation_id)
    }

    pub(crate) fn create_scoped(
        runs_root: impl AsRef<Path>,
        domain: &ReferenceDomain,
        base_generation_id: Option<String>,
    ) -> Result<Self, NativeError> {
        if !domain.allows_runs_root(runs_root.as_ref())? {
            return Err(NativeError::InvalidInput(
                "reference_run_root_mismatch".into(),
            ));
        }
        Self::create_inner(runs_root.as_ref(), Some(domain), base_generation_id)
    }

    fn create_inner(
        runs_root: &Path,
        domain: Option<&ReferenceDomain>,
        base_generation_id: Option<String>,
    ) -> Result<Self, NativeError> {
        let mutation = domain.map(ReferenceMutation::begin_in_domain).transpose()?;
        let startup = domain.and_then(|domain| {
            crate::storage::locks::open_shared_coordination(&domain.workspace_startup_path()).ok()
        });
        fs::create_dir_all(runs_root)?;
        let run_id = managed_generation_id();
        let root = runs_root.join(format!("run-{run_id}"));
        let fresh = crate::storage::layout::create_fresh_reference_root(&root)?
            .ok_or_else(|| NativeError::InvalidInput("run root already exists".into()))?;
        let anchor = crate::storage::locks::RootedDirectory::open(&root, fresh.identity())?;
        let lease = crate::storage::locks::create_new_coordination_in(
            &anchor,
            b"lease.lock",
            &root.join("lease.lock"),
        )?;
        let workspace_registration = match (domain, mutation.as_ref(), startup.as_ref()) {
            (Some(domain), Some(mutation), Some(_)) => {
                crate::storage::maintenance_controls::WorkspaceRegistration::create(
                    &fresh, domain, mutation, &lease, 1,
                )
                .ok()
                .flatten()
            }
            _ => None,
        };
        let this = Self {
            root,
            run_id,
            lease: Some(std::sync::Arc::new(lease)),
            finished: false,
            cleanup_barrier: None,
            _reference_mutation: mutation,
            workspace_registration: workspace_registration.map(std::sync::Arc::new),
        };
        if let Some(domain) = domain {
            this.write_metadata(
                &this.root.join(".reference-scope.json"),
                &RunReferenceScope {
                    version: 1,
                    run: crate::storage::locks::path_identity(&this.root)?,
                    domain: domain.clone(),
                },
            )?;
        }
        this.write_journal(RunJournal {
            run_id: this.run_id.clone(),
            phase: RunPhase::Created,
            base_generation_id,
            candidate_generation_id: None,
            active_generation_id: None,
            last_error: None,
        })?;
        fs::create_dir_all(this.staging_root())?;
        fs::create_dir_all(this.candidate_root())?;
        Ok(this)
    }

    #[cfg(test)]
    pub(crate) fn abandon_for_test(mut self) -> PathBuf {
        self.finished = true;
        drop(self.lease.take());
        self.root.clone()
    }

    pub(crate) fn set_cleanup_barrier(&mut self, root: PathBuf) -> Result<(), NativeError> {
        // Keep the in-memory guard even if persistence fails: Drop must retain
        // this run while its unknown phase can still own candidate files.
        self.cleanup_barrier = Some(root.clone());
        // Direct destinations retain their spelling for journal/lock keys.
        // Anchor only this derived private path so later janitors do not
        // reinterpret it relative to a different process working directory.
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()?.join(root)
        };
        self.cleanup_barrier = Some(root.clone());
        self.write_metadata(&self.root.join("cleanup-barrier.json"), &root)
    }

    pub(crate) fn reference_domain(root: &Path) -> Result<Option<ReferenceDomain>, NativeError> {
        read_reference_scope(root)
    }

    fn ensure_cleanup_allowed(&self) -> Result<(), NativeError> {
        if let Some(root) = &self.cleanup_barrier {
            crate::storage::containment::ensure_registry_empty(root)
                .map_err(NativeError::InvalidInput)?;
        }
        ensure_persisted_cleanup_allowed(&self.root)?;
        Ok(())
    }

    pub(crate) fn phase_owner_scope(&self) -> Option<ParentRunScope> {
        Some(ParentRunScope {
            owner: std::sync::Arc::clone(self.lease.as_ref()?),
            registration: std::sync::Arc::clone(self.workspace_registration.as_ref()?),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn staging_root(&self) -> PathBuf {
        self.root.join("staging")
    }

    pub(crate) fn candidate_root(&self) -> PathBuf {
        self.root.join("candidate")
    }

    pub(crate) fn journal_path(&self) -> PathBuf {
        self.root.join("journal.json")
    }

    pub(crate) fn journal(&self) -> Result<RunJournal, NativeError> {
        let text = fs::read_to_string(self.journal_path())?;
        Ok(serde_json::from_str(&text)?)
    }

    pub(crate) fn register_candidate(
        &self,
        candidate_generation_id: String,
    ) -> Result<(), NativeError> {
        let mut journal = self.journal()?;
        journal.candidate_generation_id = Some(candidate_generation_id);
        journal.phase = RunPhase::Staged;
        self.write_journal(journal)
    }

    pub(crate) fn mark_candidate_ready(&self) -> Result<(), NativeError> {
        let mut journal = self.journal()?;
        if journal.candidate_generation_id.is_none() {
            return Err(NativeError::InvalidInput(
                "cannot mark candidate ready without a candidate generation id".to_string(),
            ));
        }
        journal.phase = RunPhase::CandidateReady;
        journal.last_error = None;
        self.write_journal(journal)
    }

    pub(crate) fn mark_phase(
        &self,
        phase: RunPhase,
        active_generation_id: Option<String>,
        last_error: Option<String>,
    ) -> Result<(), NativeError> {
        let mut journal = self.journal()?;
        journal.phase = phase;
        journal.active_generation_id = active_generation_id;
        journal.last_error = last_error;
        self.write_journal(journal)
    }

    /// Retain exact publication evidence after an ambiguous transaction result.
    /// This changes only destructor state; ordinary guard release still runs.
    pub(crate) fn retain_for_recovery(mut self) {
        self.finished = true;
        drop(self.lease.take());
        drop(self._reference_mutation.take());
    }

    pub(crate) fn finish(mut self) -> Result<(), NativeError> {
        self.finish_inner(None)
    }

    pub(crate) fn abort(mut self, error: Option<String>) -> Result<(), NativeError> {
        self.finish_inner(error)
    }

    /// Publish the actual owner-terminal event and retain its registered root
    /// for bounded collection. No path/receipt supplied by a caller is adopted.
    #[cfg(test)]
    pub(crate) fn retire_for_maintenance(mut self) -> Result<PathBuf, NativeError> {
        if self.workspace_registration.is_none() {
            return Err(NativeError::InvalidInput(
                "workspace ownership is unverified".into(),
            ));
        }
        self.finish_inner(None)?;
        Ok(self.root.clone())
    }

    #[cfg(test)]
    pub(crate) fn cleanup_orphans(
        runs_root: impl AsRef<Path>,
    ) -> Result<RunWorkspaceRecovery, NativeError> {
        let runs_root = runs_root.as_ref();
        if !runs_root.exists() {
            return Ok(RunWorkspaceRecovery::default());
        }

        let mut report = RunWorkspaceRecovery::default();
        for entry in fs::read_dir(runs_root)? {
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

            let lease_path = path.join("lease.lock");
            let lease = match try_open_locked_in_existing_parent(&lease_path, LockMode::Exclusive) {
                Ok(Some(lease)) => lease,
                Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
                Ok(None) => {
                    report.skipped_locked += 1;
                    continue;
                }
            };
            if ensure_persisted_cleanup_allowed(&path).is_err() {
                report.skipped_locked += 1;
                continue;
            }
            let domain = match read_reference_scope(&path) {
                Ok(Some(domain)) => domain,
                _ => {
                    report.skipped_locked += 1;
                    continue;
                }
            };
            let _mutation = match ReferenceMutation::begin_in_domain(&domain) {
                Ok(mutation) => mutation,
                Err(_) => {
                    report.skipped_locked += 1;
                    continue;
                }
            };
            let journal = read_orphan_journal(&path, name)?;
            if journal.phase == RunPhase::Publishing {
                report.publishing_recovered += 1;
            }
            drop(lease);
            remove_run_tree(&path)?;
            report.deleted += 1;
        }
        Ok(report)
    }

    fn finish_inner(&mut self, primary_error: Option<String>) -> Result<(), NativeError> {
        if self
            .lease
            .as_ref()
            .is_some_and(|lease| std::sync::Arc::strong_count(lease) != 1)
        {
            self.finished = true;
            drop(self.lease.take());
            return Err(NativeError::InvalidInput(
                "workspace still has live phase ownership".into(),
            ));
        }
        if let Err(error) = self.ensure_cleanup_allowed() {
            self.finished = true;
            let _ = self.mark_cleanup_pending(primary_error, &error);
            drop(self.lease.take());
            return Err(error);
        }
        if let Some(error) = primary_error.clone() {
            self.mark_phase(RunPhase::Failed, None, Some(error))?;
        }
        if let (Some(registration), Some(owner), Some(mutation)) = (
            &self.workspace_registration,
            self.lease.as_deref(),
            self._reference_mutation.as_ref(),
        ) {
            let journal = self.journal()?;
            let disposition = if journal.phase == RunPhase::Published {
                crate::storage::maintenance_controls::WorkspaceDisposition::PublishedConsumed
            } else {
                crate::storage::maintenance_controls::WorkspaceDisposition::AbortedNotPublished
            };
            if journal.phase == RunPhase::Publishing {
                self.finished = true;
                drop(self.lease.take());
                return Err(NativeError::InvalidInput(
                    "workspace publication requires recovery".into(),
                ));
            }
            if let Err(error) = registration.terminal(owner, mutation, disposition) {
                self.finished = true;
                let _ = self.mark_cleanup_pending(primary_error, &error);
                drop(self.lease.take());
                return Err(error);
            }
        }
        self.finished = true;
        drop(self.lease.take());
        if self.workspace_registration.is_some() {
            // Automatic disposal belongs to the bounded terminal-workspace
            // lane; producer completion only publishes closure and releases.
            return Ok(());
        }
        if let Err(cleanup_error) = remove_run_tree(&self.root) {
            self.mark_cleanup_pending(primary_error, &cleanup_error)?;
            return Err(cleanup_error);
        }
        Ok(())
    }

    fn mark_cleanup_pending(
        &self,
        primary_error: Option<String>,
        cleanup_error: &NativeError,
    ) -> Result<(), NativeError> {
        let message = match primary_error {
            Some(primary) => format!("{primary}; cleanup pending: {cleanup_error}"),
            None => format!("cleanup pending: {cleanup_error}"),
        };
        self.mark_phase(RunPhase::CleanupPending, None, Some(message))
    }

    fn write_journal(&self, journal: RunJournal) -> Result<(), NativeError> {
        if let (Some(registration), Some(owner), Some(mutation)) = (
            &self.workspace_registration,
            self.lease.as_deref(),
            self._reference_mutation.as_ref(),
        ) {
            let _ = registration.source_change(
                owner,
                mutation,
                journal.phase == RunPhase::Publishing,
                journal.phase == RunPhase::Published,
            );
        }
        self.write_metadata(&self.journal_path(), &journal)
    }
    fn write_metadata(&self, path: &Path, value: &impl Serialize) -> Result<(), NativeError> {
        if let (Some(registration), Some(owner), Some(mutation)) = (
            &self.workspace_registration,
            self.lease.as_deref(),
            self._reference_mutation.as_ref(),
        ) {
            let publication = crate::process_lifecycle::write_private_json_recorded(path, value)
                .map_err(NativeError::InvalidInput)?;
            if let Some(publication) = publication {
                let _ = registration.record_metadata(owner, mutation, &publication);
            }
            crate::storage::atomic::sync_dir(&self.root)
        } else {
            write_json_atomically(path, value)
        }
    }
}

impl Drop for RunWorkspace {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Registered roots must pass the same startup/output/containment
        // closure as explicit finish. Failed closure leaves the root intact.
        let _ = self.finish_inner(Some("workspace dropped without finish or abort".into()));
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RunReferenceScope {
    version: u32,
    run: crate::storage::locks::FileIdentity,
    domain: ReferenceDomain,
}
fn read_reference_scope(root: &Path) -> Result<Option<ReferenceDomain>, NativeError> {
    let path = root.join(".reference-scope.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16384 {
        return Ok(None);
    }
    let file = fs::File::open(&path)?;
    if crate::storage::locks::file_link_count(&file)? != 1 {
        return Ok(None);
    }
    let identity = crate::storage::locks::path_identity(&path)?;
    if identity != crate::storage::locks::file_identity(&file)? {
        return Ok(None);
    }
    let scope: RunReferenceScope = serde_json::from_reader(file)?;
    if scope.version != 1
        || scope.run != crate::storage::locks::path_identity(root)?
        || !scope.domain.revalidate()?
        || !scope.domain.allows_runs_root(
            root.parent()
                .ok_or_else(|| NativeError::InvalidInput("run parent missing".into()))?,
        )?
        || identity != crate::storage::locks::path_identity(&path)?
    {
        return Ok(None);
    }
    Ok(Some(scope.domain))
}

pub(super) fn ensure_persisted_cleanup_allowed(root: &Path) -> Result<(), NativeError> {
    let bytes = match fs::read(root.join("cleanup-barrier.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let barrier: PathBuf = serde_json::from_slice(&bytes).map_err(|e| {
        NativeError::InvalidInput(format!(
            "containment_unknown: invalid run cleanup barrier: {e}"
        ))
    })?;
    if !barrier.is_absolute() {
        return Err(NativeError::InvalidInput(
            "containment_unknown: run cleanup barrier has no absolute origin".to_string(),
        ));
    }
    crate::storage::containment::ensure_registry_empty(&barrier).map_err(NativeError::InvalidInput)
}

#[cfg(test)]
fn read_orphan_journal(path: &Path, name: &str) -> Result<RunJournal, NativeError> {
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

fn remove_run_tree(root: &Path) -> Result<(), NativeError> {
    let parent = root.parent().ok_or_else(|| {
        NativeError::InvalidInput(format!("run path {} has no parent", root.display()))
    })?;
    remove_run_root_confined(root, parent)
}

pub(super) fn remove_run_root_confined(root: &Path, runs_root: &Path) -> Result<(), NativeError> {
    let Some(metadata) = symlink_metadata_if_present(root)? else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked path {}",
            root.display()
        )));
    }

    let canonical_runs_root = fs::canonicalize(runs_root)?;
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if canonical_root.parent() != Some(canonical_runs_root.as_path()) {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove run path outside root: {}",
            root.display()
        )));
    }
    remove_path_without_symlinks(&canonical_root)
}

fn remove_path_without_symlinks(path: &Path) -> Result<(), NativeError> {
    validate_tree_has_no_symlinks(path)?;
    remove_validated_path(path)
}

fn validate_tree_has_no_symlinks(path: &Path) -> Result<(), NativeError> {
    let Some(metadata) = symlink_metadata_if_present(path)? else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            validate_tree_has_no_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_validated_path(path: &Path) -> Result<(), NativeError> {
    let Some(metadata) = symlink_metadata_if_present(path)? else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            remove_validated_path(&entry.path())?;
        }
        remove_dir_if_present(path)?;
    } else {
        remove_file_if_present(path)?;
    }
    Ok(())
}

fn symlink_metadata_if_present(path: &Path) -> Result<Option<fs::Metadata>, NativeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn remove_dir_if_present(path: &Path) -> Result<(), NativeError> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_file_if_present(path: &Path) -> Result<(), NativeError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_barrier_records_its_absolute_origin() {
        let root = std::env::temp_dir().join(format!("run-origin-{}", managed_generation_id()));
        let mut workspace = RunWorkspace::create(root.join("runs"), None).unwrap();
        let relative = PathBuf::from("private-relative-phase-registry");
        workspace.set_cleanup_barrier(relative.clone()).unwrap();
        let stored: PathBuf = serde_json::from_slice(
            &fs::read(workspace.root().join("cleanup-barrier.json")).unwrap(),
        )
        .unwrap();
        assert!(stored.is_absolute());
        assert_eq!(stored, std::env::current_dir().unwrap().join(relative));
        workspace.finish().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unanchored_persisted_barrier_cannot_authorize_orphan_cleanup() {
        let root = std::env::temp_dir().join(format!("run-unbound-{}", managed_generation_id()));
        let runs = root.join("runs");
        let workspace = RunWorkspace::create(&runs, None).unwrap();
        let candidate = workspace.candidate_root().join("partial.db");
        fs::write(
            &candidate,
            b"candidate still belongs to an unverified phase",
        )
        .unwrap();
        write_json_atomically(
            &workspace.root().join("cleanup-barrier.json"),
            &PathBuf::from("unbound-phase-registry"),
        )
        .unwrap();
        drop(workspace);
        let recovery = RunWorkspace::cleanup_orphans(&runs).unwrap();
        assert_eq!(recovery.deleted, 0);
        assert_eq!(recovery.skipped_locked, 1);
        assert!(candidate.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confined_run_cleanup_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-run-cleanup-{}",
            managed_generation_id()
        ));
        let runs_root = root.join("runs");
        let run_root = runs_root.join("run-repeat");
        fs::create_dir_all(run_root.join("staging")).unwrap();
        fs::write(run_root.join("staging/source.rs"), "fn main() {}\n").unwrap();

        remove_run_root_confined(&run_root, &runs_root).unwrap();
        remove_run_root_confined(&run_root, &runs_root).unwrap();
        assert!(!run_root.exists());

        let raced_run_root = runs_root.join("run-raced");
        fs::create_dir_all(raced_run_root.join("candidate")).unwrap();
        validate_tree_has_no_symlinks(&raced_run_root).unwrap();
        fs::remove_dir_all(&raced_run_root).unwrap();
        remove_validated_path(&raced_run_root).unwrap();

        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod retain_recovery_tests {
    use super::*;
    #[test]
    fn retain_for_recovery_preserves_journal_and_payload_but_releases_actual_lease() {
        let root = std::env::temp_dir().join(format!(
            "run-retain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let workspace = RunWorkspace::create(&root, None).unwrap();
        workspace
            .mark_phase(RunPhase::Publishing, Some("published-test".into()), None)
            .unwrap();
        fs::write(
            workspace.staging_root().join("evidence"),
            b"owned recovery evidence",
        )
        .unwrap();
        let path = workspace.root().to_path_buf();
        let before = fs::read(workspace.journal_path()).unwrap();
        workspace.retain_for_recovery();
        assert_eq!(fs::read(path.join("journal.json")).unwrap(), before);
        assert_eq!(
            fs::read(path.join("staging/evidence")).unwrap(),
            b"owned recovery evidence"
        );
        assert!(
            try_open_locked_in_existing_parent(path.join("lease.lock"), LockMode::Exclusive)
                .unwrap()
                .is_some()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
