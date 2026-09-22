//! Repository-local agent loop hooks.
//!
//! Hooks are deliberately advisory. They query the already managed loopback
//! MCP daemon, never open graph storage, and always return a successful hook
//! response when the daemon is unavailable.

use crate::api::context::{read_install_config, GraphInstallConfig};
use crate::mcp_client::McpLoopbackSession;
use crate::storage::atomic::{write_bytes_atomically, write_json_atomically};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const MANAGED_ID: &str = "codebase-graph-v1";
const MAX_STDIN_BYTES: usize = 1024 * 1024;
const MAX_PROMPT_BYTES: usize = 4096;
const MAX_CONTEXT_CHARS: usize = 6000;
const CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const HOOK_DEADLINE: Duration = Duration::from_secs(3);
static CACHE_CLAIM_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHookClient {
    Codex,
    Claude,
    GithubCopilot,
}

impl AgentHookClient {
    pub fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::GithubCopilot => "github-copilot",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" | "claude-project" => Ok(Self::Claude),
            "github-copilot" | "copilot" => Ok(Self::GithubCopilot),
            other => Err(format!(
                "unsupported agent hook client: {other}; expected codex, claude, github-copilot, or all"
            )),
        }
    }
}

pub fn resolve_agent_hook_clients(
    selection: &str,
    mcp_client: &str,
) -> Result<Vec<AgentHookClient>, String> {
    let selection = selection.trim().to_ascii_lowercase();
    let selected = if selection.is_empty() || selection == "auto" {
        match mcp_client.trim().to_ascii_lowercase().as_str() {
            "codex" => vec![AgentHookClient::Codex],
            "claude" | "claude-project" => vec![AgentHookClient::Claude],
            "github-copilot" => vec![AgentHookClient::GithubCopilot],
            "all" => all_clients(),
            "none" | "" => Vec::new(),
            _ => Vec::new(),
        }
    } else if selection == "none" {
        Vec::new()
    } else if selection == "all" {
        all_clients()
    } else {
        vec![AgentHookClient::parse(&selection)?]
    };
    Ok(selected)
}

fn all_clients() -> Vec<AgentHookClient> {
    vec![
        AgentHookClient::Codex,
        AgentHookClient::Claude,
        AgentHookClient::GithubCopilot,
    ]
}

fn reload_instructions(client: AgentHookClient) -> &'static str {
    match client {
        AgentHookClient::Codex => {
            "Trust the project hook definition with /hooks, then start a new Codex session."
        }
        AgentHookClient::Claude => {
            "Trust the workspace if prompted, then restart Claude Code or begin a new session."
        }
        AgentHookClient::GithubCopilot => {
            "Reload the VS Code window or begin a new Copilot CLI session; organization policy may disable hooks."
        }
    }
}

pub fn hook_target_path(repo_root: &Path, client: AgentHookClient) -> PathBuf {
    match client {
        AgentHookClient::Codex => repo_root.join(".codex/hooks.json"),
        AgentHookClient::Claude => repo_root.join(".claude/settings.json"),
        AgentHookClient::GithubCopilot => repo_root.join(".github/hooks/codebase-graph.json"),
    }
}

#[allow(dead_code)]
pub(crate) fn setup_config_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".codebaseGraph/config.json")
}

pub(crate) fn repository_fingerprint(repo_root: &Path) -> String {
    let path = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn shell_quote(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn hook_command(client: AgentHookClient, config_path: &Path, server_command: &str) -> String {
    format!(
        "{} agent-hooks run --client {} --config {} --managed-id {}",
        shell_quote(server_command),
        client.id(),
        shell_quote(&config_path.to_string_lossy()),
        MANAGED_ID
    )
}

fn windows_hook_command(
    client: AgentHookClient,
    config_path: &Path,
    server_command: &str,
) -> String {
    format!(
        "& {} agent-hooks run --client {} --config {} --managed-id {}",
        powershell_quote(server_command),
        client.id(),
        powershell_quote(&config_path.to_string_lossy()),
        MANAGED_ID
    )
}

fn managed_hook(client: AgentHookClient, config_path: &Path, server_command: &str) -> Value {
    let command = hook_command(client, config_path, server_command);
    let command_windows = windows_hook_command(client, config_path, server_command);
    match client {
        AgentHookClient::Codex => json!({
            "type": "command",
            "command": command,
            "commandWindows": command_windows,
            "timeout": 3,
        }),
        AgentHookClient::Claude => json!({
            "type": "command",
            "command": server_command,
            "args": [
                "agent-hooks", "run", "--client", client.id(), "--config",
                config_path.to_string_lossy(), "--managed-id", MANAGED_ID
            ],
            "timeout": 3,
        }),
        AgentHookClient::GithubCopilot => {
            let executable = shell_quote(server_command);
            let powershell_executable = powershell_quote(server_command);
            let powershell_command = command_windows;
            let prompt_var = ["$", "{COPILOT_AGENT_PROMPT:-}"].concat();
            json!({
                "type": "command",
                "bash": format!(
                    "if [ -n \"{prompt_var}\" ] || ! command -v {executable} >/dev/null 2>&1; then exit 0; fi; {command}"
                ),
                "powershell": format!(
                    "if ($env:COPILOT_AGENT_PROMPT -or -not (Get-Command {powershell_executable} -ErrorAction SilentlyContinue)) {{ exit 0 }}; {powershell_command}"
                ),
                "timeout": 3,
            })
        }
    }
}

fn event_names(client: AgentHookClient) -> &'static [&'static str] {
    match client {
        AgentHookClient::Codex | AgentHookClient::Claude => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "SubagentStart",
        ],
        AgentHookClient::GithubCopilot => &[
            "sessionStart",
            "userPromptSubmitted",
            "preToolUse",
            "postToolUse",
            "subagentStart",
        ],
    }
}

