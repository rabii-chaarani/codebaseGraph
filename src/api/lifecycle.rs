use crate::api::context::{
    read_install_config, resolve_runtime, GraphInstallConfig, GraphInstallMaterializationConfig,
    GraphInstallMcpConfig, GraphInstallMcpHttpConfig, GraphInstallRefreshConfig, RepoRuntime,
    INSTALL_CONFIG_SCHEMA_VERSION,
};
use crate::api::contracts::{
    ApiError, McpInstallRequest, McpTransport, RefreshRequest, RepoSelector,
    RepositoryLifecycleRequest,
};
use crate::api::materialization::{
    build_request, default_excluded_parts, execute_candidate_materialization,
    execute_materialization, MaterializeOptions,
};
use crate::daemon_service::{
    repository_fingerprint, service_id, stable_daemon_port, DAEMON_TRANSPORT_VERSION,
};
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use crate::storage::atomic::write_json_atomically;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub(crate) fn is_retryable_refresh_failure(error: &str) -> bool {
    crate::db_writer::is_transient_database_error(error)
}

pub(crate) fn setup_repository(
    request: &RepositoryLifecycleRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    validate_lifecycle_action(request, "setup")?;
    setup_payload_for_request(request, &runtime.repo_root)
        .map_err(|error| ApiError::new("setup_failed", error))
}

pub(crate) fn reinstall_repository(
    request: &RepositoryLifecycleRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    validate_lifecycle_action(request, "reinstall")?;
    reinstall_payload_for_request(request, &runtime.repo_root)
        .map_err(|error| ApiError::new("reinstall_failed", error))
}

pub(crate) fn uninstall_repository(
    request: &RepositoryLifecycleRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    validate_lifecycle_action(request, "uninstall")?;
    uninstall_payload_for_request(request, &runtime.repo_root)
        .map_err(|error| ApiError::new("uninstall_failed", error))
}

pub(crate) fn install_mcp_client(
    request: &McpInstallRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    let transport = request.transport.resolved();
    let config_path = runtime.config_path.clone();
    let http = if transport == McpTransport::HttpDaemon {
        let path = config_path.as_ref().ok_or_else(|| {
            ApiError::new(
                "mcp_install_failed",
                "http-daemon transport requires an installed repository setup config",
            )
        })?;
        Some(
            ensure_http_config(path, &runtime.repo_root, request.daemon_port, true)
                .map_err(|error| ApiError::new("mcp_install_failed", error))?,
        )
    } else {
        None
    };
    let mut descriptor = build_mcp_descriptor(
        request.name.clone(),
        config_path.clone(),
        Some(runtime.repo_root.clone()),
    )
    .map_err(|error| ApiError::new("mcp_install_failed", error))?;
    let endpoint = match http.as_ref() {
        Some(http) => McpEndpointDescriptor::StreamableHttp {
            url: http.url.clone(),
            headers: BTreeMap::new(),
        },
        None => descriptor
            .endpoint(transport)
            .map_err(|error| ApiError::new("mcp_install_failed", error))?,
    };
    let install_options = McpClientInstallOptions {
        client: request.client.clone(),
        scope: request.scope.clone(),
        client_config_path: request.client_config_path.clone(),
        dry_run: request.dry_run,
        install_method: McpInstallMode::Auto,
        existing_entry_policy: McpExistingEntryPolicy::Replace,
        legacy_server_names: Vec::new(),
    };
    let mut preflight = install_options.clone();
    preflight.dry_run = true;
    let preflight_result = install_mcp_endpoint(&descriptor, &endpoint, &preflight)
        .map_err(|error| ApiError::new("mcp_install_failed", error))?;
    let needs_local_daemon = transport == McpTransport::HttpDaemon
        && (request.client == "all" || !manual_metadata_client(&request.client));
    if request.dry_run {
        let daemon = if needs_local_daemon {
            Some(
                ensure_managed_daemon(
                    descriptor.setup_config_path.as_deref().ok_or_else(|| {
                        ApiError::new(
                            "mcp_install_failed",
                            "managed MCP daemon requires a setup config path",
                        )
                    })?,
                    request.daemon_port,
                    true,
                )
                .map_err(|error| ApiError::new("mcp_install_failed", error))?,
            )
        } else {
            None
        };
        return Ok(attach_daemon_payload(preflight_result, daemon));
    }

    let previous_config = config_path
        .as_ref()
        .map(|path| snapshot_file(path))
        .transpose()
        .map_err(|error| ApiError::new("mcp_install_failed", error))?;
    if transport == McpTransport::HttpDaemon {
        let path = config_path
            .as_ref()
            .expect("HTTP config path was validated");
        ensure_http_config(path, &runtime.repo_root, request.daemon_port, false)
            .map_err(|error| ApiError::new("mcp_install_failed", error))?;
        descriptor = build_mcp_descriptor(
            request.name.clone(),
            config_path.clone(),
            Some(runtime.repo_root.clone()),
        )
        .map_err(|error| ApiError::new("mcp_install_failed", error))?;
    }
    let daemon = if needs_local_daemon {
        let setup_path = descriptor.setup_config_path.as_deref().ok_or_else(|| {
            ApiError::new(
                "mcp_install_failed",
                "managed MCP daemon requires a setup config path",
            )
        })?;
        match ensure_managed_daemon(setup_path, request.daemon_port, false) {
            Ok(payload) => Some(payload),
            Err(error) => {
                if let (Some(path), Some(previous)) =
                    (config_path.as_ref(), previous_config.as_ref())
                {
                    let _ = restore_file(path, previous.as_deref());
                }
                return Err(ApiError::new("mcp_install_failed", error));
            }
        }
    } else {
        None
    };
    match install_mcp_endpoint(&descriptor, &endpoint, &install_options) {
        Ok(result) => Ok(attach_daemon_payload(result, daemon)),
        Err(error) => {
            let mut rollback_errors = Vec::new();
            if daemon
                .as_ref()
                .is_some_and(|payload| payload["action"] == "started")
            {
                if let Some(path) = descriptor.setup_config_path.as_deref() {
                    if let Err(cleanup) = stop_managed_daemon(path, true, false) {
                        rollback_errors.push(format!("daemon rollback failed: {cleanup}"));
                    }
                }
            }
            if let (Some(path), Some(previous)) = (config_path.as_ref(), previous_config.as_ref()) {
                if let Err(cleanup) = restore_file(path, previous.as_deref()) {
                    rollback_errors.push(format!("config rollback failed: {cleanup}"));
                }
            }
            let message = if rollback_errors.is_empty() {
                error
            } else {
                format!("{error}; {}", rollback_errors.join("; "))
            };
            Err(ApiError::new("mcp_install_failed", message))
        }
    }
}

fn validate_lifecycle_action(
    request: &RepositoryLifecycleRequest,
    expected: &str,
) -> Result<(), ApiError> {
    if request.action == expected {
        Ok(())
    } else {
        Err(ApiError::new(
            "invalid_lifecycle_action",
            format!(
                "lifecycle request action {} does not match {expected}",
                request.action
            ),
        ))
    }
}

pub(crate) fn refresh_repository(
    request: &RefreshRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    let options = MaterializeOptions {
        source_root: Some(runtime.repo_root.clone()),
        config: runtime.config_path.clone(),
        db: Some(runtime.db_path.clone()),
        manifest: Some(runtime.manifest_path.clone()),
        storage_root: runtime.storage_root.clone(),
        use_git: false,
        mode: request.mode.clone(),
        include_fts: request.include_fts,
        semantic_enrichment: false,
        semantic_provider_mode: request.semantic_provider_mode.clone(),
        git_diff: false,
        git_base: None,
        include_patterns: Vec::new(),
        exclude_patterns: Vec::new(),
        candidate_paths: request.paths.clone(),
        parallel: request.parallel,
        progress: request.progress,
        plan_only: false,
        ..MaterializeOptions::default()
    };

    let (_request, response) =
        execute_candidate_materialization(&options, options.candidate_paths.clone()).map_err(
            |error| {
                let retryable = is_retryable_refresh_failure(&error);
                ApiError::new("refresh_failed", error).retryable(retryable)
            },
        )?;
    Ok(serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({})))
}

#[derive(Debug, Clone)]
struct LifecycleOptions {
    mode: String,
    include_fts: bool,
    semantic_provider_mode: String,
    mcp_client: String,
    mcp_config_path: Option<PathBuf>,
    skip_mcp_config: bool,
    mcp_transport: McpTransport,
    mcp_daemon_port: Option<u16>,
    dry_run: bool,
    instructions_target: String,
}

impl LifecycleOptions {
    fn from_request(request: &RepositoryLifecycleRequest) -> Self {
        Self {
            mode: request.mode.clone(),
            include_fts: request.include_fts,
            semantic_provider_mode: request.semantic_provider_mode.clone(),
            mcp_client: request
                .mcp_client
                .clone()
                .unwrap_or_else(|| "codex".to_string()),
            mcp_config_path: request.mcp_config_path.clone(),
            skip_mcp_config: request.skip_mcp_config,
            mcp_transport: request.mcp_transport,
            mcp_daemon_port: request.mcp_daemon_port,
            dry_run: request.dry_run,
            instructions_target: request
                .instructions_target
                .clone()
                .unwrap_or_else(|| "auto".to_string()),
        }
    }
}

fn setup_payload_for_request(
    request: &RepositoryLifecycleRequest,
    source_root: &Path,
) -> Result<serde_json::Value, String> {
    let options = LifecycleOptions::from_request(request);
    setup_payload_for_root(&options, source_root)
}

fn setup_payload_for_root(
    options: &LifecycleOptions,
    source_root: &Path,
) -> Result<serde_json::Value, String> {
    let source_root = source_root.to_path_buf();
    let paths = GraphStatePaths::derive(&source_root);
    reject_state_dir_root(&source_root)?;

    let materialize_options = MaterializeOptions {
        source_root: Some(source_root.clone()),
        config: Some(paths.config_path.clone()),
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        semantic_enrichment: false,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: true,
        ..MaterializeOptions::default()
    };
    let daemon_port = selected_daemon_port(&paths, &source_root, options.mcp_daemon_port)?;
    let config_payload = setup_config_payload(&paths, &source_root, daemon_port);
    let instructions_path = instruction_target_path(&source_root, &options.instructions_target)?;
    let state_dir_existed = paths.state_dir.exists();
    let graph_state_existed = managed_install_exists(&paths);
    let previous_config = snapshot_file(&paths.config_path)?;
    let previous_instructions = match instructions_path.as_ref() {
        Some(path) => Some((path.clone(), snapshot_file(path)?)),
        None => None,
    };
    let daemon_state_existed = paths.state_dir.join("mcp-daemon.json").exists();

    let (config_action, instructions, mcp_config, materialization) = if options.dry_run {
        let request = materialization_request(&materialize_options)?;
        let materialization = dry_run_materialization_payload(&request, &paths);
        let config_action = if json_file_would_change(&paths.config_path, &config_payload)? {
            "dry_run"
        } else {
            "unchanged"
        };
        let instructions = json!({
            "action": if instructions_path.is_some() { "dry_run" } else { "skipped" },
            "path": instructions_path.as_ref().map(|path| path.to_string_lossy().to_string()),
        });
        let mcp_config = setup_mcp_config(options, &paths, true)?;
        (
            config_action.to_string(),
            instructions,
            mcp_config,
            materialization,
        )
    } else {
        fs::create_dir_all(&paths.state_dir).map_err(|error| {
            format!(
                "failed to create state directory {}: {error}",
                paths.state_dir.display()
            )
        })?;
        let result = (|| {
            let config_action = write_setup_config(&paths, &source_root, daemon_port)?;
            let instructions = upsert_instruction_block(
                &source_root,
                &options.instructions_target,
                &paths.config_path,
            )?;
            let materialization = if graph_state_existed {
                existing_graph_materialization_payload(&materialize_options.mode, &paths)
            } else {
                let (_, response) = execute_materialization(&materialize_options)?;
                materialization_payload(&response, &materialize_options.mode, &paths)
            };
            let mcp_config = setup_mcp_config(options, &paths, false)?;
            Ok::<_, String>((
                config_action.to_string(),
                instructions,
                mcp_config,
                materialization,
            ))
        })();
        match result {
            Ok(result) => result,
            Err(error) => {
                let daemon_cleanup = if !daemon_state_existed
                    && options.mcp_transport.resolved() == McpTransport::HttpDaemon
                    && (options.mcp_client == "all" || !manual_metadata_client(&options.mcp_client))
                {
                    stop_managed_daemon(&paths.config_path, true, false).err()
                } else {
                    None
                };
                restore_file(&paths.config_path, previous_config.as_deref())?;
                if let Some((path, previous)) = previous_instructions.as_ref() {
                    restore_file(path, previous.as_deref())?;
                }
                if !state_dir_existed {
                    if let Err(cleanup_error) =
                        remove_partial_state_tree(&source_root, &paths.state_dir)
                    {
                        return Err(format!(
                            "{error}; cleanup failed for {}: {cleanup_error}",
                            paths.state_dir.display()
                        ));
                    }
                }
                return Err(match daemon_cleanup {
                    Some(cleanup) => format!("{error}; daemon rollback failed: {cleanup}"),
                    None => error,
                });
            }
        }
    };

    let runtime = resolved_install_runtime(&source_root);
    Ok(json!({
        "ok": true,
        "repo_root": source_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "db_path": runtime
            .as_ref()
            .map(|runtime| runtime.db_path.clone())
            .unwrap_or_else(|_| active_db_path(&paths)),
        "database_path": runtime
            .as_ref()
            .map(|runtime| runtime.db_path.clone())
            .unwrap_or_else(|_| active_db_path(&paths)),
        "manifest_path": runtime
            .as_ref()
            .map(|runtime| runtime.manifest_path.clone())
            .unwrap_or_else(|_| active_manifest_path(&paths)),
        "config_path": paths.config_path,
        "storage_root": runtime
            .as_ref()
            .ok()
            .and_then(|runtime| runtime.storage_root.clone())
            .unwrap_or_else(|| paths.state_dir.join("storage")),
        "storage_format": runtime
            .as_ref()
            .map(|runtime| runtime.storage_format())
            .unwrap_or("managed_v2"),
        "writable": runtime
            .as_ref()
            .map(|runtime| runtime.writable)
            .unwrap_or(true),
        "active_generation": runtime
            .as_ref()
            .ok()
            .and_then(|runtime| runtime.active_generation.clone())
            .or_else(|| active_generation_id(&paths)),
        "pending_runs": runtime
            .as_ref()
            .map(|runtime| runtime.pending_runs)
            .unwrap_or(0),
        "cleanup_pending": runtime
            .as_ref()
            .map(|runtime| runtime.cleanup_pending)
            .unwrap_or(false),
        "config_action": config_action,
        "mcp_config": mcp_config,
        "instructions": instructions,
        "materialization": materialization,
        "database_written": materialization.get("database_written").cloned().unwrap_or(json!(false)),
        "skipped": materialization.get("skipped").cloned().unwrap_or(json!(0)),
        "node_rows": materialization.get("node_rows").cloned().unwrap_or(json!(0)),
        "edge_rows": materialization.get("edge_rows").cloned().unwrap_or(json!(0)),
        "connector_rows": materialization.get("connector_rows").cloned().unwrap_or(json!(0)),
        "diagnostics": materialization.get("diagnostics").cloned().unwrap_or(json!([])),
    }))
}

fn reinstall_payload_for_request(
    request: &RepositoryLifecycleRequest,
    repo_root: &Path,
) -> Result<serde_json::Value, String> {
    let options = LifecycleOptions::from_request(request);
    let repo_root = repo_root.to_path_buf();
    reject_state_dir_root(&repo_root)?;

    let paths = GraphStatePaths::derive(&repo_root);
    if paths.config_path.exists() {
        setup_mcp_config(&options, &paths, true)?;
    }
    if !options.dry_run && paths.config_path.exists() {
        stop_managed_daemon(&paths.config_path, true, false)?;
    }
    let state = reinstall_state(&repo_root, &paths, options.dry_run)?;
    let install = if options.dry_run {
        setup_payload_for_root(&options, &repo_root)?
    } else {
        run_reinstall_activation_boundary(
            &repo_root,
            &paths,
            state.backup_path.as_deref(),
            || {
                let mut activation_options = options.clone();
                activation_options.skip_mcp_config = true;
                activation_options.instructions_target = "skip".to_string();
                setup_payload_for_root(&activation_options, &repo_root)
            },
            |mut payload| {
                let instructions = upsert_instruction_block(
                    &repo_root,
                    &options.instructions_target,
                    &paths.config_path,
                )?;
                let mcp_config = setup_mcp_config(&options, &paths, false)?;
                let payload_object = payload.as_object_mut().ok_or_else(|| {
                    "reinstall activation payload must be a JSON object".to_string()
                })?;
                payload_object.insert("instructions".to_string(), instructions);
                payload_object.insert("mcp_config".to_string(), mcp_config);
                Ok(payload)
            },
        )?
    };

    Ok(json!({
        "ok": true,
        "repo_root": repo_root,
        "dry_run": options.dry_run,
        "state": state.payload,
        "install": install,
    }))
}

fn uninstall_payload_for_request(
    request: &RepositoryLifecycleRequest,
    repo_root: &Path,
) -> Result<serde_json::Value, String> {
    let repo_root = repo_root.to_path_buf();
    let paths = GraphStatePaths::derive(&repo_root);
    let config_path = request
        .repo
        .config_path
        .clone()
        .unwrap_or_else(|| paths.config_path.clone());
    let mcp_client = request
        .mcp_client
        .clone()
        .unwrap_or_else(|| "all".to_string());
    let server_name = uninstall_server_name(&repo_root, &config_path)?;
    uninstall_mcp_clients(
        &mcp_client,
        request.mcp_config_path.as_deref(),
        &repo_root,
        &config_path,
        &server_name,
        true,
    )?;
    let daemon = if config_path.exists() {
        Some(stop_managed_daemon(&config_path, true, request.dry_run)?)
    } else {
        None
    };
    let state = uninstall_state_dir(&repo_root, &paths.state_dir, request.dry_run)?;
    let instructions = uninstall_instruction_blocks(&repo_root, request.dry_run)?;
    let mcp_clients = uninstall_mcp_clients(
        &mcp_client,
        request.mcp_config_path.as_deref(),
        &repo_root,
        &config_path,
        &server_name,
        request.dry_run,
    )?;

    Ok(json!({
        "ok": true,
        "repo_root": repo_root,
        "config_path": config_path,
        "server_name": server_name,
        "dry_run": request.dry_run,
        "state": state,
        "instructions": instructions,
        "mcp_clients": mcp_clients,
        "daemon": daemon,
    }))
}

fn reject_state_dir_root(repo_root: &Path) -> Result<(), String> {
    if repo_root
        .components()
        .any(|component| component.as_os_str() == ".codebaseGraph")
    {
        Err(format!(
            "Repository root may not be inside a .codebaseGraph state directory: {}",
            repo_root.display()
        ))
    } else {
        Ok(())
    }
}

fn existing_graph_materialization_payload(
    mode: &str,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    json!({
        "mode": mode,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "database_written": false,
        "skipped": true,
        "skip_reason": "existing_graph_state",
        "rebuilt": 0,
        "deleted": 0,
        "node_rows": 0,
        "edge_rows": 0,
        "connector_rows": 0,
        "diagnostics": [],
        "phase_timings": {},
    })
}

