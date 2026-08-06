use crate::api::contracts::RepoSelector;
use crate::storage::direct::DirectStore;
use crate::storage::layout::{DirectLayout, RepositoryLayout};
use crate::storage::locks::WriterLease;
use crate::storage::managed::{GraphStorage, ManagedReadSnapshot, StorageMode};
use crate::storage::run_workspace::RunWorkspace;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct RepoRuntime {
    pub repo_root: PathBuf,
    pub state_dir: PathBuf,
    pub db_path: PathBuf,
    pub manifest_path: PathBuf,
    pub config_path: Option<PathBuf>,
    pub storage_mode: StorageMode,
    pub storage_root: Option<PathBuf>,
    pub active_generation: Option<String>,
    pub writable: bool,
    pub cleanup_pending: bool,
    pub pending_runs: usize,
    #[allow(dead_code)]
    pub active_read: Option<Arc<ManagedReadSnapshot>>,
    #[allow(dead_code)]
    pub direct_read: Option<Arc<WriterLease>>,
}

impl RepoRuntime {
    pub(crate) fn storage_format(&self) -> &'static str {
        match self.storage_mode {
            StorageMode::Direct => "direct",
            StorageMode::LegacyManagedV1 => "legacy_v1",
            StorageMode::ManagedV2 => "managed_v2",
        }
    }

    pub(crate) fn require_graph_write(&self) -> Result<(), String> {
        if self.legacy_schema_version().is_some() {
            return Err(format!(
                "legacy installed graph storage requires reinstall before writes; run `{}`",
                self.reinstall_command()
            ));
        }
        Ok(())
    }

    pub(crate) fn release_read_leases(&mut self) {
        self.active_read = None;
        self.direct_read = None;
    }

    pub(crate) fn legacy_schema_version(&self) -> Option<u64> {
        matches!(self.storage_mode, StorageMode::LegacyManagedV1).then_some(1)
    }

    pub(crate) fn remediation(&self) -> Option<String> {
        self.legacy_schema_version()
            .map(|_| format!("Run `{}`.", self.reinstall_command()))
    }

    fn reinstall_command(&self) -> String {
        format!(
            "codebase-graph reinstall --repo-root {}",
            self.repo_root.display()
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RepoPaths {
    pub(crate) repo_name: String,
    pub(crate) state_dir: PathBuf,
    pub(crate) db_path: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) config_path: PathBuf,
}

impl RepoPaths {
    pub(crate) fn derive(repo_root: &Path) -> Self {
        let repo_name = safe_name(
            repo_root
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("repository"),
        );
        let state_dir = repo_root.join(".codebaseGraph");
        Self {
            repo_name: repo_name.clone(),
            state_dir: state_dir.clone(),
            db_path: state_dir.join(format!("{repo_name}_graph.ldb")),
            manifest_path: state_dir.join("manifest.json"),
            config_path: state_dir.join("config.json"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct GraphInstallMaterializationConfig {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct GraphInstallMcpConfig {
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct GraphInstallConfig {
    #[serde(default)]
    pub schema_version: Option<u64>,
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    #[serde(default)]
    pub repo_name: Option<String>,
    #[serde(default)]
    pub state_dir: Option<PathBuf>,
    #[serde(default)]
    pub storage_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<PathBuf>,
    #[serde(default)]
    pub ontology_version: Option<String>,
    #[serde(default)]
    pub package_version: Option<String>,
    #[serde(default)]
    pub materialization: GraphInstallMaterializationConfig,
    #[serde(default)]
    pub mcp: Option<GraphInstallMcpConfig>,
}

pub(crate) fn resolve_runtime(selector: &RepoSelector) -> Result<RepoRuntime, String> {
    let repo_root = resolve_repository_root(selector.repo_root.as_deref())?;
    let paths = RepoPaths::derive(&repo_root);
    let config_path = selector
        .config_path
        .clone()
        .unwrap_or_else(|| paths.config_path.clone());
    let config = if config_path.exists() {
        Some(read_install_config(&config_path)?)
    } else {
        None
    };

    if selector.db_path.is_some() || selector.manifest_path.is_some() {
        let db_path = selector
            .db_path
            .clone()
            .or_else(|| {
                config
                    .as_ref()
                    .and_then(|value| value.database_path.clone())
            })
            .unwrap_or(paths.db_path.clone());
        let manifest_path = selector
            .manifest_path
            .clone()
            .or_else(|| {
                config
                    .as_ref()
                    .and_then(|value| value.manifest_path.clone())
            })
            .unwrap_or(paths.manifest_path.clone());
        let direct_read = resolve_direct_read(&db_path, &manifest_path)?;
        let direct_cleanup = RunWorkspace::cleanup_orphans(paths.state_dir.join("direct-runs"))
            .map_err(|error| format!("failed to clean direct run workspaces: {error}"))?;
        return Ok(RepoRuntime {
            repo_root: repo_root.clone(),
            state_dir: paths.state_dir.clone(),
            db_path,
            manifest_path,
            config_path: config_path.exists().then_some(config_path),
            storage_mode: StorageMode::Direct,
            storage_root: None,
            active_generation: None,
            writable: true,
            cleanup_pending: direct_cleanup.skipped_locked > 0,
            pending_runs: direct_cleanup.skipped_locked,
            active_read: None,
            direct_read,
        });
    }

    match config.as_ref().and_then(|value| value.schema_version) {
        Some(2) => resolve_managed_runtime(repo_root, paths, config_path, config.as_ref()),
        Some(1) => Ok(RepoRuntime {
            repo_root: repo_root.clone(),
            state_dir: paths.state_dir.clone(),
            db_path: config
                .as_ref()
                .and_then(|value| value.database_path.clone())
                .unwrap_or(paths.db_path),
            manifest_path: config
                .as_ref()
                .and_then(|value| value.manifest_path.clone())
                .unwrap_or(paths.manifest_path),
            config_path: Some(config_path),
            storage_mode: StorageMode::LegacyManagedV1,
            storage_root: None,
            active_generation: None,
            writable: false,
            cleanup_pending: false,
            pending_runs: 0,
            active_read: None,
            direct_read: None,
        }),
        Some(other) => Err(format!(
            "unsupported graph storage schema_version {other} in {}",
            config_path.display()
        )),
        None => {
            let db_path = config
                .as_ref()
                .and_then(|value| value.database_path.clone())
                .unwrap_or(paths.db_path);
            let manifest_path = config
                .as_ref()
                .and_then(|value| value.manifest_path.clone())
                .unwrap_or(paths.manifest_path);
            let direct_read = resolve_direct_read(&db_path, &manifest_path)?;
            let direct_cleanup = RunWorkspace::cleanup_orphans(paths.state_dir.join("direct-runs"))
                .map_err(|error| format!("failed to clean direct run workspaces: {error}"))?;
            Ok(RepoRuntime {
                repo_root: repo_root.clone(),
                state_dir: paths.state_dir.clone(),
                db_path,
                manifest_path,
                config_path: config_path.exists().then_some(config_path),
                storage_mode: StorageMode::Direct,
                storage_root: None,
                active_generation: None,
                writable: true,
                cleanup_pending: direct_cleanup.skipped_locked > 0,
                pending_runs: direct_cleanup.skipped_locked,
                active_read: None,
                direct_read,
            })
        }
    }
}

fn resolve_managed_runtime(
    repo_root: PathBuf,
    paths: RepoPaths,
    config_path: PathBuf,
    config: Option<&GraphInstallConfig>,
) -> Result<RepoRuntime, String> {
    let storage_root = config
        .and_then(|value| value.storage_root.clone())
        .unwrap_or_else(|| {
            RepositoryLayout::new(&paths.state_dir)
                .managed()
                .storage_root()
                .to_path_buf()
        });
    let store = GraphStorage::managed(storage_root.clone());
    let cleanup = store.cleanup().map_err(|error| error.to_string())?;
    let snapshot = store
        .resolve_active_read()
        .map_err(|error| error.to_string())?;
    let (db_path, manifest_path, active_generation, active_read) = match snapshot {
        Some(snapshot) => {
            let snapshot = Arc::new(snapshot);
            (
                snapshot.db_path.clone(),
                snapshot.manifest_path.clone(),
                Some(snapshot.generation_id.clone()),
                Some(snapshot),
            )
        }
        None => (
            paths.db_path.clone(),
            paths.manifest_path.clone(),
            None,
            None,
        ),
    };
    Ok(RepoRuntime {
        repo_root,
        state_dir: paths.state_dir,
        db_path,
        manifest_path,
        config_path: Some(config_path),
        storage_mode: StorageMode::ManagedV2,
        storage_root: Some(storage_root),
        active_generation,
        writable: true,
        cleanup_pending: cleanup.retired_generations_pending > 0
            || cleanup.run_recovery.skipped_locked > 0,
        pending_runs: cleanup.run_recovery.skipped_locked,
        active_read,
        direct_read: None,
    })
}

fn resolve_direct_read(
    db_path: &Path,
    manifest_path: &Path,
) -> Result<Option<Arc<WriterLease>>, String> {
    let layout = DirectLayout::new(db_path, manifest_path);
    if !db_path.exists()
        && !manifest_path.exists()
        && !layout.journal_path().exists()
        && !layout.db_candidate_path().exists()
        && !layout.manifest_candidate_path().exists()
    {
        return Ok(None);
    }
    DirectStore::new(layout)
        .and_then(|store| store.begin_read())
        .map(Arc::new)
        .map(Some)
        .map_err(|error| format!("failed to recover direct graph publication: {error}"))
}

pub(crate) fn resolve_repository_root(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return path
            .canonicalize()
            .map_err(|error| format!("failed to resolve repo root: {error}"));
    }
    let current_dir = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    for ancestor in current_dir.ancestors() {
        let config_path = ancestor.join(".codebaseGraph").join("config.json");
        if config_path.exists() {
            let config = read_install_config(&config_path)?;
            if let Some(repo_root) = config.repo_root {
                return Ok(repo_root.canonicalize().unwrap_or(repo_root));
            }
            return Ok(ancestor.to_path_buf());
        }
    }
    if let Some(git_root) = current_dir
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
    {
        return Ok(git_root.to_path_buf());
    }
    Ok(current_dir)
}

pub(crate) fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}

pub(crate) fn read_install_config(path: &Path) -> Result<GraphInstallConfig, String> {
    let value = read_json_file(path)?;
    serde_json::from_value(value).map_err(|error| {
        format!(
            "failed to decode install config {}: {error}",
            path.display()
        )
    })
}

fn safe_name(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = normalized.trim_matches(['.', '_', '-']);
    if trimmed.is_empty() {
        "repository".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{read_install_config, resolve_runtime, RepoPaths};
    use crate::api::contracts::RepoSelector;
    use crate::storage::atomic::write_json_atomically;
    use crate::storage::direct::{DirectPublishJournal, DirectPublishPhase};
    use crate::storage::layout::DirectLayout;
    use crate::storage::locks::{try_open_locked, LockMode};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn graph_paths_are_deterministic() {
        let paths = RepoPaths::derive(&std::path::PathBuf::from("/tmp/demo"));
        assert_eq!(paths.repo_name, "demo");
        assert_eq!(
            paths.state_dir,
            std::path::PathBuf::from("/tmp/demo/.codebaseGraph")
        );
        assert!(paths.db_path.to_string_lossy().ends_with("demo_graph.ldb"));
        assert_eq!(
            paths.manifest_path,
            std::path::PathBuf::from("/tmp/demo/.codebaseGraph/manifest.json")
        );
        assert_eq!(
            paths.config_path,
            std::path::PathBuf::from("/tmp/demo/.codebaseGraph/config.json")
        );
    }

    #[test]
    fn resolve_runtime_selects_configured_graph_and_manifest_paths() {
        let root = unique_temp_dir("codebase-graph-api-runtime");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&state).expect("state directory should be created");
        let graph_path = state.join("configured.ldb");
        let manifest_path = state.join("configured-manifest.json");
        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "database_path": graph_path,
                "manifest_path": manifest_path,
            }))
            .expect("config should serialize"),
        )
        .expect("config should be written");

        let runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        })
        .expect("runtime should resolve");

        assert_eq!(
            runtime.repo_root,
            root.canonicalize().expect("root should canonicalize")
        );
        assert_eq!(runtime.db_path, graph_path);
        assert_eq!(runtime.manifest_path, manifest_path);
        assert_eq!(runtime.storage_format(), "direct");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_runtime_prefers_explicit_graph_and_manifest_paths() {
        let root = unique_temp_dir("codebase-graph-api-runtime-explicit");
        fs::create_dir_all(&root).expect("repository should be created");
        let graph_path = root.join("explicit.ldb");
        let manifest_path = root.join("explicit-manifest.json");

        let runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: Some(graph_path.clone()),
            manifest_path: Some(manifest_path.clone()),
        })
        .expect("runtime should resolve");

        assert_eq!(runtime.db_path, graph_path);
        assert_eq!(runtime.manifest_path, manifest_path);
        assert_eq!(runtime.storage_format(), "direct");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_runtime_recovers_direct_pair_before_returning_read_lease() {
        let root = unique_temp_dir("codebase-graph-api-runtime-direct-recovery");
        fs::create_dir_all(&root).expect("repository should be created");
        let db_path = root.join("graph.ldb");
        let manifest_path = root.join("manifest.json");
        let layout = DirectLayout::new(&db_path, &manifest_path);
        let db_v2 = b"database-v2";
        let manifest_v2 = b"{\"version\":2}\n";
        fs::write(&db_path, db_v2).unwrap();
        fs::write(&manifest_path, "{\"version\":1}\n").unwrap();
        fs::write(layout.manifest_candidate_path(), manifest_v2).unwrap();
        write_json_atomically(
            &layout.journal_path(),
            &DirectPublishJournal {
                phase: DirectPublishPhase::DatabasePromoted,
                db_path: db_path.clone(),
                db_candidate_path: layout.db_candidate_path(),
                manifest_path: manifest_path.clone(),
                manifest_candidate_path: layout.manifest_candidate_path(),
                db_sha256: sha256(db_v2),
                manifest_sha256: sha256(manifest_v2),
                sidecar_sha256: BTreeMap::new(),
            },
        )
        .unwrap();

        let runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: Some(db_path),
            manifest_path: Some(manifest_path.clone()),
        })
        .expect("direct runtime should recover before resolving");

        assert_eq!(fs::read(&manifest_path).unwrap(), manifest_v2);
        assert!(!layout.journal_path().exists());
        assert!(runtime.direct_read.is_some());
        assert!(
            try_open_locked(layout.writer_lock_path(), LockMode::Exclusive)
                .unwrap()
                .is_none()
        );
        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }

    fn sha256(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn resolve_runtime_reads_managed_v2_active_generation() {
        let root = unique_temp_dir("codebase-graph-api-runtime-managed");
        let state = root.join(".codebaseGraph");
        let storage = state.join("storage");
        let generation = storage.join("generations").join("gen-demo");
        fs::create_dir_all(&generation).unwrap();
        fs::write(generation.join("READY"), "ready\n").unwrap();
        fs::write(generation.join("graph.ldb"), b"db-demo").unwrap();
        fs::write(generation.join("manifest.json"), "{}\n").unwrap();
        fs::write(
            generation.join("metadata.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "demo",
                "created_at_ms": 0,
                "published_at_ms": 0,
                "logical_size_bytes": 0,
                "physical_size_bytes": 0,
                "node_count": 0,
                "edge_count": 0
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "repo_root": root,
                "storage_root": storage,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("storage").join("active.json"),
            serde_json::to_vec(&serde_json::json!({
                "generation_id": "demo",
                "published_at": "unix:0",
            }))
            .unwrap(),
        )
        .unwrap();

        let runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        })
        .unwrap();

        assert_eq!(runtime.storage_format(), "managed_v2");
        assert_eq!(runtime.active_generation.as_deref(), Some("demo"));
        assert_eq!(runtime.db_path, generation.join("graph.ldb"));
        assert_eq!(runtime.manifest_path, generation.join("manifest.json"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_config_deserializes_partial_mcp_defaults() {
        let root = unique_temp_dir("codebase-graph-api-config-defaults");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&state).unwrap();
        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&json!({
                "schema_version": 2,
                "repo_root": root,
                "mcp": {},
            }))
            .unwrap(),
        )
        .unwrap();

        let config = read_install_config(&state.join("config.json")).unwrap();
        let mcp = config.mcp.expect("mcp config should deserialize");
        assert_eq!(mcp.server_name, "");
        assert!(mcp.command.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
