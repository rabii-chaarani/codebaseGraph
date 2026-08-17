use crate::api::contracts::RepoSelector;
use crate::api::{
    context::{resolve_repository_root, RepoPaths, RepoRuntime},
    contracts::MaterializationRequest,
};
use crate::artifact_store::ArtifactStore;
use crate::protocol::{
    ManifestEntry, NativeManifest, NativeSyntaxMaterializationRequest,
    NativeSyntaxMaterializationResponse, MATERIALIZATION_MANIFEST_SCHEMA_VERSION,
};
use crate::storage::direct::{DirectStore, DirectWriteSession};
use crate::storage::layout::{DirectLayout, RepositoryLayout};
use crate::storage::managed::{GraphStorage, ManagedStore, ManagedWriteSession, StorageMode};
use crate::storage::run_workspace::{RunPhase, RunWorkspace};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[cfg(test)]
use crate::api::contracts::OutputFormat;

#[derive(Clone, Debug)]
pub(crate) struct MaterializeOptions {
    pub(crate) native_request: Option<PathBuf>,
    pub(crate) source_root: Option<PathBuf>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) db: Option<PathBuf>,
    pub(crate) manifest: Option<PathBuf>,
    pub(crate) storage_root: Option<PathBuf>,
    pub(crate) mode: String,
    pub(crate) include_fts: bool,
    pub(crate) semantic_enrichment: bool,
    pub(crate) semantic_provider_mode: String,
    pub(crate) use_git: bool,
    pub(crate) git_diff: bool,
    pub(crate) git_base: Option<String>,
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) candidate_paths: Vec<String>,
    pub(crate) parallel: bool,
    pub(crate) progress: bool,
    pub(crate) plan_only: bool,
    pub(crate) intent: MaterializationIntent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MaterializationIntent {
    #[default]
    ExplicitBuild,
    Refresh,
}