fn materialization_request(
    options: &MaterializeOptions,
) -> Result<NativeSyntaxMaterializationRequest, String> {
    build_request(options)
}

fn materialization_payload(
    response: &NativeSyntaxMaterializationResponse,
    mode: &str,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    let rebuilt_paths = response.diff.rebuild_paths();
    let skipped_paths = response
        .snapshots
        .iter()
        .filter_map(|(path, snapshot)| {
            if snapshot.language.is_none() {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let ignored_paths = response
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.strip_prefix("Ignored file: "))
        .map(str::to_string)
        .collect::<Vec<_>>();
    json!({
        "mode": mode,
        "scanned": response.snapshots.len(),
        "rebuilt": rebuilt_paths.len(),
        "skipped": skipped_paths.len(),
        "ignored": ignored_paths.len(),
        "deleted": response.diff.deleted.len(),
        "diagnostics": response.diagnostics,
        "manifest_path": paths.manifest_path,
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

fn dry_run_materialization_payload(
    request: &NativeSyntaxMaterializationRequest,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    let snapshots = scan_source_snapshots(Path::new(&request.source_root));
    let scanned = snapshots.len();
    let skipped_paths = snapshots
        .into_iter()
        .filter_map(|(path, language)| if language.is_none() { Some(path) } else { None })
        .collect::<Vec<_>>();
    json!({
        "mode": "dry_run",
        "scanned": scanned,
        "rebuilt": 0,
        "skipped": skipped_paths.len(),
        "deleted": 0,
        "diagnostics": [],
        "manifest_path": paths.manifest_path,
        "rebuilt_paths": [],
        "skipped_paths": skipped_paths,
        "deleted_paths": [],
        "graph_summary": {},
    })
}

fn scan_source_snapshots(root: &Path) -> Vec<(String, Option<&'static str>)> {
    let mut snapshots = Vec::new();
    scan_source_snapshots_inner(root, root, &mut snapshots);
    snapshots.sort_by(|left, right| left.0.cmp(&right.0));
    snapshots
}

fn scan_source_snapshots_inner(
    root: &Path,
    directory: &Path,
    snapshots: &mut Vec<(String, Option<&'static str>)>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if default_excluded_parts().iter().any(|part| part == name) {
            continue;
        }
        if path.is_dir() {
            scan_source_snapshots_inner(root, &path, snapshots);
        } else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
            snapshots.push((relative.to_string(), language_for_path(&path)));
        }
    }
}

fn language_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("py") => Some("python"),
        Some("rs") => Some("rust"),
        Some("go") => Some("go"),
        Some("css") => Some("css"),
        Some("c") | Some("h") => Some("c"),
        Some("cc") | Some("cpp") | Some("cxx") | Some("hpp") | Some("hh") => Some("cpp"),
        Some("f") | Some("f90") | Some("f95") | Some("for") => Some("fortran"),
        _ => None,
    }
}

fn setup_mcp_config(
    options: &LifecycleOptions,
    paths: &GraphStatePaths,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let descriptor = build_mcp_descriptor(
        Some("codebase_graph".to_string()),
        Some(paths.config_path.clone()),
        Some(
            paths
                .state_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
        ),
    )?;
    if options.skip_mcp_config || options.mcp_client == "none" {
        return Ok(json!({
            "action": "skipped",
            "client": options.mcp_client,
            "scope": "local",
            "server_name": descriptor.name,
            "method": serde_json::Value::Null,
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": descriptor.stdio_entry(false, true),
        }));
    }

    let transport = options.mcp_transport.resolved();
    let endpoint = descriptor.endpoint(transport)?;
    let install_options = McpClientInstallOptions {
        client: options.mcp_client.clone(),
        scope: if options.mcp_client == "claude-project" {
            "project".to_string()
        } else {
            "local".to_string()
        },
        client_config_path: options.mcp_config_path.clone(),
        dry_run,
        install_method: McpInstallMode::Auto,
        existing_entry_policy: McpExistingEntryPolicy::Replace,
        legacy_server_names: Vec::new(),
    };
    let mut preflight = install_options.clone();
    preflight.dry_run = true;
    let preflight_result = install_mcp_endpoint(&descriptor, &endpoint, &preflight)?;
    let needs_local_daemon = transport == McpTransport::HttpDaemon
        && (options.mcp_client == "all" || !manual_metadata_client(&options.mcp_client));
    let daemon = if needs_local_daemon {
        Some(ensure_managed_daemon(
            &paths.config_path,
            options.mcp_daemon_port,
            dry_run,
        )?)
    } else {
        None
    };
    let result = if dry_run {
        preflight_result
    } else {
        install_mcp_endpoint(&descriptor, &endpoint, &install_options)?
    };
    Ok(attach_daemon_payload(result, daemon))
}

fn attach_daemon_payload(
    mut registration: serde_json::Value,
    daemon: Option<serde_json::Value>,
) -> serde_json::Value {
    if let Some(object) = registration.as_object_mut() {
        object.insert(
            "daemon".to_string(),
            daemon.unwrap_or(serde_json::Value::Null),
        );
    }
    registration
}

fn ensure_http_config(
    config_path: &Path,
    repo_root: &Path,
    requested_port: Option<u16>,
    dry_run: bool,
) -> Result<GraphInstallMcpHttpConfig, String> {
    let mut paths = GraphStatePaths::derive(repo_root);
    paths.config_path = config_path.to_path_buf();
    let port = selected_daemon_port(&paths, repo_root, requested_port)?;
    let fingerprint = repository_fingerprint(repo_root);
    let http = GraphInstallMcpHttpConfig {
        url: format!("http://127.0.0.1:{port}/mcp"),
        service_id: service_id(&fingerprint),
        transport_version: DAEMON_TRANSPORT_VERSION.to_string(),
    };
    let mut config = read_install_config(config_path)?;
    let mcp = config.mcp.get_or_insert_with(|| GraphInstallMcpConfig {
        server_name: "codebase_graph".to_string(),
        command: vec![
            server_command(),
            "mcp".to_string(),
            "start".to_string(),
            "--config".to_string(),
            config_path.to_string_lossy().to_string(),
        ],
        http: None,
    });
    if mcp.http.as_ref().is_some_and(|current| {
        current.url == http.url
            && current.service_id == http.service_id
            && current.transport_version == http.transport_version
    }) {
        return Ok(http);
    }
    mcp.http = Some(http.clone());
    if !dry_run {
        let value = serde_json::to_value(&config).map_err(|error| error.to_string())?;
        write_json_atomically(config_path, &value).map_err(|error| {
            format!(
                "failed to persist managed MCP daemon endpoint {}: {error}",
                config_path.display()
            )
        })?;
    }
    Ok(http)
}

fn ensure_managed_daemon(
    config_path: &Path,
    port: Option<u16>,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    if dry_run {
        let spec = crate::daemon_service::McpDaemonSpec::from_config(config_path, port)?;
        return Ok(json!({
            "action": "dry_run",
            "endpoint": spec.endpoint,
            "service_id": spec.service_id,
            "repository_fingerprint": spec.repository_fingerprint,
        }));
    }
    #[cfg(test)]
    {
        let spec = crate::daemon_service::McpDaemonSpec::from_config(config_path, port)?;
        Ok(json!({
            "action": "test_managed",
            "endpoint": spec.endpoint,
            "service_id": spec.service_id,
            "repository_fingerprint": spec.repository_fingerprint,
        }))
    }
    #[cfg(not(test))]
    {
        let options = crate::daemon_service::McpDaemonOptions {
            repo_root: None,
            config: Some(config_path.to_path_buf()),
            port,
        };
        crate::daemon_service::start_mcp_daemon(&options)
    }
}

fn stop_managed_daemon(
    config_path: &Path,
    remove_service: bool,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    if dry_run {
        return Ok(json!({
            "action": "dry_run",
            "service_removed": remove_service,
        }));
    }
    #[cfg(test)]
    {
        let _ = config_path;
        Ok(json!({
            "action": "test_managed",
            "running": false,
            "service_removed": remove_service,
        }))
    }
    #[cfg(not(test))]
    {
        crate::daemon_service::stop_mcp_daemon(
            &crate::daemon_service::McpDaemonOptions {
                repo_root: None,
                config: Some(config_path.to_path_buf()),
                port: None,
            },
            remove_service,
        )
    }
}

fn uninstall_server_name(repo_root: &Path, config_path: &Path) -> Result<String, String> {
    if config_path.exists() {
        let config = read_json_file(config_path)?;
        if let Some(name) = config
            .pointer("/mcp/server_name")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            return Ok(name.to_string());
        }
    }
    Ok(build_mcp_descriptor(
        None,
        Some(config_path.to_path_buf()),
        Some(repo_root.to_path_buf()),
    )?
    .name)
}

fn uninstall_state_dir(
    repo_root: &Path,
    path: &Path,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(json!({"action": "unchanged", "path": path}));
    }
    if !dry_run {
        remove_partial_state_tree(repo_root, path).map_err(|error| {
            format!(
                "failed to remove state directory {}: {error}",
                path.display()
            )
        })?;
    }
    Ok(json!({"action": if dry_run { "dry_run" } else { "removed" }, "path": path}))
}

fn uninstall_instruction_blocks(
    repo_root: &Path,
    dry_run: bool,
) -> Result<Vec<serde_json::Value>, String> {
    ["AGENTS.md", "CLAUDE.md"]
        .into_iter()
        .map(|file_name| uninstall_instruction_file(&repo_root.join(file_name), dry_run))
        .collect()
}

fn uninstall_instruction_file(path: &Path, dry_run: bool) -> Result<serde_json::Value, String> {
    let Ok(existing) = fs::read_to_string(path) else {
        return Ok(json!({"action": "unchanged", "path": path}));
    };
    let (next, removed) = remove_instruction_text(&existing);
    if !removed {
        return Ok(json!({"action": "unchanged", "path": path}));
    }
    if !dry_run {
        fs::write(path, next).map_err(|error| {
            format!("failed to update instructions {}: {error}", path.display())
        })?;
    }
    Ok(json!({"action": if dry_run { "dry_run" } else { "removed" }, "path": path}))
}

fn uninstall_mcp_clients(
    mcp_client: &str,
    client_config_path: Option<&Path>,
    repo_root: &Path,
    config_path: &Path,
    server_name: &str,
    dry_run: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let clients = if mcp_client == "all" {
        supported_mcp_clients()
            .iter()
            .copied()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        vec![mcp_client.to_string()]
    };

    clients
        .into_iter()
        .map(|client| {
            uninstall_mcp_client(
                &client,
                client_config_path,
                repo_root,
                config_path,
                server_name,
                dry_run,
            )
        })
        .collect()
}

fn uninstall_mcp_client(
    client: &str,
    client_config_path: Option<&Path>,
    repo_root: &Path,
    config_path: &Path,
    server_name: &str,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    if matches!(client, "copilot-studio" | "microsoft-copilot") {
        return Ok(json!({
            "action": "skipped",
            "reason": "manual_metadata",
            "client": client,
            "server_name": server_name,
        }));
    }

    let scope = if client == "claude-project" {
        "project"
    } else {
        "local"
    };
    let descriptor = build_mcp_descriptor(
        Some(server_name.to_string()),
        Some(config_path.to_path_buf()),
        Some(repo_root.to_path_buf()),
    )?;
    let target = resolve_mcp_target(
        client,
        scope,
        &descriptor,
        client_config_path.map(Path::to_path_buf),
    )?;
    remove_mcp_server(server_name, &McpClientRemovalOptions { target, dry_run })
}

pub(crate) fn remove_instruction_text(existing: &str) -> (String, bool) {
    const START: &str = "<!-- codebaseGraph:start -->";
    const END: &str = "<!-- codebaseGraph:end -->";
    let Some(start) = existing.find(START) else {
        return (existing.to_string(), false);
    };
    let Some(end) = existing[start..].find(END).map(|index| start + index) else {
        return (existing.to_string(), false);
    };
    let after_end = end + END.len();
    let before = existing[..start].trim_end();
    let after = existing[after_end..].trim_start();
    let text = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (true, false) => format!("{after}\n"),
        (false, true) => format!("{before}\n"),
        (false, false) => format!("{before}\n\n{after}"),
    };
    (text, true)
}

pub(crate) struct GraphStatePaths {
    pub(crate) repo_name: String,
    pub(crate) state_dir: PathBuf,
    pub(crate) db_path: PathBuf,
    pub(crate) manifest_path: PathBuf,
    pub(crate) config_path: PathBuf,
}

impl GraphStatePaths {
    pub(crate) fn derive(repo_root: &Path) -> Self {
        let repo_name = safe_name(
            repo_root
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("repository"),
        );
        let state_dir = repo_root.join(".codebaseGraph");
        Self {
            db_path: state_dir.join(format!("{repo_name}_graph.ldb")),
            manifest_path: state_dir.join("manifest.json"),
            config_path: state_dir.join("config.json"),
            state_dir,
            repo_name,
        }
    }
}

pub(crate) fn safe_name(value: &str) -> String {
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

fn setup_config_payload(
    paths: &GraphStatePaths,
    repo_root: &Path,
    daemon_port: u16,
) -> serde_json::Value {
    let fingerprint = repository_fingerprint(repo_root);
    serde_json::to_value(GraphInstallConfig {
        schema_version: Some(INSTALL_CONFIG_SCHEMA_VERSION),
        repo_root: Some(repo_root.to_path_buf()),
        repo_name: Some(paths.repo_name.clone()),
        state_dir: Some(paths.state_dir.clone()),
        storage_root: Some(paths.state_dir.join("storage")),
        database_path: None,
        manifest_path: None,
        ontology_version: Some("code_ontology_v1".to_string()),
        package_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        materialization: GraphInstallMaterializationConfig::default(),
        refresh: GraphInstallRefreshConfig::default(),
        mcp: Some(GraphInstallMcpConfig {
            server_name: "codebase_graph".to_string(),
            command: vec![
                server_command(),
                "mcp".to_string(),
                "start".to_string(),
                "--config".to_string(),
                paths.config_path.to_string_lossy().to_string(),
            ],
            http: Some(GraphInstallMcpHttpConfig {
                url: format!("http://127.0.0.1:{daemon_port}/mcp"),
                service_id: service_id(&fingerprint),
                transport_version: DAEMON_TRANSPORT_VERSION.to_string(),
            }),
        }),
    })
    .expect("managed install config should serialize")
}

fn selected_daemon_port(
    paths: &GraphStatePaths,
    repo_root: &Path,
    requested: Option<u16>,
) -> Result<u16, String> {
    let persisted = paths
        .config_path
        .exists()
        .then(|| read_install_config(&paths.config_path))
        .transpose()?
        .and_then(|config| config.mcp)
        .and_then(|mcp| mcp.http)
        .and_then(|http| daemon_endpoint_port(&http.url));
    match requested {
        None => {
            if let Some(port) = persisted {
                return Ok(port);
            }
        }
        Some(port) if Some(port) == persisted => {
            return Ok(port);
        }
        Some(_) => {}
    }
    let first = requested.unwrap_or_else(|| stable_daemon_port(repo_root));
    #[cfg(test)]
    {
        Ok(first)
    }
    #[cfg(not(test))]
    {
        select_available_daemon_port(first, requested, |candidate| {
            std::net::TcpListener::bind(("127.0.0.1", candidate))
                .map(drop)
                .map_err(|error| error.to_string())
        })
    }
}

fn select_available_daemon_port(
    first: u16,
    requested: Option<u16>,
    mut probe: impl FnMut(u16) -> Result<(), String>,
) -> Result<u16, String> {
    if let Some(candidate) = requested {
        return probe(candidate).map(|()| candidate).map_err(|error| {
            format!("requested MCP daemon port {candidate} is unavailable: {error}")
        });
    }
    for offset in 0..128_u16 {
        let candidate = 41_000 + ((first - 41_000 + offset) % 8_000);
        if probe(candidate).is_ok() {
            return Ok(candidate);
        }
    }
    Err("no loopback port is available for the managed MCP daemon".to_string())
}

fn daemon_endpoint_port(url: &str) -> Option<u16> {
    url.strip_prefix("http://127.0.0.1:")?
        .split('/')
        .next()?
        .parse::<u16>()
        .ok()
}

fn write_setup_config(
    paths: &GraphStatePaths,
    repo_root: &Path,
    daemon_port: u16,
) -> Result<&'static str, String> {
    let payload = setup_config_payload(paths, repo_root, daemon_port);
    let mut action = "created";
    if paths.config_path.exists() {
        let previous = read_json_file(&paths.config_path)?;
        if previous == payload {
            return Ok("unchanged");
        }
        action = "updated";
    }
    if let Some(parent) = paths.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    write_json_atomically(&paths.config_path, &payload).map_err(|error| {
        format!(
            "failed to write install config {} atomically: {error}",
            paths.config_path.display()
        )
    })?;
    Ok(action)
}

fn managed_install_exists(paths: &GraphStatePaths) -> bool {
    paths.config_path.exists() && paths.state_dir.join("storage").join("active.json").exists()
}

fn resolved_install_runtime(repo_root: &Path) -> Result<RepoRuntime, String> {
    resolve_runtime(&RepoSelector {
        repo_root: Some(repo_root.to_path_buf()),
        config_path: None,
        db_path: None,
        manifest_path: None,
    })
}

