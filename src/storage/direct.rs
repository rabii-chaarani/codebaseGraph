use crate::error::NativeError;
use crate::storage::atomic::{sync_dir, write_json_atomically};
use crate::storage::layout::{direct_bundle_paths, DirectLayout, DIRECT_DB_SIDECAR_SUFFIXES};
use crate::storage::locks::{open_locked, LockMode, WriterLease};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirectPublishPhase {
    Prepared,
    DatabasePromoted,
    ManifestPromoted,
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DirectPublishJournal {
    pub phase: DirectPublishPhase,
    pub db_path: PathBuf,
    pub db_candidate_path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_candidate_path: PathBuf,
    pub db_sha256: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub sidecar_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct DirectPublishRequest {
    pub db_candidate_path: PathBuf,
    pub manifest_candidate_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct DirectWriteSession {
    layout: DirectLayout,
    writer_lease: Option<WriterLease>,
    finished: bool,
}

impl DirectWriteSession {
    pub(crate) fn db_candidate_path(&self) -> PathBuf {
        self.layout.db_candidate_path()
    }

    pub(crate) fn manifest_candidate_path(&self) -> PathBuf {
        self.layout.manifest_candidate_path()
    }

    pub(crate) fn publish(&mut self) -> Result<(), NativeError> {
        let store = DirectStore::new(self.layout.clone())?;
        let request = DirectPublishRequest {
            db_candidate_path: self.db_candidate_path(),
            manifest_candidate_path: self.manifest_candidate_path(),
        };
        let result = store.publish_candidate_with_lock(&request);
        if result.is_ok() || self.layout.journal_path().exists() {
            self.finished = true;
        }
        result
    }

    pub(crate) fn finish(mut self) {
        self.finished = true;
        drop(self.writer_lease.take());
    }
}

impl Drop for DirectWriteSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = cleanup_candidate_set(&self.layout);
        drop(self.writer_lease.take());
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DirectStore {
    layout: DirectLayout,
}

impl DirectStore {
    pub(crate) fn new(layout: DirectLayout) -> Result<Self, NativeError> {
        layout.validate()?;
        Ok(Self { layout })
    }

    pub(crate) fn begin_write(&self) -> Result<DirectWriteSession, NativeError> {
        self.ensure_parents()?;
        let writer_lease = self.open_writer_lock()?;
        self.recover_with_lock()?;
        cleanup_candidate_set(&self.layout)?;
        Ok(DirectWriteSession {
            layout: self.layout.clone(),
            writer_lease: Some(writer_lease),
            finished: false,
        })
    }

    pub(crate) fn begin_read(&self) -> Result<WriterLease, NativeError> {
        let writer_lease = self.open_writer_lock()?;
        self.recover_with_lock()?;
        cleanup_candidate_set(&self.layout)?;
        writer_lease.relock_shared()
    }

    fn publish_candidate_with_lock(
        &self,
        request: &DirectPublishRequest,
    ) -> Result<(), NativeError> {
        self.ensure_expected_candidate_paths(request)?;
        self.ensure_parents()?;
        self.recover_with_lock()?;
        ensure_regular_file(&request.db_candidate_path)?;
        ensure_regular_file(&request.manifest_candidate_path)?;

        let journal = DirectPublishJournal {
            phase: DirectPublishPhase::Prepared,
            db_path: self.layout.db_path().to_path_buf(),
            db_candidate_path: request.db_candidate_path.clone(),
            manifest_path: self.layout.manifest_path().to_path_buf(),
            manifest_candidate_path: request.manifest_candidate_path.clone(),
            db_sha256: sha256_path(&request.db_candidate_path)?,
            manifest_sha256: sha256_path(&request.manifest_candidate_path)?,
            sidecar_sha256: sidecar_checksums(&request.db_candidate_path)?,
        };
        self.write_journal(&journal)?;
        self.resume_publish(journal)?;
        self.remove_journal_and_shadows()?;
        cleanup_candidate_set(&self.layout)?;
        Ok(())
    }

    fn recover_with_lock(&self) -> Result<(), NativeError> {
        let journal_path = self.layout.journal_path();
        if !journal_path.exists() {
            return Ok(());
        }
        let journal = self.read_journal()?;
        self.validate_journal_destinations(&journal)?;
        self.validate_journal_sources(&journal)?;
        self.resume_publish(journal)?;
        self.remove_journal_and_shadows()?;
        cleanup_candidate_set(&self.layout)?;
        Ok(())
    }

    fn resume_publish(&self, mut journal: DirectPublishJournal) -> Result<(), NativeError> {
        if journal.phase == DirectPublishPhase::Prepared {
            promote_database_bundle(&journal)?;
            validate_checksum(&journal.db_path, &journal.db_sha256, "database")?;
            validate_sidecar_checksums(
                &journal.db_candidate_path,
                &journal.db_path,
                &journal.sidecar_sha256,
            )?;
            journal.phase = DirectPublishPhase::DatabasePromoted;
            self.write_journal(&journal)?;
        }

        if matches!(
            journal.phase,
            DirectPublishPhase::DatabasePromoted
                | DirectPublishPhase::ManifestPromoted
                | DirectPublishPhase::Committed
        ) {
            validate_checksum(&journal.db_path, &journal.db_sha256, "database")?;
            validate_sidecar_checksums(
                &journal.db_candidate_path,
                &journal.db_path,
                &journal.sidecar_sha256,
            )?;
        }

        if journal.phase == DirectPublishPhase::DatabasePromoted {
            replace_with_shadow(&journal.manifest_candidate_path, &journal.manifest_path)?;
            validate_checksum(&journal.manifest_path, &journal.manifest_sha256, "manifest")?;
            journal.phase = DirectPublishPhase::ManifestPromoted;
            self.write_journal(&journal)?;
        }

        if matches!(
            journal.phase,
            DirectPublishPhase::ManifestPromoted | DirectPublishPhase::Committed
        ) {
            validate_checksum(&journal.manifest_path, &journal.manifest_sha256, "manifest")?;
        }

        if journal.phase == DirectPublishPhase::ManifestPromoted {
            journal.phase = DirectPublishPhase::Committed;
            self.write_journal(&journal)?;
        }

        Ok(())
    }

    fn ensure_parents(&self) -> Result<(), NativeError> {
        if let Some(parent) = self.layout.db_path().parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.layout.manifest_path().parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn ensure_expected_candidate_paths(
        &self,
        request: &DirectPublishRequest,
    ) -> Result<(), NativeError> {
        if request.db_candidate_path != self.layout.db_candidate_path()
            || request.manifest_candidate_path != self.layout.manifest_candidate_path()
        {
            return Err(NativeError::InvalidInput(
                "direct candidates must be the deterministic sibling candidate paths".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_journal_destinations(
        &self,
        journal: &DirectPublishJournal,
    ) -> Result<(), NativeError> {
        if journal.db_path != self.layout.db_path()
            || journal.manifest_path != self.layout.manifest_path()
        {
            return Err(NativeError::InvalidInput(
                "direct publish journal does not match this store's configured destinations"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn validate_journal_sources(&self, journal: &DirectPublishJournal) -> Result<(), NativeError> {
        if journal.db_candidate_path != self.layout.db_candidate_path()
            || journal.manifest_candidate_path != self.layout.manifest_candidate_path()
            || journal
                .sidecar_sha256
                .keys()
                .any(|suffix| !DIRECT_DB_SIDECAR_SUFFIXES.contains(&suffix.as_str()))
        {
            return Err(NativeError::InvalidInput(
                "direct publish journal contains unexpected candidate paths or sidecars"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn write_journal(&self, journal: &DirectPublishJournal) -> Result<(), NativeError> {
        write_json_atomically(&self.layout.journal_path(), journal)
    }

    fn read_journal(&self) -> Result<DirectPublishJournal, NativeError> {
        let journal_path = self.layout.journal_path();
        ensure_regular_file(&journal_path)?;
        let text = fs::read_to_string(journal_path)?;
        Ok(serde_json::from_str(&text)?)
    }

    fn remove_journal_and_shadows(&self) -> Result<(), NativeError> {
        let journal_path = self.layout.journal_path();
        if journal_path.exists() {
            fs::remove_file(&journal_path)?;
            sync_parent(&journal_path)?;
        }
        cleanup_shadow_set(self.layout.db_path())?;
        cleanup_shadow_set(self.layout.manifest_path())?;
        Ok(())
    }

    fn open_writer_lock(&self) -> Result<WriterLease, NativeError> {
        open_locked(self.layout.writer_lock_path(), LockMode::Exclusive)
    }
}

fn promote_database_bundle(journal: &DirectPublishJournal) -> Result<(), NativeError> {
    replace_with_shadow(&journal.db_candidate_path, &journal.db_path)?;

    for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
        let from = PathBuf::from(format!(
            "{}.{}",
            journal.db_candidate_path.display(),
            suffix
        ));
        let to = PathBuf::from(format!("{}.{}", journal.db_path.display(), suffix));
        if from.exists() {
            replace_with_shadow(&from, &to)?;
        } else if to.exists() {
            remove_if_safe(&to)?;
        }
    }
    Ok(())
}

fn replace_with_shadow(from: &Path, to: &Path) -> Result<(), NativeError> {
    if !from.exists() {
        if to.exists() {
            return Ok(());
        }
        return Err(NativeError::InvalidInput(format!(
            "publish source does not exist: {}",
            from.display()
        )));
    }
    ensure_regular_file(from)?;
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let shadow = shadow_path(to);
    if shadow.exists() {
        remove_if_safe(&shadow)?;
    }
    if to.exists() {
        ensure_not_symlink(to)?;
        fs::rename(to, &shadow)?;
        sync_parent(&shadow)?;
    }
    fs::rename(from, to)?;
    sync_parent(to)?;
    Ok(())
}

fn cleanup_candidate_set(layout: &DirectLayout) -> Result<(), NativeError> {
    for path in direct_bundle_paths(&layout.db_candidate_path()) {
        remove_if_safe_if_present(&path)?;
    }
    remove_if_safe_if_present(&layout.manifest_candidate_path())?;
    Ok(())
}

fn cleanup_shadow_set(path: &Path) -> Result<(), NativeError> {
    remove_if_safe_if_present(&shadow_path(path))
}

fn sidecar_checksums(db_candidate_path: &Path) -> Result<BTreeMap<String, String>, NativeError> {
    let mut checksums = BTreeMap::new();
    for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
        let path = PathBuf::from(format!("{}.{}", db_candidate_path.display(), suffix));
        if path.exists() {
            checksums.insert((*suffix).to_string(), sha256_path(&path)?);
        }
    }
    Ok(checksums)
}

fn validate_sidecar_checksums(
    candidate_db_path: &Path,
    db_path: &Path,
    checksums: &BTreeMap<String, String>,
) -> Result<(), NativeError> {
    for (suffix, expected) in checksums {
        let _candidate_path = PathBuf::from(format!("{}.{}", candidate_db_path.display(), suffix));
        let promoted_path = PathBuf::from(format!("{}.{}", db_path.display(), suffix));
        validate_checksum(
            &promoted_path,
            expected,
            &format!("database sidecar .{suffix}"),
        )?;
    }
    Ok(())
}

fn shadow_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.shadow", path.display()))
}

fn remove_if_safe_if_present(path: &Path) -> Result<(), NativeError> {
    if path.exists() {
        remove_if_safe(path)?;
    }
    Ok(())
}

fn remove_if_safe(path: &Path) -> Result<(), NativeError> {
    ensure_not_symlink(path)?;
    fs::remove_file(path)?;
    sync_parent(path)
}

fn ensure_regular_file(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to publish symlinked file {}",
            path.display()
        )));
    }
    if metadata.is_file() {
        Ok(())
    } else {
        Err(NativeError::InvalidInput(format!(
            "direct publish candidate is not a file: {}",
            path.display()
        )))
    }
}

fn ensure_not_symlink(path: &Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to replace symlinked path {}",
            path.display()
        )));
    }
    Ok(())
}

fn sync_parent(path: &Path) -> Result<(), NativeError> {
    let parent = path.parent().ok_or_else(|| {
        NativeError::InvalidInput(format!("path {} has no parent", path.display()))
    })?;
    sync_dir(parent)?;
    if path.exists() {
        File::open(path)?.sync_all()?;
    }
    Ok(())
}

fn sha256_path(path: &Path) -> Result<String, NativeError> {
    let bytes = fs::read(path)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_checksum(path: &Path, expected: &str, label: &str) -> Result<(), NativeError> {
    let actual = sha256_path(path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(NativeError::InvalidInput(format!(
            "{label} checksum mismatch for {}",
            path.display()
        )))
    }
}
