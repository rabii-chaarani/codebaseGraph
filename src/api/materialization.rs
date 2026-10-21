use crate::api::contracts::RepoSelector;
use crate::api::{
    context::{
        resolve_identity_path, resolve_repository_root, RepoPaths, RepoRuntime, RepositoryIdentity,
    },
    contracts::MaterializationRequest,
};
use crate::artifact_store::ArtifactStore;
use crate::protocol::{
    MaintenanceSummary, ManifestEntry, NativeManifest, NativeSyntaxMaterializationRequest,
    NativeSyntaxMaterializationResponse, MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
};
use crate::storage::direct::{DirectStore, DirectWriteSession};
use crate::storage::layout::{DirectLayout, RepositoryLayout, DIRECT_DB_SIDECAR_SUFFIXES};
use crate::storage::managed::{
    GraphStorage, ManagedJanitorReport, ManagedStore, ManagedWriteSession, StorageMode,
};
use crate::storage::run_workspace::{RunPhase, RunWorkspace};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[cfg(test)]
use crate::api::contracts::OutputFormat;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MaterializeOptions {
    pub(crate) native_request: Option<PathBuf>,
    #[serde(default)]
    pub(crate) native_request_unbound: bool,
    pub(crate) source_root: Option<PathBuf>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) db: Option<PathBuf>,
    pub(crate) manifest: Option<PathBuf>,
    pub(crate) storage_root: Option<PathBuf>,
    pub(crate) mode: String,
    pub(crate) include_fts: bool,
    pub(crate) use_git: bool,
    pub(crate) git_diff: bool,
    pub(crate) git_base: Option<String>,
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) candidate_paths: Vec<String>,
    pub(crate) parallel: bool,
    pub(crate) worker_memory_mib: Option<u64>,
    pub(crate) rust_memory_mib: Option<u64>,
    pub(crate) spill_chunk_mib: Option<u64>,
    pub(crate) max_parallelism: Option<usize>,
    pub(crate) progress: bool,
    pub(crate) plan_only: bool,
    pub(crate) intent: MaterializationIntent,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) enum MaterializationIntent {
    #[default]
    ExplicitBuild,
    Refresh,
}

impl Default for MaterializeOptions {
    fn default() -> Self {
        Self {
            native_request: None,
            native_request_unbound: false,
            source_root: None,
            config: None,
            db: None,
            manifest: None,
            storage_root: None,
            mode: String::new(),
            include_fts: false,
            use_git: false,
            git_diff: false,
            git_base: None,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            parallel: true,
            worker_memory_mib: None,
            rust_memory_mib: None,
            spill_chunk_mib: None,
            max_parallelism: None,
            progress: false,
            plan_only: false,
            intent: MaterializationIntent::ExplicitBuild,
        }
    }
}

impl MaterializeOptions {
    pub(crate) fn from_request(
        request: &MaterializationRequest,
        runtime: &RepoRuntime,
        plan_only: bool,
    ) -> Self {
        Self {
            native_request: request.native_request_path.clone(),
            native_request_unbound: request.native_request_path.is_some()
                && matches!(&runtime.storage_mode, &StorageMode::Direct)
                && request.repo.repo_root.is_none()
                && request.repo.config_path.is_none()
                && request.source_root.is_none(),
            source_root: Some(runtime.repo_root.clone()),
            config: runtime.config_path.clone(),
            db: Some(runtime.db_path.clone()),
            manifest: Some(runtime.manifest_path.clone()),
            storage_root: runtime.storage_root.clone(),
            mode: request.mode.clone(),
            include_fts: request.include_fts,
            use_git: request.use_git,
            git_diff: request.git_diff,
            git_base: request.git_base.clone(),
            include_patterns: request.include_patterns.clone(),
            exclude_patterns: request.exclude_patterns.clone(),
            candidate_paths: request.candidate_paths.clone(),
            parallel: request.parallel,
            worker_memory_mib: request.worker_memory_mib,
            rust_memory_mib: request.rust_memory_mib,
            spill_chunk_mib: request.spill_chunk_mib,
            max_parallelism: request.max_parallelism,
            progress: request.progress,
            plan_only,
            ..Self::default()
        }
    }
}

fn capture_materialization_identity(
    options: &MaterializeOptions,
) -> Result<Option<RepositoryIdentity>, String> {
    // A standalone native request has always been allowed to choose its own
    // source root while the caller supplies only direct output paths. There is
    // no caller-bound repository tuple to validate in that mode.
    if options.native_request_unbound && options.storage_root.is_none() {
        return Ok(None);
    }
    RepositoryIdentity::capture(&RepoSelector {
        repo_root: options.source_root.clone(),
        config_path: options.config.clone(),
        db_path: options.db.clone(),
        manifest_path: options.manifest.clone(),
    })
    .map(Some)
    .map_err(|error| format!("failed to capture materialization repository identity: {error}"))
}

fn validate_native_request_identity(
    identity: Option<&RepositoryIdentity>,
    request_source_root: &str,
) -> Result<(), String> {
    let Some(identity) = identity else {
        return Ok(());
    };
    let request_root = resolve_materialization_path(Path::new(request_source_root))?;
    if request_root == identity.repo_root {
        return Ok(());
    }
    Err(format!(
        "materialization source root {} does not match selected repository root {}",
        request_root.display(),
        identity.repo_root.display()
    ))
}

fn validate_configured_storage_root(
    options: &MaterializeOptions,
    identity: Option<&RepositoryIdentity>,
) -> Result<(), String> {
    let Some(identity) = identity else {
        return Ok(());
    };
    let Some(configured_storage_root) = identity.storage_root.as_deref() else {
        return Ok(());
    };
    let Some(selected_storage_root) = options.storage_root.as_deref() else {
        return Ok(());
    };
    let selected_storage_root = resolve_materialization_path(selected_storage_root)?;
    if selected_storage_root == configured_storage_root {
        return Ok(());
    }
    Err(format!(
        "materialization storage root {} does not match configured repository storage root {}",
        selected_storage_root.display(),
        configured_storage_root.display()
    ))
}

fn resolve_materialization_path(path: &Path) -> Result<PathBuf, String> {
    resolve_identity_path(path, None)
}

#[derive(Default)]
pub(crate) struct ConfigScanRules {
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) worker_memory_mib: Option<u64>,
    pub(crate) rust_memory_mib: Option<u64>,
    pub(crate) spill_chunk_mib: Option<u64>,
    pub(crate) max_parallelism: Option<usize>,
}

#[cfg(test)]
pub(crate) fn materialization_request(
    options: &MaterializeOptions,
    output_format: OutputFormat,
) -> Result<MaterializationRequest, String> {
    Ok(MaterializationRequest {
        repo: RepoSelector {
            repo_root: options.source_root.clone(),
            config_path: options.config.clone(),
            db_path: options.db.clone(),
            manifest_path: options.manifest.clone(),
        },
        native_request_path: options.native_request.clone(),
        source_root: options
            .source_root
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        use_git: options.use_git,
        git_diff: options.git_diff,
        git_base: options.git_base.clone(),
        include_patterns: options.include_patterns.clone(),
        exclude_patterns: options.exclude_patterns.clone(),
        candidate_paths: options.candidate_paths.clone(),
        parallel: options.parallel,
        worker_memory_mib: options.worker_memory_mib,
        rust_memory_mib: options.rust_memory_mib,
        spill_chunk_mib: options.spill_chunk_mib,
        max_parallelism: options.max_parallelism,
        progress: options.progress,
        output_format,
    })
}

pub(crate) fn execute_materialization(
    options: &MaterializeOptions,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let request = match options.native_request.as_ref() {
        Some(request_path) => read_request(request_path)?,
        None => build_request(options)?,
    };
    execute_materialization_request(options, request)
}

pub(crate) fn execute_candidate_materialization(
    options: &MaterializeOptions,
    candidate_paths: Vec<String>,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let mut request = build_request(options)?;
    request.candidate_paths = candidate_paths;
    execute_materialization_request(options, request)
}