fn active_generation_id(paths: &GraphStatePaths) -> Option<String> {
    let path = paths.state_dir.join("storage").join("active.json");
    let value = read_json_file(&path).ok()?;
    value
        .get("generation_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn active_db_path(paths: &GraphStatePaths) -> PathBuf {
    match active_generation_id(paths) {
        Some(generation) => paths
            .state_dir
            .join("storage")
            .join("generations")
            .join(format!("gen-{generation}"))
            .join("graph.ldb"),
        None => paths.db_path.clone(),
    }
}

fn active_manifest_path(paths: &GraphStatePaths) -> PathBuf {
    match active_generation_id(paths) {
        Some(generation) => paths
            .state_dir
            .join("storage")
            .join("generations")
            .join(format!("gen-{generation}"))
            .join("manifest.json"),
        None => paths.manifest_path.clone(),
    }
}

fn json_file_would_change(path: &Path, payload: &serde_json::Value) -> Result<bool, String> {
    if !path.exists() {
        return Ok(true);
    }
    Ok(read_json_file(path)? != *payload)
}

fn instruction_target_path(repo_root: &Path, target: &str) -> Result<Option<PathBuf>, String> {
    match target {
        "skip" => Ok(None),
        "agents" => Ok(Some(repo_root.join("AGENTS.md"))),
        "claude" => Ok(Some(repo_root.join("CLAUDE.md"))),
        "auto" => {
            let agents = repo_root.join("AGENTS.md");
            if agents.exists() {
                return Ok(Some(agents));
            }
            let claude = repo_root.join("CLAUDE.md");
            if claude.exists() {
                return Ok(Some(claude));
            }
            Ok(Some(agents))
        }
        other => Err(format!("Unsupported instruction target: {other}")),
    }
}

fn upsert_instruction_block(
    repo_root: &Path,
    target: &str,
    config_path: &Path,
) -> Result<serde_json::Value, String> {
    let Some(path) = instruction_target_path(repo_root, target)? else {
        return Ok(json!({"action": "skipped", "path": serde_json::Value::Null}));
    };
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let created = !path.exists();
    let block = instruction_block(config_path);
    let (next_text, action) = upsert_instruction_text(&existing, &block, created);
    if next_text == existing {
        return Ok(json!({"action": "unchanged", "path": path.to_string_lossy()}));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create instruction directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, next_text)
        .map_err(|error| format!("failed to write instructions {}: {error}", path.display()))?;
    Ok(json!({"action": action, "path": path.to_string_lossy()}))
}

fn instruction_block(config_path: &Path) -> String {
    format!(
        "<!-- codebaseGraph:start -->\n\
## codebaseGraph workflow\n\
- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.\n\
- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.\n\
- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: \"json\"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.\n\
- Use MCP `graph_search` with `layer: \"semantic\"`, `detail: \"slim\"`, and `context_limit: 1` before answering entity, relationship, or repository-structure questions and before coding tasks.\n\
- Use MCP `graph_context` with `layer: \"semantic\"`, `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when semantic relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.\n\
- For parse-tree, grammar-field, source-construct, or Tree-sitter node questions, first use MCP `graph_syntax` with the file language, then use `graph_search` with `layer: \"syntax\"` to find `SyntaxCapture` nodes.\n\
- Use MCP `graph_context` with `layer: \"syntax\"` only for a `SyntaxCapture` result, passing its `node_id` and `node_type: \"SyntaxCapture\"`; follow ordered `SyntaxChild` rows through `field_name` and `child_index`.\n\
- Use `layer: \"hybrid\"` only when semantic relationships and syntax structure are both needed; hybrid search returns semantic matches before syntax matches.\n\
- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.\n\
- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.\n\
- If MCP tools are unavailable, fall back to CLI: `{command} codebase-search <query> --layer semantic --no-refresh --detail slim --context-limit 1`, `{command} codebase-context <query> --layer semantic --profile <profile> --no-refresh --detail slim --context-limit 2`, `{command} syntax <language>`, `{command} codebase-search <query> --layer syntax --no-refresh --detail slim --context-limit 1`, `{command} codebase-context --node-id <id> --node-type SyntaxCapture --layer syntax --no-refresh --detail slim --context-limit 2`, `{command} codebase-context <query> --layer hybrid --profile <profile> --no-refresh --detail slim --context-limit 2`, `{command} codebase-architecture-queries`, `{command} graph-query \"<statement>\"`, `{command} schema`, and `{command} query-helpers`.\n\
- Do not rerun install to refresh the graph. The MCP server started from this setup config watches the repo and refreshes automatically; use `{command} build --mode full` only for explicit manual rebuilds. Setup config: `{config_path}`.\n\
<!-- codebaseGraph:end -->\n",
        command = server_command(),
        config_path = config_path.to_string_lossy(),
    )
}

fn upsert_instruction_text(existing: &str, block: &str, created: bool) -> (String, &'static str) {
    const START: &str = "<!-- codebaseGraph:start -->";
    const END: &str = "<!-- codebaseGraph:end -->";
    if existing.trim().is_empty() {
        return (block.to_string(), "created");
    }
    let Some(start) = existing.find(START) else {
        let separator = if existing.ends_with('\n') { "" } else { "\n" };
        let action = if created { "created" } else { "updated" };
        return (
            format!("{}{separator}\n{}", existing.trim_end(), block),
            action,
        );
    };
    let Some(end) = existing.find(END) else {
        return (
            format!("{}\n\n{}", existing.trim_end(), block),
            if created { "created" } else { "updated" },
        );
    };
    if end < start {
        return (
            format!("{}\n\n{}", existing.trim_end(), block),
            if created { "created" } else { "updated" },
        );
    }
    let after_end = end + END.len();
    let text = format!(
        "{}\n\n{}\n\n{}",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start()
    )
    .trim()
    .to_string()
        + "\n";
    (text, "updated")
}

fn snapshot_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| format!("failed to snapshot {}: {error}", path.display()))
}

fn restore_file(path: &Path, previous: Option<&[u8]>) -> Result<(), String> {
    match previous {
        Some(previous) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "failed to create restore directory {}: {error}",
                        parent.display()
                    )
                })?;
            }
            fs::write(path, previous)
                .map_err(|error| format!("failed to restore {}: {error}", path.display()))
        }
        None => {
            if path.exists() {
                fs::remove_file(path)
                    .map_err(|error| format!("failed to remove {}: {error}", path.display()))?;
            }
            Ok(())
        }
    }
}

struct ReinstallState {
    payload: serde_json::Value,
    backup_path: Option<PathBuf>,
}

fn reinstall_state(
    repo_root: &Path,
    paths: &GraphStatePaths,
    dry_run: bool,
) -> Result<ReinstallState, String> {
    if !paths.state_dir.exists() {
        return Ok(ReinstallState {
            payload: json!({
                "action": "unchanged",
                "path": paths.state_dir,
                "backup_path": serde_json::Value::Null,
            }),
            backup_path: None,
        });
    }
    let backup_path = next_backup_path(repo_root, &paths.state_dir)?;
    if dry_run {
        return Ok(ReinstallState {
            payload: json!({
                "action": "dry_run",
                "path": paths.state_dir,
                "backup_path": backup_path,
            }),
            backup_path: None,
        });
    }
    fs::rename(&paths.state_dir, &backup_path).map_err(|error| {
        format!(
            "failed to move existing graph state {} to {}: {error}",
            paths.state_dir.display(),
            backup_path.display()
        )
    })?;
    Ok(ReinstallState {
        payload: json!({
            "action": "backed_up",
            "path": paths.state_dir,
            "backup_path": backup_path,
        }),
        backup_path: Some(backup_path),
    })
}

fn next_backup_path(repo_root: &Path, state_dir: &Path) -> Result<PathBuf, String> {
    let parent = repo_root.parent().ok_or_else(|| {
        format!(
            "repository root {} must have a parent directory for reinstall backup",
            repo_root.display()
        )
    })?;
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repository");
    for index in 0..1000 {
        let suffix = if index == 0 {
            "reinstall-backup".to_string()
        } else {
            format!("reinstall-backup-{index}")
        };
        let candidate = parent.join(format!("{repo_name}.codebaseGraph.{suffix}"));
        validate_reinstall_backup_path(repo_root, &candidate)?;
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "failed to choose reinstall backup path outside repository {} for {}",
        repo_root.display(),
        state_dir.display()
    ))
}

fn validate_reinstall_backup_path(repo_root: &Path, candidate: &Path) -> Result<(), String> {
    let repo_root = canonical_or_self(repo_root);
    let repo_parent = repo_root.parent().ok_or_else(|| {
        format!(
            "repository root {} must have a parent directory for reinstall backup",
            repo_root.display()
        )
    })?;
    let candidate_parent = candidate.parent().ok_or_else(|| {
        format!(
            "reinstall backup path must have a parent directory: {}",
            candidate.display()
        )
    })?;
    if canonical_or_self(candidate_parent) != repo_parent {
        return Err(format!(
            "reinstall backup path must be a sibling of repository root {}: {}",
            repo_root.display(),
            candidate.display()
        ));
    }
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repository");
    if !valid_backup_name(
        candidate.file_name().and_then(|value| value.to_str()),
        repo_name,
    ) {
        return Err(format!(
            "reinstall backup path must use the managed backup filename for repository {}: {}",
            repo_root.display(),
            candidate.display()
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(candidate) {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing to use symlinked reinstall backup path {}",
                candidate.display()
            ));
        }
    }
    if candidate.starts_with(&repo_root) {
        return Err(format!(
            "reinstall backup path must live outside repository root {}: {}",
            repo_root.display(),
            candidate.display()
        ));
    }
    Ok(())
}

fn remove_backup(repo_root: &Path, path: Option<&Path>) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    validate_reinstall_backup_path(repo_root, path)?;
    remove_exact_path(path).map_err(|error| {
        format!(
            "failed to remove reinstall backup {} after successful setup: {error}",
            path.display()
        )
    })
}

fn remove_exact_path(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    remove_exact_path_with_metadata(path, &metadata)
}

fn restore_backup(
    repo_root: &Path,
    state_dir: &Path,
    backup_path: Option<&Path>,
) -> Result<(), String> {
    let Some(backup_path) = backup_path else {
        if state_dir.exists() {
            remove_partial_state_tree(repo_root, state_dir).map_err(|error| {
                format!(
                    "failed to remove partial graph state {} after setup failure: {error}",
                    state_dir.display()
                )
            })?;
        }
        return Ok(());
    };
    if state_dir.exists() {
        remove_partial_state_tree(repo_root, state_dir).map_err(|error| {
            format!(
                "failed to remove partial graph state {} before restore: {error}",
                state_dir.display()
            )
        })?;
    }
    fs::rename(backup_path, state_dir).map_err(|error| {
        format!(
            "failed to restore graph state backup {} to {}: {error}",
            backup_path.display(),
            state_dir.display()
        )
    })
}

fn remove_partial_state_tree(repo_root: &Path, state_dir: &Path) -> std::io::Result<()> {
    validate_state_dir_path(repo_root, state_dir)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    remove_exact_path(state_dir)
}

fn validate_state_dir_path(repo_root: &Path, state_dir: &Path) -> Result<(), String> {
    let expected = repo_root.join(".codebaseGraph");
    if canonical_or_self(state_dir) != canonical_or_self(&expected) {
        return Err(format!(
            "refusing to remove unexpected state directory {}; expected {}",
            state_dir.display(),
            expected.display()
        ));
    }
    Ok(())
}

fn remove_exact_path_with_metadata(path: &Path, metadata: &fs::Metadata) -> std::io::Result<()> {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing to remove symlink {}", path.display()),
        ));
    }
    if file_type.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let child = entry.path();
            let child_metadata = fs::symlink_metadata(&child)?;
            remove_exact_path_with_metadata(&child, &child_metadata)?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn valid_backup_name(candidate_name: Option<&str>, repo_name: &str) -> bool {
    let Some(candidate_name) = candidate_name else {
        return false;
    };
    let prefix = format!("{repo_name}.codebaseGraph.reinstall-backup");
    if candidate_name == prefix {
        return true;
    }
    candidate_name
        .strip_prefix(&(prefix + "-"))
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

fn run_reinstall_activation_boundary<Activate, AfterActivate>(
    repo_root: &Path,
    paths: &GraphStatePaths,
    backup_path: Option<&Path>,
    activate: Activate,
    after_activate: AfterActivate,
) -> Result<serde_json::Value, String>
where
    Activate: FnOnce() -> Result<serde_json::Value, String>,
    AfterActivate: FnOnce(serde_json::Value) -> Result<serde_json::Value, String>,
{
    let activation_payload = match activate() {
        Ok(payload) => payload,
        Err(error) => {
            restore_backup(repo_root, &paths.state_dir, backup_path)?;
            return Err(error);
        }
    };
    remove_backup(repo_root, backup_path)?;
    after_activate(activation_payload)
}

fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}

fn server_command() -> String {
    env::var("CODEBASE_GRAPH_SERVER_COMMAND").unwrap_or_else(|_| "codebase-graph".to_string())
}

