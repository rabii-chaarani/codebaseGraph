use super::{
    format::uninstall_help,
    install::{
        build_mcp_descriptor, default_client_config_path, install_scope, remove_client_config,
        supported_install_clients, supported_install_clients_with_all, write_text_atomic,
        McpInstallOptions,
    },
    setup::{remove_instruction_text, GraphStatePaths},
    util::{read_json_file, required_arg},
};
use crate::api::{
    contracts::{OperationRequest, OutputFormat, RepositoryLifecycleRequest},
    CodebaseGraphApi,
};
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(in crate::cli) struct UninstallOptions {
    repo_root: Option<PathBuf>,
    config: Option<PathBuf>,
    mcp_client: String,
    client_config_path: Option<PathBuf>,
    dry_run: bool,
    json: bool,
    help: bool,
}

impl UninstallOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: None,
            config: None,
            mcp_client: "all".to_string(),
            client_config_path: None,
            dry_run: false,
            json: false,
            help: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--repo-root" | "--source-root" => {
                    options.repo_root =
                        Some(PathBuf::from(required_arg(args, index, "--repo-root")?));
                    index += 2;
                }
                "--config" => {
                    options.config = Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--mcp-client" => {
                    let client = required_arg(args, index, "--mcp-client")?;
                    if client != "all" && !supported_install_clients().contains(&client) {
                        return Err(format!(
                            "--mcp-client must be one of {}",
                            supported_install_clients_with_all().join(", ")
                        ));
                    }
                    options.mcp_client = client.to_string();
                    index += 2;
                }
                "--client-config-path" => {
                    options.client_config_path = Some(PathBuf::from(required_arg(
                        args,
                        index,
                        "--client-config-path",
                    )?));
                    index += 2;
                }
                "--dry-run" => {
                    options.dry_run = true;
                    index += 1;
                }
                "--json" => {
                    options.json = true;
                    index += 1;
                }
                other => {
                    return Err(format!(
                        "unknown uninstall option: {other}\n\n{}",
                        uninstall_help()
                    ));
                }
            }
        }
        if options.client_config_path.is_some() && options.mcp_client == "all" {
            return Err("--client-config-path requires --mcp-client <client>".to_string());
        }
        Ok(options)
    }
}

pub(in crate::cli) fn run_uninstall<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = UninstallOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", uninstall_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let request = RepositoryLifecycleRequest {
        repo: crate::api::contracts::RepoSelector {
            repo_root: options.repo_root.clone(),
            config_path: options.config.clone(),
            db_path: None,
            manifest_path: None,
        },
        action: "uninstall".to_string(),
        output_format: if options.json {
            OutputFormat::Typed
        } else {
            OutputFormat::Block
        },
        dry_run: options.dry_run,
        mcp_client: Some(options.mcp_client.clone()),
        mcp_config_path: options.client_config_path.clone(),
        instructions_target: None,
        skip_mcp_config: false,
        mode: "changed".to_string(),
        include_fts: true,
        semantic_enrichment: true,
        semantic_provider_mode: "local_only".to_string(),
    };
    let payload = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Uninstall(request))
        .map_err(|error| error.message)?
        .payload;
    if options.json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
    } else {
        let text = payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "block response did not contain text".to_string())?;
        write!(stdout, "{text}").map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn execute_uninstall_operation(
    request: &RepositoryLifecycleRequest,
    repo_root: &Path,
) -> Result<serde_json::Value, String> {
    uninstall_payload_for_request(request, repo_root)
}

pub(crate) fn uninstall_payload_for_request(
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
    let options = UninstallOptions {
        repo_root: request.repo.repo_root.clone(),
        config: request.repo.config_path.clone(),
        mcp_client,
        client_config_path: request.mcp_config_path.clone(),
        dry_run: request.dry_run,
        json: request.output_format == crate::api::contracts::OutputFormat::Typed,
        help: false,
    };
    let mcp_clients = uninstall_mcp_clients(&options, &repo_root, &config_path, &server_name)?;
    let output = json!({
        "ok": true,
        "repo_root": repo_root,
        "config_path": config_path,
        "server_name": server_name,
        "dry_run": request.dry_run,
        "state": state,
        "instructions": instructions,
        "mcp_clients": mcp_clients,
    });
    Ok(output)
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
    Ok(build_mcp_descriptor(&McpInstallOptions {
        client: "generic".to_string(),
        scope: "local".to_string(),
        name: None,
        config_path: Some(config_path.to_path_buf()),
        client_config_path: None,
        repo_root: Some(repo_root.to_path_buf()),
        dry_run: true,
        verify: false,
        json: true,
        help: false,
    })?
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
    options: &UninstallOptions,
    repo_root: &Path,
    config_path: &Path,
    server_name: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let clients = if options.mcp_client == "all" {
        supported_install_clients()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        vec![options.mcp_client.clone()]
    };
    Ok(clients
        .into_iter()
        .map(|client| {
            uninstall_mcp_client(&client, options, repo_root, config_path, server_name)
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
    options: &UninstallOptions,
    repo_root: &Path,
    config_path: &Path,
    server_name: &str,
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
    let descriptor = build_mcp_descriptor(&McpInstallOptions {
        client: client.to_string(),
        scope: scope.to_string(),
        name: Some(server_name.to_string()),
        config_path: Some(config_path.to_path_buf()),
        client_config_path: options.client_config_path.clone(),
        repo_root: Some(repo_root.to_path_buf()),
        dry_run: true,
        verify: false,
        json: true,
        help: false,
    })?;
    let scope = install_scope(client, scope);
    let path = options
        .client_config_path
        .clone()
        .unwrap_or_else(|| default_client_config_path(client, &scope, &descriptor));
    let existing = fs::read_to_string(&path).ok();
    let removed = remove_client_config(client, &scope, existing.as_deref(), server_name)?;
    if removed.action == "removed" && !options.dry_run {
        write_text_atomic(&path, &removed.text)?;
    }
    let action = if removed.action == "removed" && options.dry_run {
        "dry_run".to_string()
    } else {
        removed.action
    };
    Ok(json!({
        "action": action,
        "client": client,
        "scope": scope,
        "server_name": server_name,
        "path": path,
        "previous": removed.previous,
        "payload": removed.payload,
    }))
}

pub(crate) fn serialize_uninstall_block(output: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "uninstall ok={} server_name={}",
        output["ok"].as_bool().unwrap_or(false),
        output["server_name"].as_str().unwrap_or_default()
    )];
    if let Some(state) = output["state"].as_object() {
        lines.push(format!(
            "state action={} path={}",
            state
                .get("action")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            state
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
        ));
    }
    for item in output["instructions"].as_array().into_iter().flatten() {
        lines.push(format!(
            "instructions action={} path={}",
            item["action"].as_str().unwrap_or("unknown"),
            item["path"].as_str().unwrap_or_default()
        ));
    }
    for item in output["mcp_clients"].as_array().into_iter().flatten() {
        lines.push(format!(
            "mcp client={} action={} path={}",
            item["client"].as_str().unwrap_or("unknown"),
            item["action"].as_str().unwrap_or("unknown"),
            item["path"].as_str().unwrap_or_default()
        ));
    }
    lines.join("\n") + "\n"
}