pub(crate) fn execute_materialization_request(
    options: &MaterializeOptions,
    request: NativeSyntaxMaterializationRequest,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let identity = capture_materialization_identity(options)?;
    validate_native_request_identity(identity.as_ref(), &request.source_root)?;
    validate_configured_storage_root(options, identity.as_ref())?;

    let mut request = request;
    let execution = prepare_storage_execution(options)?;
    request.db_path = execution.request_db_path().to_string_lossy().into_owned();
    request.staging_dir = execution.staging_root().to_string_lossy().into_owned();
    request.previous_manifest = execution.previous_manifest().clone();
    request.artifact_root = execution.artifact_root().to_string_lossy().into_owned();
    request.manifest_schema_version = MATERIALIZATION_MANIFEST_SCHEMA_VERSION;
    if let Some(identity) = identity.as_ref() {
        if let Err(error) = identity.validate() {
            return Err(execution.abort_with_cleanup(format!(
                "materialization repository identity changed before execution: {error}"
            )));
        }
    }
    if options.intent == MaterializationIntent::Refresh {
        let mut response = match crate::plan_materialization(&request) {
            Ok(response) => response,
            Err(error) => return Err(execution.abort_with_cleanup(error.to_string())),
        };
        if refresh_plan_is_current(&response) {
            acknowledge_refresh_noop(execution, identity.as_ref(), &mut response)?;
            return Ok((request, response));
        }
    }
    let started = Instant::now();
    let final_request = request;
    let mut response =
        match crate::db_writer::with_phase_workspace_root(execution.phase_workspace_root(), || {
            crate::execute_materialization_pipeline(&final_request)
        }) {
            Ok(response) => response,
            Err(error) => return Err(execution.abort_with_cleanup(error.to_string())),
        };
    response.phase_timings.insert(
        "native_cli_seconds".to_string(),
        started.elapsed().as_secs_f64(),
    );
    finalize_materialization(execution, &final_request, &mut response, identity.as_ref())?;
    Ok((final_request, response))
}

fn acknowledge_refresh_noop(
    execution: StorageExecution,
    identity: Option<&RepositoryIdentity>,
    response: &mut NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    if let Some(identity) = identity {
        if let Err(error) = identity.validate() {
            return Err(execution.abort_with_cleanup(format!(
                "materialization repository identity changed before refresh acknowledgement: {error}"
            )));
        }
    }
    response.storage_format = execution.storage_format().to_string();
    response.active_generation = execution.active_generation();
    execution.finish_without_publish()
}

pub(crate) fn plan_materialization_payload(
    response: &NativeSyntaxMaterializationResponse,
    mode: &str,
    manifest_path: &Path,
) -> serde_json::Value {
    let rebuilt_paths = response.diff.rebuild_paths();
    let skipped_paths = response
        .snapshots
        .iter()
        .filter(|(_, snapshot)| snapshot.language.is_none())
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let ignored_paths = response
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.strip_prefix("Ignored file: "))
        .map(str::to_string)
        .collect::<Vec<_>>();
    serde_json::json!({
        "mode": mode,
        "scanned": response.snapshots.len(),
        "rebuilt": rebuilt_paths.len(),
        "skipped": skipped_paths.len(),
        "ignored": ignored_paths.len(),
        "deleted": response.diff.deleted.len(),
        "diagnostics": response.diagnostics,
        "manifest_path": manifest_path,
        "rebuilt_paths": rebuilt_paths,
        "skipped_paths": skipped_paths.clone(),
        "ignored_paths": ignored_paths,
        "deleted_paths": response.diff.deleted.clone(),
        "would_rebuild": response.diff.rebuild_paths(),
        "would_delete": response.diff.deleted,
        "would_skip": skipped_paths,
        "graph_summary": response.graph_summary,
        "node_rows": response.node_rows,
        "edge_rows": response.edge_rows,
        "connector_rows": response.connector_rows,
        "database_written": response.database_written,
        "progress_events": response.progress_events,
        "phase_timings": response.phase_timings,
    })
}

#[derive(Debug)]
enum StorageExecution {
    Direct {
        db_path: PathBuf,
        manifest_path: PathBuf,
        artifact_root: PathBuf,
        session: DirectWriteSession,
        workspace: RunWorkspace,
        previous_manifest: Option<NativeManifest>,
    },
    Managed {
        artifact_root: PathBuf,
        session: ManagedWriteSession,
        previous_manifest: Option<NativeManifest>,
        store: ManagedStore,
    },
}

impl StorageExecution {
    fn phase_workspace_root(&self) -> PathBuf {
        match self {
            Self::Direct { session, .. } => session.phase_workspace_root(),
            Self::Managed { session, .. } => session.phase_workspace_root(),
        }
    }

    fn storage_format(&self) -> &'static str {
        match self {
            Self::Direct { .. } => "direct",
            Self::Managed { .. } => "managed_v2",
        }
    }

    fn active_generation(&self) -> Option<String> {
        match self {
            Self::Direct { .. } => None,
            Self::Managed { session, .. } => session
                .base_generation
                .as_ref()
                .map(|generation| generation.generation_id.clone()),
        }
    }

    fn finish_without_publish(self) -> Result<(), String> {
        match self {
            Self::Direct {
                session, workspace, ..
            } => {
                let cleanup_error = cleanup_direct_candidate(&session).err();
                let workspace_error = workspace.finish().err().map(|error| error.to_string());
                session.finish();
                match (cleanup_error, workspace_error) {
                    (None, None) => Ok(()),
                    (Some(primary), secondary) => Err(append_cleanup_error(primary, secondary)),
                    (None, Some(error)) => Err(error),
                }
            }
            Self::Managed { session, .. } => session
                .abort(None)
                .map_err(|error| format!("failed to discard no-op materialization: {error}")),
        }
    }

    fn request_db_path(&self) -> PathBuf {
        match self {
            Self::Direct { session, .. } => session.db_candidate_path(),
            Self::Managed { session, .. } => session.candidate_db_path(),
        }
    }

    fn request_manifest_path(&self) -> PathBuf {
        match self {
            Self::Direct { session, .. } => session.manifest_candidate_path(),
            Self::Managed { session, .. } => session.candidate_manifest_path(),
        }
    }

    fn staging_root(&self) -> PathBuf {
        match self {
            Self::Direct { workspace, .. } => workspace.staging_root(),
            Self::Managed { session, .. } => session.staging_root().unwrap_or_else(|| {
                run_root_from_candidate(session.candidate.paths().root()).join("staging")
            }),
        }
    }

    fn previous_manifest(&self) -> &Option<NativeManifest> {
        match self {
            Self::Direct {
                previous_manifest, ..
            }
            | Self::Managed {
                previous_manifest, ..
            } => previous_manifest,
        }
    }

    fn artifact_root(&self) -> &Path {
        match self {
            Self::Direct { artifact_root, .. } | Self::Managed { artifact_root, .. } => {
                artifact_root
            }
        }
    }

    fn abort_with_cleanup(self, primary_error: String) -> String {
        match self {
            Self::Direct {
                session, workspace, ..
            } => {
                let mut cleanup_errors = Vec::new();
                if let Err(error) = cleanup_direct_candidate(&session) {
                    cleanup_errors.push(error);
                }
                if let Err(error) = workspace.abort(Some(primary_error.clone())) {
                    cleanup_errors.push(error.to_string());
                }
                append_cleanup_errors(primary_error, cleanup_errors)
            }
            Self::Managed { session, .. } => append_cleanup_error(
                primary_error.clone(),
                session
                    .abort(Some(primary_error))
                    .err()
                    .map(|error| error.to_string()),
            ),
        }
    }
}

fn refresh_plan_is_current(response: &NativeSyntaxMaterializationResponse) -> bool {
    !response.diff.force_rebuild
        && response.diff.added.is_empty()
        && response.diff.modified.is_empty()
        && response.diff.deleted.is_empty()
}

