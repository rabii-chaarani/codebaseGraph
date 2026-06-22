use super::{expand_path, McpInstallOptions};
use crate::cli::{
    constants::server_command,
    setup::{safe_name, GraphStatePaths},
    util::read_json_file,
};
use serde_json::json;
use std::path::Path;

#[derive(Debug, Clone)]
pub(in crate::cli) struct NativeMcpDescriptor {
    pub(in crate::cli) name: String,
    pub(in crate::cli) command: String,
    pub(in crate::cli) args: Vec<String>,
    pub(in crate::cli) setup_config_path: String,
    pub(in crate::cli) repo_root: String,
    pub(in crate::cli) timeout: u64,
}

impl NativeMcpDescriptor {
    pub(in crate::cli) fn as_json(&self) -> serde_json::Value {
        json!({
            "name": self.name,
            "transport": "stdio",
            "command": self.command,
            "args": self.args,
            "env": {},
            "cwd": serde_json::Value::Null,
            "setup_config_path": self.setup_config_path,
            "repo_root": self.repo_root,
            "timeout": self.timeout,
            "tool_policy": "graph_query_read_only",
        })
    }

    pub(in crate::cli) fn stdio_entry(
        &self,
        include_type: bool,
        include_timeout: bool,
    ) -> serde_json::Value {
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

pub(in crate::cli) fn build_mcp_descriptor(
    options: &McpInstallOptions,
) -> Result<NativeMcpDescriptor, String> {
    let config_path = options.config_path.clone().unwrap_or_else(|| {
        GraphStatePaths::derive(&expand_path(options.repo_root.to_string_lossy().as_ref()))
            .config_path
    });
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
                .unwrap_or_else(|| options.repo_root.clone())
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
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| format!("codebase_graph_{}", install_safe_name(&repo_name)));
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
    Ok(NativeMcpDescriptor {
        name,
        command,
        args,
        setup_config_path: config_path.to_string_lossy().to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        timeout: 60,
    })
}

pub(in crate::cli) fn install_safe_name(value: &str) -> String {
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
