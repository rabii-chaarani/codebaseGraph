use crate::api::contracts::RepoSelector;
use crate::storage::direct::DirectStore;
use crate::storage::layout::{DirectLayout, RepositoryLayout};
use crate::storage::locks::WriterLease;
use crate::storage::managed::{GraphStorage, ManagedReadSnapshot, StorageMode};
use crate::storage::run_workspace::RunWorkspace;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
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

/// The repository paths that must remain stable for the lifetime of a
/// coordinator. Configuration settings may be reloaded, but changing this
/// tuple requires restarting the coordinator so its locks and refresh worker
/// cannot be split across two graph installations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryIdentity {
    pub(crate) repo_root: PathBuf,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) storage_root: Option<PathBuf>,
    direct_db_path: Option<PathBuf>,
    direct_manifest_path: Option<PathBuf>,
    explicit_db_path: bool,
    explicit_manifest_path: bool,
}

impl RepositoryIdentity {
    pub(crate) fn capture(selector: &RepoSelector) -> Result<Self, String> {
        let selector = bind_repo_selector(selector)?;
        Self::capture_bound(&selector)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let current_selector = bind_repo_selector(&RepoSelector {
            repo_root: Some(self.repo_root.clone()),
            config_path: self.config_path.clone(),
            db_path: self
                .explicit_db_path
                .then(|| self.direct_db_path.clone())
                .flatten(),
            manifest_path: self
                .explicit_manifest_path
                .then(|| self.direct_manifest_path.clone())
                .flatten(),
        })?;
        let current = Self::capture_bound(&current_selector)?;
        if current == *self {
            return Ok(());
        }
        if current.repo_root != self.repo_root {
            return Err(format!(
                "repository root changed; expected {}, current {}",
                self.repo_root.display(),
                current.repo_root.display()
            ));
        }
        if current.storage_root != self.storage_root {
            return Err(format!(
                "repository storage root changed; expected {}, current {}",
                self.storage_root
                    .as_deref()
                    .map(Path::display)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                current
                    .storage_root
                    .as_deref()
                    .map(Path::display)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
        }
        Err(format!(
            "repository identity changed; restart the coordinator (expected config {}, current config {})",
            self.config_path
                .as_deref()
                .map(Path::display)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            current
                .config_path
                .as_deref()
                .map(Path::display)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string())
        ))
    }

    fn capture_bound(selector: &RepoSelector) -> Result<Self, String> {
        let repo_root = selector
            .repo_root
            .clone()
            .ok_or_else(|| "repository selector did not resolve a repository root".to_string())?;
        let config_path = selector.config_path.clone();
        let config = config_path
            .as_deref()
            .map(read_install_config)
            .transpose()?;
        let storage_root = config.as_ref().and_then(|value| {
            if matches!(
                value.schema_version,
                Some(2) | Some(INSTALL_CONFIG_SCHEMA_VERSION)
            ) {
                let configured = config_path.as_deref().and_then(|config_path| {
                    value
                        .storage_root
                        .as_deref()
                        .map(|path| resolve_config_path(path, config_path))
                });
                Some(configured.unwrap_or_else(|| {
                    RepositoryLayout::new(&RepoPaths::derive(&repo_root).state_dir)
                        .managed()
                        .storage_root()
                        .to_path_buf()
                }))
            } else {
                None
            }
        });
        let direct_db_path = selector
            .db_path
            .as_deref()
            .or_else(|| {
                config
                    .as_ref()
                    .and_then(|value| value.database_path.as_deref())
            })
            .map(|path| {
                config_path
                    .as_deref()
                    .map(|config_path| resolve_config_path(path, config_path))
                    .unwrap_or_else(|| path.to_path_buf())
            });
        let direct_manifest_path = selector
            .manifest_path
            .as_deref()
            .or_else(|| {
                config
                    .as_ref()
                    .and_then(|value| value.manifest_path.as_deref())
            })
            .map(|path| {
                config_path
                    .as_deref()
                    .map(|config_path| resolve_config_path(path, config_path))
                    .unwrap_or_else(|| path.to_path_buf())
            });
        Ok(Self {
            repo_root,
            config_path,
            storage_root: storage_root
                .map(|path| resolve_identity_path(&path, None))
                .transpose()?,
            direct_db_path,
            direct_manifest_path,
            explicit_db_path: selector.db_path.is_some(),
            explicit_manifest_path: selector.manifest_path.is_some(),
        })
    }
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

pub(crate) const INSTALL_CONFIG_SCHEMA_VERSION: u64 = 3;
pub(crate) const DEFAULT_WORKER_MEMORY_MIB: u64 = 768;
pub(crate) const DEFAULT_RUST_MEMORY_MIB: u64 = 384;
pub(crate) const DEFAULT_SPILL_CHUNK_MIB: u64 = 32;
pub(crate) const DEFAULT_MAX_PARALLELISM: usize = 2;
pub(crate) const DEFAULT_RECONCILE_INTERVAL_MS: u64 = 30_000;

const fn default_true() -> bool {
    true
}

const fn default_worker_memory_mib() -> u64 {
    DEFAULT_WORKER_MEMORY_MIB
}

const fn default_rust_memory_mib() -> u64 {
    DEFAULT_RUST_MEMORY_MIB
}

const fn default_spill_chunk_mib() -> u64 {
    DEFAULT_SPILL_CHUNK_MIB
}

const fn default_max_parallelism() -> usize {
    DEFAULT_MAX_PARALLELISM
}

const fn default_reconcile_interval_ms() -> u64 {
    DEFAULT_RECONCILE_INTERVAL_MS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphInstallMaterializationConfig {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_true")]
    pub include_fts: bool,
    #[serde(default)]
    pub semantic_enrichment: bool,
    #[serde(default = "default_worker_memory_mib")]
    pub worker_memory_mib: u64,
    #[serde(default = "default_rust_memory_mib")]
    pub rust_memory_mib: u64,
    #[serde(default = "default_spill_chunk_mib")]
    pub spill_chunk_mib: u64,
    #[serde(default = "default_max_parallelism")]
    pub max_parallelism: usize,
}

impl Default for GraphInstallMaterializationConfig {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            include_fts: true,
            semantic_enrichment: false,
            worker_memory_mib: DEFAULT_WORKER_MEMORY_MIB,
            rust_memory_mib: DEFAULT_RUST_MEMORY_MIB,
            spill_chunk_mib: DEFAULT_SPILL_CHUNK_MIB,
            max_parallelism: DEFAULT_MAX_PARALLELISM,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphRefreshPolicy {
    Off,
    #[default]
    Leader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GraphRefreshBackend {
    #[default]
    Auto,
    Poll,
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphInstallRefreshConfig {
    #[serde(default)]
    pub policy: GraphRefreshPolicy,
    #[serde(default)]
    pub backend: GraphRefreshBackend,
    #[serde(default = "default_reconcile_interval_ms")]
    pub reconcile_interval_ms: u64,
}

impl Default for GraphInstallRefreshConfig {
    fn default() -> Self {
        Self {
            policy: GraphRefreshPolicy::default(),
            backend: GraphRefreshBackend::default(),
            reconcile_interval_ms: DEFAULT_RECONCILE_INTERVAL_MS,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct GraphInstallMcpConfig {
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<GraphInstallMcpHttpConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct GraphInstallMcpHttpConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub service_id: String,
    #[serde(default)]
    pub transport_version: String,
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
    pub refresh: GraphInstallRefreshConfig,
    #[serde(default)]
    pub mcp: Option<GraphInstallMcpConfig>,
}

/// Resolve one repository selector into the canonical repository identity used by
/// storage, coordination, source scanning, and configuration lookup.
///
/// This function deliberately only reads configuration and canonicalizes paths;
/// it does not open a graph database or acquire a lease. Callers that need a
/// runtime can safely use the returned selector as the single pinned identity.
pub(crate) fn bind_repo_selector(selector: &RepoSelector) -> Result<RepoSelector, String> {
    let explicit_root = selector
        .repo_root
        .as_deref()
        .map(canonical_repository_path)
        .transpose()?;
    let explicit_config = selector
        .config_path
        .as_deref()
        .map(canonical_config_path)
        .transpose()?;

    let (config_path, config) = if let Some(path) = explicit_config.clone() {
        (Some(path.clone()), Some(read_install_config(&path)?))
    } else if let Some(root) = explicit_root.as_deref() {
        let path = root.join(".codebaseGraph").join("config.json");
        if path.exists() {
            let path = canonical_config_path(&path)?;
            (Some(path.clone()), Some(read_install_config(&path)?))
        } else {
            (None, None)
        }
    } else {
        discover_install_config()?
    };

    let config_root = config
        .as_ref()
        .and_then(|value| value.repo_root.as_deref())
        .map(|path| canonical_config_repository_path(path, config_path.as_deref()))
        .transpose()?;

    let repo_root = if let Some(root) = explicit_root {
        if let Some(config_root) = config_root.as_ref() {
            let direct_override = selector.db_path.is_some() || selector.manifest_path.is_some();
            let managed_config = config.as_ref().is_some_and(|value| {
                value.schema_version.is_some() || value.storage_root.is_some()
            });
            if (!direct_override || managed_config) && config_root != &root {
                return Err(format!(
                    "repository root {} conflicts with install config root {} in {}",
                    root.display(),
                    config_root.display(),
                    config_path
                        .as_deref()
                        .map(Path::display)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "selected install config".to_string())
                ));
            }
        }
        root
    } else if let Some(root) = config_root {
        root
    } else if let Some(path) = config_path.as_deref() {
        if let Some(root) = conventional_config_repository(path) {
            root?
        } else {
            discover_repository_root()?
        }
    } else {
        discover_repository_root()?
    };

    Ok(RepoSelector {
        repo_root: Some(repo_root),
        config_path,
        db_path: selector
            .db_path
            .as_deref()
            .map(resolve_selector_path)
            .transpose()?,
        manifest_path: selector
            .manifest_path
            .as_deref()
            .map(resolve_selector_path)
            .transpose()?,
    })
}

pub(crate) fn resolve_runtime(selector: &RepoSelector) -> Result<RepoRuntime, String> {
    let selector = bind_repo_selector(selector)?;
    let repo_root = selector
        .repo_root
        .clone()
        .ok_or_else(|| "repository selector did not resolve a repository root".to_string())?;
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
        let db_path = match selector.db_path.clone() {
            Some(path) => path,
            None => config
                .as_ref()
                .and_then(|value| value.database_path.as_deref())
                .map(|path| resolve_config_path(path, &config_path))
                .unwrap_or(paths.db_path.clone()),
        };
        let manifest_path = match selector.manifest_path.clone() {
            Some(path) => path,
            None => config
                .as_ref()
                .and_then(|value| value.manifest_path.as_deref())
                .map(|path| resolve_config_path(path, &config_path))
                .unwrap_or(paths.manifest_path.clone()),
        };
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
        Some(2) | Some(INSTALL_CONFIG_SCHEMA_VERSION) => {
            resolve_managed_runtime(repo_root, paths, config_path, config.as_ref())
        }
        Some(1) => Ok(RepoRuntime {
            repo_root: repo_root.clone(),
            state_dir: paths.state_dir.clone(),
            db_path: config
                .as_ref()
                .and_then(|value| value.database_path.as_deref())
                .map(|path| resolve_config_path(path, &config_path))
                .unwrap_or(paths.db_path),
            manifest_path: config
                .as_ref()
                .and_then(|value| value.manifest_path.as_deref())
                .map(|path| resolve_config_path(path, &config_path))
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
                .and_then(|value| value.database_path.as_deref())
                .map(|path| resolve_config_path(path, &config_path))
                .unwrap_or(paths.db_path);
            let manifest_path = config
                .as_ref()
                .and_then(|value| value.manifest_path.as_deref())
                .map(|path| resolve_config_path(path, &config_path))
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
        .and_then(|value| value.storage_root.as_deref())
        .map(|path| resolve_config_path(path, &config_path))
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
        return canonical_repository_path(path);
    }
    let (config_path, config) = discover_install_config()?;
    if let Some(config) = config {
        if let Some(repo_root) = config.repo_root.as_deref() {
            return canonical_config_repository_path(repo_root, config_path.as_deref());
        }
        if let Some(config_path) = config_path {
            if let Some(root) = conventional_config_repository(&config_path) {
                return root;
            }
        }
    }
    discover_repository_root()
}

fn discover_install_config() -> Result<(Option<PathBuf>, Option<GraphInstallConfig>), String> {
    let current_dir = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    for ancestor in current_dir.ancestors() {
        let config_path = ancestor.join(".codebaseGraph").join("config.json");
        if config_path.exists() {
            let config_path = canonical_config_path(&config_path)?;
            return Ok((
                Some(config_path.clone()),
                Some(read_install_config(&config_path)?),
            ));
        }
    }
    Ok((None, None))
}

fn discover_repository_root() -> Result<PathBuf, String> {
    let current_dir = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    if let Some(git_root) = current_dir
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
    {
        return canonical_repository_path(git_root);
    }
    canonical_repository_path(&current_dir)
}

fn canonical_repository_path(path: &Path) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve repo root {}: {error}", path.display()))?;
    if !path.is_dir() {
        return Err(format!(
            "resolved repository root is not a directory: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn conventional_config_repository(config_path: &Path) -> Option<Result<PathBuf, String>> {
    let state_dir = config_path.parent()?;
    if state_dir.file_name()?.to_str()? != ".codebaseGraph" {
        return None;
    }
    Some(canonical_repository_path(state_dir.parent()?))
}

fn canonical_config_path(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve install config {}: {error}",
            path.display()
        )
    })
}

fn canonical_config_repository_path(
    path: &Path,
    config_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(config_path) = config_path {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to read current directory: {error}"))?
            .join(path)
    };
    canonical_repository_path(&resolved)
}

fn resolve_config_path(path: &Path, config_path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

fn resolve_selector_path(path: &Path) -> Result<PathBuf, String> {
    // Direct-layout journal and lock names hash the destination spelling. Keep
    // that spelling stable for recovery; RepositoryIdentity canonicalizes the
    // same paths separately when it compares repository identity.
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| format!("failed to read current directory: {error}"))
}

pub(crate) fn resolve_identity_path(
    path: &Path,
    config_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let path = config_path
        .map(|config_path| resolve_config_path(path, config_path))
        .unwrap_or_else(|| path.to_path_buf());
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to read current directory: {error}"))?
            .join(path)
    };
    let mut existing = PathBuf::new();
    let mut unresolved = Vec::new();
    let mut peeling = false;
    for component in absolute.components() {
        if peeling {
            unresolved.push(component);
            continue;
        }
        match component {
            Component::Prefix(prefix) => existing.push(prefix.as_os_str()),
            Component::RootDir => existing.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::Normal(_) | Component::ParentDir => {
                let mut candidate = existing.clone();
                candidate.push(component.as_os_str());
                if candidate.exists() {
                    existing = candidate;
                } else {
                    peeling = true;
                    unresolved.push(component);
                }
            }
        }
    }
    let mut normalized = existing.canonicalize().map_err(|error| {
        format!(
            "failed to resolve repository identity path {}: {error}",
            existing.display()
        )
    })?;
    for component in unresolved {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
            Component::Prefix(_) | Component::RootDir => {}
        }
    }
    Ok(normalized)
}

pub(crate) fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}

pub(crate) fn read_install_config(path: &Path) -> Result<GraphInstallConfig, String> {
    let value = read_json_file(path)?;
    let config: GraphInstallConfig = serde_json::from_value(value).map_err(|error| {
        format!(
            "failed to decode install config {}: {error}",
            path.display()
        )
    })?;
    if config.refresh.reconcile_interval_ms == 0 {
        return Err(format!(
            "install config {} refresh.reconcile_interval_ms must be positive",
            path.display()
        ));
    }
    Ok(config)
}

pub(crate) fn read_selected_install_config(
    selector: &RepoSelector,
) -> Result<Option<GraphInstallConfig>, String> {
    let selector = bind_repo_selector(selector)?;
    let repo_root = selector
        .repo_root
        .ok_or_else(|| "repository selector did not resolve a repository root".to_string())?;
    let config_path = selector
        .config_path
        .clone()
        .unwrap_or_else(|| RepoPaths::derive(&repo_root).config_path);
    if config_path.exists() {
        read_install_config(&config_path).map(Some)
    } else {
        Ok(None)
    }
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
    use super::{
        bind_repo_selector, read_install_config, resolve_runtime, GraphRefreshBackend,
        GraphRefreshPolicy, RepoPaths, RepositoryIdentity, DEFAULT_MAX_PARALLELISM,
        DEFAULT_RECONCILE_INTERVAL_MS, DEFAULT_RUST_MEMORY_MIB, DEFAULT_SPILL_CHUNK_MIB,
        DEFAULT_WORKER_MEMORY_MIB,
    };
    use crate::api::contracts::RepoSelector;
    use crate::storage::atomic::write_json_atomically;
    use crate::storage::direct::{DirectPublishJournal, DirectPublishPhase};
    use crate::storage::layout::DirectLayout;
    use crate::storage::locks::{try_open_locked, LockMode};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
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
    fn bind_repo_selector_uses_config_root_before_cwd_discovery() {
        let root = unique_temp_dir("codebase-graph-bind-config-root");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&state).unwrap();
        let config_path = state.join("config.json");
        fs::write(
            &config_path,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 3,
                "repo_root": root,
                "refresh": {"reconcile_interval_ms": 100}
            }))
            .unwrap(),
        )
        .unwrap();

        let bound = bind_repo_selector(&RepoSelector {
            repo_root: None,
            config_path: Some(config_path.clone()),
            db_path: None,
            manifest_path: None,
        })
        .unwrap();

        assert_eq!(bound.repo_root, Some(root.canonicalize().unwrap()));
        assert_eq!(bound.config_path, Some(config_path.canonicalize().unwrap()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_repo_selector_rejects_managed_root_config_mismatch() {
        let root = unique_temp_dir("codebase-graph-bind-explicit-root");
        let configured_root = unique_temp_dir("codebase-graph-bind-configured-root");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        fs::create_dir_all(&configured_root).unwrap();
        let config_path = root.join(".codebaseGraph/config.json");
        fs::write(
            &config_path,
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 3,
                "repo_root": configured_root
            }))
            .unwrap(),
        )
        .unwrap();

        let error = bind_repo_selector(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: Some(config_path),
            db_path: None,
            manifest_path: None,
        })
        .unwrap_err();
        assert!(error.contains("conflicts with install config root"));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(configured_root);
    }