pub(crate) fn build_request(
    options: &MaterializeOptions,
) -> Result<NativeSyntaxMaterializationRequest, String> {
    let source_root = resolved_source_root(options)?;
    let paths = RepoPaths::derive(&source_root);
    let db_path = options.db.clone().unwrap_or_else(|| paths.db_path.clone());
    let manifest_path = options
        .manifest
        .clone()
        .unwrap_or_else(|| paths.manifest_path.clone());
    let previous_manifest = if options.plan_only && manifest_path.exists() {
        Some(read_manifest(&manifest_path)?)
    } else {
        None
    };
    let config_path = options
        .config
        .clone()
        .unwrap_or_else(|| paths.config_path.clone());
    let config_rules = read_materialization_config_rules(&config_path)?;
    let mut include_patterns = config_rules.include_patterns;
    include_patterns.extend(options.include_patterns.clone());
    let mut exclude_patterns = config_rules.exclude_patterns;
    exclude_patterns.extend(options.exclude_patterns.clone());
    let ignore_patterns = read_codebase_graph_ignore(&source_root)?;
    let candidate_paths = if options.candidate_paths.is_empty() {
        git_candidate_paths(&source_root, options)?
    } else {
        normalized_candidate_paths(&options.candidate_paths)
    };
    let staging_dir = paths.state_dir.join("native-staging");
    let artifact_root = paths.state_dir.join("artifacts");
    let worker_memory_mib = options
        .worker_memory_mib
        .or(config_rules.worker_memory_mib)
        .unwrap_or(crate::api::context::DEFAULT_WORKER_MEMORY_MIB);
    let rust_memory_mib = options
        .rust_memory_mib
        .or(config_rules.rust_memory_mib)
        .unwrap_or(crate::api::context::DEFAULT_RUST_MEMORY_MIB);
    let spill_chunk_mib = options
        .spill_chunk_mib
        .or(config_rules.spill_chunk_mib)
        .unwrap_or(crate::api::context::DEFAULT_SPILL_CHUNK_MIB);
    let max_parallelism = options
        .max_parallelism
        .or(config_rules.max_parallelism)
        .unwrap_or(crate::api::context::DEFAULT_MAX_PARALLELISM);
    validate_materialization_limits(
        worker_memory_mib,
        rust_memory_mib,
        spill_chunk_mib,
        max_parallelism,
    )?;

    Ok(NativeSyntaxMaterializationRequest {
        source_root: source_root.to_string_lossy().to_string(),
        repository_label: paths.repo_name,
        mode: options.mode.clone(),
        parser_version: "native-rust-cli-v1".to_string(),
        manifest_schema_version: MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
        ontology: "code_ontology_v1".to_string(),
        ontology_schema: Default::default(),
        previous_manifest,
        profiles: Vec::new(),
        excluded_parts: default_excluded_parts(),
        include_patterns,
        exclude_patterns,
        ignore_patterns,
        candidate_paths,
        artifact_root: artifact_root.to_string_lossy().into_owned(),
        db_path: db_path.to_string_lossy().to_string(),
        include_fts: options.include_fts,
        schema_statements: Vec::new(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        atomic_rebuild: true,
        strict: true,
        parallel: options.parallel,
        worker_memory_mib,
        rust_memory_mib,
        spill_chunk_mib,
        max_parallelism,
        progress: options.progress,
    })
}

pub(crate) fn read_materialization_config_rules(path: &Path) -> Result<ConfigScanRules, String> {
    if !path.exists() {
        return Ok(ConfigScanRules::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse config {}: {error}", path.display()))?;
    let materialization = value
        .get("materialization")
        .and_then(serde_json::Value::as_object);
    Ok(ConfigScanRules {
        include_patterns: materialization
            .and_then(|payload| payload.get("include"))
            .map(json_string_array)
            .unwrap_or_default(),
        exclude_patterns: materialization
            .and_then(|payload| payload.get("exclude"))
            .map(json_string_array)
            .unwrap_or_default(),
        worker_memory_mib: materialization
            .and_then(|payload| payload.get("worker_memory_mib"))
            .and_then(serde_json::Value::as_u64),
        rust_memory_mib: materialization
            .and_then(|payload| payload.get("rust_memory_mib"))
            .and_then(serde_json::Value::as_u64),
        spill_chunk_mib: materialization
            .and_then(|payload| payload.get("spill_chunk_mib"))
            .and_then(serde_json::Value::as_u64),
        max_parallelism: materialization
            .and_then(|payload| payload.get("max_parallelism"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
    })
}

fn validate_materialization_limits(
    worker_memory_mib: u64,
    rust_memory_mib: u64,
    spill_chunk_mib: u64,
    max_parallelism: usize,
) -> Result<(), String> {
    if worker_memory_mib == 0
        || rust_memory_mib == 0
        || spill_chunk_mib == 0
        || max_parallelism == 0
    {
        return Err("materialization memory limits and max_parallelism must be positive".into());
    }
    if rust_memory_mib > worker_memory_mib {
        return Err("rust_memory_mib must not exceed worker_memory_mib".into());
    }
    if spill_chunk_mib > rust_memory_mib {
        return Err("spill_chunk_mib must not exceed rust_memory_mib".into());
    }
    Ok(())
}

pub(crate) fn json_string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn read_codebase_graph_ignore(source_root: &Path) -> Result<Vec<String>, String> {
    let path = source_root.join(".codebaseGraphignore");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

pub(crate) fn git_candidate_paths(
    source_root: &Path,
    options: &MaterializeOptions,
) -> Result<Vec<String>, String> {
    if !options.use_git {
        return Ok(Vec::new());
    }
    let mut paths = if options.git_diff && options.plan_only {
        let base = options.git_base.as_deref().unwrap_or("HEAD");
        git_paths(
            source_root,
            &["diff", "--name-only", "--diff-filter=ACMRTD", base, "--"],
        )
        .unwrap_or_default()
    } else {
        git_paths(
            source_root,
            &["ls-files", "--cached", "--others", "--exclude-standard"],
        )
        .unwrap_or_default()
    };
    if options.git_diff && options.plan_only {
        if let Ok(untracked) =
            git_paths(source_root, &["ls-files", "--others", "--exclude-standard"])
        {
            paths.extend(untracked);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub(crate) fn git_paths(source_root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(source_root)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.replace('\\', "/"))
        .collect())
}

pub(crate) fn normalized_candidate_paths(paths: &[String]) -> Vec<String> {
    let mut paths = paths
        .iter()
        .map(|path| path.trim().trim_start_matches("./").replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub(crate) fn default_excluded_parts() -> Vec<String> {
    [
        ".bzr",
        ".cache",
        ".codebaseGraph",
        ".direnv",
        ".eggs",
        ".git",
        ".hg",
        ".mypy_cache",
        ".nox",
        ".svn",
        ".tox",
        ".venv",
        "dist",
        "node_modules",
        "target",
        "venv",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
pub(crate) fn request_manifest_path(options: &MaterializeOptions) -> Option<PathBuf> {
    if options.native_request.is_some() {
        return options.manifest.clone();
    }
    let source_root = resolved_source_root(options).unwrap_or_else(|_| PathBuf::from("."));
    Some(
        options
            .manifest
            .clone()
            .unwrap_or_else(|| RepoPaths::derive(&source_root).manifest_path),
    )
}

pub(crate) fn read_manifest(path: &Path) -> Result<NativeManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read manifest {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse manifest {}: {error}", path.display()))
}

pub(crate) fn read_request(path: &Path) -> Result<NativeSyntaxMaterializationRequest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read native request {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse native request {}: {error}", path.display()))
}

pub(crate) fn write_manifest(
    path: &Path,
    request: &NativeSyntaxMaterializationRequest,
    materialized_entries: &BTreeMap<String, ManifestEntry>,
    graph_build_digest: Option<String>,
    search_backend: Option<crate::protocol::SearchBackendMetadata>,
) -> Result<NativeManifest, String> {
    let _reference_mutation =
        crate::storage::maintenance_ownership::ReferenceMutation::begin_producing(
            &request.resolved_artifact_root(),
        )
        .map_err(|error| error.to_string())?;
    let files = materialized_entries
        .iter()
        .map(|(path, entry)| (path.clone(), entry.clone().compact()))
        .collect();
    let manifest = NativeManifest {
        schema_version: request.manifest_schema_version,
        ontology: request.ontology.clone(),
        parser_version: request.parser_version.clone(),
        graph_build_digest,
        search_backend,
        files,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create manifest directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("failed to write manifest {}: {error}", path.display()))?;
    Ok(manifest)
}

fn prepare_storage_execution(options: &MaterializeOptions) -> Result<StorageExecution, String> {
    let selector = RepoSelector {
        repo_root: options.source_root.clone(),
        config_path: options.config.clone(),
        db_path: options.db.clone(),
        manifest_path: options.manifest.clone(),
    };
    let mut runtime = crate::api::context::resolve_runtime(&selector)
        .map_err(|error| format!("failed to resolve materialization storage target: {error}"))?;
    runtime.active_read = None;
    runtime.direct_read = None;
    let managed_storage_root = options.storage_root.clone().or_else(|| {
        matches!(runtime.storage_mode, StorageMode::ManagedV2)
            .then(|| runtime.storage_root.clone())
            .flatten()
    });
    match managed_storage_root {
        Some(storage_root) => {
            let store = GraphStorage::managed(storage_root.clone());
            let session = store
                .begin_write()
                .map_err(|error| format!("failed to create managed write session: {error}"))?;
            let previous_manifest = match session
                .base_manifest_path()
                .map_err(|error| error.to_string())?
            {
                Some(path) if path.exists() => Some(read_manifest(&path)?),
                _ => None,
            };
            Ok(StorageExecution::Managed {
                artifact_root: store.layout().artifacts_root(),
                session,
                previous_manifest,
                store,
            })
        }
        None => {
            let layout = DirectLayout::new(runtime.db_path.clone(), runtime.manifest_path.clone());
            let artifact_root = layout.artifact_root_path();
            let store = DirectStore::new(layout.clone())
                .map_err(|error| format!("failed to prepare direct storage target: {error}"))?;
            let session = store
                .begin_write()
                .map_err(|error| format!("failed to create direct write session: {error}"))?;
            let previous_manifest = if runtime.manifest_path.exists() {
                Some(read_manifest(&runtime.manifest_path)?)
            } else {
                None
            };
            let reference_domain =
                crate::storage::maintenance_ownership::ReferenceDomain::direct(&layout)
                    .map_err(|error| error.to_string())?;
            let mut workspace = RunWorkspace::create_scoped(
                RepositoryLayout::new(&runtime.state_dir)
                    .state_root()
                    .join("direct-runs"),
                &reference_domain,
                None,
            )
            .map_err(|error| format!("failed to create direct run workspace: {error}"))?;
            workspace
                .set_cleanup_barrier(session.phase_workspace_root())
                .map_err(|error| format!("failed to protect direct run workspace: {error}"))?;
            workspace
                .mark_phase(RunPhase::Staged, None, None)
                .map_err(|error| format!("failed to stage direct run workspace: {error}"))?;
            Ok(StorageExecution::Direct {
                db_path: runtime.db_path,
                manifest_path: runtime.manifest_path,
                artifact_root,
                session,
                workspace,
                previous_manifest,
            })
        }
    }
}

fn finalize_materialization(
    execution: StorageExecution,
    request: &NativeSyntaxMaterializationRequest,
    response: &mut NativeSyntaxMaterializationResponse,
    identity: Option<&RepositoryIdentity>,
) -> Result<(), String> {
    // Finalization occurs after the public pipeline's guard is released.
    // The execution/session and scoped run remain active through all effects.
    let _reference_mutation =
        crate::storage::maintenance_ownership::ReferenceMutation::begin_existing(
            execution.artifact_root(),
        )
        .map_err(|error| error.to_string())?;
    let request_db_path = execution.request_db_path();
    let request_manifest_path = execution.request_manifest_path();
    let artifact_root = execution.artifact_root().to_path_buf();
    let manifest = match write_manifest(
        &request_manifest_path,
        request,
        &response.materialized_entries,
        response.graph_build_digest.clone(),
        response.search_backend.clone(),
    ) {
        Ok(manifest) => manifest,
        Err(error) => return Err(execution.abort_with_cleanup(error)),
    };
    if let Err(error) = validate_candidate_bundle(
        &request_db_path,
        &request_manifest_path,
        &artifact_root,
        &manifest,
        response,
    ) {
        return Err(execution.abort_with_cleanup(error));
    }

    if let Some(identity) = identity {
        if let Err(error) = identity.validate() {
            return Err(execution.abort_with_cleanup(format!(
                "materialization repository identity changed before publication: {error}"
            )));
        }
    }

    match execution {
        StorageExecution::Direct {
            db_path,
            manifest_path,
            artifact_root,
            mut session,
            workspace,
            ..
        } => {
            if let Err(error) = workspace.mark_phase(RunPhase::Publishing, None, None) {
                return Err(append_cleanup_error(
                    format!("failed to journal direct publication: {error}"),
                    workspace
                        .abort(Some(error.to_string()))
                        .err()
                        .map(|cleanup| cleanup.to_string()),
                ));
            }
            if let Err(error) = session.publish() {
                return Err(append_cleanup_error(
                    format!("failed to publish direct graph candidate: {error}"),
                    workspace
                        .abort(Some(error.to_string()))
                        .err()
                        .map(|cleanup| cleanup.to_string()),
                ));
            }
            if let Err(error) = workspace.mark_phase(RunPhase::Published, None, None) {
                return Err(append_cleanup_error(
                    format!("failed to journal completed direct publication: {error}"),
                    workspace
                        .abort(Some(error.to_string()))
                        .err()
                        .map(|cleanup| cleanup.to_string()),
                ));
            }
            let published_manifest = match read_manifest(&manifest_path) {
                Ok(manifest) => manifest,
                Err(error) => {
                    return Err(append_cleanup_error(
                        error.clone(),
                        workspace
                            .abort(Some(error))
                            .err()
                            .map(|cleanup| cleanup.to_string()),
                    ));
                }
            };
            if let Err(error) = validate_candidate_bundle(
                &db_path,
                &manifest_path,
                &artifact_root,
                &published_manifest,
                response,
            ) {
                return Err(append_cleanup_error(
                    error.clone(),
                    workspace
                        .abort(Some(error))
                        .err()
                        .map(|cleanup| cleanup.to_string()),
                ));
            }
            defer_artifact_maintenance(response);
            response.storage_format = "direct".to_string();
            response.active_generation = None;
            response.cleanup_pending = false;
            response.pending_runs = 0;
            if !manifest_path.exists() {
                return Err(format!(
                    "direct manifest publication did not produce {}",
                    manifest_path.display()
                ));
            }
            // Publication, its journal transition and required readback have
            // succeeded. Failure to dispose the published run is independent.
            #[cfg(test)]
            POST_DIRECT_CLEANUP_HOOK.with(|hook| {
                if let Some(hook) = hook.take() {
                    hook(workspace.root());
                }
            });
            if let Err(error) = workspace.finish() {
                response.cleanup_pending = true;
                response.pending_runs = 1;
                record_postpublication_cleanup_error(
                    response,
                    "direct_run_cleanup_pending",
                    format!("failed to clean direct run workspace: {error}"),
                );
            }
            session.finish();
        }
        StorageExecution::Managed {
            artifact_root,
            mut session,
            store,
            ..
        } => {
            let publication = session
                .publish_with_stats_typed(&response.graph_summary)
                .map_err(|error| format!("failed to publish managed graph generation: {error}"))?;
            let published_generation = publication.generation_id;
            let mut janitor = publication.janitor;
            let read = match store.open_read() {
                Ok(read) => read,
                Err(error) => {
                    return Err(append_managed_janitor_secondary(
                        format!("failed to reopen managed graph generation: {error}"),
                        &mut janitor,
                    ))
                }
            };
            if read.generation_id != published_generation {
                let error = format!(
                    "managed publish activated {}, but reopened generation was {}",
                    published_generation, read.generation_id
                );
                drop(read);
                return Err(append_managed_janitor_secondary(error, &mut janitor));
            }
            let published_manifest = match read_manifest(&read.manifest_path) {
                Ok(manifest) => manifest,
                Err(error) => {
                    drop(read);
                    return Err(append_managed_janitor_secondary(error, &mut janitor));
                }
            };
            if let Err(error) = validate_candidate_bundle(
                &read.db_path,
                &read.manifest_path,
                &artifact_root,
                &published_manifest,
                response,
            ) {
                drop(read);
                return Err(append_managed_janitor_secondary(error, &mut janitor));
            }
            #[cfg(test)]
            POST_MANAGED_VERIFICATION_HOOK.with(|hook| {
                if let Some(hook) = hook.take() {
                    hook(&session, &store);
                }
            });
            let reconciliation = match session.reconcile_typed() {
                Ok(report) => report,
                Err(error) => {
                    drop(read);
                    return Err(append_managed_janitor_secondary(
                        format!("failed to reconcile managed storage cleanup: {error}"),
                        &mut janitor,
                    ));
                }
            };
            #[cfg(test)]
            POST_MANAGED_RECONCILE_HOOK.with(|hook| {
                if let Some(hook) = hook.take() {
                    hook(&session, &store);
                }
            });
            let recovery_complete = reconciliation.recovery.observation_complete;
            janitor.merge_latest(reconciliation.janitor);
            if let Err(error) = session.check_active_generation(&published_generation) {
                drop(read);
                return Err(append_managed_janitor_secondary(
                    error.to_string(),
                    &mut janitor,
                ));
            }
            if !recovery_complete {
                janitor.deferred_reason = Some("managed_recovery_deferred");
                janitor.pending = true;
                janitor.observation_complete = false;
            }
            // GenerationLease::drop must see this writer still held, otherwise
            // it could start an unreported best-effort cleanup opportunity.
            drop(read);
            defer_artifact_maintenance(response);
            response.storage_format = "managed_v2".to_string();
            response.active_generation = Some(published_generation);
            response.cleanup_pending = janitor.pending
                || janitor.legacy.retired_generations_pending > 0
                || janitor.legacy.run_recovery.skipped_locked > 0;
            response.pending_runs = janitor.legacy.run_recovery.skipped_locked;
            record_managed_janitor_report(response, janitor);
            session.finish_without_cleanup();
        }
    }
    Ok(())
}

fn run_root_from_candidate(candidate_root: &Path) -> PathBuf {
    candidate_root
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| candidate_root.to_path_buf())
}

fn validate_candidate_bundle(
    db_path: &Path,
    manifest_path: &Path,
    artifact_root: &Path,
    manifest: &NativeManifest,
    response: &NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    if manifest.schema_version != MATERIALIZATION_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "published manifest {} has schema_version {}, expected {}",
            manifest_path.display(),
            manifest.schema_version,
            MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
        ));
    }
    if manifest.graph_build_digest != response.graph_build_digest {
        return Err(format!(
            "published manifest {} has graph_build_digest {:?}, expected {:?}",
            manifest_path.display(),
            manifest.graph_build_digest,
            response.graph_build_digest
        ));
    }
    if manifest.search_backend != response.search_backend {
        return Err(format!(
            "published manifest {} does not match the materialized search backend",
            manifest_path.display()
        ));
    }
    let expected_files = response
        .materialized_entries
        .iter()
        .map(|(path, entry)| (path.clone(), entry.clone().compact()))
        .collect::<BTreeMap<_, _>>();
    if manifest.files != expected_files {
        return Err(format!(
            "published manifest {} does not match materialized entries",
            manifest_path.display()
        ));
    }
    if let Some(search_backend) = manifest.search_backend.as_ref() {
        crate::search_index::validate(db_path, search_backend).map_err(|error| {
            format!(
                "published search sidecar for {} is invalid: {error}",
                db_path.display()
            )
        })?;
    }

    let node_count = crate::api::graph_read::count_graph_nodes(db_path)?;
    let edge_count = crate::api::graph_read::count_graph_edges(db_path)?;
    let expected_physical_nodes = (response.node_rows + response.edge_rows) as u64;
    let expected_physical_edges = response.connector_rows as u64;
    if node_count != expected_physical_nodes || edge_count != expected_physical_edges {
        return Err(format!(
            "published graph counts were nodes={node_count}, edges={edge_count}; expected physical nodes={expected_physical_nodes}, edges={expected_physical_edges}"
        ));
    }

    let artifact_store = ArtifactStore::new(artifact_root);
    let artifact_keys = artifact_store
        .list_keys()
        .map_err(|error| {
            format!(
                "failed to list artifacts in {}: {error}",
                artifact_root.display()
            )
        })?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for (path, entry) in &manifest.files {
        let artifact_key = entry
            .artifact_key
            .as_deref()
            .ok_or_else(|| format!("manifest entry {path} is missing an artifact_key"))?;
        if artifact_key.is_empty() {
            return Err(format!("manifest entry {path} has an empty artifact_key"));
        }
        if !artifact_keys.contains(artifact_key) {
            return Err(format!(
                "manifest entry {path} references missing artifact {artifact_key}"
            ));
        }
    }
    Ok(())
}

/// The former manifest-only deletion loop had no destination/use ownership
/// proof and could scan without a budget. Leave the owned collector's work
/// pending rather than exercising authorityless ArtifactStore::delete_key.
fn defer_artifact_maintenance(response: &mut NativeSyntaxMaterializationResponse) {
    let summary = response
        .maintenance
        .get_or_insert_with(MaintenanceSummary::default);
    summary.pending = true;
    let reason = "artifact_collection_requires_owned_maintenance";
    if !summary.reasons.iter().any(|value| value == reason) {
        summary.reasons.push(reason.into());
    }
}

fn record_postpublication_cleanup_error(
    response: &mut NativeSyntaxMaterializationResponse,
    reason: &str,
    error: String,
) {
    let summary = response
        .maintenance
        .get_or_insert_with(MaintenanceSummary::default);
    summary.pending = true;
    summary.reasons.push(reason.into());
    summary.errors.push(error.clone());
    response
        .diagnostics
        .push(format!("cleanup pending: {error}"));
}

fn record_managed_janitor_report(
    response: &mut NativeSyntaxMaterializationResponse,
    mut report: ManagedJanitorReport,
) {
    let summary = response
        .maintenance
        .get_or_insert_with(MaintenanceSummary::default);
    if !report.observation_complete {
        summary.observation_complete = false;
        summary
            .reasons
            .push("managed_janitor_observation_incomplete".into());
    }
    if let Some(reason) = report.deferred_reason {
        if !report.observation_complete {
            summary.reasons.push(reason.into());
        }
    }
    summary.pending |= report.pending;
    if let Some(issue) = report.first_issue.take() {
        let reason = if report.pending {
            issue.stage.reason()
        } else {
            "managed_janitor_issue_resolved"
        };
        summary.reasons.push(reason.into());
        let error = issue.source.to_string();
        summary.errors.push(error.clone());
        response.diagnostics.push(format!(
            "{}: {error}",
            if report.pending {
                "cleanup pending"
            } else {
                "cleanup diagnostic"
            }
        ));
    }
    if report.issue_count > 1 {
        summary
            .reasons
            .push("additional_managed_janitor_errors_omitted".into());
        response.diagnostics.push(format!(
            "{} additional managed cleanup errors",
            report.issue_count - 1
        ));
    }
}

#[cfg(test)]
thread_local! {
    // Inject a real workspace cleanup failure only after commit/readback.
    static POST_DIRECT_CLEANUP_HOOK: std::cell::Cell<Option<fn(&Path)>> = const { std::cell::Cell::new(None) };
    static POST_MANAGED_VERIFICATION_HOOK: std::cell::Cell<Option<fn(&ManagedWriteSession, &ManagedStore)>> = const { std::cell::Cell::new(None) };
    static POST_MANAGED_RECONCILE_HOOK: std::cell::Cell<Option<fn(&ManagedWriteSession, &ManagedStore)>> = const { std::cell::Cell::new(None) };
}

fn cleanup_direct_candidate(session: &DirectWriteSession) -> Result<(), String> {
    crate::storage::containment::ensure_registry_empty(&session.phase_workspace_root())?;
    let mut cleanup_errors = Vec::new();
    for path in [
        session.db_candidate_path(),
        session.manifest_candidate_path(),
    ] {
        if let Err(error) = remove_path_if_exists(&path) {
            cleanup_errors.push(error);
        }
    }
    for suffix in DIRECT_DB_SIDECAR_SUFFIXES {
        let sidecar = PathBuf::from(format!(
            "{}.{}",
            session.db_candidate_path().display(),
            suffix
        ));
        if let Err(error) = remove_path_if_exists(&sidecar) {
            cleanup_errors.push(error);
        }
    }
    if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(cleanup_errors.join("; "))
    }
}

fn remove_path_if_exists(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to remove symlinked path {}",
            path.display()
        ));
    }
    if metadata.is_dir() {
        Err(format!(
            "refusing to remove unexpected directory {}",
            path.display()
        ))
    } else {
        fs::remove_file(path)
            .map_err(|error| format!("failed to remove file {}: {error}", path.display()))
    }
}

fn append_managed_janitor_secondary(
    primary_error: String,
    report: &mut ManagedJanitorReport,
) -> String {
    append_cleanup_error(
        primary_error,
        report
            .first_issue
            .take()
            .map(|issue| issue.source.to_string()),
    )
}

fn append_cleanup_error(primary_error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) if !cleanup_error.is_empty() => {
            format!("{primary_error}; cleanup failed: {cleanup_error}")
        }
        _ => primary_error,
    }
}

fn append_cleanup_errors(primary_error: String, cleanup_errors: Vec<String>) -> String {
    if cleanup_errors.is_empty() {
        primary_error
    } else {
        format!(
            "{primary_error}; cleanup failed: {}",
            cleanup_errors.join("; ")
        )
    }
}

fn resolved_source_root(options: &MaterializeOptions) -> Result<PathBuf, String> {
    resolve_repository_root(options.source_root.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
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
    fn dc_epoch_manifest_finalization_advances_tracked_reference_domain() {
        use crate::storage::layout::{create_fresh_reference_root, ManagedLayout};
        use crate::storage::locks::{open_locked, LockMode};
        use crate::storage::maintenance_ownership::{
            initialize_fresh_reference_domain, observe_reference_epoch, EpochObservation,
            ReferenceDomain,
        };
        let root = unique_temp_dir("epoch-finalization-manifest");
        fs::create_dir_all(&root).unwrap();
        let storage = root.join("storage");
        let fresh = create_fresh_reference_root(&storage).unwrap().unwrap();
        let layout = ManagedLayout::new(&storage);
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        drop(writer);
        let before = match observe_reference_epoch(&domain).unwrap() {
            EpochObservation::Verified(epoch) => epoch,
            other => panic!("{other:?}"),
        };
        let path = layout
            .generations_root()
            .join("gen-finalization/manifest.json");
        let mut request = build_request(&MaterializeOptions {
            source_root: Some(root.clone()),
            db: Some(storage.join("candidate.ldb")),
            manifest: Some(path.clone()),
            ..MaterializeOptions::default()
        })
        .unwrap();
        request.artifact_root = layout.artifacts_root().to_string_lossy().into_owned();
        let manifest = write_manifest(&path, &request, &BTreeMap::new(), None, None).unwrap();
        assert!(manifest.files.is_empty());
        assert!(path.is_file());
        let after = match observe_reference_epoch(&domain).unwrap() {
            EpochObservation::Verified(epoch) => epoch,
            other => panic!("{other:?}"),
        };
        assert!(after.counter() > before.counter());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_request_source_root_must_match_a_bound_repository_identity() {
        let selected_root = unique_temp_dir("codebase-graph-materialization-identity-source");
        let request_root = unique_temp_dir("codebase-graph-materialization-identity-request");
        fs::create_dir_all(&selected_root).unwrap();
        fs::create_dir_all(&request_root).unwrap();
        let options = MaterializeOptions {
            source_root: Some(selected_root.clone()),
            mode: "full".to_string(),
            ..MaterializeOptions::default()
        };
        let mut request = build_request(&options).unwrap();
        request.source_root = request_root.to_string_lossy().into_owned();
        let error = execute_materialization_request(&options, request).unwrap_err();
        assert!(error.contains("source root"));
        assert!(error.contains("does not match"));
        assert!(!selected_root.join(".codebaseGraph").exists());

        let _ = fs::remove_dir_all(selected_root);
        let _ = fs::remove_dir_all(request_root);
    }

    #[test]
    fn refresh_noop_aborts_prepared_writer_when_storage_identity_rebinds() {
        let root = unique_temp_dir("codebase-graph-materialization-noop-rebind");
        let storage_one = root.join("storage-one");
        let storage_two = root.join("storage-two");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&storage_one).unwrap();
        fs::create_dir_all(&storage_two).unwrap();
        fs::create_dir_all(&state).unwrap();
        let config_path = state.join("config.json");
        let write_config = |storage: &Path, configured_root: &Path| {
            fs::write(
                &config_path,
                serde_json::to_vec(&json!({
                    "schema_version": 3,
                    "repo_root": configured_root,
                    "storage_root": storage,
                }))
                .unwrap(),
            )
            .unwrap();
        };
        write_config(&storage_one, &root);
        let options = MaterializeOptions {
            source_root: Some(root.clone()),
            config: Some(config_path.clone()),
            storage_root: Some(storage_one.clone()),
            intent: MaterializationIntent::Refresh,
            ..MaterializeOptions::default()
        };
        let identity = capture_materialization_identity(&options)
            .unwrap()
            .expect("bound refresh should capture identity");
        let execution = prepare_storage_execution(&options).unwrap();
        let active_pointer = GraphStorage::managed(storage_one.clone())
            .layout()
            .active_pointer_path();
        assert!(!active_pointer.exists());

        write_config(&storage_two, &root);
        let mut response = NativeSyntaxMaterializationResponse::skipped(
            BTreeMap::new(),
            crate::protocol::ManifestDiff {
                added: Vec::new(),
                modified: Vec::new(),
                unchanged: Vec::new(),
                deleted: Vec::new(),
                force_rebuild: false,
            },
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        let error =
            acknowledge_refresh_noop(execution, Some(&identity), &mut response).unwrap_err();
        assert!(error.contains("storage root changed"));
        assert!(!active_pointer.exists());
        assert!(fs::read_dir(
            GraphStorage::managed(storage_one.clone())
                .layout()
                .runs_root()
        )
        .unwrap()
        .next()
        .is_none());

        let other_root = unique_temp_dir("codebase-graph-materialization-noop-root-rebind");
        fs::create_dir_all(&other_root).unwrap();
        write_config(&storage_one, &root);
        let execution = prepare_storage_execution(&options).unwrap();
        write_config(&storage_one, &other_root);
        let mut response = NativeSyntaxMaterializationResponse::skipped(
            BTreeMap::new(),
            crate::protocol::ManifestDiff {
                added: Vec::new(),
                modified: Vec::new(),
                unchanged: Vec::new(),
                deleted: Vec::new(),
                force_rebuild: false,
            },
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        let error =
            acknowledge_refresh_noop(execution, Some(&identity), &mut response).unwrap_err();
        assert!(
            error.contains("repository root")
                || error.contains("conflicts with install config root")
        );
        assert!(!active_pointer.exists());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(other_root);
    }

    #[test]
    fn deferred_artifact_collection_preserves_unregistered_foreign_state() {
        let root = unique_temp_dir("unregistered-artifacts");
        let retained_key = "a".repeat(64);
        let stale_key = "b".repeat(64);
        let retained_dir = root.join("aa").join(&retained_key);
        let stale_dir = root.join("bb").join(&stale_key);
        fs::create_dir_all(&retained_dir).unwrap();
        fs::create_dir_all(&stale_dir).unwrap();
        fs::write(retained_dir.join("partition.json"), "retained").unwrap();
        fs::write(stale_dir.join("partition.json"), "stale").unwrap();
        let mut response = empty_test_response();
        defer_artifact_maintenance(&mut response);
        let summary = response.maintenance.unwrap();
        assert!(summary.pending);
        assert!(summary.inventory_status.is_none());
        assert!(summary.bytes.unknown.allocated_bytes.is_none());
        assert!(summary
            .reasons
            .iter()
            .any(|reason| reason == "artifact_collection_requires_owned_maintenance"));
        let authority_error = ArtifactStore::new(&root)
            .delete_key(&stale_key)
            .unwrap_err();
        assert!(authority_error
            .to_string()
            .contains("requires_current_maintenance_authority"));
        assert!(!summary.requested);
        assert!(!summary.running);
        assert_eq!(
            fs::read(retained_dir.join("partition.json")).unwrap(),
            b"retained"
        );
        assert_eq!(
            fs::read(stale_dir.join("partition.json")).unwrap(),
            b"stale"
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn empty_test_response() -> NativeSyntaxMaterializationResponse {
        NativeSyntaxMaterializationResponse::skipped(
            BTreeMap::new(),
            crate::protocol::ManifestDiff {
                added: Vec::new(),
                modified: Vec::new(),
                unchanged: Vec::new(),
                deleted: Vec::new(),
                force_rebuild: false,
            },
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        )
    }

    fn committed_artifact_fixture(managed: bool) -> (PathBuf, MaterializeOptions, PathBuf) {
        let root = unique_temp_dir(if managed { "postgc-m" } else { "postgc-d" });
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn first_version() {}\n").unwrap();
        let options = MaterializeOptions {
            source_root: Some(root.clone()),
            mode: "full".into(),
            parallel: false,
            storage_root: managed.then(|| root.join(".codebaseGraph/storage")),
            db: (!managed).then(|| root.join("output/graph.ldb")),
            manifest: (!managed).then(|| root.join("output/manifest.json")),
            ..MaterializeOptions::default()
        };
        let (request, _) = execute_materialization(&options).unwrap();
        let artifact_root = PathBuf::from(request.artifact_root);
        (root, options, artifact_root)
    }

    fn assert_published_graph_readable(
        options: &MaterializeOptions,
        response: &NativeSyntaxMaterializationResponse,
    ) {
        let database = if let Some(storage_root) = &options.storage_root {
            GraphStorage::managed(storage_root)
                .layout()
                .generation(response.active_generation.as_ref().unwrap())
                .unwrap()
                .db_path()
        } else {
            options.db.clone().unwrap()
        };
        assert!(crate::api::graph_read::count_graph_nodes(&database).unwrap() > 0);
    }

    fn verify_committed_artifact_deferral(managed: bool) {
        let (root, options, artifact_root) = committed_artifact_fixture(managed);
        // This residue was introduced outside the producing protocol and must
        // survive. It reproduced the old authorityless postcommit GC failure.
        let stale_key = "b".repeat(64);
        let foreign_payload = artifact_root
            .join("bb")
            .join(stale_key)
            .join("partition.json");
        fs::create_dir_all(foreign_payload.parent().unwrap()).unwrap();
        fs::write(&foreign_payload, "foreign unvalidated payload").unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn second_version() {}\n").unwrap();
        let (_, response) = execute_materialization(&options)
            .expect("committed publication must survive deferred artifact GC");
        assert!(response.database_written);
        assert_eq!(
            response.storage_format,
            if managed { "managed_v2" } else { "direct" }
        );
        let summary = response.maintenance.as_ref().unwrap();
        assert!(summary.pending);
        assert!(summary
            .reasons
            .iter()
            .any(|reason| reason == "artifact_collection_requires_owned_maintenance"));
        assert!(summary.last_attempt.is_none());
        assert!(summary.bytes.total.allocated_bytes.is_none());
        assert_eq!(
            fs::read_to_string(&foreign_payload).unwrap(),
            "foreign unvalidated payload"
        );
        assert_published_graph_readable(&options, &response);
        // The new optional field must decode old worker/native responses.
        let mut old_payload = serde_json::to_value(&response).unwrap();
        old_payload.as_object_mut().unwrap().remove("maintenance");
        assert!(
            serde_json::from_value::<NativeSyntaxMaterializationResponse>(old_payload)
                .unwrap()
                .maintenance
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn committed_direct_graph_survives_deferred_artifact_collection() {
        verify_committed_artifact_deferral(false);
    }

    #[test]
    fn committed_managed_graph_survives_deferred_artifact_collection() {
        verify_committed_artifact_deferral(true);
    }

    #[test]
    fn committed_direct_graph_survives_real_workspace_cleanup_failure() {
        let (root, options, _) = committed_artifact_fixture(false);
        POST_DIRECT_CLEANUP_HOOK.with(|hook| {
            hook.set(Some(|run_root| {
                fs::write(
                    run_root.join("cleanup-barrier.json"),
                    "invalid barrier fixture",
                )
                .unwrap();
            }))
        });
        fs::write(
            root.join("src/lib.rs"),
            "pub fn after_cleanup_failure() {}\n",
        )
        .unwrap();
        let (_, response) = execute_materialization(&options)
            .expect("cleanup failure must not undo committed result");
        assert!(response.database_written);
        assert!(response.cleanup_pending);
        assert_eq!(response.pending_runs, 1);
        let summary = response.maintenance.as_ref().unwrap();
        assert!(summary.pending);
        assert!(summary
            .errors
            .iter()
            .any(|error| error.contains("invalid run cleanup barrier")));
        assert!(response
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.starts_with("cleanup pending:")));
        assert_published_graph_readable(&options, &response);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_abort_retains_candidate_staging_and_phase_evidence_while_lease_is_held() {
        let root = unique_temp_dir("codebase-graph-protected-direct-abort");
        let layout = DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
        let store = DirectStore::new(layout.clone()).unwrap();
        let session = store.begin_write().unwrap();
        let db_candidate = session.db_candidate_path();
        fs::write(&db_candidate, "partial-db").unwrap();
        let phase_root = session.phase_workspace_root();
        let phase_workspace = phase_root.join("phase-test");
        let lifecycle = phase_workspace.join(".lifecycle-test");
        fs::create_dir_all(&lifecycle).unwrap();
        let request_path = phase_workspace.join("request.json");
        fs::write(&request_path, "retained request").unwrap();
        let lease = fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(lifecycle.join("lease"))
            .unwrap();
        fs2::FileExt::lock_exclusive(&lease).unwrap();
        let mut workspace = RunWorkspace::create(root.join("direct-runs"), None).unwrap();
        workspace.set_cleanup_barrier(phase_root).unwrap();
        let staging_file = workspace.staging_root().join("partial.json");
        fs::write(&staging_file, "[]").unwrap();
        let error = StorageExecution::Direct {
            db_path: layout.db_path().to_path_buf(),
            manifest_path: layout.manifest_path().to_path_buf(),
            artifact_root: layout.artifact_root_path(),
            session,
            workspace,
            previous_manifest: None,
        }
        .abort_with_cleanup("guardian unavailable".into());
        assert!(error.contains("containment_unknown"), "{error}");
        assert!(db_candidate.exists());
        assert!(staging_file.exists());
        assert!(request_path.exists());
        assert!(lifecycle.join("lease").exists());
        drop(lease);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepublication_failures_clean_managed_and_direct_run_state() {
        for phase in ["parsing", "staging", "database", "candidate_validation"] {
            let root = unique_temp_dir(&format!("codebase-graph-cleanup-{phase}"));
            let managed_root = root.join("managed");
            let managed_store = GraphStorage::managed(&managed_root);
            let managed_session = managed_store.begin_write().unwrap();
            fs::write(managed_session.candidate_db_path(), "partial-db").unwrap();
            let managed_error = StorageExecution::Managed {
                artifact_root: managed_store.layout().artifacts_root(),
                session: managed_session,
                previous_manifest: None,
                store: managed_store.clone(),
            }
            .abort_with_cleanup(format!("{phase} failed"));
            assert_eq!(managed_error, format!("{phase} failed"));
            assert!(fs::read_dir(managed_store.layout().runs_root())
                .unwrap()
                .next()
                .is_none());

            let direct_layout =
                DirectLayout::new(root.join("graph.ldb"), root.join("manifest.json"));
            let direct_store = DirectStore::new(direct_layout.clone()).unwrap();
            let direct_session = direct_store.begin_write().unwrap();
            fs::write(direct_session.db_candidate_path(), "partial-db").unwrap();
            let direct_workspace = RunWorkspace::create(root.join("direct-runs"), None).unwrap();
            fs::write(direct_workspace.staging_root().join("partial.json"), "[]").unwrap();
            let direct_error = StorageExecution::Direct {
                db_path: direct_layout.db_path().to_path_buf(),
                manifest_path: direct_layout.manifest_path().to_path_buf(),
                artifact_root: direct_layout.artifact_root_path(),
                session: direct_session,
                workspace: direct_workspace,
                previous_manifest: None,
            }
            .abort_with_cleanup(format!("{phase} failed"));
            assert_eq!(direct_error, format!("{phase} failed"));
            assert!(!direct_layout.db_candidate_path().exists());
            assert!(!direct_layout.manifest_candidate_path().exists());
            assert!(fs::read_dir(root.join("direct-runs"))
                .unwrap()
                .next()
                .is_none());
            let _ = fs::remove_dir_all(root);
        }
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_errors_are_appended_without_hiding_the_primary_failure() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("codebase-graph-cleanup-error-reporting");
        let store = GraphStorage::managed(root.join("storage"));
        let session = store.begin_write().unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, "keep").unwrap();
        symlink(
            &outside,
            run_root_from_candidate(session.candidate.paths().root()).join("blocked-link"),
        )
        .unwrap();

        let error = StorageExecution::Managed {
            artifact_root: store.layout().artifacts_root(),
            session,
            previous_manifest: None,
            store,
        }
        .abort_with_cleanup("staging failed".to_string());

        assert!(error.contains("staging failed"));
        assert!(error.contains("cleanup failed"));
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
    }

    #[test]
    fn public_materialization_request_preserves_transport_fields_without_preparation() {
        let root = unique_temp_dir("codebase-graph-api-materialization-request");
        fs::create_dir_all(&root).expect("temp root should exist");
        let request_path = root.join("request.json");

        let request = materialization_request(
            &MaterializeOptions {
                native_request: Some(request_path.clone()),
                source_root: Some(root.clone()),
                db: Some(root.join("graph.ldb")),
                manifest: Some(root.join("manifest.json")),
                mode: "changed".to_string(),
                include_fts: true,
                use_git: true,
                git_diff: true,
                git_base: Some("origin/main".to_string()),
                include_patterns: vec!["src/**/*.rs".to_string()],
                exclude_patterns: vec!["target/**".to_string()],
                candidate_paths: vec!["src/lib.rs".to_string()],
                parallel: false,
                progress: true,
                ..MaterializeOptions::default()
            },
            OutputFormat::Typed,
        )
        .expect("request should be built");

        assert_eq!(request.native_request_path, Some(request_path));
        assert_eq!(request.repo.repo_root, Some(root.clone()));
        assert_eq!(request.repo.db_path, Some(root.join("graph.ldb")));
        assert_eq!(request.include_patterns, vec!["src/**/*.rs"]);
        assert_eq!(request.exclude_patterns, vec!["target/**"]);
        assert_eq!(request.candidate_paths, vec!["src/lib.rs"]);
        assert_eq!(request.git_base.as_deref(), Some("origin/main"));
        assert_eq!(request.output_format, OutputFormat::Typed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_request_reads_manifest_and_merges_config_rules() {
        let root = unique_temp_dir("codebase-graph-api-build-request");
        let state_dir = root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).expect("state directory should exist");
        fs::write(
            state_dir.join("config.json"),
            serde_json::to_vec(&json!({
                "materialization": {
                    "include": ["src/**/*.rs"],
                    "exclude": ["target/**"],
                    "worker_memory_mib": 640,
                    "rust_memory_mib": 320,
                    "spill_chunk_mib": 16,
                    "max_parallelism": 1
                }
            }))
            .expect("config should serialize"),
        )
        .expect("config should be written");
        fs::write(
            state_dir.join("manifest.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "ontology": "code_ontology_v1",
                "parser_version": "native-test",
                "files": {}
            }))
            .expect("manifest should serialize"),
        )
        .expect("manifest should be written");
        fs::write(root.join(".codebaseGraphignore"), "build/\n# note\n").expect("ignore file");

        let request = build_request(&MaterializeOptions {
            source_root: Some(root.clone()),
            plan_only: true,
            mode: "full".to_string(),
            include_fts: true,
            use_git: false,
            include_patterns: vec!["tests/**/*.rs".to_string()],
            exclude_patterns: vec!["dist/**".to_string()],
            parallel: true,
            ..MaterializeOptions::default()
        })
        .expect("request should build");

        assert_eq!(
            request.repository_label,
            root.file_name()
                .and_then(|value| value.to_str())
                .expect("temp root should have a file name")
        );
        assert!(request.previous_manifest.is_some());
        assert_eq!(
            request.include_patterns,
            vec!["src/**/*.rs".to_string(), "tests/**/*.rs".to_string()]
        );
        assert_eq!(
            request.exclude_patterns,
            vec!["target/**".to_string(), "dist/**".to_string()]
        );
        assert_eq!(request.ignore_patterns, vec!["build/".to_string()]);
        assert!(request.candidate_paths.is_empty());
        assert_eq!(request.worker_memory_mib, 640);
        assert_eq!(request.rust_memory_mib, 320);
        assert_eq!(request.spill_chunk_mib, 16);
        assert_eq!(request.max_parallelism, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialization_option_limits_override_install_config() {
        let root = unique_temp_dir("codebase-graph-api-build-limit-override");
        let state_dir = root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(
            state_dir.join("config.json"),
            serde_json::to_vec(&json!({
                "materialization": {
                    "worker_memory_mib": 640,
                    "rust_memory_mib": 320,
                    "spill_chunk_mib": 16,
                    "max_parallelism": 1
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let request = build_request(&MaterializeOptions {
            source_root: Some(root.clone()),
            worker_memory_mib: Some(768),
            rust_memory_mib: Some(384),
            spill_chunk_mib: Some(32),
            max_parallelism: Some(2),
            ..MaterializeOptions::default()
        })
        .unwrap();

        assert_eq!(request.worker_memory_mib, 768);
        assert_eq!(request.rust_memory_mib, 384);
        assert_eq!(request.spill_chunk_mib, 32);
        assert_eq!(request.max_parallelism, 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn request_manifest_path_uses_override_for_native_requests_only_when_present() {
        let root = unique_temp_dir("codebase-graph-api-manifest-path");
        fs::create_dir_all(&root).expect("temp root should exist");
        let override_path = root.join("override-manifest.json");

        let native = request_manifest_path(&MaterializeOptions {
            native_request: Some(root.join("request.json")),
            manifest: Some(override_path.clone()),
            ..MaterializeOptions::default()
        });
        assert_eq!(native, Some(override_path.clone()));

        let direct = request_manifest_path(&MaterializeOptions {
            source_root: Some(root.clone()),
            ..MaterializeOptions::default()
        });
        assert_eq!(
            direct,
            Some(
                root.canonicalize()
                    .expect("temp root should canonicalize")
                    .join(".codebaseGraph")
                    .join("manifest.json")
            )
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_committed_native_result_survives_real_run_disposal_failure() {
        let (root, options, _) = committed_artifact_fixture(true);
        crate::storage::managed::set_managed_cleanup_hook(|run| {
            fs::write(run.join("cleanup-barrier.json"), b"invalid cleanup barrier").unwrap();
        });
        fs::write(
            root.join("src/lib.rs"),
            "pub fn managed_cleanup_pending_symbol() {}\n",
        )
        .unwrap();
        let (_, response) = execute_materialization(&options)
            .expect("verified committed graph must survive optional run disposal failure");
        assert!(response.database_written);
        assert!(response.cleanup_pending);
        assert_eq!(
            response.pending_runs, 1,
            "same run counted more than once across publication/reconcile"
        );
        let summary = response.maintenance.as_ref().unwrap();
        assert!(summary.pending);
        assert!(!summary.requested);
        assert!(!summary.observation_complete);
        assert!(
            summary
                .errors
                .iter()
                .any(|e| e.contains("invalid run cleanup barrier")),
            "{summary:?}"
        );
        assert!(response
            .diagnostics
            .iter()
            .any(|d| d.starts_with("cleanup pending:")));
        assert_published_graph_readable(&options, &response);
        let store = GraphStorage::managed(options.storage_root.as_ref().unwrap());
        for entry in fs::read_dir(store.layout().runs_root()).unwrap() {
            let entry = entry.unwrap();
            if entry.path().join("cleanup-barrier.json").exists() {
                fs::remove_file(entry.path().join("cleanup-barrier.json")).unwrap();
            }
        }
        store.cleanup().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_native_pointer_error_is_primary_and_retains_publishing_evidence() {
        use crate::storage::managed::{inject_managed_fault, ManagedFault};
        let (root, options, _) = committed_artifact_fixture(true);
        fs::write(
            root.join("src/lib.rs"),
            "pub fn uncertain_managed_publication() {}\n",
        )
        .unwrap();
        let fault = inject_managed_fault(ManagedFault::PointerAfterRename);
        let error = execute_materialization(&options).unwrap_err();
        assert!(
            error.starts_with("failed to publish managed graph generation:"),
            "{error}"
        );
        assert!(error.contains("after rename"));
        drop(fault);
        let store = GraphStorage::managed(options.storage_root.as_ref().unwrap());
        let active = store.read_active_generation().unwrap().unwrap();
        let generation = store.layout().generation(&active.generation_id).unwrap();
        assert!(!generation.retired_path().exists());
        let runs = fs::read_dir(store.layout().runs_root())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(runs.len(), 1);
        let journal = fs::read(runs[0].join("journal.json")).unwrap();
        assert_eq!(
            serde_json::from_slice::<crate::storage::run_workspace::RunJournal>(&journal)
                .unwrap()
                .phase,
            RunPhase::Publishing
        );
        assert!(crate::api::graph_read::count_graph_nodes(&generation.db_path()).unwrap() > 0);
        assert_eq!(fs::read(runs[0].join("journal.json")).unwrap(), journal);
        store.cleanup().unwrap();
        assert!(!runs[0].exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_native_postreconcile_active_change_remains_primary() {
        let (root, options, _) = committed_artifact_fixture(true);
        POST_MANAGED_VERIFICATION_HOOK.with(|hook| {
            hook.set(Some(|_, _| {
                crate::storage::managed::reset_best_effort_entries()
            }))
        });
        POST_MANAGED_RECONCILE_HOOK.with(|hook| {
            hook.set(Some(|_, store| {
                let _state = crate::storage::locks::open_locked(
                    store.layout().state_lock_path(),
                    crate::storage::locks::LockMode::Exclusive,
                )
                .unwrap();
                crate::storage::atomic::write_json_atomically(
                    &store.layout().active_pointer_path(),
                    &crate::storage::managed::ActiveGeneration {
                        schema_version: 2,
                        generation_id: "unexpected-active".into(),
                        published_at: "fixture".into(),
                        activated_at_ms: 0,
                    },
                )
                .unwrap();
            }))
        });
        fs::write(
            root.join("src/lib.rs"),
            "pub fn changed_before_report() {}\n",
        )
        .unwrap();
        let error = execute_materialization(&options).unwrap_err();
        assert!(error.starts_with("managed publish activated"), "{error}");
        assert!(error.contains("unexpected-active"), "{error}");
        assert_eq!(
            crate::storage::managed::best_effort_entries(),
            0,
            "error-path snapshot outlived writer ownership"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_native_verification_snapshot_drops_before_writer_release() {
        let (root, options, _) = committed_artifact_fixture(true);
        POST_MANAGED_VERIFICATION_HOOK.with(|hook| {
            hook.set(Some(|_, _| {
                crate::storage::managed::reset_best_effort_entries()
            }))
        });
        fs::write(root.join("src/lib.rs"), "pub fn no_hidden_janitor() {}\n").unwrap();
        let (_, response) = execute_materialization(&options).unwrap();
        assert_eq!(
            crate::storage::managed::best_effort_entries(),
            0,
            "last-reader Drop started a hidden post-release cleanup"
        );
        assert_published_graph_readable(&options, &response);
        fs::remove_dir_all(root).unwrap();
    }
}