#[derive(Debug, Clone)]
pub struct McpServerDescriptor {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub repo_root: PathBuf,
    pub timeout: u64,
    pub setup_config_path: Option<PathBuf>,
    pub tool_policy: Option<String>,
    pub manual_http_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum McpEndpointDescriptor {
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpHarnessCapabilities {
    pub stdio: bool,
    pub loopback_streamable_http: bool,
    pub manual_remote_https: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpTargetLocality {
    RepositoryLocal,
    Shared,
    Manual,
}

impl McpTargetLocality {
    fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryLocal => "repository_local",
            Self::Shared => "shared",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpInstallMode {
    Auto,
    FileAdapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpExistingEntryPolicy {
    Replace,
    RejectDifferent,
}

impl McpServerDescriptor {
    pub fn as_json(&self) -> serde_json::Value {
        json!({
            "name": &self.name,
            "transport": "stdio",
            "command": &self.command,
            "args": &self.args,
            "env": {},
            "cwd": serde_json::Value::Null,
            "setup_config_path": self.setup_config_path.as_ref().map(|path| path.to_string_lossy().to_string()),
            "repo_root": self.repo_root.to_string_lossy(),
            "timeout": self.timeout,
            "tool_policy": self.tool_policy,
        })
    }

    pub fn stdio_entry(&self, include_type: bool, include_timeout: bool) -> serde_json::Value {
        let mut entry = serde_json::Map::new();
        entry.insert("command".to_string(), json!(self.command));
        entry.insert("args".to_string(), json!(self.args));
        if include_type {
            entry.insert("type".to_string(), json!("stdio"));
        }
        if include_timeout {
            entry.insert("startup_timeout_sec".to_string(), json!(self.timeout));
        }
        serde_json::Value::Object(entry)
    }

    pub fn endpoint(&self, transport: McpTransport) -> Result<McpEndpointDescriptor, String> {
        match transport.resolved() {
            McpTransport::Stdio => Ok(McpEndpointDescriptor::Stdio {
                command: self.command.clone(),
                args: self.args.clone(),
                env: BTreeMap::new(),
            }),
            McpTransport::HttpDaemon => {
                let url = self
                    .manual_http_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("url"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|url| !url.trim().is_empty())
                    .ok_or_else(|| {
                        "setup config does not define a managed MCP HTTP endpoint".to_string()
                    })?;
                Ok(McpEndpointDescriptor::StreamableHttp {
                    url: url.to_string(),
                    headers: BTreeMap::new(),
                })
            }
            McpTransport::Auto => unreachable!("MCP transport was resolved"),
        }
    }
}

pub fn mcp_harness_capabilities(client: &str, scope: &str) -> McpHarnessCapabilities {
    let adapter = adapter_id(client, scope);
    if manual_metadata_client(adapter) {
        McpHarnessCapabilities {
            stdio: false,
            loopback_streamable_http: false,
            manual_remote_https: true,
        }
    } else if adapter == "claude-desktop" {
        McpHarnessCapabilities {
            stdio: true,
            loopback_streamable_http: false,
            manual_remote_https: false,
        }
    } else {
        McpHarnessCapabilities {
            stdio: true,
            loopback_streamable_http: true,
            manual_remote_https: false,
        }
    }
}

fn build_mcp_descriptor(
    name: Option<String>,
    config_path: Option<PathBuf>,
    repo_root: Option<PathBuf>,
) -> Result<McpServerDescriptor, String> {
    let resolved_repo_root = repo_root.unwrap_or_else(|| PathBuf::from("."));
    let config_path = config_path
        .clone()
        .unwrap_or_else(|| GraphStatePaths::derive(&resolved_repo_root).config_path);
    let setup_config = if config_path.exists() {
        Some(read_json_file(&config_path)?)
    } else {
        None
    };
    let repo_root = setup_config
        .as_ref()
        .and_then(|payload| payload.get("repo_root"))
        .and_then(serde_json::Value::as_str)
        .map(expand_path)
        .unwrap_or_else(|| {
            config_path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or(resolved_repo_root.clone())
        });
    let repo_name = setup_config
        .as_ref()
        .and_then(|payload| payload.get("repo_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            safe_name(
                repo_root
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("repository"),
            )
        });
    let name = name.unwrap_or_else(|| format!("codebase_graph_{}", install_safe_name(&repo_name)));
    let command_from_config = setup_config
        .as_ref()
        .and_then(|payload| payload.pointer("/mcp/command"))
        .and_then(serde_json::Value::as_array)
        .and_then(|values| {
            let command: Option<Vec<String>> = values
                .iter()
                .map(|value| value.as_str().map(str::to_string))
                .collect();
            command.filter(|parts| parts.len() >= 5)
        });
    let (command, args) = if let Some(mut parts) = command_from_config {
        let command = parts.remove(0);
        (command, parts)
    } else {
        (
            server_command(),
            vec![
                "mcp".to_string(),
                "start".to_string(),
                "--config".to_string(),
                config_path.to_string_lossy().to_string(),
            ],
        )
    };
    let http_url = setup_config
        .as_ref()
        .and_then(|payload| payload.pointer("/mcp/http/url"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("http://127.0.0.1:{}/mcp", stable_daemon_port(&repo_root)));
    let http_port =
        daemon_endpoint_port(&http_url).unwrap_or_else(|| stable_daemon_port(&repo_root));
    let manual_http_metadata = json!({
        "url": http_url,
        "start_command": [
            command,
            "mcp",
            "daemon",
            "start",
            "--config",
            config_path.to_string_lossy(),
            "--port",
            http_port.to_string()
        ],
        "host": "127.0.0.1",
        "port": http_port,
        "path": "/mcp",
        "transport_version": DAEMON_TRANSPORT_VERSION,
    });
    Ok(McpServerDescriptor {
        name,
        command,
        args,
        setup_config_path: Some(config_path),
        repo_root,
        timeout: 60,
        tool_policy: Some("graph_query_read_only".to_string()),
        manual_http_metadata: Some(manual_http_metadata),
    })
}

#[derive(Debug, Clone)]
pub struct McpClientInstallOptions {
    pub client: String,
    pub scope: String,
    pub client_config_path: Option<PathBuf>,
    pub dry_run: bool,
    pub install_method: McpInstallMode,
    pub existing_entry_policy: McpExistingEntryPolicy,
    pub legacy_server_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedMcpTarget {
    pub client: String,
    pub scope: String,
    pub locality: McpTargetLocality,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct McpClientRemovalOptions {
    pub target: ResolvedMcpTarget,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerRegistration {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub transport: String,
}

#[derive(Debug, Clone)]
pub struct McpClientRenameOptions {
    pub target: ResolvedMcpTarget,
    pub dry_run: bool,
}

pub fn install_mcp_server(
    descriptor: &McpServerDescriptor,
    options: &McpClientInstallOptions,
) -> Result<serde_json::Value, String> {
    let client = options.client.trim().to_ascii_lowercase();
    let scope = options.scope.trim().to_ascii_lowercase();
    if client != "all" && !supported_mcp_clients().contains(&client.as_str()) {
        return Err(format!("unsupported MCP client: {client}"));
    }
    if !matches!(scope.as_str(), "local" | "user" | "project") {
        return Err("MCP install scope must be local, user, or project".to_string());
    }
    if client == "all" {
        let results = supported_mcp_clients()
            .iter()
            .copied()
            .map(|client| {
                install_mcp_client_configuration(client, &scope, descriptor, options)
                    .unwrap_or_else(|error| {
                        json!({
                            "action": "failed",
                            "client": client,
                            "scope": install_scope(client, &scope),
                            "server_name": &descriptor.name,
                            "method": serde_json::Value::Null,
                            "path": serde_json::Value::Null,
                            "command": serde_json::Value::Null,
                            "descriptor": descriptor.as_json(),
                            "entry": {},
                            "target_locality": serde_json::Value::Null,
                            "legacy_cleanup": {"action": "not_run"},
                            "error": error,
                        })
                    })
            })
            .collect::<Vec<_>>();
        return Ok(json!({ "results": results }));
    }
    install_mcp_client_configuration(&client, &scope, descriptor, options)
}

pub fn install_mcp_endpoint(
    descriptor: &McpServerDescriptor,
    endpoint: &McpEndpointDescriptor,
    options: &McpClientInstallOptions,
) -> Result<serde_json::Value, String> {
    match endpoint {
        McpEndpointDescriptor::Stdio { .. } => install_mcp_server(descriptor, options),
        McpEndpointDescriptor::StreamableHttp { .. } => {
            install_mcp_http_endpoint(descriptor, endpoint, options)
        }
    }
}

fn install_mcp_http_endpoint(
    descriptor: &McpServerDescriptor,
    endpoint: &McpEndpointDescriptor,
    options: &McpClientInstallOptions,
) -> Result<serde_json::Value, String> {
    let client = options.client.trim().to_ascii_lowercase();
    let scope = options.scope.trim().to_ascii_lowercase();
    if client != "all" && !supported_mcp_clients().contains(&client.as_str()) {
        return Err(format!("unsupported MCP client: {client}"));
    }
    if !matches!(scope.as_str(), "local" | "user" | "project") {
        return Err("MCP install scope must be local, user, or project".to_string());
    }
    if client != "all" {
        return install_mcp_http_client_configuration(
            &client, &scope, descriptor, endpoint, options,
        );
    }

    let clients = supported_mcp_clients();
    for candidate in clients {
        let mut preflight = options.clone();
        preflight.client = (*candidate).to_string();
        preflight.dry_run = true;
        install_mcp_http_client_configuration(candidate, &scope, descriptor, endpoint, &preflight)?;
    }
    if options.dry_run {
        let results = clients
            .iter()
            .map(|client| {
                let mut child = options.clone();
                child.client = (*client).to_string();
                install_mcp_http_client_configuration(client, &scope, descriptor, endpoint, &child)
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({"results": results}));
    }

    let mut snapshots = BTreeMap::<PathBuf, Option<Vec<u8>>>::new();
    for candidate in clients {
        let target = resolve_mcp_target(candidate, &scope, descriptor, None)?;
        if let Some(path) = target.path {
            snapshots
                .entry(path.clone())
                .or_insert(snapshot_file(&path)?);
        }
    }
    let mut results = Vec::new();
    for candidate in clients {
        let mut child = options.clone();
        child.client = (*candidate).to_string();
        match install_mcp_http_client_configuration(candidate, &scope, descriptor, endpoint, &child)
        {
            Ok(result) => results.push(result),
            Err(error) => {
                for (path, previous) in &snapshots {
                    let _ = restore_file(path, previous.as_deref());
                }
                return Err(format!(
                    "HTTP MCP registration failed for {candidate}; all prior file changes were rolled back: {error}"
                ));
            }
        }
    }
    Ok(json!({"results": results}))
}

fn install_mcp_http_client_configuration(
    client: &str,
    scope: &str,
    descriptor: &McpServerDescriptor,
    endpoint: &McpEndpointDescriptor,
    options: &McpClientInstallOptions,
) -> Result<serde_json::Value, String> {
    let target = resolve_mcp_target(
        client,
        scope,
        descriptor,
        options.client_config_path.clone(),
    )?;
    if target.locality == McpTargetLocality::Manual {
        return Ok(json!({
            "action": "manual_remote_required",
            "client": client,
            "scope": target.scope,
            "server_name": descriptor.name,
            "method": "manual_metadata",
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": {
                "name": descriptor.name,
                "transport": "remote_https",
                "repo_root": descriptor.repo_root,
                "tool_policy": descriptor.tool_policy,
            },
            "entry": serde_json::Value::Null,
            "payload": {
                "public_https_required": true,
                "loopback_registered": false,
                "tunnel_provisioned": false,
                "instructions": "Deploy codebase-graph behind a publicly reachable HTTPS endpoint and register that URL manually."
            },
            "target_locality": target.locality.as_str(),
            "legacy_cleanup": manual_legacy_cleanup_payload(client, &options.legacy_server_names),
        }));
    }
    if client == "claude"
        && target.path.as_ref().is_some_and(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("claude_desktop_config.json")
        })
    {
        return Err(
            "Claude Desktop does not accept the managed loopback HTTP registration; use --mcp-transport stdio explicitly or target Claude Code"
                .to_string(),
        );
    }
    let path = target.path.clone().ok_or_else(|| {
        format!(
            "no file-backed MCP config target is available for {}",
            target.client
        )
    })?;
    let existing = read_optional_text(&path)?;
    let adapter = adapter_id(&target.client, &target.scope);
    let rendered = render_client_http_config(
        adapter,
        existing.as_deref(),
        descriptor,
        endpoint,
        &options.legacy_server_names,
    )?;
    if !options.dry_run && rendered.action != "unchanged" {
        write_text_atomic(&path, &rendered.text)?;
    }
    Ok(json!({
        "action": if options.dry_run { "dry_run" } else { rendered.action.as_str() },
        "client": target.client,
        "scope": target.scope,
        "server_name": descriptor.name,
        "method": "file_adapter",
        "path": path,
        "command": serde_json::Value::Null,
        "descriptor": endpoint_json(descriptor, endpoint),
        "entry": rendered.entry,
        "patch": rendered.patch,
        "payload": rendered.payload,
        "target_locality": target.locality.as_str(),
        "legacy_cleanup": rendered.legacy_cleanup,
        "restart_required": rendered.action == "updated",
        "restart_instructions": if rendered.action == "updated" {
            Some(format!("Restart {} so any existing host-owned stdio process is released.", target.client))
        } else {
            None
        },
    }))
}

fn endpoint_json(
    descriptor: &McpServerDescriptor,
    endpoint: &McpEndpointDescriptor,
) -> serde_json::Value {
    match endpoint {
        McpEndpointDescriptor::Stdio { .. } => descriptor.as_json(),
        McpEndpointDescriptor::StreamableHttp { url, headers } => json!({
            "name": descriptor.name,
            "transport": "streamable_http",
            "url": url,
            "headers": headers,
            "repo_root": descriptor.repo_root,
            "setup_config_path": descriptor.setup_config_path,
            "tool_policy": descriptor.tool_policy,
        }),
    }
}

fn install_mcp_client_configuration(
    client: &str,
    scope: &str,
    descriptor: &McpServerDescriptor,
    options: &McpClientInstallOptions,
) -> Result<serde_json::Value, String> {
    let target = resolve_mcp_target(
        client,
        scope,
        descriptor,
        options.client_config_path.clone(),
    )?;
    let native_command = native_client_command(client, descriptor, &target.scope);
    let native_available = native_command
        .as_ref()
        .and_then(|command| command.first())
        .is_some_and(|executable| executable_in_path(executable));
    if target.locality == McpTargetLocality::Manual {
        let metadata = copilot_studio_metadata(descriptor);
        return Ok(json!({
            "action": if options.dry_run { "dry_run" } else { "reported" },
            "client": client,
            "scope": target.scope,
            "server_name": descriptor.name,
            "method": "manual_metadata",
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": metadata["stdio"].clone(),
            "payload": metadata,
            "target_locality": target.locality.as_str(),
            "legacy_cleanup": manual_legacy_cleanup_payload(client, &options.legacy_server_names),
        }));
    }

    let can_use_native = matches!(options.install_method, McpInstallMode::Auto)
        && options.client_config_path.is_none()
        && options.legacy_server_names.is_empty()
        && target.locality == McpTargetLocality::Shared;

    if can_use_native && options.dry_run && native_available {
        return Ok(json!({
            "action": "dry_run",
            "client": client,
            "scope": target.scope,
            "server_name": descriptor.name,
            "method": "native_cli",
            "path": serde_json::Value::Null,
            "command": native_command,
            "descriptor": descriptor.as_json(),
            "entry": descriptor.stdio_entry(false, false),
            "target_locality": target.locality.as_str(),
            "legacy_cleanup": {"action": "unchanged", "requested": &options.legacy_server_names},
        }));
    }
    if can_use_native && !options.dry_run && native_available {
        let Some(command) = native_command.clone() else {
            return file_adapter_result(
                &target,
                descriptor,
                options.dry_run,
                options.existing_entry_policy,
                &options.legacy_server_names,
            );
        };
        let completed = Command::new(&command[0])
            .args(&command[1..])
            .output()
            .map_err(|error| format!("failed to run native client installer: {error}"))?;
        if completed.status.success() {
            return Ok(json!({
                "action": "updated",
                "client": client,
                "scope": target.scope,
                "server_name": descriptor.name,
                "method": "native_cli",
                "path": serde_json::Value::Null,
                "command": command,
                "descriptor": descriptor.as_json(),
                "entry": descriptor.stdio_entry(false, false),
                "target_locality": target.locality.as_str(),
                "legacy_cleanup": {"action": "unchanged", "requested": &options.legacy_server_names},
            }));
        }
        let error = subprocess_error(&completed);
        let mut payload = file_adapter_result(
            &target,
            descriptor,
            options.dry_run,
            options.existing_entry_policy,
            &options.legacy_server_names,
        )?;
        payload["native_command"] = json!(command);
        payload["native_error"] = json!(error);
        return Ok(payload);
    }

    let native_error = native_command.as_ref().and_then(|command| {
        command.first().and_then(|executable| {
            if executable_in_path(executable) {
                None
            } else {
                Some(format!("{executable} executable not found"))
            }
        })
    });
    let mut payload = file_adapter_result(
        &target,
        descriptor,
        options.dry_run,
        options.existing_entry_policy,
        &options.legacy_server_names,
    )?;
    if let Some(command) = native_command {
        payload["native_command"] = json!(command);
    }
    if let Some(error) = native_error {
        payload["native_error"] = json!(error);
    }
    Ok(payload)
}

fn file_adapter_result(
    target: &ResolvedMcpTarget,
    descriptor: &McpServerDescriptor,
    dry_run: bool,
    existing_entry_policy: McpExistingEntryPolicy,
    legacy_server_names: &[String],
) -> Result<serde_json::Value, String> {
    let path = target.path.clone().ok_or_else(|| {
        format!(
            "no file-backed MCP config target is available for {}",
            target.client
        )
    })?;
    let existing = read_optional_text(&path)?;
    let adapter = adapter_id(&target.client, &target.scope);
    let rendered = render_client_config(
        adapter,
        existing.as_deref(),
        descriptor,
        existing_entry_policy,
        legacy_server_names,
    )?;
    let action = if dry_run {
        "dry_run".to_string()
    } else {
        rendered.action.clone()
    };
    if !dry_run && rendered.action != "unchanged" {
        write_text_atomic(&path, &rendered.text)?;
    }
    let payload = json!({
        "action": action,
        "client": target.client,
        "scope": target.scope,
        "server_name": descriptor.name,
        "method": "file_adapter",
        "path": path.to_string_lossy(),
        "command": serde_json::Value::Null,
        "descriptor": descriptor.as_json(),
        "entry": rendered.entry,
        "patch": rendered.patch,
        "payload": rendered.payload,
        "target_locality": target.locality.as_str(),
        "legacy_cleanup": rendered.legacy_cleanup,
    });
    Ok(payload)
}

pub fn resolve_mcp_target(
    client: &str,
    scope: &str,
    descriptor: &McpServerDescriptor,
    client_config_path: Option<PathBuf>,
) -> Result<ResolvedMcpTarget, String> {
    let client = client.trim().to_ascii_lowercase();
    if client == "all" || !supported_mcp_clients().contains(&client.as_str()) {
        return Err(format!("unsupported MCP client target: {client}"));
    }
    let requested_scope = scope.trim().to_ascii_lowercase();
    if !matches!(requested_scope.as_str(), "local" | "user" | "project") {
        return Err("MCP target scope must be local, user, or project".to_string());
    }
    let scope = install_scope(&client, &requested_scope);
    let adapter = adapter_id(&client, &scope).to_string();
    let locality = resolve_target_locality(
        &adapter,
        &scope,
        client_config_path.as_deref(),
        &descriptor.repo_root,
    )?;
    let path = match locality {
        McpTargetLocality::Manual => None,
        _ => Some(
            client_config_path
                .map(|path| absolutize_path(&path))
                .unwrap_or_else(|| {
                    default_client_config_path(&adapter, &scope, &descriptor.repo_root)
                }),
        ),
    };
    Ok(ResolvedMcpTarget {
        client,
        scope,
        locality,
        path,
    })
}

pub fn remove_mcp_server(
    server_name: &str,
    options: &McpClientRemovalOptions,
) -> Result<serde_json::Value, String> {
    let target = &options.target;
    if target.locality == McpTargetLocality::Manual {
        return Ok(json!({
            "action": "skipped",
            "reason": "manual_metadata",
            "client": target.client,
            "scope": target.scope,
            "server_name": server_name,
            "path": serde_json::Value::Null,
            "target_locality": target.locality.as_str(),
            "payload": manual_legacy_cleanup_payload(&target.client, &[server_name.to_string()]),
        }));
    }
    let path = target.path.clone().ok_or_else(|| {
        format!(
            "no file-backed MCP config target is available for {}",
            target.client
        )
    })?;
    let existing = read_optional_text(&path)?;
    let removed = remove_client_config(
        adapter_id(&target.client, &target.scope),
        existing.as_deref(),
        server_name,
    )?;
    if removed.action == "removed" && !options.dry_run {
        write_text_atomic(&path, &removed.text)?;
    }
    let action = if removed.action == "removed" && options.dry_run {
        "dry_run".to_string()
    } else {
        removed.action.clone()
    };
    Ok(json!({
        "action": action,
        "client": target.client,
        "scope": target.scope,
        "server_name": server_name,
        "path": path.to_string_lossy(),
        "target_locality": target.locality.as_str(),
        "previous": removed.previous,
        "payload": removed.payload,
    }))
}

pub fn inspect_mcp_server(
    server_name: &str,
    target: &ResolvedMcpTarget,
) -> Result<Option<McpServerRegistration>, String> {
    if target.locality == McpTargetLocality::Manual {
        return Ok(None);
    }
    let path = target.path.as_ref().ok_or_else(|| {
        format!(
            "no file-backed MCP config target is available for {}",
            target.client
        )
    })?;
    let Some(existing) = read_optional_text(path)? else {
        return Ok(None);
    };
    inspect_client_endpoint_registration(
        adapter_id(&target.client, &target.scope),
        &existing,
        server_name,
    )
}

pub fn rename_mcp_server(
    source_name: &str,
    destination_name: &str,
    options: &McpClientRenameOptions,
) -> Result<serde_json::Value, String> {
    let target = &options.target;
    if target.locality == McpTargetLocality::Manual {
        return Err("manual MCP registrations cannot be renamed automatically".to_string());
    }
    if source_name.trim().is_empty() || destination_name.trim().is_empty() {
        return Err("MCP server names must not be empty".to_string());
    }
    if source_name == destination_name {
        return Err("source and destination MCP server names must differ".to_string());
    }
    let path = target.path.clone().ok_or_else(|| {
        format!(
            "no file-backed MCP config target is available for {}",
            target.client
        )
    })?;
    let existing = read_optional_text(&path)?.unwrap_or_default();
    let renamed = rename_client_registration(
        adapter_id(&target.client, &target.scope),
        &existing,
        source_name,
        destination_name,
    )?;
    if renamed.action != "unchanged" && !options.dry_run {
        write_text_atomic(&path, &renamed.text)?;
    }
    Ok(json!({
        "action": if options.dry_run && renamed.action != "unchanged" { "dry_run" } else { renamed.action.as_str() },
        "planned_action": renamed.action,
        "client": target.client,
        "scope": target.scope,
        "path": path.to_string_lossy(),
        "target_locality": target.locality.as_str(),
        "source_name": source_name,
        "destination_name": destination_name,
        "registration": renamed.registration.map(|entry| json!({"command": entry.command, "args": entry.args})),
    }))
}

fn resolve_target_locality(
    adapter: &str,
    scope: &str,
    explicit_path: Option<&Path>,
    repo_root: &Path,
) -> Result<McpTargetLocality, String> {
    if manual_metadata_client(adapter) {
        return Ok(McpTargetLocality::Manual);
    }
    if let Some(path) = explicit_path {
        let repo_root = repo_root.canonicalize().map_err(|error| {
            format!(
                "failed to resolve repository root {}: {error}",
                repo_root.display()
            )
        })?;
        let absolute = normalize_existing_or_absolute_path(&absolutize_path(path));
        return Ok(if absolute.starts_with(&repo_root) {
            McpTargetLocality::RepositoryLocal
        } else {
            McpTargetLocality::Shared
        });
    }
    Ok(match adapter {
        "codex" => {
            if scope == "user" {
                McpTargetLocality::Shared
            } else {
                McpTargetLocality::RepositoryLocal
            }
        }
        "claude-project" | "github-copilot" => McpTargetLocality::RepositoryLocal,
        "generic" => {
            if scope == "project" {
                McpTargetLocality::RepositoryLocal
            } else {
                McpTargetLocality::Shared
            }
        }
        "claude" => {
            if scope == "project" {
                McpTargetLocality::RepositoryLocal
            } else {
                McpTargetLocality::Shared
            }
        }
        "lmstudio" | "hermes" | "openclaw" => McpTargetLocality::Shared,
        _ => McpTargetLocality::Shared,
    })
}

fn manual_metadata_client(client: &str) -> bool {
    matches!(client, "copilot-studio" | "microsoft-copilot")
}

fn manual_legacy_cleanup_payload(
    client: &str,
    legacy_server_names: &[String],
) -> serde_json::Value {
    let requested = legacy_server_names
        .iter()
        .filter(|name| !name.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    json!({
        "action": if requested.is_empty() { "unchanged" } else { "manual_required" },
        "requested": requested,
        "instructions": if requested.is_empty() {
            Vec::<String>::new()
        } else {
            vec![format!(
                "Remove legacy MCP server entries {:?} from the manual {} configuration.",
                requested, client
            )]
        },
    })
}

fn absolutize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn normalize_existing_or_absolute_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let absolute = absolutize_path(path);
    let mut candidate = absolute.as_path();
    while let Some(parent) = candidate.parent() {
        if let Ok(canonical_parent) = parent.canonicalize() {
            if let Ok(suffix) = absolute.strip_prefix(parent) {
                return normalize_path_components(&canonical_parent.join(suffix));
            }
            break;
        }
        candidate = parent;
    }
    normalize_path_components(&absolute)
}

fn normalize_path_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn default_client_config_path(adapter: &str, scope: &str, repo_root: &Path) -> PathBuf {
    let home = home_dir();
    match adapter {
        "codex" => {
            if scope == "user" {
                env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".codex"))
                    .join("config.toml")
            } else {
                repo_root.join(".codex/config.toml")
            }
        }
        "claude" => {
            if scope == "project" {
                repo_root.join(".mcp.json")
            } else {
                home.join(".claude.json")
            }
        }
        "claude-project" => repo_root.join(".mcp.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "github-copilot" => repo_root.join(".vscode/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"))
            .join("mcp.json5"),
        "generic" => {
            if scope == "project" {
                repo_root.join(".mcp.json")
            } else {
                home.join(".config/mcp/mcp.json")
            }
        }
        _ => home.join(".config/mcp/mcp.json"),
    }
}

pub fn supported_mcp_clients() -> &'static [&'static str] {
    &[
        "claude",
        "claude-project",
        "codex",
        "copilot-studio",
        "generic",
        "github-copilot",
        "hermes",
        "lmstudio",
        "microsoft-copilot",
        "openclaw",
    ]
}

fn install_scope(client: &str, scope: &str) -> String {
    if client == "claude-project" {
        "project".to_string()
    } else {
        scope.to_string()
    }
}

fn adapter_id<'a>(client: &'a str, scope: &str) -> &'a str {
    if client == "claude" && scope == "project" {
        "claude-project"
    } else {
        client
    }
}

fn native_client_command(
    client: &str,
    descriptor: &McpServerDescriptor,
    scope: &str,
) -> Option<Vec<String>> {
    match client {
        "codex" => Some(native_stdio_command(
            vec![
                "codex".to_string(),
                "mcp".to_string(),
                "add".to_string(),
                descriptor.name.clone(),
                "--".to_string(),
            ],
            descriptor,
        )),
        "claude" | "claude-project" => Some(native_stdio_command(
            vec![
                "claude".to_string(),
                "mcp".to_string(),
                "add".to_string(),
                "--transport".to_string(),
                "stdio".to_string(),
                "--scope".to_string(),
                install_scope(client, scope),
                descriptor.name.clone(),
                "--".to_string(),
            ],
            descriptor,
        )),
        "openclaw" => Some(vec![
            "openclaw".to_string(),
            "mcp".to_string(),
            "set".to_string(),
            descriptor.name.clone(),
            serde_json::to_string(&descriptor.stdio_entry(true, false)).ok()?,
        ]),
        _ => None,
    }
}

fn native_stdio_command(mut command: Vec<String>, descriptor: &McpServerDescriptor) -> Vec<String> {
    command.push(descriptor.command.clone());
    command.extend(descriptor.args.iter().cloned());
    command
}

#[derive(Debug)]
struct RenderedNativeConfig {
    text: String,
    action: String,
    entry: serde_json::Value,
    patch: serde_json::Value,
    payload: serde_json::Value,
    legacy_cleanup: serde_json::Value,
}

struct RemovedNativeConfig {
    text: String,
    action: String,
    previous: serde_json::Value,
    payload: serde_json::Value,
}

struct RenamedNativeConfig {
    text: String,
    action: String,
    registration: Option<ManagedStdioEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedStdioEntry {
    command: String,
    args: Vec<String>,
}

fn inspect_client_endpoint_registration(
    adapter: &str,
    existing: &str,
    server_name: &str,
) -> Result<Option<McpServerRegistration>, String> {
    let http_url = match adapter {
        "codex" => {
            find_toml_block(existing, server_name).and_then(|block| parse_toml_http_url(&block))
        }
        "hermes" => parse_hermes_endpoint_entries(existing)?
            .0
            .get(server_name)
            .and_then(|entry| match entry {
                ManagedHermesEndpoint::Http { url } => Some(url.clone()),
                ManagedHermesEndpoint::Stdio(_) => None,
            }),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            let payload = if existing.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(existing)
                    .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
            };
            json_container(&payload, &json_adapter_root_path(adapter))?
                .and_then(|container| container.get(server_name))
                .and_then(|entry| entry.get("url"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        }
        other => return Err(format!("Unsupported MCP client adapter: {other}")),
    };
    if let Some(url) = http_url {
        return Ok(Some(McpServerRegistration {
            command: String::new(),
            args: Vec::new(),
            url: Some(url),
            transport: "streamable_http".to_string(),
        }));
    }
    inspect_client_registration(adapter, existing, server_name).map(|entry| {
        entry.map(|entry| McpServerRegistration {
            command: entry.command,
            args: entry.args,
            url: None,
            transport: "stdio".to_string(),
        })
    })
}

fn inspect_client_registration(
    adapter: &str,
    existing: &str,
    server_name: &str,
) -> Result<Option<ManagedStdioEntry>, String> {
    match adapter {
        "codex" => find_toml_block(existing, server_name)
            .map(|block| parse_toml_stdio_entry(&block))
            .transpose(),
        "hermes" => parse_hermes_managed_entries(existing)
            .map(|(entries, _)| entries.get(server_name).cloned()),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            let payload = if existing.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str::<serde_json::Value>(existing)
                    .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
            };
            let root_path = json_adapter_root_path(adapter);
            let Some(container) = json_container(&payload, &root_path)? else {
                return Ok(None);
            };
            container
                .get(server_name)
                .map(parse_json_stdio_entry)
                .transpose()
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn rename_client_registration(
    adapter: &str,
    existing: &str,
    source_name: &str,
    destination_name: &str,
) -> Result<RenamedNativeConfig, String> {
    if inspect_client_endpoint_registration(adapter, existing, source_name)?
        .as_ref()
        .is_some_and(|entry| entry.transport == "streamable_http")
    {
        return rename_http_client_registration(adapter, existing, source_name, destination_name);
    }
    match adapter {
        "codex" => rename_codex_registration(existing, source_name, destination_name),
        "hermes" => rename_hermes_registration(existing, source_name, destination_name),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            rename_json_registration(adapter, existing, source_name, destination_name)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn rename_http_client_registration(
    adapter: &str,
    existing: &str,
    source_name: &str,
    destination_name: &str,
) -> Result<RenamedNativeConfig, String> {
    match adapter {
        "codex" => {
            let source = find_toml_block(existing, source_name)
                .ok_or_else(|| format!("MCP server {source_name} was not found"))?;
            let source_url = parse_toml_http_url(&source)
                .ok_or_else(|| format!("MCP server {source_name} is not an HTTP entry"))?;
            if let Some(destination) = find_toml_block(existing, destination_name) {
                if parse_toml_http_url(&destination).as_deref() != Some(source_url.as_str()) {
                    return Err(format!(
                        "refusing to rename MCP server {source_name}: destination {destination_name} has a different endpoint"
                    ));
                }
                return Ok(RenamedNativeConfig {
                    text: remove_toml_block(existing, source_name).0,
                    action: "deduplicated".to_string(),
                    registration: None,
                });
            }
            let renamed = source.replace(
                &format!("[mcp_servers.{source_name}]"),
                &format!("[mcp_servers.{destination_name}]"),
            );
            let without_source = remove_toml_block(existing, source_name).0;
            Ok(RenamedNativeConfig {
                text: upsert_toml_block(&without_source, destination_name, &renamed).0,
                action: "renamed".to_string(),
                registration: None,
            })
        }
        "hermes" => {
            let (mut entries, _) = parse_hermes_endpoint_entries(existing)?;
            let source = entries
                .get(source_name)
                .cloned()
                .ok_or_else(|| format!("MCP server {source_name} was not found"))?;
            let action = if let Some(destination) = entries.get(destination_name) {
                if destination != &source {
                    return Err(format!(
                        "refusing to rename MCP server {source_name}: destination {destination_name} has a different endpoint"
                    ));
                }
                "deduplicated"
            } else {
                entries.insert(destination_name.to_string(), source);
                "renamed"
            };
            entries.remove(source_name);
            let patch = hermes_yaml_block_from_endpoint_entries(&entries);
            Ok(RenamedNativeConfig {
                text: if entries.is_empty() {
                    remove_marked_block(existing).0
                } else {
                    upsert_marked_block(existing, &patch).0
                },
                action: action.to_string(),
                registration: None,
            })
        }
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            let mut payload = serde_json::from_str::<serde_json::Value>(existing)
                .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?;
            let container = json_container_mut(&mut payload, &json_adapter_root_path(adapter))?;
            let source = container
                .get(source_name)
                .cloned()
                .ok_or_else(|| format!("MCP server {source_name} was not found"))?;
            let action = if let Some(destination) = container.get(destination_name) {
                if destination != &source {
                    return Err(format!(
                        "refusing to rename MCP server {source_name}: destination {destination_name} has a different endpoint"
                    ));
                }
                "deduplicated"
            } else {
                container.insert(destination_name.to_string(), source);
                "renamed"
            };
            container.remove(source_name);
            Ok(RenamedNativeConfig {
                text: serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
                    + "\n",
                action: action.to_string(),
                registration: None,
            })
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn descriptor_signature(descriptor: &McpServerDescriptor) -> ManagedStdioEntry {
    ManagedStdioEntry {
        command: descriptor.command.clone(),
        args: descriptor.args.clone(),
    }
}

fn render_client_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    existing_entry_policy: McpExistingEntryPolicy,
    legacy_server_names: &[String],
) -> Result<RenderedNativeConfig, String> {
    let mut rendered = match adapter {
        "codex" => render_codex_config(existing, descriptor, existing_entry_policy),
        "hermes" => render_hermes_config(existing, descriptor, existing_entry_policy),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            render_json_config(adapter, existing, descriptor, existing_entry_policy)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }?;
    rendered.legacy_cleanup = apply_legacy_cleanup(
        adapter,
        rendered.text.as_str(),
        descriptor.name.as_str(),
        legacy_server_names,
    )?;
    if let Some(cleaned_text) = rendered
        .legacy_cleanup
        .get("text")
        .and_then(serde_json::Value::as_str)
    {
        rendered.text = cleaned_text.to_string();
    }
    if rendered.action == "unchanged"
        && rendered
            .legacy_cleanup
            .get("action")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value != "unchanged")
    {
        rendered.action = "updated".to_string();
    }
    Ok(rendered)
}

fn render_client_http_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    endpoint: &McpEndpointDescriptor,
    legacy_server_names: &[String],
) -> Result<RenderedNativeConfig, String> {
    let McpEndpointDescriptor::StreamableHttp { url, headers } = endpoint else {
        return Err("HTTP renderer requires a Streamable HTTP endpoint".to_string());
    };
    let mut rendered = match adapter {
        "codex" => render_codex_http_config(existing, descriptor, url, headers),
        "hermes" => render_hermes_http_config(existing, descriptor, url),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            render_json_http_config(adapter, existing, descriptor, url, headers)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }?;
    rendered.legacy_cleanup = apply_legacy_cleanup(
        adapter,
        rendered.text.as_str(),
        descriptor.name.as_str(),
        legacy_server_names,
    )?;
    if let Some(cleaned_text) = rendered
        .legacy_cleanup
        .get("text")
        .and_then(serde_json::Value::as_str)
    {
        rendered.text = cleaned_text.to_string();
    }
    if rendered.action == "unchanged"
        && rendered
            .legacy_cleanup
            .get("action")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|action| action != "unchanged")
    {
        rendered.action = "updated".to_string();
    }
    Ok(rendered)
}

fn render_codex_http_config(
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    url: &str,
    headers: &BTreeMap<String, String>,
) -> Result<RenderedNativeConfig, String> {
    let mut lines = vec![
        format!("[mcp_servers.{}]", descriptor.name),
        format!(
            "url = {}",
            serde_json::to_string(url).map_err(|error| error.to_string())?
        ),
        format!("startup_timeout_sec = {}", descriptor.timeout),
    ];
    if !headers.is_empty() {
        lines.push(format!(
            "http_headers = {}",
            serde_json::to_string(headers).map_err(|error| error.to_string())?
        ));
    }
    let patch = lines.join("\n") + "\n";
    let entry =
        json!({"url": url, "http_headers": headers, "startup_timeout_sec": descriptor.timeout});
    let existing_text = existing.unwrap_or_default();
    let previous = find_toml_block(existing_text, &descriptor.name);
    if let Some(block) = previous.as_ref() {
        if parse_toml_http_url(block).as_deref() == Some(url) {
            return Ok(RenderedNativeConfig {
                text: existing_text.to_string(),
                action: "unchanged".to_string(),
                entry,
                patch: json!(block),
                payload: json!(existing_text),
                legacy_cleanup: json!({"action": "unchanged", "requested": []}),
            });
        }
        let recognized_stdio = parse_toml_stdio_entry(block)
            .ok()
            .is_some_and(|entry| entry == descriptor_signature(descriptor));
        if !recognized_stdio {
            return Err(format!(
                "refusing to overwrite existing MCP server {} because it is not the recognized managed stdio entry",
                descriptor.name
            ));
        }
    }
    let (text, previous) = upsert_toml_block(existing_text, &descriptor.name, &patch);
    Ok(RenderedNativeConfig {
        action: if previous.is_none() {
            "created"
        } else {
            "updated"
        }
        .to_string(),
        text,
        entry,
        patch: json!(patch),
        payload: json!(patch),
        legacy_cleanup: json!({"action": "unchanged", "requested": []}),
    })
}

fn parse_toml_http_url(block: &str) -> Option<String> {
    block.lines().find_map(|line| {
        line.trim()
            .strip_prefix("url = ")
            .and_then(|value| serde_json::from_str::<String>(value).ok())
    })
}

fn render_json_http_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    url: &str,
    headers: &BTreeMap<String, String>,
) -> Result<RenderedNativeConfig, String> {
    let mut payload = existing
        .filter(|text| !text.trim().is_empty())
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
        .unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Err("MCP config must contain a JSON object".to_string());
    }
    let entry = http_json_entry(adapter, url, headers);
    let root_path = json_adapter_root_path(adapter);
    let previous = json_container_mut(&mut payload, &root_path)?
        .get(&descriptor.name)
        .cloned();
    if previous.as_ref() == Some(&entry) {
        return Ok(RenderedNativeConfig {
            text: existing.unwrap_or_default().to_string(),
            action: "unchanged".to_string(),
            entry,
            patch: json!({}),
            payload,
            legacy_cleanup: json!({"action": "unchanged", "requested": []}),
        });
    }
    if let Some(previous) = previous.as_ref() {
        let recognized_stdio = parse_json_stdio_entry(previous)
            .ok()
            .is_some_and(|candidate| candidate == descriptor_signature(descriptor));
        if !recognized_stdio {
            return Err(format!(
                "refusing to overwrite existing MCP server {} because it is not the recognized managed stdio entry",
                descriptor.name
            ));
        }
    }
    json_container_mut(&mut payload, &root_path)?.insert(descriptor.name.clone(), entry.clone());
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())? + "\n";
    Ok(RenderedNativeConfig {
        action: if previous.is_none() {
            "created"
        } else {
            "updated"
        }
        .to_string(),
        text,
        entry: entry.clone(),
        patch: json!({"path": root_path, "server_name": descriptor.name, "entry": entry}),
        payload,
        legacy_cleanup: json!({"action": "unchanged", "requested": []}),
    })
}

fn http_json_entry(
    adapter: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    match adapter {
        "lmstudio" => {}
        "openclaw" => {
            entry.insert("transport".to_string(), json!("streamable-http"));
        }
        _ => {
            entry.insert("type".to_string(), json!("http"));
        }
    }
    entry.insert("url".to_string(), json!(url));
    if !headers.is_empty() {
        entry.insert("headers".to_string(), json!(headers));
    }
    serde_json::Value::Object(entry)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManagedHermesEndpoint {
    Stdio(ManagedStdioEntry),
    Http { url: String },
}

fn render_hermes_http_config(
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    url: &str,
) -> Result<RenderedNativeConfig, String> {
    let existing_text = existing.unwrap_or_default();
    let (mut entries, previous) = parse_hermes_endpoint_entries(existing_text)?;
    if entries.get(&descriptor.name)
        == Some(&ManagedHermesEndpoint::Http {
            url: url.to_string(),
        })
    {
        return Ok(RenderedNativeConfig {
            text: existing_text.to_string(),
            action: "unchanged".to_string(),
            entry: json!({"url": url}),
            patch: json!(previous.unwrap_or_default()),
            payload: json!(existing_text),
            legacy_cleanup: json!({"action": "unchanged", "requested": []}),
        });
    }
    if let Some(previous) = entries.get(&descriptor.name) {
        let recognized = matches!(
            previous,
            ManagedHermesEndpoint::Stdio(entry) if entry == &descriptor_signature(descriptor)
        );
        if !recognized {
            return Err(format!(
                "refusing to overwrite existing MCP server {} because it is not the recognized managed stdio entry",
                descriptor.name
            ));
        }
    }
    let existed = entries.contains_key(&descriptor.name);
    entries.insert(
        descriptor.name.clone(),
        ManagedHermesEndpoint::Http {
            url: url.to_string(),
        },
    );
    let patch = hermes_yaml_block_from_endpoint_entries(&entries);
    let (text, _) = upsert_marked_block(existing_text, &patch);
    Ok(RenderedNativeConfig {
        text,
        action: if existed { "updated" } else { "created" }.to_string(),
        entry: json!({"url": url}),
        patch: json!(patch),
        payload: json!(patch),
        legacy_cleanup: json!({"action": "unchanged", "requested": []}),
    })
}

fn parse_hermes_endpoint_entries(
    existing: &str,
) -> Result<(BTreeMap<String, ManagedHermesEndpoint>, Option<String>), String> {
    validate_hermes_managed_markers(existing)?;
    let Some((start, end, start_marker, end_marker)) = find_marked_block(existing) else {
        return Ok((BTreeMap::new(), None));
    };
    let block = existing[start..end + end_marker.len()]
        .trim_end()
        .to_string();
    let mut entries = BTreeMap::new();
    let mut current_name: Option<String> = None;
    let mut command: Option<String> = None;
    let mut args = Vec::new();
    let mut args_seen = false;
    let mut url: Option<String> = None;
    let flush = |entries: &mut BTreeMap<String, ManagedHermesEndpoint>,
                 current_name: &mut Option<String>,
                 command: &mut Option<String>,
                 args: &mut Vec<String>,
                 args_seen: &mut bool,
                 url: &mut Option<String>|
     -> Result<(), String> {
        if let Some(name) = current_name.take() {
            let endpoint = if let Some(url) = url.take() {
                ManagedHermesEndpoint::Http { url }
            } else {
                let command = command.take().ok_or_else(|| {
                    format!("existing Hermes MCP block is missing command or url for {name}")
                })?;
                if !*args_seen {
                    return Err(format!(
                        "existing Hermes MCP block is missing args for {name}"
                    ));
                }
                ManagedHermesEndpoint::Stdio(ManagedStdioEntry {
                    command,
                    args: std::mem::take(args),
                })
            };
            entries.insert(name, endpoint);
            *args_seen = false;
            command.take();
            args.clear();
        }
        Ok(())
    };
    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed == start_marker || trimmed == end_marker || trimmed == "mcp_servers:" {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("    ") {
            if let Some(name) = line
                .strip_prefix("  ")
                .and_then(|value| value.strip_suffix(':'))
            {
                flush(
                    &mut entries,
                    &mut current_name,
                    &mut command,
                    &mut args,
                    &mut args_seen,
                    &mut url,
                )?;
                current_name = Some(name.to_string());
                continue;
            }
        }
        if trimmed == "args:" {
            args_seen = true;
        } else if let Some(value) = trimmed.strip_prefix("command: ") {
            command = Some(
                serde_json::from_str(value)
                    .map_err(|_| "existing Hermes MCP command is not parseable".to_string())?,
            );
        } else if let Some(value) = trimmed.strip_prefix("url: ") {
            url = Some(
                serde_json::from_str(value)
                    .map_err(|_| "existing Hermes MCP url is not parseable".to_string())?,
            );
        } else if let Some(value) = trimmed.strip_prefix("- ") {
            args.push(
                serde_json::from_str(value)
                    .map_err(|_| "existing Hermes MCP arg is not parseable".to_string())?,
            );
        }
    }
    flush(
        &mut entries,
        &mut current_name,
        &mut command,
        &mut args,
        &mut args_seen,
        &mut url,
    )?;
    Ok((entries, Some(block)))
}

fn hermes_yaml_block_from_endpoint_entries(
    entries: &BTreeMap<String, ManagedHermesEndpoint>,
) -> String {
    let mut lines = vec![
        "# codebaseGraph MCP servers start".to_string(),
        "mcp_servers:".to_string(),
    ];
    for (name, endpoint) in entries {
        lines.push(format!("  {name}:"));
        match endpoint {
            ManagedHermesEndpoint::Http { url } => {
                lines.push(format!("    url: {}", yaml_scalar(url)));
            }
            ManagedHermesEndpoint::Stdio(entry) => {
                lines.push("    type: stdio".to_string());
                lines.push(format!("    command: {}", yaml_scalar(&entry.command)));
                lines.push("    args:".to_string());
                for arg in &entry.args {
                    lines.push(format!("      - {}", yaml_scalar(arg)));
                }
            }
        }
    }
    lines.push("# codebaseGraph MCP servers end".to_string());
    lines.join("\n") + "\n"
}

fn remove_client_config(
    adapter: &str,
    existing: Option<&str>,
    server_name: &str,
) -> Result<RemovedNativeConfig, String> {
    match adapter {
        "codex" => remove_codex_config(existing, server_name),
        "hermes" => remove_hermes_config(existing, server_name),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            remove_json_config(adapter, existing, server_name)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn apply_legacy_cleanup(
    adapter: &str,
    text: &str,
    server_name: &str,
    legacy_server_names: &[String],
) -> Result<serde_json::Value, String> {
    let mut current = text.to_string();
    let mut removed = Vec::new();
    for legacy_name in legacy_server_names {
        if legacy_name == server_name {
            continue;
        }
        let removed_config = remove_client_config(adapter, Some(&current), legacy_name)?;
        if removed_config.action == "removed" {
            removed.push(legacy_name.clone());
            current = removed_config.text;
        }
    }
    Ok(json!({
        "action": if removed.is_empty() { "unchanged" } else { "removed" },
        "requested": legacy_server_names,
        "removed": removed,
        "text": current,
    }))
}

fn render_json_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    existing_entry_policy: McpExistingEntryPolicy,
) -> Result<RenderedNativeConfig, String> {
    let mut payload = existing
        .filter(|text| !text.trim().is_empty())
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
        .unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Err("MCP config must contain a JSON object".to_string());
    }
    let root_path = match adapter {
        "github-copilot" => vec!["servers"],
        "openclaw" => vec!["mcp", "servers"],
        _ => vec!["mcpServers"],
    };
    let include_type = !matches!(adapter, "claude" | "generic");
    let entry = descriptor.stdio_entry(include_type, false);
    let previous = json_container_mut(&mut payload, &root_path)?
        .get(&descriptor.name)
        .cloned();
    let matching_managed_entry = previous
        .as_ref()
        .and_then(|value| json_stdio_matches(value, descriptor).ok())
        .unwrap_or(false);
    if matches!(
        existing_entry_policy,
        McpExistingEntryPolicy::RejectDifferent
    ) && previous.is_some()
        && !matching_managed_entry
    {
        if let Some(previous) = previous.as_ref() {
            json_stdio_matches(previous, descriptor)?;
        }
        return Err(format!(
            "refusing to overwrite existing MCP server {} with a different command or args",
            descriptor.name
        ));
    }
    if !matching_managed_entry {
        json_container_mut(&mut payload, &root_path)?
            .insert(descriptor.name.clone(), entry.clone());
    }
    let action = if matching_managed_entry {
        "unchanged".to_string()
    } else {
        action_for_json(previous.as_ref(), &entry, existing.is_some())
    };
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())? + "\n";
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else {
        action
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: payload.clone(),
        payload,
        legacy_cleanup: json!({"action": "unchanged", "requested": []}),
    })
}

fn remove_json_config(
    adapter: &str,
    existing: Option<&str>,
    server_name: &str,
) -> Result<RemovedNativeConfig, String> {
    let Some(existing) = existing else {
        return Ok(RemovedNativeConfig {
            text: String::new(),
            action: "unchanged".to_string(),
            previous: serde_json::Value::Null,
            payload: json!({}),
        });
    };
    let mut payload = if existing.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<serde_json::Value>(existing)
            .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
    };
    if !payload.is_object() {
        return Err("MCP config must contain a JSON object".to_string());
    }
    let root_path = match adapter {
        "github-copilot" => vec!["servers"],
        "openclaw" => vec!["mcp", "servers"],
        _ => vec!["mcpServers"],
    };
    let previous = json_container_mut(&mut payload, &root_path)?.remove(server_name);
    let action = if previous.is_some() {
        "removed"
    } else {
        "unchanged"
    }
    .to_string();
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())? + "\n";
    Ok(RemovedNativeConfig {
        text,
        action,
        previous: previous.unwrap_or(serde_json::Value::Null),
        payload,
    })
}

