use crate::error::NativeError;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) const DIRECT_DB_SIDECAR_SUFFIXES: &[&str] = &["wal", "tmp", "lock"];

#[derive(Debug, Clone)]
pub(crate) struct RepositoryLayout {
    state_root: PathBuf,
}

impl RepositoryLayout {
    pub(crate) fn new(state_root: impl Into<PathBuf>) -> Self {
        Self {
            state_root: state_root.into(),
        }
    }

    pub(crate) fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub(crate) fn managed(&self) -> ManagedLayout {
        ManagedLayout::new(self.state_root.join("storage"))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedLayout {
    storage_root: PathBuf,
}

impl ManagedLayout {
    pub(crate) fn new(storage_root: impl Into<PathBuf>) -> Self {
        Self {
            storage_root: storage_root.into(),
        }
    }

    pub(crate) fn storage_root(&self) -> &Path {
        &self.storage_root
    }

    pub(crate) fn writer_lock_path(&self) -> PathBuf {
        self.storage_root.join("writer.lock")
    }

    pub(crate) fn state_lock_path(&self) -> PathBuf {
        self.storage_root.join("state.lock")
    }

    pub(crate) fn active_pointer_path(&self) -> PathBuf {
        self.storage_root.join("active.json")
    }

    pub(crate) fn generations_root(&self) -> PathBuf {
        self.storage_root.join("generations")
    }

    pub(crate) fn runs_root(&self) -> PathBuf {
        self.storage_root.join("runs")
    }

    pub(crate) fn artifacts_root(&self) -> PathBuf {
        self.storage_root.join("artifacts")
    }

    pub(crate) fn generation(
        &self,
        generation_id: impl AsRef<str>,
    ) -> Result<GenerationPaths, NativeError> {
        let generation_id = generation_id.as_ref().to_owned();
        validate_generation_id(&generation_id)?;
        Ok(GenerationPaths::new(
            self.generations_root().join(format!("gen-{generation_id}")),
            generation_id,
        ))
    }

    pub(crate) fn ensure_roots(&self) -> Result<(), NativeError> {
        for root in [
            self.storage_root().to_path_buf(),
            self.generations_root(),
            self.runs_root(),
            self.artifacts_root(),
        ] {
            fs::create_dir_all(&root)?;
            let metadata = fs::symlink_metadata(&root)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(NativeError::InvalidInput(format!(
                    "managed storage path must be a real directory: {}",
                    root.display()
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GenerationPaths {
    root: PathBuf,
    generation_id: String,
}

impl GenerationPaths {
    pub(crate) fn new(root: impl Into<PathBuf>, generation_id: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            generation_id: generation_id.into(),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn generation_id(&self) -> &str {
        &self.generation_id
    }

    pub(crate) fn db_path(&self) -> PathBuf {
        self.root.join("graph.ldb")
    }

    pub(crate) fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    pub(crate) fn metadata_path(&self) -> PathBuf {
        self.root.join("metadata.json")
    }

    pub(crate) fn ready_path(&self) -> PathBuf {
        self.root.join("READY")
    }

    pub(crate) fn retired_path(&self) -> PathBuf {
        self.root.join("retired.json")
    }

    pub(crate) fn lease_path(&self) -> PathBuf {
        self.root.join("lease.lock")
    }

    pub(crate) fn ensure_root(&self) -> Result<(), NativeError> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateGenerationLayout {
    root: PathBuf,
    generation_id: String,
}

impl CandidateGenerationLayout {
    pub(crate) fn new(root: impl Into<PathBuf>, generation_id: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            generation_id: generation_id.into(),
        }
    }

    pub(crate) fn generation_paths(&self) -> GenerationPaths {
        GenerationPaths::new(
            self.root.join(format!("gen-{}", self.generation_id)),
            self.generation_id.clone(),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DirectLayout {
    db_path: PathBuf,
    manifest_path: PathBuf,
}

impl DirectLayout {
    pub(crate) fn new(db_path: impl Into<PathBuf>, manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            manifest_path: manifest_path.into(),
        }
    }

    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub(crate) fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub(crate) fn db_candidate_path(&self) -> PathBuf {
        sibling_candidate_path(&self.db_path)
    }

    pub(crate) fn manifest_candidate_path(&self) -> PathBuf {
        sibling_candidate_path(&self.manifest_path)
    }

    pub(crate) fn journal_path(&self) -> PathBuf {
        let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!(
            ".direct-publish-{}.journal.json",
            self.destination_key()
        ))
    }

    pub(crate) fn writer_lock_path(&self) -> PathBuf {
        let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!(".direct-publish-{}.lock", self.destination_key()))
    }

    pub(crate) fn artifact_root_path(&self) -> PathBuf {
        let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(format!(".direct-artifacts-{}", self.destination_key()))
    }

    pub(crate) fn validate(&self) -> Result<(), NativeError> {
        self.db_path.parent().ok_or_else(|| {
            NativeError::InvalidInput("direct database path must have a parent".into())
        })?;
        self.manifest_path.parent().ok_or_else(|| {
            NativeError::InvalidInput("direct manifest path must have a parent".into())
        })?;
        Ok(())
    }

    fn destination_key(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(self.db_path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(self.manifest_path.to_string_lossy().as_bytes());
        let hex: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        hex[..16].to_string()
    }
}

fn sibling_candidate_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "graph".to_string());
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{file_name}.candidate"))
}

pub(crate) fn managed_generation_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = UNIQUE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}", std::process::id(), ts, seq)
}

pub(crate) fn validate_generation_id(generation_id: &str) -> Result<(), NativeError> {
    if generation_id.is_empty()
        || generation_id.len() > 128
        || !generation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(NativeError::InvalidInput(format!(
            "invalid managed generation id: {generation_id:?}"
        )));
    }
    Ok(())
}

pub(crate) fn direct_bundle_paths(base: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(DIRECT_DB_SIDECAR_SUFFIXES.len() + 1);
    paths.push(base.to_path_buf());
    for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
        paths.push(PathBuf::from(format!("{}.{}", base.display(), suffix)));
    }
    paths
}