impl Default for MaterializeOptions {
    fn default() -> Self {
        Self {
            native_request: None,
            source_root: None,
            config: None,
            db: None,
            manifest: None,
            storage_root: None,
            mode: String::new(),
            include_fts: false,
            semantic_enrichment: false,
            semantic_provider_mode: String::new(),
            use_git: false,
            git_diff: false,
            git_base: None,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            parallel: true,
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
            source_root: Some(runtime.repo_root.clone()),
            config: runtime.config_path.clone(),
            db: Some(runtime.db_path.clone()),
            manifest: Some(runtime.manifest_path.clone()),
            storage_root: runtime.storage_root.clone(),
            mode: request.mode.clone(),
            include_fts: request.include_fts,
            semantic_enrichment: request.semantic_enrichment,
            semantic_provider_mode: request.semantic_provider_mode.clone(),
            use_git: request.use_git,
            git_diff: request.git_diff,
            git_base: request.git_base.clone(),
            include_patterns: request.include_patterns.clone(),
            exclude_patterns: request.exclude_patterns.clone(),
            candidate_paths: request.candidate_paths.clone(),
            parallel: request.parallel,
            progress: request.progress,
            plan_only,
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub(crate) struct ConfigScanRules {
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
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
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: options.use_git,
        git_diff: options.git_diff,
        git_base: options.git_base.clone(),
        include_patterns: options.include_patterns.clone(),
        exclude_patterns: options.exclude_patterns.clone(),
        candidate_paths: options.candidate_paths.clone(),
        parallel: options.parallel,
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
    let mut request = request;
    let execution = prepare_storage_execution(options)?;
    request.db_path = execution.request_db_path().to_string_lossy().into_owned();
    request.staging_dir = execution.staging_root().to_string_lossy().into_owned();
    request.previous_manifest = execution.previous_manifest().clone();
    request.artifact_root = execution.artifact_root().to_string_lossy().into_owned();
    request.manifest_schema_version = MATERIALIZATION_MANIFEST_SCHEMA_VERSION;
    if options.intent == MaterializationIntent::Refresh {
        let mut response = match crate::plan_materialization(&request) {
            Ok(response) => response,
            Err(error) => return Err(execution.abort_with_cleanup(error.to_string())),
        };
        if refresh_plan_is_current(&response) {
            response.storage_format = execution.storage_format().to_string();
            response.active_generation = execution.active_generation();
            execution.finish_without_publish()?;
            return Ok((request, response));
        }
    }
    let started = Instant::now();
    let final_request = request;
    let mut response = match crate::execute_materialization_pipeline(&final_request) {
        Ok(response) => response,
        Err(error) => return Err(execution.abort_with_cleanup(error.to_string())),
    };
    response.phase_timings.insert(
        "native_cli_seconds".to_string(),
        started.elapsed().as_secs_f64(),
    );
    finalize_materialization(execution, &final_request, &mut response)?;
    Ok((final_request, response))
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
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        schema_statements: Vec::new(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        atomic_rebuild: true,
        strict: true,
        parallel: options.parallel,
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
    })
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
) -> Result<NativeManifest, String> {
    let manifest = NativeManifest {
        schema_version: request.manifest_schema_version,
        ontology: request.ontology.clone(),
        parser_version: request.parser_version.clone(),
        graph_build_digest,
        files: materialized_entries.clone(),
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
            let workspace = RunWorkspace::create(
                RepositoryLayout::new(&runtime.state_dir)
                    .state_root()
                    .join("direct-runs"),
                None,
            )
            .map_err(|error| format!("failed to create direct run workspace: {error}"))?;
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
) -> Result<(), String> {
    let request_db_path = execution.request_db_path();
    let request_manifest_path = execution.request_manifest_path();
    let artifact_root = execution.artifact_root().to_path_buf();
    let manifest = match write_manifest(
        &request_manifest_path,
        request,
        &response.materialized_entries,
        response.graph_build_digest.clone(),
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
            if let Err(error) = garbage_collect_artifacts(&artifact_root, &published_manifest) {
                return Err(append_cleanup_error(
                    error.clone(),
                    workspace
                        .abort(Some(error))
                        .err()
                        .map(|cleanup| cleanup.to_string()),
                ));
            }
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
            workspace
                .finish()
                .map_err(|error| format!("failed to clean direct run workspace: {error}"))?;
            session.finish();
        }
        StorageExecution::Managed {
            artifact_root,
            mut session,
            store,
            ..
        } => {
            let published_generation = session
                .publish_with_stats(&response.graph_summary)
                .map_err(|error| format!("failed to publish managed graph generation: {error}"))?;
            let read = store
                .open_read()
                .map_err(|error| format!("failed to reopen managed graph generation: {error}"))?;
            if read.generation_id != published_generation {
                return Err(format!(
                    "managed publish activated {}, but reopened generation was {}",
                    published_generation, read.generation_id
                ));
            }
            let published_manifest = read_manifest(&read.manifest_path)?;
            validate_candidate_bundle(
                &read.db_path,
                &read.manifest_path,
                &artifact_root,
                &published_manifest,
                response,
            )?;
            let cleanup = store
                .recover_and_gc()
                .map_err(|error| format!("failed to reconcile managed storage cleanup: {error}"))?;
            if cleanup.run_recovery.skipped_locked == 0 {
                garbage_collect_artifacts(&artifact_root, &published_manifest)?;
            }
            response.storage_format = "managed_v2".to_string();
            response.active_generation = Some(published_generation);
            response.cleanup_pending =
                cleanup.retired_generations_pending > 0 || cleanup.run_recovery.skipped_locked > 0;
            response.pending_runs = cleanup.run_recovery.skipped_locked;
            session.finish();
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
    if manifest.files != response.materialized_entries {
        return Err(format!(
            "published manifest {} does not match materialized entries",
            manifest_path.display()
        ));
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

fn garbage_collect_artifacts(
    artifact_root: &Path,
    active_manifest: &NativeManifest,
) -> Result<(), String> {
    let retained = active_manifest
        .files
        .values()
        .filter_map(|entry| entry.artifact_key.clone())
        .collect::<BTreeSet<_>>();
    let store = ArtifactStore::new(artifact_root);
    for key in store.list_keys().map_err(|error| {
        format!(
            "failed to list artifacts in {}: {error}",
            artifact_root.display()
        )
    })? {
        if retained.contains(&key) {
            continue;
        }
        store
            .delete_key(&key)
            .map_err(|error| format!("failed to delete artifact {key}: {error}"))?;
    }
    Ok(())
}

fn cleanup_direct_candidate(session: &DirectWriteSession) -> Result<(), String> {
    let mut cleanup_errors = Vec::new();
    for path in [
        session.db_candidate_path(),
        session.manifest_candidate_path(),
    ] {
        if let Err(error) = remove_path_if_exists(&path) {
            cleanup_errors.push(error);
        }
    }
    for suffix in ["wal", "tmp", "lock"] {
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
    fn artifact_gc_retains_active_manifest_keys_and_removes_stale_keys() {
        let root = unique_temp_dir("codebase-graph-artifact-gc");
        let retained_key = "a".repeat(64);
        let stale_key = "b".repeat(64);
        let retained_dir = root.join("aa").join(&retained_key);
        let stale_dir = root.join("bb").join(&stale_key);
        fs::create_dir_all(&retained_dir).unwrap();
        fs::create_dir_all(&stale_dir).unwrap();
        fs::write(retained_dir.join("partition.json"), "retained").unwrap();
        fs::write(stale_dir.join("partition.json"), "stale").unwrap();
        let manifest = NativeManifest {
            schema_version: 2,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native".to_string(),
            graph_build_digest: Some("digest".to_string()),
            files: BTreeMap::from([(
                "src/lib.rs".to_string(),
                ManifestEntry {
                    path: "src/lib.rs".to_string(),
                    content_hash: "hash".to_string(),
                    language: "rust".to_string(),
                    partition_id: "partition".to_string(),
                    artifact_key: Some(retained_key.clone()),
                    node_ids: Vec::new(),
                    edge_ids: Vec::new(),
                    node_types: BTreeMap::new(),
                    edge_types: BTreeMap::new(),
                    materialized_at: "unix:0".to_string(),
                },
            )]),
        };

        garbage_collect_artifacts(&root, &manifest).unwrap();

        assert!(retained_dir.exists());
        assert!(!stale_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepublication_failures_clean_managed_and_direct_run_state() {
        for phase in [
            "parsing",
            "enrichment",
            "staging",
            "database",
            "candidate_validation",
        ] {
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
        .abort_with_cleanup("enrichment failed".to_string());

        assert!(error.contains("enrichment failed"));
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
                semantic_enrichment: false,
                semantic_provider_mode: "local_only".to_string(),
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
                    "exclude": ["target/**"]
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
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
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
}
