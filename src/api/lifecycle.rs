use crate::api::context::RepoRuntime;
use crate::api::contracts::{
    ApiError, McpInstallRequest, RefreshRequest, RepositoryLifecycleRequest,
};
use crate::api::materialization::{
    build_request, default_excluded_parts, execute_candidate_materialization,
    execute_materialization, MaterializeOptions,
};
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
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
    let config_path = runtime.config_path.clone();
    let install = |client: &str| {
        let descriptor = build_mcp_descriptor(
            request.name.clone(),
            config_path.clone(),
            Some(runtime.repo_root.clone()),
        )?;
        install_mcp_server(
            &descriptor,
            &McpClientInstallOptions {
                client: client.to_string(),
                scope: request.scope.clone(),
                client_config_path: request.client_config_path.clone(),
                dry_run: request.dry_run,
            },
        )
    };
    if request.client == "all" {
        let results = supported_mcp_clients()
            .iter()
            .copied()
            .map(|client| {
                install(client).unwrap_or_else(|error| {
                    json!({
                        "action": "failed",
                        "client": client,
                        "scope": install_scope(client, &request.scope),
                        "server_name": request.name.clone().unwrap_or_else(|| "codebase_graph".to_string()),
                        "method": serde_json::Value::Null,
                        "path": serde_json::Value::Null,
                        "command": serde_json::Value::Null,
                        "descriptor": {},
                        "entry": {},
                        "error": error,
                    })
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({ "results": results }))
    } else {
        install(&request.client).map_err(|error| ApiError::new("mcp_install_failed", error))
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
        use_git: false,
        mode: request.mode.clone(),
        include_fts: request.include_fts,
        semantic_enrichment: request.semantic_enrichment,
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
    semantic_enrichment: bool,
    semantic_provider_mode: String,
    mcp_client: String,
    mcp_config_path: Option<PathBuf>,
    skip_mcp_config: bool,
    dry_run: bool,
    instructions_target: String,
}

impl LifecycleOptions {
    fn from_request(request: &RepositoryLifecycleRequest) -> Self {
        Self {
            mode: request.mode.clone(),
            include_fts: request.include_fts,
            semantic_enrichment: request.semantic_enrichment,
            semantic_provider_mode: request.semantic_provider_mode.clone(),
            mcp_client: request
                .mcp_client
                .clone()
                .unwrap_or_else(|| "codex".to_string()),
            mcp_config_path: request.mcp_config_path.clone(),
            skip_mcp_config: request.skip_mcp_config,
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
        db: Some(paths.db_path.clone()),
        manifest: Some(paths.manifest_path.clone()),
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: true,
        ..MaterializeOptions::default()
    };
    let config_payload = setup_config_payload(&paths, &source_root);
    let instructions_path = instruction_target_path(&source_root, &options.instructions_target)?;
    let state_dir_existed = paths.state_dir.exists();
    let graph_state_existed =
        paths.config_path.exists() && paths.db_path.exists() && paths.manifest_path.exists();
    let previous_config = snapshot_file(&paths.config_path)?;
    let previous_instructions = match instructions_path.as_ref() {
        Some(path) => Some((path.clone(), snapshot_file(path)?)),
        None => None,
    };

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
            let config_action = write_setup_config(&paths, &source_root)?;
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
                restore_file(&paths.config_path, previous_config.as_deref())?;
                if let Some((path, previous)) = previous_instructions.as_ref() {
                    restore_file(path, previous.as_deref())?;
                }
                if !state_dir_existed {
                    let _ = fs::remove_dir_all(&paths.state_dir);
                }
                return Err(error);
            }
        }
    };

    Ok(json!({
        "ok": true,
        "repo_root": source_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "db_path": paths.db_path,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "config_path": paths.config_path,
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
    let state = reinstall_state(&paths, options.dry_run)?;
    let install = if options.dry_run {
        setup_payload_for_root(&options, &repo_root)?
    } else {
        match setup_payload_for_root(&options, &repo_root) {
            Ok(payload) => {
                remove_backup(state.backup_path.as_deref())?;
                payload
            }
            Err(error) => {
                restore_backup(&paths.state_dir, state.backup_path.as_deref())?;
                return Err(error);
            }
        }
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
    let state = uninstall_state_dir(&paths.state_dir, request.dry_run)?;
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

    install_mcp_server(
        &descriptor,
        &McpClientInstallOptions {
            client: options.mcp_client.clone(),
            scope: if options.mcp_client == "claude-project" {
                "project".to_string()
            } else {
                "local".to_string()
            },
            client_config_path: options.mcp_config_path.clone(),
            dry_run,
        },
    )
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

fn uninstall_state_dir(path: &Path, dry_run: bool) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(json!({"action": "unchanged", "path": path}));
    }
    if !dry_run {
        fs::remove_dir_all(path).map_err(|error| {
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

    Ok(clients
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
            .unwrap_or_else(|error| {
                json!({
                    "action": "failed",
                    "client": client,
                    "server_name": server_name,
                    "error": error,
                })
            })
        })
        .collect())
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
    let normalized_scope = install_scope(client, scope);
    let path = client_config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_client_config_path(client, &normalized_scope, &descriptor));
    let existing = fs::read_to_string(&path).ok();
    let removed =
        remove_client_config(client, &normalized_scope, existing.as_deref(), server_name)?;
    if removed.action == "removed" && !dry_run {
        write_text_atomic(&path, &removed.text)?;
    }
    let action = if removed.action == "removed" && dry_run {
        "dry_run".to_string()
    } else {
        removed.action
    };
    Ok(json!({
        "action": action,
        "client": client,
        "scope": normalized_scope,
        "server_name": server_name,
        "path": path,
        "previous": removed.previous,
        "payload": removed.payload,
    }))
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

fn setup_config_payload(paths: &GraphStatePaths, repo_root: &Path) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "repo_root": repo_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "ontology_version": "code_ontology_v1",
        "package_version": env!("CARGO_PKG_VERSION"),
        "materialization": {
            "include": [],
            "exclude": []
        },
        "mcp": {
            "server_name": "codebase_graph",
            "command": [
                server_command(),
                "mcp",
                "start",
                "--config",
                paths.config_path.to_string_lossy()
            ]
        }
    })
}

fn write_setup_config(paths: &GraphStatePaths, repo_root: &Path) -> Result<&'static str, String> {
    let payload = setup_config_payload(paths, repo_root);
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
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    fs::write(&paths.config_path, format!("{text}\n")).map_err(|error| {
        format!(
            "failed to write install config {}: {error}",
            paths.config_path.display()
        )
    })?;
    Ok(action)
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
- Use MCP `graph_search` with `detail: \"slim\"` and `context_limit: 1` before answering repo-structure questions or performing coding tasks.\n\
- Use MCP `graph_context` with `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.\n\
- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.\n\
- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.\n\
- If MCP tools are unavailable, fall back to CLI: `{command} codebase-search <query> --no-refresh --detail slim --context-limit 1`, `{command} codebase-context <query> --profile <profile> --no-refresh --detail slim --context-limit 2`, `{command} codebase-architecture-queries`, `{command} graph-query \"<statement>\"`, `{command} schema`, and `{command} query-helpers`.\n\
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

fn reinstall_state(paths: &GraphStatePaths, dry_run: bool) -> Result<ReinstallState, String> {
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
    let backup_path = next_backup_path(&paths.state_dir)?;
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

fn next_backup_path(state_dir: &Path) -> Result<PathBuf, String> {
    let parent = state_dir.parent().unwrap_or_else(|| Path::new("."));
    let file_name = state_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(".codebaseGraph");
    for index in 0..1000 {
        let suffix = if index == 0 {
            "reinstall-backup".to_string()
        } else {
            format!("reinstall-backup-{index}")
        };
        let candidate = parent.join(format!("{file_name}.{suffix}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "failed to choose backup path for {}",
        state_dir.display()
    ))
}

fn remove_backup(path: Option<&Path>) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    remove_path(path).map_err(|error| {
        format!(
            "failed to remove reinstall backup {} after successful setup: {error}",
            path.display()
        )
    })
}

fn restore_backup(state_dir: &Path, backup_path: Option<&Path>) -> Result<(), String> {
    let Some(backup_path) = backup_path else {
        if state_dir.exists() {
            remove_path(state_dir).map_err(|error| {
                format!(
                    "failed to remove partial graph state {} after setup failure: {error}",
                    state_dir.display()
                )
            })?;
        }
        return Ok(());
    };
    if state_dir.exists() {
        remove_path(state_dir).map_err(|error| {
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

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
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
    let manual_http_metadata = json!({
        "url": "http://127.0.0.1:8765/mcp",
        "start_command": [
            command,
            "mcp",
            "http",
            "--config",
            config_path.to_string_lossy(),
            "--host",
            "127.0.0.1",
            "--port",
            "8765",
            "--path",
            "/mcp"
        ],
        "host": "127.0.0.1",
        "port": 8765,
        "path": "/mcp",
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
                install_mcp_client_configuration(
                    client,
                    &scope,
                    descriptor,
                    options.client_config_path.clone(),
                    options.dry_run,
                )
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
                        "error": error,
                    })
                })
            })
            .collect::<Vec<_>>();
        return Ok(json!({ "results": results }));
    }
    install_mcp_client_configuration(
        &client,
        &scope,
        descriptor,
        options.client_config_path.clone(),
        options.dry_run,
    )
}

fn install_mcp_client_configuration(
    client: &str,
    scope: &str,
    descriptor: &McpServerDescriptor,
    client_config_path: Option<PathBuf>,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    if client == "copilot-studio" || client == "microsoft-copilot" {
        let metadata = copilot_studio_metadata(descriptor);
        return Ok(json!({
            "action": if dry_run { "dry_run" } else { "reported" },
            "client": client,
            "scope": scope,
            "server_name": descriptor.name,
            "method": "manual_metadata",
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": metadata["stdio"].clone(),
            "payload": metadata,
        }));
    }

    let native_command = native_client_command(client, descriptor, scope);
    let native_available = native_command
        .as_ref()
        .and_then(|command| command.first())
        .is_some_and(|executable| executable_in_path(executable));
    let normalized_scope = install_scope(client, scope);

    if dry_run && client_config_path.is_none() && native_available {
        return Ok(json!({
            "action": "dry_run",
            "client": client,
            "scope": normalized_scope,
            "server_name": descriptor.name,
            "method": "native_cli",
            "path": serde_json::Value::Null,
            "command": native_command,
            "descriptor": descriptor.as_json(),
            "entry": descriptor.stdio_entry(false, false),
        }));
    }
    if !dry_run && client_config_path.is_none() && native_available {
        let Some(command) = native_command.clone() else {
            return file_adapter_result(
                client,
                &normalized_scope,
                descriptor,
                None,
                None,
                dry_run,
                client_config_path,
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
                "scope": normalized_scope,
                "server_name": descriptor.name,
                "method": "native_cli",
                "path": serde_json::Value::Null,
                "command": command,
                "descriptor": descriptor.as_json(),
                "entry": descriptor.stdio_entry(false, false),
            }));
        }
        let error = subprocess_error(&completed);
        return file_adapter_result(
            client,
            &normalized_scope,
            descriptor,
            Some(command),
            Some(error),
            dry_run,
            client_config_path,
        );
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
    file_adapter_result(
        client,
        &normalized_scope,
        descriptor,
        native_command,
        native_error,
        dry_run,
        client_config_path,
    )
}