    #[test]
    fn bind_repo_selector_does_not_treat_arbitrary_config_parent_as_repository() {
        let config_dir = unique_temp_dir("codebase-graph-bind-custom-config");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.json");
        fs::write(&config_path, br#"{"schema_version":3}"#).unwrap();

        let bound = bind_repo_selector(&RepoSelector {
            repo_root: None,
            config_path: Some(config_path),
            db_path: None,
            manifest_path: None,
        })
        .unwrap();
        assert_ne!(bound.repo_root, Some(PathBuf::from("/")));
        let _ = fs::remove_dir_all(config_dir);
    }

    #[test]
    fn repository_identity_rejects_storage_root_rebind() {
        let root = unique_temp_dir("codebase-graph-identity");
        let storage_one = root.join("storage-one");
        let storage_two = root.join("storage-two");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&storage_one).unwrap();
        fs::create_dir_all(&storage_two).unwrap();
        let config_path = state.join("config.json");
        let write_config = |storage: &Path| {
            fs::write(
                &config_path,
                serde_json::to_vec(&serde_json::json!({
                    "schema_version": 3,
                    "repo_root": root,
                    "storage_root": storage
                }))
                .unwrap(),
            )
            .unwrap();
        };
        write_config(&storage_one);
        let identity = RepositoryIdentity::capture(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: Some(config_path.clone()),
            db_path: None,
            manifest_path: None,
        })
        .unwrap();
        write_config(&storage_two);
        let error = identity.validate().unwrap_err();
        assert!(error.contains("storage root changed"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_repo_selector_preserves_explicit_direct_path_spelling() {
        let root = unique_temp_dir("codebase-graph-bind-paths");
        fs::create_dir_all(&root).unwrap();
        let bound = bind_repo_selector(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: Some(root.join("missing/../graph.ldb")),
            manifest_path: Some(root.join("missing/../manifest.json")),
        })
        .unwrap();
        assert_eq!(bound.db_path, Some(root.join("missing/../graph.ldb")));
        assert_eq!(
            bound.manifest_path,
            Some(root.join("missing/../manifest.json"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_identity_path_resolves_symlink_parent_before_missing_suffix() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("codebase-graph-bind-symlink");
        let real = root.join("real");
        fs::create_dir_all(real.join("sub")).unwrap();
        symlink(real.join("sub"), root.join("link")).unwrap();
        let resolved =
            super::resolve_identity_path(&root.join("link/../missing.ldb"), None).unwrap();
        assert_eq!(resolved, real.canonicalize().unwrap().join("missing.ldb"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repository_identity_rejects_config_direct_path_rebind() {
        let root = unique_temp_dir("codebase-graph-identity-direct");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&state).unwrap();
        let config_path = state.join("config.json");
        let write_config = |database_path: &str| {
            fs::write(
                &config_path,
                serde_json::to_vec(&serde_json::json!({
                    "database_path": database_path,
                    "manifest_path": "manifest.json"
                }))
                .unwrap(),
            )
            .unwrap();
        };
        write_config("database-a.ldb");
        let identity = RepositoryIdentity::capture(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: Some(config_path.clone()),
            db_path: None,
            manifest_path: None,
        })
        .unwrap();
        write_config("database-b.ldb");
        let error = identity.validate().unwrap_err();
        assert!(error.contains("repository identity changed"));
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

        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 3,
                "repo_root": root,
                "storage_root": storage,
            }))
            .unwrap(),
        )
        .unwrap();
        let schema_v3_runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        })
        .expect("schema-v3 managed runtime should resolve");
        assert_eq!(schema_v3_runtime.storage_format(), "managed_v2");
        assert_eq!(schema_v3_runtime.active_generation.as_deref(), Some("demo"));
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
        assert_eq!(config.refresh.policy, GraphRefreshPolicy::Leader);
        assert_eq!(config.refresh.backend, GraphRefreshBackend::Auto);
        assert_eq!(
            config.refresh.reconcile_interval_ms,
            DEFAULT_RECONCILE_INTERVAL_MS
        );
        assert!(config.materialization.include_fts);
        assert!(!config.materialization.semantic_enrichment);
        assert_eq!(
            config.materialization.worker_memory_mib,
            DEFAULT_WORKER_MEMORY_MIB
        );
        assert_eq!(
            config.materialization.rust_memory_mib,
            DEFAULT_RUST_MEMORY_MIB
        );
        assert_eq!(
            config.materialization.spill_chunk_mib,
            DEFAULT_SPILL_CHUNK_MIB
        );
        assert_eq!(
            config.materialization.max_parallelism,
            DEFAULT_MAX_PARALLELISM
        );
        let _ = fs::remove_dir_all(root);
    }
}