fn managed_command(value: &Value) -> bool {
    value
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains("--managed-id") && command.contains(MANAGED_ID))
        || value
            .get("bash")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("--managed-id") && command.contains(MANAGED_ID))
        || value
            .get("powershell")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("--managed-id") && command.contains(MANAGED_ID))
        || value
            .get("args")
            .and_then(Value::as_array)
            .is_some_and(|args| {
                args.windows(2).any(|pair| {
                    pair[0].as_str() == Some("--managed-id") && pair[1].as_str() == Some(MANAGED_ID)
                })
            })
}

fn remove_managed_from_event(event: &mut Value) -> bool {
    let mut changed = false;
    if let Some(hooks) = event.get_mut("hooks").and_then(Value::as_array_mut) {
        let before = hooks.len();
        hooks.retain(|hook| !managed_command(hook));
        for hook in hooks.iter_mut() {
            if let Some(nested) = hook.get_mut("hooks").and_then(Value::as_array_mut) {
                let nested_before = nested.len();
                nested.retain(|child| !managed_command(child));
                if nested.len() != nested_before {
                    changed = true;
                }
            }
        }
        hooks.retain(|hook| {
            hook.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|nested| !nested.is_empty())
        });
        changed |= hooks.len() != before;
    }
    changed
}

fn merge_hooks(
    client: AgentHookClient,
    mut root: Value,
    config_path: &Path,
    server_command: &str,
) -> Result<(Value, bool), String> {
    if !root.is_object() {
        return Err("agent hook configuration must be a JSON object".to_string());
    }
    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| "agent hook configuration must be a JSON object".to_string())?;
    if client == AgentHookClient::GithubCopilot && !root_obj.contains_key("version") {
        root_obj.insert("version".to_string(), json!(1));
    }
    if !root_obj.get("hooks").is_some_and(Value::is_object) {
        root_obj.insert("hooks".to_string(), Value::Object(Map::new()));
    }
    let hooks = root_obj
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "agent hook configuration hooks must be an object".to_string())?;
    let mut changed = false;
    for name in event_names(client) {
        let events = hooks
            .entry((*name).to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if !events.is_array() {
            return Err(format!("agent hook event {name} must be an array"));
        }
        let entries = events
            .as_array_mut()
            .ok_or_else(|| format!("agent hook event {name} must be an array"))?;
        entries.retain(|entry| !managed_command(entry));
        for entry in entries.iter_mut() {
            let _ = remove_managed_from_event(entry);
        }
        entries.retain(|entry| {
            entry
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|children| !children.is_empty())
        });
        let mut generated = if client == AgentHookClient::GithubCopilot {
            managed_hook(client, config_path, server_command)
        } else {
            json!({
                "hooks": [managed_hook(client, config_path, server_command)]
            })
        };
        if matches!(
            *name,
            "PreToolUse" | "PostToolUse" | "preToolUse" | "postToolUse"
        ) {
            generated["matcher"] = Value::String(".*".to_string());
        }
        entries.push(generated);
        changed = true;
    }
    Ok((root, changed))
}

fn load_json(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| {
            format!(
                "failed to parse agent hook config {}: {error}",
                path.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => Err(format!(
            "failed to read agent hook config {}: {error}",
            path.display()
        )),
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    write_json_atomically(path, value).map_err(|error| {
        format!(
            "failed to write agent hook config {}: {error}",
            path.display()
        )
    })
}

fn snapshot_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to snapshot {}: {error}", path.display())),
    }
}

fn restore_file(path: &Path, previous: Option<&[u8]>) -> Result<(), String> {
    match previous {
        Some(bytes) => write_bytes_atomically(path, bytes)
            .map_err(|error| format!("failed to restore {}: {error}", path.display())),
        None if path.exists() => fs::remove_file(path).map_err(|error| {
            format!(
                "failed to remove {} during rollback: {error}",
                path.display()
            )
        }),
        None => Ok(()),
    }
}

fn update_ownership(
    setup_config: &Path,
    clients: &[AgentHookClient],
    install: bool,
) -> Result<(), String> {
    let mut payload = load_json(setup_config)?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| "setup config must be a JSON object".to_string())?;
    let agent_hooks = object
        .entry("agent_hooks".to_string())
        .or_insert_with(|| json!({}));
    if !agent_hooks.is_object() {
        return Err("setup config agent_hooks must be an object".to_string());
    }
    agent_hooks["format_version"] = json!(1);
    agent_hooks["policy"] = json!("advisory");
    let mut installed = agent_hooks
        .get("installed_clients")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    for client in clients {
        if install {
            if !installed.iter().any(|value| value == client.id()) {
                installed.push(client.id().to_string());
            }
        } else {
            installed.retain(|value| value != client.id());
        }
    }
    installed.sort();
    installed.dedup();
    agent_hooks["installed_clients"] = json!(installed);
    write_json(setup_config, &payload)
}