fn file_adapter_result(
    client: &str,
    scope: &str,
    descriptor: &McpServerDescriptor,
    native_command: Option<Vec<String>>,
    native_error: Option<String>,
    dry_run: bool,
    client_config_path: Option<PathBuf>,
) -> Result<serde_json::Value, String> {
    let path =
        client_config_path.unwrap_or_else(|| default_client_config_path(client, scope, descriptor));
    let existing = fs::read_to_string(&path).ok();
    let rendered = render_client_config(client, scope, existing.as_deref(), descriptor)?;
    let action = if dry_run {
        "dry_run".to_string()
    } else {
        rendered.action.clone()
    };
    if !dry_run {
        write_text_atomic(&path, &rendered.text)?;
    }
    let mut payload = json!({
        "action": action,
        "client": client,
        "scope": scope,
        "server_name": descriptor.name,
        "method": "file_adapter",
        "path": path.to_string_lossy(),
        "command": serde_json::Value::Null,
        "descriptor": descriptor.as_json(),
        "entry": rendered.entry,
        "patch": rendered.patch,
        "payload": rendered.payload,
    });
    if let Some(command) = native_command {
        payload["native_command"] = json!(command);
    }
    if let Some(error) = native_error {
        payload["native_error"] = json!(error);
    }
    Ok(payload)
}

