use super::{home_dir, NativeMcpDescriptor};
use std::{
    env,
    path::{Path, PathBuf},
};

pub(in crate::product_cli) fn default_client_config_path(
    client: &str,
    scope: &str,
    descriptor: &NativeMcpDescriptor,
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
        "claude-project" => PathBuf::from(&descriptor.repo_root).join(".mcp.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "github-copilot" => PathBuf::from(&descriptor.repo_root).join(".vscode/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"))
            .join("mcp.json5"),
        _ => home.join(".config/mcp/mcp.json"),
    }
}

pub(in crate::product_cli) fn supported_install_clients() -> Vec<&'static str> {
    vec![
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

pub(in crate::product_cli) fn supported_install_clients_with_all() -> Vec<&'static str> {
    let mut clients = supported_install_clients();
    clients.push("all");
    clients
}

pub(in crate::product_cli) fn install_scope(client: &str, scope: &str) -> String {
    if client == "claude-project" {
        "project".to_string()
    } else {
        scope.to_string()
    }
}

pub(in crate::product_cli) fn adapter_id<'a>(client: &'a str, scope: &str) -> &'a str {
    if client == "claude" && scope == "project" {
        "claude-project"
    } else {
        client
    }
}

pub(in crate::product_cli) fn native_client_command(
    client: &str,
    descriptor: &NativeMcpDescriptor,
    scope: &str,
) -> Option<Vec<String>> {
    match client {
        "codex" => Some(vec![
            "codex".to_string(),
            "mcp".to_string(),
            "add".to_string(),
            descriptor.name.clone(),
            "--".to_string(),
            descriptor.command.clone(),
            descriptor.args[0].clone(),
            descriptor.args[1].clone(),
            descriptor.args[2].clone(),
            descriptor.args[3].clone(),
        ]),
        "claude" | "claude-project" => Some(vec![
            "claude".to_string(),
            "mcp".to_string(),
            "add".to_string(),
            "--transport".to_string(),
            "stdio".to_string(),
            "--scope".to_string(),
            install_scope(client, scope),
            descriptor.name.clone(),
            "--".to_string(),
            descriptor.command.clone(),
            descriptor.args[0].clone(),
            descriptor.args[1].clone(),
            descriptor.args[2].clone(),
            descriptor.args[3].clone(),
        ]),
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
