use crate::error::NativeError;
use crate::storage::atomic::write_json_atomically;
use crate::storage::layout::managed_generation_id;
use crate::storage::locks::{open_locked, try_open_locked, LockMode, RunLease};
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
    lease: Option<RunLease>,
    finished: bool,
}

impl RunWorkspace {
    pub(crate) fn create(
        runs_root: impl AsRef<Path>,
        base_generation_id: Option<String>,
    ) -> Result<Self, NativeError> {
        let runs_root = runs_root.as_ref();
        fs::create_dir_all(runs_root)?;
        let run_id = managed_generation_id();
        let root = runs_root.join(format!("run-{run_id}"));
        fs::create_dir(&root)?;
        let lease = open_locked(root.join("lease.lock"), LockMode::Exclusive)?;
        let this = Self {
            root,
            run_id,
            lease: Some(lease),
            finished: false,
        };
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

    pub(crate) fn finish(mut self) -> Result<(), NativeError> {
        self.finish_inner(None)
    }

    pub(crate) fn abort(mut self, error: Option<String>) -> Result<(), NativeError> {
        self.finish_inner(error)
    }

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
            let lease = match try_open_locked(&lease_path, LockMode::Exclusive)? {
                Some(lease) => lease,
                None => {
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
        if let Some(error) = primary_error.clone() {
            self.mark_phase(RunPhase::Failed, None, Some(error))?;
        }
        self.finished = true;
        drop(self.lease.take());
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
        write_json_atomically(&self.journal_path(), &journal)
    }
}

impl Drop for RunWorkspace {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = self.mark_phase(
            RunPhase::CleanupPending,
            None,
            Some("workspace dropped without finish or abort".to_string()),
        );
        drop(self.lease.take());
        let _ = remove_run_tree(&self.root);
    }
}

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
    let canonical_parent = fs::canonicalize(parent)?;
    let canonical_root = fs::canonicalize(root)?;
    if canonical_root.parent() != Some(canonical_parent.as_path()) {
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
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            validate_tree_has_no_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_validated_path(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            remove_validated_path(&entry.path())?;
        }
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
