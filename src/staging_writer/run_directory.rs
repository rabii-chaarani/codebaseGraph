use crate::error::NativeError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Owns staging files for one materialization so concurrent runs cannot overwrite each other.
pub(crate) struct StagingRunDirectory {
    path: PathBuf,
}

impl StagingRunDirectory {
    pub(crate) fn create(root: &str) -> Result<Self, NativeError> {
        let root = Path::new(root);
        fs::create_dir_all(root)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        loop {
            let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("run-{}-{timestamp}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn cleanup(self) {
        let _ = fs::remove_dir_all(self.path);
    }
}