fn rename_json_registration(
    adapter: &str,
    existing: &str,
    source_name: &str,
    destination_name: &str,
) -> Result<RenamedNativeConfig, String> {
    let mut payload = if existing.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<serde_json::Value>(existing)
            .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
    };
    let root_path = json_adapter_root_path(adapter);
    let container = json_container_mut(&mut payload, &root_path)?;
    let Some(source) = container.get(source_name).cloned() else {
        return Ok(RenamedNativeConfig {
            text: existing.to_string(),
            action: "unchanged".to_string(),
            registration: None,
        });
    };
    let registration = parse_json_stdio_entry(&source)?;
    let action = if let Some(destination) = container.get(destination_name) {
        let destination = parse_json_stdio_entry(destination)?;
        if destination != registration {
            return Err(format!(
                "refusing to rename MCP server {source_name}: destination {destination_name} has a different command or args"
            ));
        }
        "deduplicated"
    } else {
        container.insert(destination_name.to_string(), source);
        "renamed"
    };
    container.remove(source_name);
    Ok(RenamedNativeConfig {
        text: serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())? + "\n",
        action: action.to_string(),
        registration: Some(registration),
    })
}

fn json_adapter_root_path(adapter: &str) -> Vec<&'static str> {
    match adapter {
        "github-copilot" => vec!["servers"],
        "openclaw" => vec!["mcp", "servers"],
        _ => vec!["mcpServers"],
    }
}