fn default_client_config_path(
    client: &str,
    scope: &str,
    descriptor: &McpServerDescriptor,
) -> PathBuf {
    let home = home_dir();
    match adapter_id(client, scope) {
        "codex" => env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"))
            .join("config.toml"),
        "claude" => {
            let mac_path =
                home.join("Library/Application Support/Claude/claude_desktop_config.json");
            if mac_path.parent().is_some_and(Path::exists) {
                mac_path
            } else {
                home.join(".config/claude/claude_desktop_config.json")
            }
        }
        "claude-project" => descriptor.repo_root.join(".mcp.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "github-copilot" => descriptor.repo_root.join(".vscode/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"))
            .join("mcp.json5"),
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

struct RenderedNativeConfig {
    text: String,
    action: String,
    entry: serde_json::Value,
    patch: serde_json::Value,
    payload: serde_json::Value,
}

struct RemovedNativeConfig {
    text: String,
    action: String,
    previous: serde_json::Value,
    payload: serde_json::Value,
}

fn render_client_config(
    client: &str,
    scope: &str,
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
) -> Result<RenderedNativeConfig, String> {
    match adapter_id(client, scope) {
        "codex" => render_codex_config(existing, descriptor),
        "hermes" => render_hermes_config(existing, descriptor),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            render_json_config(adapter_id(client, scope), existing, descriptor)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn remove_client_config(
    client: &str,
    scope: &str,
    existing: Option<&str>,
    server_name: &str,
) -> Result<RemovedNativeConfig, String> {
    match adapter_id(client, scope) {
        "codex" => remove_codex_config(existing, server_name),
        "hermes" => remove_hermes_config(existing, server_name),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            remove_json_config(adapter_id(client, scope), existing, server_name)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn render_json_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
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
        .insert(descriptor.name.clone(), entry.clone());
    let action = action_for_json(previous.as_ref(), &entry, existing.is_some());
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

fn render_codex_config(
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(false, true);
    let patch = codex_toml_block(descriptor);
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

fn render_hermes_config(
    existing: Option<&str>,
    descriptor: &McpServerDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(true, false);
    let patch = hermes_yaml_block(descriptor);
    let (text, previous) = upsert_marked_block(existing.unwrap_or_default(), &patch);
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
    let (text, previous) = remove_marked_block(existing);
    let previous = previous.filter(|block| block.contains(&format!("  {server_name}:")));
    let action = if previous.is_some() {
        "removed".to_string()
    } else {
        "unchanged".to_string()
    };
    Ok(RemovedNativeConfig {
        text: if previous.is_some() {
            text
        } else {
            existing.to_string()
        },
        action,
        previous: previous
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
        payload: json!(existing),
    })
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

fn hermes_yaml_block(descriptor: &McpServerDescriptor) -> String {
    let mut lines = vec![
        "# codebaseGraph MCP server start".to_string(),
        "mcp_servers:".to_string(),
        format!("  {}:", descriptor.name),
        "    type: stdio".to_string(),
        format!("    command: {}", yaml_scalar(&descriptor.command)),
        "    args:".to_string(),
    ];
    for arg in &descriptor.args {
        lines.push(format!("      - {}", yaml_scalar(arg)));
    }
    lines.push("# codebaseGraph MCP server end".to_string());
    lines.join("\n") + "\n"
}

fn yaml_scalar(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn upsert_marked_block(existing: &str, block: &str) -> (String, Option<String>) {
    const START: &str = "# codebaseGraph MCP server start";
    const END: &str = "# codebaseGraph MCP server end";
    let Some(start) = existing.find(START) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    let Some(end) = existing.find(END) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    if end < start {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    }
    let after_end = end + END.len();
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
    const START: &str = "# codebaseGraph MCP server start";
    const END: &str = "# codebaseGraph MCP server end";
    let Some(start) = existing.find(START) else {
        return (existing.to_string(), None);
    };
    let Some(end) = existing.find(END) else {
        return (existing.to_string(), None);
    };
    if end < start {
        return (existing.to_string(), None);
    }
    let after_end = end + END.len();
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
    use super::{native_client_command, McpServerDescriptor};
    use std::path::PathBuf;

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
}