fn config_root(config_path: &Path, config: &GraphInstallConfig) -> Result<PathBuf, String> {
    let root = config
        .repo_root
        .clone()
        .or_else(|| {
            config_path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
        .ok_or_else(|| "setup config does not identify a repository root".to_string())?;
    root.canonicalize()
        .map_err(|error| format!("repository root is not readable: {error}"))
}

pub(crate) fn reconcile_agent_hooks(
    repo_root: &Path,
    setup_config: &Path,
    clients: &[AgentHookClient],
    dry_run: bool,
) -> Result<Value, String> {
    let server_command = std::env::current_exe()
        .map_err(|error| format!("failed to resolve codebase-graph executable: {error}"))?;
    reconcile_agent_hooks_with_command(
        repo_root,
        setup_config,
        &server_command.to_string_lossy(),
        clients,
        dry_run,
    )
}

pub(crate) fn reconcile_agent_hooks_with_command(
    repo_root: &Path,
    setup_config: &Path,
    server_command: &str,
    clients: &[AgentHookClient],
    dry_run: bool,
) -> Result<Value, String> {
    let setup_config = setup_config
        .canonicalize()
        .unwrap_or_else(|_| setup_config.to_path_buf());
    let config = read_install_config(&setup_config)?;
    let root = repo_root
        .canonicalize()
        .map_err(|error| format!("repository root is not readable: {error}"))?;
    let configured_root = config_root(&setup_config, &config)?;
    if root != configured_root {
        return Err(format!(
            "repository root {} does not match setup config root {}",
            root.display(),
            configured_root.display()
        ));
    }
    let config_snapshot = snapshot_file(&setup_config)?;
    let mut planned = Vec::new();
    for client in clients {
        let path = hook_target_path(&root, *client);
        let previous = snapshot_file(&path)?;
        let existing = load_json(&path)?;
        let (rendered, _) = merge_hooks(*client, existing.clone(), &setup_config, server_command)?;
        let changed = rendered != existing;
        let action = if !changed {
            "unchanged"
        } else if previous.is_some() {
            "updated"
        } else {
            "created"
        };
        planned.push((*client, path, previous, rendered, changed, action));
    }
    let any_changed = planned.iter().any(|item| item.4);
    if !dry_run {
        let mut write_result = Ok(());
        for (_, path, _, rendered, changed, _) in &planned {
            if *changed {
                if let Err(error) = write_json(path, rendered) {
                    write_result = Err(error);
                    break;
                }
            }
        }
        if write_result.is_ok() {
            write_result = update_ownership(&setup_config, clients, true);
        }
        if let Err(error) = write_result {
            let mut rollback_errors = Vec::new();
            for (_, path, previous, _, _, _) in &planned {
                if let Err(rollback) = restore_file(path, previous.as_deref()) {
                    rollback_errors.push(rollback);
                }
            }
            if let Err(rollback) = restore_file(&setup_config, config_snapshot.as_deref()) {
                rollback_errors.push(rollback);
            }
            return Err(if rollback_errors.is_empty() {
                error
            } else {
                format!("{error}; rollback failed: {}", rollback_errors.join("; "))
            });
        }
    }
    let results = planned
        .into_iter()
        .map(|(client, path, _, _, changed, action)| {
            json!({
                "client": client.id(),
                "action": if dry_run && changed { "dry_run" } else { action },
                "path": path,
                "verification": "managed_handlers_present",
                "restart_required": true,
                "restart_instructions": reload_instructions(client),
                "trust_required": true,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "action": if dry_run && any_changed {
            "dry_run"
        } else if any_changed {
            "updated"
        } else {
            "unchanged"
        },
        "clients": results,
        "managed_id": MANAGED_ID,
    }))
}

pub(crate) fn remove_agent_hooks(
    repo_root: &Path,
    setup_config: &Path,
    clients: &[AgentHookClient],
    dry_run: bool,
) -> Result<Value, String> {
    let setup_config = setup_config
        .canonicalize()
        .unwrap_or_else(|_| setup_config.to_path_buf());
    let config = read_install_config(&setup_config)?;
    let root = repo_root
        .canonicalize()
        .map_err(|error| format!("repository root is not readable: {error}"))?;
    if root != config_root(&setup_config, &config)? {
        return Err("repository root does not match setup config root".to_string());
    }
    let config_snapshot = snapshot_file(&setup_config)?;
    let mut planned = Vec::new();
    for client in clients {
        let path = hook_target_path(&root, *client);
        if !path.exists() {
            planned.push((*client, path, None, None, false));
            continue;
        }
        let previous = snapshot_file(&path)?;
        let mut value = load_json(&path)?;
        let mut changed = false;
        if let Some(hooks) = value.get_mut("hooks").and_then(Value::as_object_mut) {
            for event in hooks.values_mut() {
                if let Some(entries) = event.as_array_mut() {
                    let before = entries.len();
                    entries.retain(|entry| !managed_command(entry));
                    changed |= entries.len() != before;
                    for entry in entries.iter_mut() {
                        changed |= remove_managed_from_event(entry);
                    }
                    entries.retain(|entry| {
                        entry
                            .get("hooks")
                            .and_then(Value::as_array)
                            .is_none_or(|children| !children.is_empty())
                    });
                }
            }
        }
        planned.push((*client, path, previous, Some(value), changed));
    }
    let any_changed = planned.iter().any(|item| item.4);
    if !dry_run {
        let mut write_result = Ok(());
        for (_, path, _, value, changed) in &planned {
            if *changed {
                if let Err(error) =
                    write_json(path, value.as_ref().expect("changed file has value"))
                {
                    write_result = Err(error);
                    break;
                }
            }
        }
        if write_result.is_ok() {
            write_result = update_ownership(&setup_config, clients, false);
        }
        if let Err(error) = write_result {
            let mut rollback_errors = Vec::new();
            for (_, path, previous, _, _) in &planned {
                if let Err(rollback) = restore_file(path, previous.as_deref()) {
                    rollback_errors.push(rollback);
                }
            }
            if let Err(rollback) = restore_file(&setup_config, config_snapshot.as_deref()) {
                rollback_errors.push(rollback);
            }
            return Err(if rollback_errors.is_empty() {
                error
            } else {
                format!("{error}; rollback failed: {}", rollback_errors.join("; "))
            });
        }
    }
    let results = planned
        .into_iter()
        .map(|(client, path, _, _, changed)| {
            json!({
                "client": client.id(),
                "action": if !changed { "unchanged" } else if dry_run { "dry_run" } else { "removed" },
                "path": path,
                "verification": if changed { "managed_handlers_removed" } else { "managed_handlers_absent" },
                "restart_required": changed,
                "restart_instructions": reload_instructions(client),
                "trust_required": false,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "action": if dry_run && any_changed { "dry_run" } else if any_changed { "removed" } else { "unchanged" },
        "clients": results,
        "managed_id": MANAGED_ID
    }))
}

pub(crate) fn verify_agent_hooks(
    repo_root: &Path,
    setup_config: &Path,
    clients: &[AgentHookClient],
) -> Result<Value, String> {
    let setup_config = setup_config
        .canonicalize()
        .unwrap_or_else(|_| setup_config.to_path_buf());
    let config = read_install_config(&setup_config)?;
    let root = repo_root
        .canonicalize()
        .map_err(|error| format!("repository root is not readable: {error}"))?;
    if root != config_root(&setup_config, &config)? {
        return Err("repository root does not match setup config root".to_string());
    }
    let mut results = Vec::new();
    for client in clients {
        let path = hook_target_path(&root, *client);
        let value = load_json(&path)?;
        let hooks = value.get("hooks").and_then(Value::as_object);
        let event_details = event_names(*client)
            .iter()
            .map(|name| {
                let managed_handler_count = hooks
                    .and_then(|events| events.get(*name))
                    .map(count_managed_handlers)
                    .unwrap_or(0);
                json!({
                    "event": name,
                    "managed_handler_count": managed_handler_count,
                    "ok": managed_handler_count > 0,
                })
            })
            .collect::<Vec<_>>();
        let all_events_present = event_details
            .iter()
            .all(|event| event["ok"].as_bool().unwrap_or(false));
        let managed_handler_count = event_details
            .iter()
            .filter_map(|event| event["managed_handler_count"].as_u64())
            .sum::<u64>();
        let runner_smoke = run_agent_hook_json(*client, &setup_config, "{");
        let runner_ok = runner_smoke
            .get("advisory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        results.push(json!({
            "client": client.id(),
            "path": path,
            "ok": all_events_present && runner_ok,
            "managed_handler_count": managed_handler_count,
            "events": event_details,
            "runner_smoke": if runner_ok { "fail_open" } else { "failed" },
            "trust_required": true,
            "restart_instructions": reload_instructions(*client),
        }));
    }
    Ok(json!({"ok": results.iter().all(|item| item["ok"] == true), "clients": results}))
}

fn count_managed_handlers(event: &Value) -> u64 {
    event
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            let direct = managed_command(entry) as u64;
            let nested = entry
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|child| managed_command(child))
                .count() as u64;
            direct + nested
        })
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HookEvent {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    SubagentStart,
    Other,
}