fn render_codex_config(
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    existing_entry_policy: McpExistingEntryPolicy,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(false, true);
    let patch = codex_toml_block(descriptor);
    if let Some(previous) = find_toml_block(existing.unwrap_or_default(), &descriptor.name) {
        match toml_stdio_matches(&previous, descriptor) {
            Ok(true) => {
                return Ok(RenderedNativeConfig {
                    text: existing.unwrap_or_default().to_string(),
                    action: "unchanged".to_string(),
                    entry,
                    patch: json!(previous),
                    payload: json!(existing.unwrap_or_default()),
                    legacy_cleanup: json!({"action": "unchanged", "requested": []}),
                });
            }
            Ok(false)
                if matches!(
                    existing_entry_policy,
                    McpExistingEntryPolicy::RejectDifferent
                ) =>
            {
                return Err(format!(
                    "refusing to overwrite existing MCP server {} with a different command or args",
                    descriptor.name
                ));
            }
            Err(error)
                if matches!(
                    existing_entry_policy,
                    McpExistingEntryPolicy::RejectDifferent
                ) =>
            {
                return Err(error);
            }
            Ok(false) | Err(_) => {}
        }
    }
    let (text, previous) =
        upsert_toml_block(existing.unwrap_or_default(), &descriptor.name, &patch);
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else if previous.is_none() {
        "created".to_string()
    } else if previous.as_deref() == Some(patch.trim_end()) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: json!(patch),
        payload: json!(patch),
        legacy_cleanup: json!({"action": "unchanged", "requested": []}),
    })
}

fn remove_codex_config(
    existing: Option<&str>,
    server_name: &str,
) -> Result<RemovedNativeConfig, String> {
    let Some(existing) = existing else {
        return Ok(RemovedNativeConfig {
            text: String::new(),
            action: "unchanged".to_string(),
            previous: serde_json::Value::Null,
            payload: json!(""),
        });
    };
    let (text, previous) = remove_toml_block(existing, server_name);
    Ok(RemovedNativeConfig {
        text,
        action: if previous.is_some() {
            "removed".to_string()
        } else {
            "unchanged".to_string()
        },
        previous: previous
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
        payload: json!(existing),
    })
}

fn rename_codex_registration(
    existing: &str,
    source_name: &str,
    destination_name: &str,
) -> Result<RenamedNativeConfig, String> {
    let Some(source_block) = find_toml_block(existing, source_name) else {
        return Ok(RenamedNativeConfig {
            text: existing.to_string(),
            action: "unchanged".to_string(),
            registration: None,
        });
    };
    let registration = parse_toml_stdio_entry(&source_block)?;
    if let Some(destination_block) = find_toml_block(existing, destination_name) {
        if parse_toml_stdio_entry(&destination_block)? != registration {
            return Err(format!(
                "refusing to rename MCP server {source_name}: destination {destination_name} has a different command or args"
            ));
        }
        return Ok(RenamedNativeConfig {
            text: remove_toml_block(existing, source_name).0,
            action: "deduplicated".to_string(),
            registration: Some(registration),
        });
    }
    let renamed_block = source_block
        .replace(
            &format!("[mcp_servers.{source_name}]"),
            &format!("[mcp_servers.{destination_name}]"),
        )
        .replace(
            &format!("[mcp_servers.{source_name}.env]"),
            &format!("[mcp_servers.{destination_name}.env]"),
        );
    let without_source = remove_toml_block(existing, source_name).0;
    let (text, _) = upsert_toml_block(&without_source, destination_name, &renamed_block);
    Ok(RenamedNativeConfig {
        text,
        action: "renamed".to_string(),
        registration: Some(registration),
    })
}

fn render_hermes_config(
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
    existing_entry_policy: McpExistingEntryPolicy,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(true, false);
    let (mut managed, previous) = parse_hermes_managed_entries(existing.unwrap_or_default())?;
    let matching_managed_entry = managed
        .get(&descriptor.name)
        .is_some_and(|value| value == &descriptor_signature(descriptor));
    if matches!(
        existing_entry_policy,
        McpExistingEntryPolicy::RejectDifferent
    ) {
        if let Some(previous) = managed.get(&descriptor.name) {
            if previous != &descriptor_signature(descriptor) {
                return Err(format!(
                    "refusing to overwrite existing MCP server {} with a different command or args",
                    descriptor.name
                ));
            }
        }
    }
    let already_uses_multi_server_block = previous
        .as_deref()
        .is_some_and(|block| block.contains("# codebaseGraph MCP servers start"));
    if matching_managed_entry && already_uses_multi_server_block {
        return Ok(RenderedNativeConfig {
            text: existing.unwrap_or_default().to_string(),
            action: "unchanged".to_string(),
            entry,
            patch: json!(previous.unwrap_or_default()),
            payload: json!(existing.unwrap_or_default()),
            legacy_cleanup: json!({"action": "unchanged", "requested": []}),
        });
    }
    managed.insert(descriptor.name.clone(), descriptor_signature(descriptor));
    let patch = hermes_yaml_block_from_entries(&managed);
    let (text, _) = upsert_marked_block(existing.unwrap_or_default(), &patch);
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else if previous.is_none() {
        "created".to_string()
    } else if previous.as_deref() == Some(patch.trim_end()) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: json!(patch),
        payload: json!(patch),
        legacy_cleanup: json!({"action": "unchanged", "requested": []}),
    })
}

fn remove_hermes_config(
    existing: Option<&str>,
    server_name: &str,
) -> Result<RemovedNativeConfig, String> {
    let Some(existing) = existing else {
        return Ok(RemovedNativeConfig {
            text: String::new(),
            action: "unchanged".to_string(),
            previous: serde_json::Value::Null,
            payload: json!(""),
        });
    };
    let (mut managed, _) = parse_hermes_endpoint_entries(existing)?;
    let previous = managed.remove(server_name);
    let action = if previous.is_some() {
        "removed".to_string()
    } else {
        "unchanged".to_string()
    };
    let patch = if managed.is_empty() {
        String::new()
    } else {
        hermes_yaml_block_from_endpoint_entries(&managed)
    };
    let text = if previous.is_some() {
        if patch.is_empty() {
            remove_marked_block(existing).0
        } else {
            upsert_marked_block(existing, &patch).0
        }
    } else {
        existing.to_string()
    };
    Ok(RemovedNativeConfig {
        text,
        action,
        previous: previous
            .map(|entry| match entry {
                ManagedHermesEndpoint::Stdio(entry) => {
                    json!({"command": entry.command, "args": entry.args})
                }
                ManagedHermesEndpoint::Http { url } => json!({"url": url}),
            })
            .unwrap_or(serde_json::Value::Null),
        payload: json!(existing),
    })
}

fn rename_hermes_registration(
    existing: &str,
    source_name: &str,
    destination_name: &str,
) -> Result<RenamedNativeConfig, String> {
    let (mut managed, _) = parse_hermes_managed_entries(existing)?;
    let Some(registration) = managed.get(source_name).cloned() else {
        return Ok(RenamedNativeConfig {
            text: existing.to_string(),
            action: "unchanged".to_string(),
            registration: None,
        });
    };
    let action = if let Some(destination) = managed.get(destination_name) {
        if destination != &registration {
            return Err(format!(
                "refusing to rename MCP server {source_name}: destination {destination_name} has a different command or args"
            ));
        }
        "deduplicated"
    } else {
        managed.insert(destination_name.to_string(), registration.clone());
        "renamed"
    };
    managed.remove(source_name);
    let patch = if managed.is_empty() {
        String::new()
    } else {
        hermes_yaml_block_from_entries(&managed)
    };
    let text = if patch.is_empty() {
        remove_marked_block(existing).0
    } else {
        upsert_marked_block(existing, &patch).0
    };
    Ok(RenamedNativeConfig {
        text,
        action: action.to_string(),
        registration: Some(registration),
    })
}

fn json_stdio_matches(
    previous: &serde_json::Value,
    descriptor: &McpServerDescriptor,
) -> Result<bool, String> {
    Ok(parse_json_stdio_entry(previous)? == descriptor_signature(descriptor))
}

fn parse_json_stdio_entry(value: &serde_json::Value) -> Result<ManagedStdioEntry, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "existing MCP entry must be an object".to_string())?;
    let command = object
        .get("command")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "existing MCP entry must define a string command".to_string())?;
    let args = object
        .get("args")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "existing MCP entry must define a string args array".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "existing MCP entry args must be strings".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ManagedStdioEntry {
        command: command.to_string(),
        args,
    })
}

fn find_toml_block(existing: &str, server_name: &str) -> Option<String> {
    let lines = existing.lines().collect::<Vec<_>>();
    let header = format!("[mcp_servers.{server_name}]");
    let env_header = format!("[mcp_servers.{server_name}.env]");
    let start = lines
        .iter()
        .position(|line| line.trim() == header || line.trim() == env_header)?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[')
                && trimmed.ends_with(']')
                && trimmed != header
                && trimmed != env_header
        })
        .map(|index| start + 1 + index)
        .unwrap_or(lines.len());
    Some(lines[start..end].join("\n"))
}

fn toml_stdio_matches(block: &str, descriptor: &McpServerDescriptor) -> Result<bool, String> {
    Ok(parse_toml_stdio_entry(block)? == descriptor_signature(descriptor))
}

fn parse_toml_stdio_entry(block: &str) -> Result<ManagedStdioEntry, String> {
    let mut command = None;
    let mut args = None;
    for line in block.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("command = ") {
            command = Some(
                serde_json::from_str::<String>(value)
                    .map_err(|_| "existing MCP TOML command is not parseable".to_string())?,
            );
        } else if let Some(value) = trimmed.strip_prefix("args = ") {
            args = Some(
                serde_json::from_str::<Vec<String>>(value)
                    .map_err(|_| "existing MCP TOML args are not parseable".to_string())?,
            );
        }
    }
    Ok(ManagedStdioEntry {
        command: command.ok_or_else(|| "existing MCP TOML block is missing command".to_string())?,
        args: args.ok_or_else(|| "existing MCP TOML block is missing args".to_string())?,
    })
}

fn parse_hermes_managed_entries(
    existing: &str,
) -> Result<(BTreeMap<String, ManagedStdioEntry>, Option<String>), String> {
    validate_hermes_managed_markers(existing)?;
    let Some((start, end, start_marker, end_marker)) = find_marked_block(existing) else {
        return Ok((BTreeMap::new(), None));
    };
    if end < start {
        return Err("existing Hermes MCP block markers are out of order".to_string());
    }
    let after_end = end + end_marker.len();
    let block = existing[start..after_end].trim_end().to_string();
    let mut entries = BTreeMap::new();
    let mut current_name: Option<String> = None;
    let mut current_command: Option<String> = None;
    let mut current_args: Vec<String> = Vec::new();
    let mut current_args_seen = false;
    let flush = |entries: &mut BTreeMap<String, ManagedStdioEntry>,
                 current_name: &mut Option<String>,
                 current_command: &mut Option<String>,
                 current_args: &mut Vec<String>,
                 current_args_seen: &mut bool|
     -> Result<(), String> {
        if let Some(name) = current_name.take() {
            let command = current_command.take().ok_or_else(|| {
                format!("existing Hermes MCP block is missing command for {name}")
            })?;
            if !*current_args_seen {
                return Err(format!(
                    "existing Hermes MCP block is missing args for {name}"
                ));
            }
            entries.insert(
                name,
                ManagedStdioEntry {
                    command,
                    args: std::mem::take(current_args),
                },
            );
            *current_args_seen = false;
        }
        Ok(())
    };
    for line in block.lines() {
        if line.trim() == "mcp_servers:" {
            continue;
        }
        if line.trim() == start_marker || line.trim() == end_marker {
            continue;
        }
        if line.trim() == "args:" {
            current_args_seen = true;
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("    ") {
            if let Some(name) = line
                .strip_prefix("  ")
                .and_then(|value| value.strip_suffix(':'))
            {
                flush(
                    &mut entries,
                    &mut current_name,
                    &mut current_command,
                    &mut current_args,
                    &mut current_args_seen,
                )?;
                current_name = Some(name.to_string());
                continue;
            }
        }
        if let Some(value) = line.trim().strip_prefix("command: ") {
            current_command = Some(
                serde_json::from_str::<String>(value)
                    .map_err(|_| "existing Hermes MCP command is not parseable".to_string())?,
            );
            continue;
        }
        if let Some(value) = line.trim().strip_prefix("- ") {
            current_args.push(
                serde_json::from_str::<String>(value)
                    .map_err(|_| "existing Hermes MCP arg is not parseable".to_string())?,
            );
        }
    }
    flush(
        &mut entries,
        &mut current_name,
        &mut current_command,
        &mut current_args,
        &mut current_args_seen,
    )?;
    Ok((entries, Some(block)))
}

fn hermes_yaml_block_from_entries(entries: &BTreeMap<String, ManagedStdioEntry>) -> String {
    let mut lines = vec![
        "# codebaseGraph MCP servers start".to_string(),
        "mcp_servers:".to_string(),
    ];
    for (name, entry) in entries {
        lines.push(format!("  {name}:"));
        lines.push("    type: stdio".to_string());
        lines.push(format!("    command: {}", yaml_scalar(&entry.command)));
        lines.push("    args:".to_string());
        for arg in &entry.args {
            lines.push(format!("      - {}", yaml_scalar(arg)));
        }
    }
    lines.push("# codebaseGraph MCP servers end".to_string());
    lines.join("\n") + "\n"
}

fn json_container_mut<'a>(
    payload: &'a mut serde_json::Value,
    path: &[&str],
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>, String> {
    let mut cursor = payload
        .as_object_mut()
        .ok_or_else(|| "MCP config must contain a JSON object".to_string())?;
    for key in path {
        let next = cursor
            .entry((*key).to_string())
            .or_insert_with(|| json!({}));
        cursor = next
            .as_object_mut()
            .ok_or_else(|| format!("MCP config key must contain an object: {}", path.join(".")))?;
    }
    Ok(cursor)
}

fn json_container<'a>(
    payload: &'a serde_json::Value,
    path: &[&str],
) -> Result<Option<&'a serde_json::Map<String, serde_json::Value>>, String> {
    let Some(mut cursor) = payload.as_object() else {
        return Err("MCP config must contain a JSON object".to_string());
    };
    for key in path {
        let Some(next) = cursor.get(*key) else {
            return Ok(None);
        };
        cursor = next
            .as_object()
            .ok_or_else(|| format!("MCP config key must contain an object: {}", path.join(".")))?;
    }
    Ok(Some(cursor))
}

