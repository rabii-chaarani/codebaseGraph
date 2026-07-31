use std::path::{Path, PathBuf};

use crate::projection::ProjectionStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    Initialized { state_root: PathBuf },
    AlreadyInitialized { state_root: PathBuf },
}

pub fn install_repository(repository_root: &Path) -> Result<InstallOutcome, String> {
    let repository_root = repository_root
        .canonicalize()
        .map_err(|_| "the repository root could not be located".to_string())?;
    if !repository_root.is_dir() {
        return Err("the repository root is not a directory".to_string());
    }

    let store = ProjectionStore::new(repository_root);
    let state_root = store.state_root();
    let already_initialized = state_root.is_dir();
    store
        .initialize()
        .map_err(|_| "the .kwiki state directory could not be initialized".to_string())?;

    Ok(if already_initialized {
        InstallOutcome::AlreadyInitialized { state_root }
    } else {
        InstallOutcome::Initialized { state_root }
    })
}

#[cfg(test)]
mod tests {
    use super::{install_repository, InstallOutcome};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn install_repository_initializes_and_reuses_local_wiki_state() {
        let root = unique_temp_dir("install");

        let outcome = install_repository(&root).expect("initialize repository state");
        assert!(matches!(outcome, InstallOutcome::Initialized { .. }));
        for directory in [
            ".kwiki/staging",
            ".kwiki/generations",
            ".kwiki/cache",
            ".kwiki/site",
        ] {
            assert!(root.join(directory).is_dir(), "missing {directory}");
        }

        let repeat = install_repository(&root).expect("reuse repository state");
        assert!(matches!(repeat, InstallOutcome::AlreadyInitialized { .. }));

        fs::remove_dir_all(root).expect("remove temp root");
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("k_wiki_{prefix}_{}_{}", std::process::id(), unique));
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }
}