fn event_from_input(input: &Value) -> HookEvent {
    let event = ["hook_event_name", "hookEventName", "event", "type", "name"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_ascii_lowercase()
        .replace(['_', '-'], "");
    let explicit = match event.as_str() {
        "sessionstart" | "session" => HookEvent::SessionStart,
        "userpromptsubmit" | "userpromptsubmitted" | "prompt" | "userprompt" => {
            HookEvent::UserPromptSubmit
        }
        "pretooluse" | "pretool" => HookEvent::PreToolUse,
        "posttooluse" | "posttool" => HookEvent::PostToolUse,
        "subagentstart" | "subagent" => HookEvent::SubagentStart,
        _ => HookEvent::Other,
    };
    if explicit != HookEvent::Other {
        return explicit;
    }

    // Copilot CLI's native camelCase payloads identify the configured event by
    // shape rather than including a hook-event-name field. Keep this inference
    // deliberately narrow so arbitrary malformed input remains a no-op.
    if input.get("prompt").and_then(Value::as_str).is_some() {
        HookEvent::UserPromptSubmit
    } else if input.get("toolResult").is_some()
        || input.get("tool_result").is_some()
        || input.get("tool_response").is_some()
    {
        HookEvent::PostToolUse
    } else if input.get("toolName").is_some()
        || input.get("tool_name").is_some()
        || input.get("toolArgs").is_some()
        || input.get("tool_input").is_some()
    {
        HookEvent::PreToolUse
    } else if input.get("agentName").is_some()
        || input.get("agent_id").is_some()
        || input.get("agent_type").is_some()
    {
        HookEvent::SubagentStart
    } else if input.get("source").is_some() || input.get("initialPrompt").is_some() {
        HookEvent::SessionStart
    } else {
        HookEvent::Other
    }
}

fn get_string<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn truncate_utf8_bytes(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut boundary = max;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_string()
}

fn prompt_from_input(input: &Value) -> String {
    truncate_utf8_bytes(
        get_string(input, &["prompt", "user_prompt", "userPrompt", "message"]).unwrap_or(""),
        MAX_PROMPT_BYTES,
    )
}

fn input_session_id(input: &Value, prompt: &str) -> String {
    get_string(
        input,
        &[
            "session_id",
            "sessionId",
            "conversation_id",
            "conversationId",
        ],
    )
    .unwrap_or(prompt)
    .to_string()
}

fn relevant_tool(input: &Value) -> bool {
    let name = get_string(input, &["tool_name", "toolName", "tool", "name"])
        .unwrap_or("")
        .to_ascii_lowercase();
    name.is_empty()
        || [
            "edit",
            "write",
            "patch",
            "replace",
            "read",
            "grep",
            "glob",
            "view",
            "bash",
            "shell",
            "terminal",
            "runin",
            "createfile",
            "create_file",
            "create",
        ]
        .iter()
        .any(|term| name.contains(term))
}

fn is_vscode_payload(input: &Value) -> bool {
    input.get("tool_name").is_some()
        || input.get("hook_event_name").is_some()
        || input.get("workspace_folder").is_some()
}

fn hash_id(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn cache_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".codebaseGraph/agent-hooks/sessions")
}