fn action_for_json(
    previous: Option<&serde_json::Value>,
    next_value: &serde_json::Value,
    file_exists: bool,
) -> String {
    if !file_exists {
        "created".to_string()
    } else if previous == Some(next_value) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    }
}

fn copilot_studio_metadata(descriptor: &McpServerDescriptor) -> serde_json::Value {
    let mut metadata = json!({
        "kind": "copilot_studio_manual_metadata",
        "stdio": descriptor.stdio_entry(true, false),
        "notes": [
            "No local client configuration file is written for Copilot Studio.",
            "Remote Copilot Studio use requires user-managed endpoint exposure, bearer-token configuration, and TLS.",
        ],
    });
    if let Some(http) = &descriptor.manual_http_metadata {
        metadata["http"] = http.clone();
    }
    metadata
}

fn codex_toml_block(descriptor: &McpServerDescriptor) -> String {
    format!(
        "[mcp_servers.{}]\ncommand = {}\nargs = {}\nstartup_timeout_sec = {}\n",
        descriptor.name,
        toml_string(&descriptor.command),
        toml_array(&descriptor.args),
        descriptor.timeout
    )
}

fn toml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn upsert_toml_block(existing: &str, server_name: &str, block: &str) -> (String, Option<String>) {
    let lines = existing.lines().collect::<Vec<_>>();
    let header = format!("[mcp_servers.{server_name}]");
    let env_header = format!("[mcp_servers.{server_name}.env]");
    let start = lines
        .iter()
        .position(|line| line.trim() == header || line.trim() == env_header);
    let Some(start) = start else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[')
                && trimmed.ends_with(']')
                && trimmed != header
                && trimmed != env_header
        })
        .map(|index| start + 1 + index)
        .unwrap_or(lines.len());
    let previous = lines[start..end].join("\n").trim_end().to_string();
    let mut next_lines = Vec::new();
    next_lines.extend(lines[..start].iter().map(|value| (*value).to_string()));
    next_lines.extend(block.trim_end().lines().map(str::to_string));
    next_lines.extend(lines[end..].iter().map(|value| (*value).to_string()));
    (
        next_lines.join("\n").trim_end().to_string() + "\n",
        Some(previous),
    )
}

fn remove_toml_block(existing: &str, server_name: &str) -> (String, Option<String>) {
    let lines = existing.lines().collect::<Vec<_>>();
    let header = format!("[mcp_servers.{server_name}]");
    let env_header = format!("[mcp_servers.{server_name}.env]");
    let start = lines
        .iter()
        .position(|line| line.trim() == header || line.trim() == env_header);
    let Some(start) = start else {
        return (existing.to_string(), None);
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[')
                && trimmed.ends_with(']')
                && trimmed != header
                && trimmed != env_header
        })
        .map(|index| start + 1 + index)
        .unwrap_or(lines.len());
    let previous = lines[start..end].join("\n").trim_end().to_string();
    let mut next_lines = Vec::new();
    next_lines.extend(lines[..start].iter().map(|value| (*value).to_string()));
    next_lines.extend(lines[end..].iter().map(|value| (*value).to_string()));
    let text = next_lines.join("\n").trim().to_string();
    let text = if text.is_empty() {
        String::new()
    } else {
        text + "\n"
    };
    (text, Some(previous))
}

fn yaml_scalar(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn validate_hermes_managed_markers(existing: &str) -> Result<(), String> {
    let mut complete_blocks = 0;
    for (start_marker, end_marker) in [
        (
            "# codebaseGraph MCP servers start",
            "# codebaseGraph MCP servers end",
        ),
        (
            "# codebaseGraph MCP server start",
            "# codebaseGraph MCP server end",
        ),
    ] {
        match (existing.find(start_marker), existing.find(end_marker)) {
            (None, None) => {}
            (Some(start), Some(end)) if end >= start => complete_blocks += 1,
            _ => {
                return Err(
                    "existing Hermes MCP managed block has missing or out-of-order markers"
                        .to_string(),
                );
            }
        }
    }
    if complete_blocks > 1 {
        return Err("existing Hermes config contains multiple managed MCP blocks".to_string());
    }
    Ok(())
}

fn find_marked_block(existing: &str) -> Option<(usize, usize, &'static str, &'static str)> {
    [
        (
            "# codebaseGraph MCP servers start",
            "# codebaseGraph MCP servers end",
        ),
        (
            "# codebaseGraph MCP server start",
            "# codebaseGraph MCP server end",
        ),
    ]
    .into_iter()
    .find_map(|(start_marker, end_marker)| {
        let start = existing.find(start_marker)?;
        let end = existing.find(end_marker)?;
        Some((start, end, start_marker, end_marker))
    })
}

fn upsert_marked_block(existing: &str, block: &str) -> (String, Option<String>) {
    let Some((start, end, _start_marker, end_marker)) = find_marked_block(existing) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    if end < start {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    }
    let after_end = end + end_marker.len();
    let previous = existing[start..after_end].trim_end().to_string();
    let text = format!(
        "{}\n\n{}\n\n{}",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start()
    )
    .trim()
    .to_string()
        + "\n";
    (text, Some(previous))
}

fn remove_marked_block(existing: &str) -> (String, Option<String>) {
    let Some((start, end, _start_marker, end_marker)) = find_marked_block(existing) else {
        return (existing.to_string(), None);
    };
    if end < start {
        return (existing.to_string(), None);
    }
    let after_end = end + end_marker.len();
    let previous = existing[start..after_end].trim_end().to_string();
    let before = existing[..start].trim_end();
    let after = existing[after_end..].trim_start();
    let text = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (true, false) => format!("{after}\n"),
        (false, true) => format!("{before}\n"),
        (false, false) => format!("{before}\n\n{after}"),
    };
    (text, Some(previous))
}

fn read_optional_text(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to read MCP client config {}: {error}",
            path.display()
        )),
    }
}

fn write_text_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    ));
    fs::write(&tmp_path, text).map_err(|error| {
        format!(
            "failed to write temporary config {}: {error}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path)
        .map_err(|error| format!("failed to replace config {}: {error}", path.display()))
}

pub(crate) fn expand_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn executable_in_path(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| dir.join(executable).is_file()))
        .unwrap_or(false)
}

fn subprocess_error(completed: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&completed.stdout)
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(&completed.stderr)
        .trim()
        .to_string();
    let output = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let code = completed.status.code().unwrap_or(1);
    if output.is_empty() {
        format!("exit {code}")
    } else {
        format!("exit {code}: {output}")
    }
}