fn purge_cache(repo_root: &Path) {
    let Ok(entries) = fs::read_dir(cache_dir(repo_root)) else {
        return;
    };
    let cutoff = now_unix_ms().saturating_sub(CACHE_MAX_AGE.as_millis() as u64);
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(value) = fs::read_to_string(&path).and_then(|text| {
            serde_json::from_str::<Value>(&text)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        }) else {
            continue;
        };
        if value.get("timestamp").and_then(Value::as_u64).unwrap_or(0) < cutoff {
            let _ = fs::remove_file(path);
        }
    }
}

fn write_cache(repo_root: &Path, session_id: &str, prompt: &str, context: &str) {
    let path = cache_dir(repo_root).join(format!("{}.json", hash_id(session_id)));
    let context = truncate_chars(
        &format!("[codebaseGraph advisory]\n{context}"),
        MAX_CONTEXT_CHARS,
    );
    let value = json!({
        "prompt_hash": hash_id(prompt),
        "bounded_graph_result": context,
        "delivered": false,
        "timestamp": now_unix_ms(),
    });
    let _ = write_json_atomically(&path, &value);
}

fn take_cached_context(repo_root: &Path, session_id: &str) -> Option<String> {
    let path = cache_dir(repo_root).join(format!("{}.json", hash_id(session_id)));
    let claim = cache_dir(repo_root).join(format!(
        "{}.claimed.{}.{}.json",
        hash_id(session_id),
        std::process::id(),
        CACHE_CLAIM_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(&path, &claim).ok()?;
    let text = fs::read_to_string(&claim).ok()?;
    let mut value: Value = serde_json::from_str(&text).ok()?;
    if value.get("delivered").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let context = value
        .get("bounded_graph_result")
        .and_then(Value::as_str)
        .map(str::to_string)?;
    value["delivered"] = Value::Bool(true);
    let _ = write_json_atomically(&claim, &value);
    Some(context)
}

fn hook_output(client: AgentHookClient, event: HookEvent, context: String) -> Value {
    let context = truncate_chars(
        &format!("[codebaseGraph advisory]\n{context}"),
        MAX_CONTEXT_CHARS,
    );
    if matches!(client, AgentHookClient::Codex | AgentHookClient::Claude) {
        let hook_event_name = match event {
            HookEvent::SessionStart => "SessionStart",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::SubagentStart => "SubagentStart",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::Other => "Unknown",
        };
        json!({
            "hookSpecificOutput": {
                "hookEventName": hook_event_name,
                "additionalContext": context,
            },
            "advisory": true,
        })
    } else {
        json!({"additionalContext": context, "advisory": true})
    }
}

fn hook_fallback(client: AgentHookClient, event: HookEvent, message: impl AsRef<str>) -> Value {
    let context = format!(
        "{}\nGraph lookup was non-blocking; continue with bounded local discovery.",
        message.as_ref()
    );
    hook_output(client, event, context)
}

fn value_text(value: &Value) -> String {
    value
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .pointer("/structuredContent")
                .filter(|value| !value.is_null())
                .and_then(|value| serde_json::to_string(value).ok())
        })
        .unwrap_or_else(|| serde_json::to_string(value).unwrap_or_default())
}

fn health_summary(value: &Value) -> String {
    let health = value.pointer("/structuredContent").unwrap_or(value);
    let readable = health
        .get("graph_readable")
        .and_then(Value::as_bool)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let state = health
        .pointer("/refresh_status/state")
        .and_then(Value::as_str)
        .or_else(|| {
            health
                .pointer("/refresh_state/state")
                .and_then(Value::as_str)
        })
        .or_else(|| health.get("freshness").and_then(Value::as_str))
        .unwrap_or("unknown");
    truncate_chars(&format!("readable={readable}, freshness={state}"), 600)
}

fn freshness_advisory(value: &Value) -> Option<String> {
    let health = value.pointer("/structuredContent").unwrap_or(value);
    let candidate = health
        .get("freshness")
        .and_then(|value| value.as_str())
        .or_else(|| {
            health
                .pointer("/refresh_status/state")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            health
                .pointer("/refresh_state/state")
                .and_then(Value::as_str)
        })
        .unwrap_or("unknown");
    let normalized = candidate.to_ascii_lowercase();
    if matches!(normalized.as_str(), "pending" | "overdue" | "unknown") {
        Some(format!(
            "Graph freshness advisory: {candidate}; treat results as provisional."
        ))
    } else {
        None
    }
}

fn retryable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "connect",
        "timed out",
        "timeout",
        "refused",
        "not running",
        "startup",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

fn open_session_with_budget(
    endpoint: &str,
    fingerprint: &str,
    deadline: Instant,
) -> Result<McpLoopbackSession, String> {
    let mut attempts = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("agent hook graph lookup exceeded its deadline".to_string());
        }
        match McpLoopbackSession::connect(
            endpoint,
            Some(fingerprint),
            remaining.min(Duration::from_millis(900)),
        ) {
            Ok(session) => return Ok(session),
            Err(error) if attempts == 0 && retryable(&error) => attempts += 1,
            Err(error) => return Err(error),
        }
    }
}

fn call_session_with_budget(
    session: &McpLoopbackSession,
    repo_root: &Path,
    name: &str,
    arguments: Value,
    deadline: Instant,
) -> Result<Value, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("agent hook graph lookup exceeded its deadline".to_string());
    }
    session.call_tool(
        name,
        arguments,
        Some(repo_root),
        remaining.min(Duration::from_millis(900)),
    )
}

fn run_graph_context(input: &Value, config_path: &Path) -> Result<String, String> {
    let config = read_install_config(config_path)?;
    let root = config_root(config_path, &config)?;
    let cwd = get_string(
        input,
        &[
            "cwd",
            "working_directory",
            "workspaceRoot",
            "workspace_root",
        ],
    )
    .map(PathBuf::from)
    .unwrap_or_else(|| root.clone());
    let cwd = cwd
        .canonicalize()
        .map_err(|error| format!("hook working directory is not readable: {error}"))?;
    if !cwd.starts_with(&root) {
        return Err(format!(
            "hook working directory {} does not belong to repository {}",
            cwd.display(),
            root.display()
        ));
    }
    let endpoint = config
        .mcp
        .as_ref()
        .and_then(|mcp| mcp.http.as_ref())
        .map(|http| http.url.clone())
        .filter(|url| !url.is_empty())
        .ok_or_else(|| {
            "setup config does not contain the managed loopback MCP endpoint".to_string()
        })?;
    let fingerprint = repository_fingerprint(&root);
    let deadline = Instant::now() + HOOK_DEADLINE;
    let session = open_session_with_budget(&endpoint, &fingerprint, deadline)?;
    let health = call_session_with_budget(
        &session,
        &root,
        "graph_health",
        json!({"include_structured_content": true, "output_format": "json"}),
        deadline,
    )?;
    let health_text = health_summary(&health);
    let prompt = prompt_from_input(input);
    let mut context = format!(
        "Repository: {}\nGraph health: {}",
        root.display(),
        health_text
    );
    if let Some(advisory) = freshness_advisory(&health) {
        context.push('\n');
        context.push_str(&advisory);
    }
    if !prompt.is_empty() {
        let search = call_session_with_budget(
            &session,
            &root,
            "graph_search",
            json!({
                "query": prompt,
                "layer": "semantic",
                "detail": "slim",
                "context_limit": 1,
                "limit": 5,
                "output_format": "block",
                "include_structured_content": false,
            }),
            deadline,
        )?;
        context.push_str("\nSemantic graph search:\n");
        context.push_str(&value_text(&search));
    }
    context.push_str(
        "\nUse graph_context with change_impact, dependencies, callgraph, or runtime for deeper analysis.",
    );
    Ok(truncate_chars(&context, MAX_CONTEXT_CHARS))
}