fn install_safe_name(value: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    normalized.trim_matches(['.', '_', '-']).to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        codex_toml_block, descriptor_signature, hermes_yaml_block_from_entries, inspect_mcp_server,
        install_mcp_endpoint, install_mcp_server, instruction_block, native_client_command,
        parse_hermes_managed_entries, parse_toml_stdio_entry, reinstall_state, remove_mcp_server,
        remove_partial_state_tree, rename_mcp_server, render_client_http_config,
        resolve_mcp_target, run_reinstall_activation_boundary, select_available_daemon_port,
        upsert_instruction_text, yaml_scalar, GraphStatePaths, ManagedStdioEntry,
        McpClientInstallOptions, McpClientRemovalOptions, McpClientRenameOptions,
        McpEndpointDescriptor, McpExistingEntryPolicy, McpInstallMode, McpServerDescriptor,
        McpTargetLocality, ResolvedMcpTarget,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "codebasegraph-lifecycle-{label}-{}-{unique}",
                process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn join(&self, child: &str) -> PathBuf {
            self.path.join(child)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_descriptor(repo_root: &Path, name: &str) -> McpServerDescriptor {
        McpServerDescriptor {
            name: name.to_string(),
            command: "codebase-graph".to_string(),
            args: vec![
                "mcp".to_string(),
                "start".to_string(),
                "--config".to_string(),
                repo_root
                    .join(".codebaseGraph/config.json")
                    .display()
                    .to_string(),
            ],
            repo_root: repo_root.to_path_buf(),
            timeout: 60,
            setup_config_path: Some(repo_root.join(".codebaseGraph/config.json")),
            tool_policy: Some("graph_query_read_only".to_string()),
            manual_http_metadata: None,
        }
    }

    #[test]
    fn managed_instruction_block_replaces_stale_guidance_and_preserves_surrounding_text() {
        let existing = "before\n\n<!-- codebaseGraph:start -->\nold semantic-only guidance\n<!-- codebaseGraph:end -->\n\nafter\n";
        let (updated, action) = upsert_instruction_text(
            existing,
            &instruction_block(Path::new("/tmp/repository/.codebaseGraph/config.json")),
            false,
        );

        assert_eq!(action, "updated");
        assert!(updated.contains("before"));
        assert!(updated.contains("after"));
        assert!(!updated.contains("old semantic-only guidance"));
        for expected in [
            "graph_syntax",
            "layer: \"semantic\"",
            "layer: \"syntax\"",
            "layer: \"hybrid\"",
            "node_type: \"SyntaxCapture\"",
            "field_name",
            "child_index",
            "syntax <language>",
            "--layer syntax",
            "--layer hybrid",
            "--node-type SyntaxCapture",
        ] {
            assert!(updated.contains(expected), "missing {expected}");
        }
    }

    fn install_options(
        client: &str,
        scope: &str,
        client_config_path: PathBuf,
    ) -> McpClientInstallOptions {
        McpClientInstallOptions {
            client: client.to_string(),
            scope: scope.to_string(),
            client_config_path: Some(client_config_path),
            dry_run: false,
            install_method: McpInstallMode::FileAdapter,
            existing_entry_policy: McpExistingEntryPolicy::Replace,
            legacy_server_names: Vec::new(),
        }
    }

    fn http_endpoint() -> McpEndpointDescriptor {
        McpEndpointDescriptor::StreamableHttp {
            url: "http://127.0.0.1:43123/mcp".to_string(),
            headers: BTreeMap::new(),
        }
    }

    #[test]
    fn http_registration_shapes_match_every_loopback_harness() {
        let repo = TestDir::new("http-registration-shapes");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let endpoint = http_endpoint();
        let cases = [
            (
                "codex",
                json!({"url": "http://127.0.0.1:43123/mcp", "http_headers": {}, "startup_timeout_sec": 60}),
            ),
            (
                "claude",
                json!({"type": "http", "url": "http://127.0.0.1:43123/mcp"}),
            ),
            (
                "claude-project",
                json!({"type": "http", "url": "http://127.0.0.1:43123/mcp"}),
            ),
            (
                "github-copilot",
                json!({"type": "http", "url": "http://127.0.0.1:43123/mcp"}),
            ),
            ("lmstudio", json!({"url": "http://127.0.0.1:43123/mcp"})),
            ("hermes", json!({"url": "http://127.0.0.1:43123/mcp"})),
            (
                "openclaw",
                json!({"transport": "streamable-http", "url": "http://127.0.0.1:43123/mcp"}),
            ),
            (
                "generic",
                json!({"type": "http", "url": "http://127.0.0.1:43123/mcp"}),
            ),
        ];
        for (adapter, expected) in cases {
            let rendered =
                render_client_http_config(adapter, None, &descriptor, &endpoint, &[]).unwrap();
            assert_eq!(rendered.entry, expected, "{adapter}");
            assert!(!rendered.text.contains("command"), "{adapter}");
            assert!(rendered.text.contains("http://127.0.0.1:43123/mcp"));
        }
    }

    #[test]
    fn daemon_port_selection_migrates_collisions_but_rejects_unavailable_overrides() {
        let selected = select_available_daemon_port(43_123, None, |candidate| {
            if candidate < 43_126 {
                Err("occupied".to_string())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(selected, 43_126);

        let error =
            select_available_daemon_port(43_123, Some(43_123), |_| Err("occupied".to_string()))
                .unwrap_err();
        assert!(error.contains("requested MCP daemon port 43123 is unavailable"));
    }

    #[test]
    fn http_registration_migrates_only_the_recognized_stdio_entry() {
        let repo = TestDir::new("http-registration-migration");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let endpoint = http_endpoint();
        let existing = serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "codebase_graph_repo": descriptor.stdio_entry(false, false),
                "unrelated": {"command": "keep", "args": []}
            }
        }))
        .unwrap();
        let migrated =
            render_client_http_config("generic", Some(&existing), &descriptor, &endpoint, &[])
                .unwrap();
        assert_eq!(migrated.action, "updated");
        assert!(migrated.text.contains("\"unrelated\""));
        assert!(migrated.entry.get("command").is_none());

        let custom = existing.replace("codebase-graph", "custom-server");
        let error =
            render_client_http_config("generic", Some(&custom), &descriptor, &endpoint, &[])
                .unwrap_err();
        assert!(error.contains("not the recognized managed stdio entry"));
    }

    #[test]
    fn cloud_targets_require_public_https_without_exposing_loopback() {
        let repo = TestDir::new("http-manual-cloud");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        for client in ["copilot-studio", "microsoft-copilot"] {
            let result = install_mcp_endpoint(
                &descriptor,
                &http_endpoint(),
                &McpClientInstallOptions {
                    client: client.to_string(),
                    scope: "user".to_string(),
                    client_config_path: None,
                    dry_run: false,
                    install_method: McpInstallMode::FileAdapter,
                    existing_entry_policy: McpExistingEntryPolicy::Replace,
                    legacy_server_names: Vec::new(),
                },
            )
            .unwrap();
            assert_eq!(result["action"], "manual_remote_required");
            assert_eq!(result["payload"]["public_https_required"], true);
            assert!(!result["payload"].to_string().contains("127.0.0.1"));
        }
    }

    #[test]
    fn claude_desktop_path_is_rejected_for_loopback_http() {
        let repo = TestDir::new("claude-desktop-http");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let error = install_mcp_endpoint(
            &descriptor,
            &http_endpoint(),
            &install_options("claude", "user", repo.join("claude_desktop_config.json")),
        )
        .unwrap_err();
        assert!(error.contains("Claude Desktop"));
    }

    #[test]
    fn http_registration_can_be_inspected_renamed_and_removed() {
        let repo = TestDir::new("http-registration-lifecycle");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let config_path = repo.join("mcp.json");
        install_mcp_endpoint(
            &descriptor,
            &http_endpoint(),
            &install_options("generic", "project", config_path.clone()),
        )
        .unwrap();
        let target = ResolvedMcpTarget {
            client: "generic".to_string(),
            scope: "project".to_string(),
            locality: McpTargetLocality::RepositoryLocal,
            path: Some(config_path.clone()),
        };
        let inspected = inspect_mcp_server(&descriptor.name, &target)
            .unwrap()
            .unwrap();
        assert_eq!(inspected.transport, "streamable_http");
        assert_eq!(inspected.url.as_deref(), Some("http://127.0.0.1:43123/mcp"));
        assert!(inspected.command.is_empty());

        let renamed = rename_mcp_server(
            &descriptor.name,
            "codebase_graph_renamed",
            &McpClientRenameOptions {
                target: target.clone(),
                dry_run: false,
            },
        )
        .unwrap();
        assert_eq!(renamed["action"], "renamed");
        assert!(inspect_mcp_server(&descriptor.name, &target)
            .unwrap()
            .is_none());
        assert!(inspect_mcp_server("codebase_graph_renamed", &target)
            .unwrap()
            .is_some());

        let removed = remove_mcp_server(
            "codebase_graph_renamed",
            &McpClientRemovalOptions {
                target: target.clone(),
                dry_run: false,
            },
        )
        .unwrap();
        assert_eq!(removed["action"], "removed");
        assert!(inspect_mcp_server("codebase_graph_renamed", &target)
            .unwrap()
            .is_none());
    }

    fn hermes_block(name: &str, command: &str, args: &[&str]) -> String {
        let mut lines = vec![
            "# codebaseGraph MCP servers start".to_string(),
            "mcp_servers:".to_string(),
            format!("  {name}:"),
            "    type: stdio".to_string(),
            format!("    command: {}", yaml_scalar(command)),
            "    args:".to_string(),
        ];
        for arg in args {
            lines.push(format!("      - {}", yaml_scalar(arg)));
        }
        lines.push("# codebaseGraph MCP servers end".to_string());
        lines.join("\n") + "\n"
    }

    #[test]
    fn codex_toml_block_round_trips_windows_style_arguments() {
        let descriptor = McpServerDescriptor {
            name: "codebase_graph_repo".to_string(),
            command: r"C:\Program Files\codebase-graph.exe".to_string(),
            args: vec![
                "--config".to_string(),
                r"C:\workspace\.codebaseGraph\config.json".to_string(),
                "value \"quoted\"".to_string(),
            ],
            repo_root: PathBuf::from(r"C:\workspace"),
            timeout: 60,
            setup_config_path: Some(PathBuf::from(r"C:\workspace\.codebaseGraph\config.json")),
            tool_policy: Some("graph_query_read_only".to_string()),
            manual_http_metadata: None,
        };

        let block = codex_toml_block(&descriptor);
        let parsed = parse_toml_stdio_entry(&block).unwrap();

        assert_eq!(parsed, descriptor_signature(&descriptor));
    }

    #[test]
    fn hermes_managed_block_round_trips_windows_style_arguments() {
        let entry = ManagedStdioEntry {
            command: r"C:\Program Files\codebase-graph.exe".to_string(),
            args: vec![
                "--config".to_string(),
                r"C:\workspace\.codebaseGraph\config.json".to_string(),
                "value \"quoted\"".to_string(),
            ],
        };
        let entries = BTreeMap::from([("codebase_graph_repo".to_string(), entry)]);

        let block = hermes_yaml_block_from_entries(&entries);
        let (parsed, previous) = parse_hermes_managed_entries(&block).unwrap();

        assert_eq!(parsed, entries);
        assert_eq!(previous.as_deref(), Some(block.trim_end()));
    }

    #[test]
    fn native_client_commands_preserve_every_server_argument() {
        let descriptor = McpServerDescriptor {
            name: "k_wiki_repository".to_string(),
            command: "k-wiki".to_string(),
            args: vec!["mcp".to_string(), "/workspace/knowledge".to_string()],
            repo_root: PathBuf::from("/workspace"),
            timeout: 60,
            setup_config_path: None,
            tool_policy: None,
            manual_http_metadata: None,
        };

        assert_eq!(
            native_client_command("codex", &descriptor, "local"),
            Some(vec![
                "codex".to_string(),
                "mcp".to_string(),
                "add".to_string(),
                "k_wiki_repository".to_string(),
                "--".to_string(),
                "k-wiki".to_string(),
                "mcp".to_string(),
                "/workspace/knowledge".to_string(),
            ])
        );
    }

    #[test]
    fn resolve_mcp_target_uses_expected_default_locality_and_paths() {
        let repo = TestDir::new("targets-default");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let claude_user_path = home.join(".claude.json");
        let openclaw_home = std::env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"));
        let codex_home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        let mut cases = vec![
            (
                "claude",
                "project",
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".mcp.json")),
            ),
            (
                "claude",
                "user",
                McpTargetLocality::Shared,
                Some(claude_user_path.clone()),
            ),
            (
                "claude-project",
                "local",
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".mcp.json")),
            ),
            (
                "codex",
                "local",
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".codex/config.toml")),
            ),
            (
                "codex",
                "user",
                McpTargetLocality::Shared,
                Some(codex_home.join("config.toml")),
            ),
            (
                "generic",
                "project",
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".mcp.json")),
            ),
            (
                "generic",
                "user",
                McpTargetLocality::Shared,
                Some(home.join(".config/mcp/mcp.json")),
            ),
            (
                "github-copilot",
                "local",
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".vscode/mcp.json")),
            ),
            (
                "hermes",
                "user",
                McpTargetLocality::Shared,
                Some(home.join(".hermes/config.yaml")),
            ),
            (
                "lmstudio",
                "user",
                McpTargetLocality::Shared,
                Some(home.join(".lmstudio/mcp.json")),
            ),
            (
                "openclaw",
                "user",
                McpTargetLocality::Shared,
                Some(openclaw_home.join("mcp.json5")),
            ),
            ("copilot-studio", "user", McpTargetLocality::Manual, None),
            ("microsoft-copilot", "user", McpTargetLocality::Manual, None),
        ];
        cases.extend([
            (
                "claude",
                "local",
                McpTargetLocality::Shared,
                Some(claude_user_path.clone()),
            ),
            (
                "codex",
                "project",
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".codex/config.toml")),
            ),
            (
                "generic",
                "local",
                McpTargetLocality::Shared,
                Some(home.join(".config/mcp/mcp.json")),
            ),
        ]);
        for scope in ["user", "project"] {
            cases.push((
                "claude-project",
                scope,
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".mcp.json")),
            ));
            cases.push((
                "github-copilot",
                scope,
                McpTargetLocality::RepositoryLocal,
                Some(repo.join(".vscode/mcp.json")),
            ));
        }
        for scope in ["local", "project"] {
            cases.push((
                "hermes",
                scope,
                McpTargetLocality::Shared,
                Some(home.join(".hermes/config.yaml")),
            ));
            cases.push((
                "lmstudio",
                scope,
                McpTargetLocality::Shared,
                Some(home.join(".lmstudio/mcp.json")),
            ));
            cases.push((
                "openclaw",
                scope,
                McpTargetLocality::Shared,
                Some(openclaw_home.join("mcp.json5")),
            ));
            cases.push(("copilot-studio", scope, McpTargetLocality::Manual, None));
            cases.push(("microsoft-copilot", scope, McpTargetLocality::Manual, None));
        }

        for (client, scope, expected_locality, expected_path) in cases {
            let resolved = resolve_mcp_target(client, scope, &descriptor, None).unwrap();
            assert_eq!(resolved.locality, expected_locality, "{client}:{scope}");
            assert_eq!(resolved.path, expected_path, "{client}:{scope}");
        }
    }

    #[test]
    fn resolve_mcp_target_treats_explicit_paths_inside_repo_as_local_and_outside_as_shared() {
        let repo = TestDir::new("targets-explicit");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let inside = repo.join("configs/inside.json");
        let outside_root = TestDir::new("targets-explicit-outside");
        let outside = outside_root.join("outside.json");
        let escaping = repo.join("missing/../../escaped.json");

        let inside_target =
            resolve_mcp_target("generic", "user", &descriptor, Some(inside.clone())).unwrap();
        let outside_target =
            resolve_mcp_target("generic", "user", &descriptor, Some(outside.clone())).unwrap();
        let escaping_target =
            resolve_mcp_target("generic", "user", &descriptor, Some(escaping)).unwrap();

        assert_eq!(inside_target.locality, McpTargetLocality::RepositoryLocal);
        assert_eq!(inside_target.path, Some(inside));
        assert_eq!(outside_target.locality, McpTargetLocality::Shared);
        assert_eq!(outside_target.path, Some(outside));
        assert_eq!(escaping_target.locality, McpTargetLocality::Shared);
    }

    #[test]
    fn resolve_mcp_target_rejects_aggregate_unknown_and_invalid_scope_targets() {
        let repo = TestDir::new("targets-invalid");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");

        assert!(resolve_mcp_target("all", "local", &descriptor, None).is_err());
        assert!(resolve_mcp_target("unknown", "local", &descriptor, None).is_err());
        assert!(resolve_mcp_target("codex", "workspace", &descriptor, None).is_err());
    }

    #[test]
    fn json_reject_different_leaves_existing_file_byte_identical() {
        let repo = TestDir::new("json-conflict");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let config_path = repo.join("generic.json");
        let original = serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "codebase_graph_repo": {
                    "command": "other-binary",
                    "args": ["serve"]
                }
            }
        }))
        .unwrap()
            + "\n";
        fs::write(&config_path, &original).unwrap();
        let mut options = install_options("generic", "user", config_path.clone());
        options.existing_entry_policy = McpExistingEntryPolicy::RejectDifferent;

        let error = install_mcp_server(&descriptor, &options).unwrap_err();

        assert!(error.contains("refusing to overwrite existing MCP server"));
        assert_eq!(fs::read(&config_path).unwrap(), original.into_bytes());
    }

    #[test]
    fn registration_rename_preserves_codex_entry_and_rejects_conflicting_destination() {
        let repo = TestDir::new("rename-codex");
        let config_path = repo.join("config.toml");
        fs::write(
            &config_path,
            "model = \"example\"\n\n[mcp_servers.k_wiki]\ncommand = \"k-wiki\"\nargs = [\"mcp\", \"/repo/knowledge\"]\nstartup_timeout_sec = 60\n",
        )
        .unwrap();
        let target = ResolvedMcpTarget {
            client: "codex".to_string(),
            scope: "user".to_string(),
            locality: McpTargetLocality::Shared,
            path: Some(config_path.clone()),
        };
        let inspected = inspect_mcp_server("k_wiki", &target).unwrap().unwrap();
        assert_eq!(inspected.command, "k-wiki");
        assert_eq!(inspected.args, vec!["mcp", "/repo/knowledge"]);
        let renamed = rename_mcp_server(
            "k_wiki",
            "k_wiki_repo_deadbeef",
            &McpClientRenameOptions {
                target: target.clone(),
                dry_run: false,
            },
        )
        .unwrap();
        assert_eq!(renamed["action"], "renamed");
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("model = \"example\""));
        assert!(text.contains("[mcp_servers.k_wiki_repo_deadbeef]"));
        assert!(!text.contains("[mcp_servers.k_wiki]"));

        let conflict = format!(
            "{text}\n[mcp_servers.k_wiki]\ncommand = \"other\"\nargs = [\"mcp\", \"/other/knowledge\"]\n"
        );
        fs::write(&config_path, &conflict).unwrap();
        let error = rename_mcp_server(
            "k_wiki",
            "k_wiki_repo_deadbeef",
            &McpClientRenameOptions {
                target,
                dry_run: false,
            },
        )
        .unwrap_err();
        assert!(error.contains("different command or args"));
        assert_eq!(fs::read_to_string(config_path).unwrap(), conflict);
    }

    #[test]
    fn registration_rename_deduplicates_matching_json_destination_atomically() {
        let repo = TestDir::new("rename-json");
        let config_path = repo.join("mcp.json");
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&json!({
                "unrelated": true,
                "mcpServers": {
                    "k_wiki": {"command": "k-wiki", "args": ["mcp", "/repo/knowledge"]},
                    "k_wiki_repo_deadbeef": {"command": "k-wiki", "args": ["mcp", "/repo/knowledge"]}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let target = ResolvedMcpTarget {
            client: "generic".to_string(),
            scope: "user".to_string(),
            locality: McpTargetLocality::Shared,
            path: Some(config_path.clone()),
        };
        let renamed = rename_mcp_server(
            "k_wiki",
            "k_wiki_repo_deadbeef",
            &McpClientRenameOptions {
                target,
                dry_run: false,
            },
        )
        .unwrap();
        assert_eq!(renamed["action"], "deduplicated");
        let payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert_eq!(payload["unrelated"], true);
        assert!(payload["mcpServers"]["k_wiki"].is_null());
        assert_eq!(
            payload["mcpServers"]["k_wiki_repo_deadbeef"]["command"],
            "k-wiki"
        );
    }

    #[test]
    fn codex_toml_reject_different_leaves_existing_file_byte_identical() {
        let repo = TestDir::new("codex-conflict");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let config_path = repo.join("config.toml");
        let original = concat!(
            "[mcp_servers.codebase_graph_repo]\n",
            "command = \"other-binary\"\n",
            "args = [\"serve\"]\n",
            "startup_timeout_sec = 60\n",
        );
        fs::write(&config_path, original).unwrap();
        let mut options = install_options("codex", "user", config_path.clone());
        options.existing_entry_policy = McpExistingEntryPolicy::RejectDifferent;

        let error = install_mcp_server(&descriptor, &options).unwrap_err();

        assert!(error.contains("refusing to overwrite existing MCP server"));
        assert_eq!(fs::read(&config_path).unwrap(), original.as_bytes());
    }

    #[test]
    fn hermes_reject_different_leaves_existing_file_byte_identical() {
        let repo = TestDir::new("hermes-conflict");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let config_path = repo.join("config.yaml");
        let original = hermes_block("codebase_graph_repo", "other-binary", &["serve"]);
        fs::write(&config_path, &original).unwrap();
        let mut options = install_options("hermes", "user", config_path.clone());
        options.existing_entry_policy = McpExistingEntryPolicy::RejectDifferent;

        let error = install_mcp_server(&descriptor, &options).unwrap_err();

        assert!(error.contains("refusing to overwrite existing MCP server"));
        assert_eq!(fs::read(&config_path).unwrap(), original.into_bytes());
    }

    #[test]
    fn matching_entries_remain_idempotent_across_json_codex_and_hermes_configs() {
        let repo = TestDir::new("idempotent");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let json_path = repo.join("generic.json");
        let codex_path = repo.join("config.toml");
        let hermes_path = repo.join("config.yaml");

        let json_text = serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "codebase_graph_repo": descriptor.stdio_entry(true, false)
            }
        }))
        .unwrap()
            + "\n";
        let codex_text = codex_toml_block(&descriptor);
        let hermes_text = hermes_block(
            "codebase_graph_repo",
            &descriptor.command,
            &[
                descriptor.args[0].as_str(),
                descriptor.args[1].as_str(),
                descriptor.args[2].as_str(),
                descriptor.args[3].as_str(),
            ],
        );
        fs::write(&json_path, &json_text).unwrap();
        fs::write(&codex_path, &codex_text).unwrap();
        fs::write(&hermes_path, &hermes_text).unwrap();

        let json_result = install_mcp_server(
            &descriptor,
            &install_options("generic", "user", json_path.clone()),
        )
        .unwrap();
        let codex_result = install_mcp_server(
            &descriptor,
            &install_options("codex", "user", codex_path.clone()),
        )
        .unwrap();
        let hermes_result = install_mcp_server(
            &descriptor,
            &install_options("hermes", "user", hermes_path.clone()),
        )
        .unwrap();

        assert_eq!(json_result["action"], "unchanged");
        assert_eq!(codex_result["action"], "unchanged");
        assert_eq!(hermes_result["action"], "unchanged");
        assert_eq!(fs::read(&json_path).unwrap(), json_text.into_bytes());
        assert_eq!(fs::read(&codex_path).unwrap(), codex_text.into_bytes());
        assert_eq!(fs::read(&hermes_path).unwrap(), hermes_text.into_bytes());
    }

    #[test]
    fn install_removes_legacy_k_wiki_entry_in_the_same_file_update() {
        let repo = TestDir::new("legacy-cleanup");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let config_path = repo.join("generic.json");
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "k_wiki_repository": {
                        "command": "k-wiki",
                        "args": ["mcp", "/workspace/knowledge"]
                    },
                    "unrelated": {
                        "command": "keep",
                        "args": ["still-here"]
                    }
                }
            }))
            .unwrap()
                + "\n",
        )
        .unwrap();
        let mut options = install_options("generic", "user", config_path.clone());
        options.legacy_server_names = vec!["k_wiki_repository".to_string()];

        let payload = install_mcp_server(&descriptor, &options).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();

        assert_eq!(payload["action"], "updated");
        assert_eq!(payload["legacy_cleanup"]["action"], "removed");
        assert_eq!(
            payload["legacy_cleanup"]["removed"],
            json!(["k_wiki_repository"])
        );
        assert!(saved["mcpServers"].get("k_wiki_repository").is_none());
        assert_eq!(
            saved["mcpServers"]["codebase_graph_repo"]["command"],
            descriptor.command
        );
        assert_eq!(saved["mcpServers"]["unrelated"]["args"][0], "still-here");
    }

    #[test]
    fn hermes_install_migrates_singular_marker_blocks_without_dropping_existing_entries() {
        let repo = TestDir::new("hermes-migrate");
        let descriptor = test_descriptor(repo.path(), "codebase_graph_repo");
        let config_path = repo.join("config.yaml");
        let original = concat!(
            "before\n\n",
            "# codebaseGraph MCP server start\n",
            "mcp_servers:\n",
            "  k_wiki_repository:\n",
            "    type: stdio\n",
            "    command: \"k-wiki\"\n",
            "    args:\n",
            "      - \"mcp\"\n",
            "      - \"/workspace/knowledge\"\n",
            "# codebaseGraph MCP server end\n\n",
            "after\n"
        );
        fs::write(&config_path, original).unwrap();

        let payload = install_mcp_server(
            &descriptor,
            &install_options("hermes", "user", config_path.clone()),
        )
        .unwrap();
        let updated = fs::read_to_string(&config_path).unwrap();

        assert_eq!(payload["action"], "updated");
        assert!(updated.contains("# codebaseGraph MCP servers start"));
        assert!(updated.contains("# codebaseGraph MCP servers end"));
        assert!(!updated.contains("# codebaseGraph MCP server start"));
        assert!(!updated.contains("# codebaseGraph MCP server end"));
        assert!(updated.contains("  k_wiki_repository:"));
        assert!(updated.contains("  codebase_graph_repo:"));
        assert!(updated.contains("before"));
        assert!(updated.contains("after"));
    }

    #[test]
    fn remove_mcp_server_dry_run_reports_previous_without_writing_and_write_removes_entry() {
        let repo = TestDir::new("remove");
        let config_path = repo.join("generic.json");
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "codebase_graph_repo": {
                        "command": "codebase-graph",
                        "args": ["mcp", "start"]
                    },
                    "unrelated": {
                        "command": "keep",
                        "args": ["present"]
                    }
                }
            }))
            .unwrap()
                + "\n",
        )
        .unwrap();
        let target = ResolvedMcpTarget {
            client: "generic".to_string(),
            scope: "project".to_string(),
            locality: McpTargetLocality::RepositoryLocal,
            path: Some(config_path.clone()),
        };
        let before = fs::read(&config_path).unwrap();

        let dry_run = remove_mcp_server(
            "codebase_graph_repo",
            &McpClientRemovalOptions {
                target: target.clone(),
                dry_run: true,
            },
        )
        .unwrap();
        let after_dry_run = fs::read(&config_path).unwrap();
        let written = remove_mcp_server(
            "codebase_graph_repo",
            &McpClientRemovalOptions {
                target,
                dry_run: false,
            },
        )
        .unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();

        assert_eq!(dry_run["action"], "dry_run");
        assert_eq!(dry_run["previous"]["command"], "codebase-graph");
        assert_eq!(before, after_dry_run);
        assert_eq!(written["action"], "removed");
        assert!(saved["mcpServers"].get("codebase_graph_repo").is_none());
        assert_eq!(saved["mcpServers"]["unrelated"]["args"][0], "present");
    }

    #[test]
    fn reinstall_activation_boundary_restores_backup_on_activation_failure() {
        let repo = TestDir::new("reinstall-restore-on-activation-failure");
        let repo_root = repo.path().to_path_buf();
        let state_dir = repo_root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("legacy.txt"), "legacy").unwrap();
        let paths = GraphStatePaths::derive(&repo_root);
        let state = reinstall_state(&repo_root, &paths, false).unwrap();
        let backup_path = state.backup_path.clone().unwrap();

        let error = run_reinstall_activation_boundary(
            &repo_root,
            &paths,
            state.backup_path.as_deref(),
            || Err("activation failed".to_string()),
            Ok,
        )
        .unwrap_err();

        assert_eq!(error, "activation failed");
        assert!(state_dir.exists());
        assert_eq!(
            fs::read_to_string(state_dir.join("legacy.txt")).unwrap(),
            "legacy"
        );
        assert!(!backup_path.exists());
    }

    #[test]
    fn reinstall_activation_boundary_keeps_new_state_on_post_activation_failure() {
        let repo = TestDir::new("reinstall-post-activation-failure");
        let repo_root = repo.path().to_path_buf();
        let state_dir = repo_root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("legacy.txt"), "legacy").unwrap();
        let paths = GraphStatePaths::derive(&repo_root);
        let state = reinstall_state(&repo_root, &paths, false).unwrap();
        let backup_path = state.backup_path.clone().unwrap();

        let error = run_reinstall_activation_boundary(
            &repo_root,
            &paths,
            state.backup_path.as_deref(),
            || {
                fs::create_dir_all(&state_dir).unwrap();
                fs::write(state_dir.join("new.txt"), "new").unwrap();
                Ok(json!({"ok": true}))
            },
            |payload| {
                assert!(!backup_path.exists());
                assert_eq!(
                    fs::read_to_string(state_dir.join("new.txt")).unwrap(),
                    "new"
                );
                Err(format!("post activation failed: {payload}"))
            },
        )
        .unwrap_err();

        assert!(error.starts_with("post activation failed:"));
        assert!(state_dir.exists());
        assert_eq!(
            fs::read_to_string(state_dir.join("new.txt")).unwrap(),
            "new"
        );
        assert!(!state_dir.join("legacy.txt").exists());
        assert!(!backup_path.exists());
    }

    #[test]
    fn remove_partial_state_tree_rejects_symlinks() {
        #[cfg(unix)]
        {
            let repo = TestDir::new("reinstall-remove-symlink");
            let repo_root = repo.path().to_path_buf();
            let state_dir = repo_root.join(".codebaseGraph");
            let outside = repo.join("outside.txt");
            fs::create_dir_all(&state_dir).unwrap();
            fs::write(&outside, "outside").unwrap();
            std::os::unix::fs::symlink(&outside, state_dir.join("linked.txt")).unwrap();

            let error = remove_partial_state_tree(&repo_root, &state_dir).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            assert!(state_dir.exists());
        }
    }
}