pub(crate) fn run_agent_hook(client: AgentHookClient, config_path: &Path, input: &Value) -> Value {
    if client == AgentHookClient::GithubCopilot && env::var_os("COPILOT_AGENT_PROMPT").is_some() {
        return json!({});
    }
    let event = event_from_input(input);
    let config_path = config_path.to_path_buf();
    let config = read_install_config(&config_path);
    let root = config
        .as_ref()
        .ok()
        .and_then(|config| config_root(&config_path, config).ok());
    if let Some(root) = root.as_deref() {
        purge_cache(root);
    }
    match event {
        HookEvent::SessionStart | HookEvent::SubagentStart => {
            match run_graph_context(input, &config_path) {
                Ok(context) => hook_output(client, event, context),
                Err(error) => hook_fallback(client, event, error),
            }
        }
        HookEvent::UserPromptSubmit => {
            let prompt = prompt_from_input(input);
            if prompt.is_empty() {
                return json!({});
            }
            match run_graph_context(input, &config_path) {
                Ok(context) => {
                    if client == AgentHookClient::GithubCopilot {
                        if let Some(root) = root.as_deref() {
                            write_cache(root, &input_session_id(input, &prompt), &prompt, &context);
                        }
                    }
                    hook_output(client, event, context)
                }
                Err(error) => hook_fallback(client, event, error),
            }
        }
        HookEvent::PreToolUse | HookEvent::PostToolUse
            if client == AgentHookClient::GithubCopilot =>
        {
            if event == HookEvent::PreToolUse
                && (!is_vscode_payload(input) || !relevant_tool(input))
            {
                return json!({});
            }
            if event == HookEvent::PostToolUse && is_vscode_payload(input) {
                return json!({});
            }
            let Some(root) = root.as_deref() else {
                return json!({});
            };
            let session = input_session_id(input, "");
            take_cached_context(root, &session)
                .map(|context| {
                    if is_vscode_payload(input) {
                        json!({
                            "hookSpecificOutput": {
                                "hookEventName": "PreToolUse",
                                "additionalContext": context,
                            },
                            "advisory": true,
                        })
                    } else {
                        json!({"additionalContext": context, "advisory": true})
                    }
                })
                .unwrap_or_else(|| json!({}))
        }
        _ => json!({}),
    }
}

pub(crate) fn read_bounded_stdin<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_STDIN_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_STDIN_BYTES {
        return Err("agent hook input exceeded the 1 MiB bound".to_string());
    }
    String::from_utf8(bytes).map_err(|error| format!("agent hook input was not UTF-8: {error}"))
}

pub(crate) fn run_agent_hook_json(
    client: AgentHookClient,
    config_path: &Path,
    input: &str,
) -> Value {
    let parsed = match serde_json::from_str::<Value>(input) {
        Ok(value) => value,
        Err(error) => {
            return hook_fallback(
                client,
                HookEvent::UserPromptSubmit,
                format!("malformed hook input: {error}"),
            )
        }
    };
    run_agent_hook(client, config_path, &parsed)
}

pub(crate) fn run_agent_hook_stdin<W: Write>(
    client: AgentHookClient,
    config_path: &Path,
    stdin: &mut impl Read,
    stdout: &mut W,
) -> Result<(), String> {
    let input = match read_bounded_stdin(stdin) {
        Ok(input) => input,
        Err(error) => {
            serde_json::to_writer(
                &mut *stdout,
                &hook_fallback(client, HookEvent::UserPromptSubmit, error),
            )
            .map_err(|e| e.to_string())?;
            writeln!(stdout).map_err(|e| e.to_string())?;
            return Ok(());
        }
    };
    serde_json::to_writer(
        &mut *stdout,
        &run_agent_hook_json(client, config_path, &input),
    )
    .map_err(|error| error.to_string())?;
    writeln!(stdout).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("codebase-graph-{label}-{nonce}"))
    }

    #[test]
    fn auto_selection_maps_supported_clients_and_ignores_other_mcp_targets() {
        assert_eq!(
            resolve_agent_hook_clients("auto", "codex").unwrap(),
            vec![AgentHookClient::Codex]
        );
        assert_eq!(
            resolve_agent_hook_clients("auto", "claude-project").unwrap(),
            vec![AgentHookClient::Claude]
        );
        assert_eq!(
            resolve_agent_hook_clients("auto", "generic").unwrap(),
            Vec::new()
        );
        assert_eq!(resolve_agent_hook_clients("all", "none").unwrap().len(), 3);
        assert!(resolve_agent_hook_clients("wat", "codex").is_err());
    }

    #[test]
    fn input_bounds_are_utf8_safe_and_malformed_input_fails_open() {
        assert_eq!(truncate_chars("ééé", 2), "éé");
        assert_eq!(truncate_utf8_bytes("ééé", 5), "éé");
        assert!(prompt_from_input(&json!({"prompt": "é".repeat(4096)})).len() <= 4096);
        let bounded = hook_output(
            AgentHookClient::Codex,
            HookEvent::SessionStart,
            "x".repeat(MAX_CONTEXT_CHARS),
        );
        assert!(
            bounded["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .chars()
                .count()
                <= MAX_CONTEXT_CHARS
        );
        let mut input = Cursor::new(vec![b'x'; MAX_STDIN_BYTES + 1]);
        assert!(read_bounded_stdin(&mut input).is_err());
        let result = run_agent_hook_json(
            AgentHookClient::Codex,
            Path::new("/missing/config.json"),
            "not-json",
        );
        assert_eq!(result["advisory"], true);
    }

    #[test]
    fn copilot_native_payloads_infer_events_without_an_event_name() {
        assert_eq!(
            event_from_input(&json!({"sessionId": "s", "cwd": "/tmp", "prompt": "find graph"})),
            HookEvent::UserPromptSubmit
        );
        assert_eq!(
            event_from_input(&json!({"sessionId": "s", "toolName": "view", "toolArgs": {}})),
            HookEvent::PreToolUse
        );
        assert_eq!(
            event_from_input(&json!({"sessionId": "s", "toolName": "view", "toolResult": {}})),
            HookEvent::PostToolUse
        );
        assert_eq!(
            event_from_input(&json!({"sessionId": "s", "source": "new"})),
            HookEvent::SessionStart
        );
    }

    #[test]
    fn copilot_cache_hashes_prompts_and_delivers_once() {
        let root = temp_root("hook-cache");
        fs::create_dir_all(&root).unwrap();
        write_cache(
            &root,
            "session",
            "private prompt text",
            "bounded graph context",
        );
        let path = cache_dir(&root).join(format!("{}.json", hash_id("session")));
        let stored = fs::read_to_string(&path).unwrap();
        assert!(!stored.contains("private prompt text"));
        let delivered = take_cached_context(&root, "session").unwrap();
        assert!(delivered.starts_with("[codebaseGraph advisory]\n"));
        assert!(delivered.contains("bounded graph context"));
        assert!(take_cached_context(&root, "session").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copilot_cache_claim_is_atomic_across_concurrent_tool_hooks() {
        let root = std::sync::Arc::new(temp_root("hook-cache-race"));
        fs::create_dir_all(root.as_ref()).unwrap();
        write_cache(root.as_ref(), "shared-session", "prompt", "graph context");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads = (0..8)
            .map(|_| {
                let root = std::sync::Arc::clone(&root);
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    take_cached_context(root.as_ref(), "shared-session")
                })
            })
            .collect::<Vec<_>>();
        let delivered = threads
            .into_iter()
            .filter_map(|thread| thread.join().unwrap())
            .count();
        assert_eq!(delivered, 1);
        fs::remove_dir_all(root.as_ref()).unwrap();
    }

    #[test]
    fn missing_freshness_is_explicitly_advisory() {
        assert!(freshness_advisory(&json!({"structuredContent": {}}))
            .unwrap()
            .contains("unknown"));
    }

    #[test]
    fn hook_target_paths_are_repository_local() {
        let root = Path::new("/tmp/repository");
        assert_eq!(
            hook_target_path(root, AgentHookClient::Codex),
            PathBuf::from("/tmp/repository/.codex/hooks.json")
        );
        assert_eq!(
            hook_target_path(root, AgentHookClient::GithubCopilot),
            PathBuf::from("/tmp/repository/.github/hooks/codebase-graph.json")
        );
    }

    #[test]
    fn merge_preserves_foreign_handlers_and_is_idempotent() {
        let config_path = Path::new("/tmp/repository/.codebaseGraph/config.json");
        let existing = json!({
            "foreign": true,
            "hooks": {
                "SessionStart": [{
                    "hooks": [{
                        "type": "command",
                        "command": "scryer hook"
                    }]
                }]
            }
        });
        let (first, _) = merge_hooks(
            AgentHookClient::Codex,
            existing,
            config_path,
            "/usr/local/bin/codebase-graph",
        )
        .unwrap();
        let (second, _) = merge_hooks(
            AgentHookClient::Codex,
            first.clone(),
            config_path,
            "/usr/local/bin/codebase-graph",
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first["foreign"], true);
        assert_eq!(
            first["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "scryer hook"
        );
        assert!(serde_json::to_string(&first).unwrap().contains(MANAGED_ID));

        let (claude, _) = merge_hooks(
            AgentHookClient::Claude,
            json!({}),
            config_path,
            "C:\\Program Files\\codebase-graph.exe",
        )
        .unwrap();
        let handler = &claude["hooks"]["SessionStart"][0]["hooks"][0];
        assert_eq!(handler["command"], "C:\\Program Files\\codebase-graph.exe");
        assert!(handler["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == MANAGED_ID));
    }

    #[test]
    fn multi_client_reconciliation_preflights_every_file_before_writing() {
        let root = temp_root("hook-transaction");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        let config = root.join(".codebaseGraph/config.json");
        write_json(
            &config,
            &json!({"schema_version": 3, "repo_root": root.clone()}),
        )
        .unwrap();
        let codex = hook_target_path(&root, AgentHookClient::Codex);
        write_json(&codex, &json!({"hooks": {}, "foreign": true})).unwrap();
        let before = fs::read(&codex).unwrap();
        let claude = hook_target_path(&root, AgentHookClient::Claude);
        fs::create_dir_all(claude.parent().unwrap()).unwrap();
        fs::write(&claude, b"{").unwrap();

        let error = reconcile_agent_hooks_with_command(
            &root,
            &config,
            "/usr/local/bin/codebase-graph",
            &[AgentHookClient::Codex, AgentHookClient::Claude],
            false,
        )
        .unwrap_err();
        assert!(error.contains("failed to parse agent hook config"));
        assert_eq!(fs::read(&codex).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
}
