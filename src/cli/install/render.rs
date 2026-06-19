use super::{adapter_id, NativeMcpDescriptor};
use serde_json::json;

pub(in crate::cli) struct RenderedNativeConfig {
    pub(in crate::cli) text: String,
    pub(in crate::cli) action: String,
    pub(in crate::cli) entry: serde_json::Value,
    pub(in crate::cli) patch: serde_json::Value,
    pub(in crate::cli) payload: serde_json::Value,
}

pub(in crate::cli) struct RemovedNativeConfig {
    pub(in crate::cli) text: String,
    pub(in crate::cli) action: String,
    pub(in crate::cli) previous: serde_json::Value,
    pub(in crate::cli) payload: serde_json::Value,
}

pub(in crate::cli) fn render_client_config(
    client: &str,
    scope: &str,
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
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

pub(in crate::cli) fn remove_client_config(
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

pub(in crate::cli) fn render_json_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
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

pub(in crate::cli) fn remove_json_config(
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

pub(in crate::cli) fn render_codex_config(
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
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

pub(in crate::cli) fn remove_codex_config(
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

pub(in crate::cli) fn render_hermes_config(
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
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

pub(in crate::cli) fn remove_hermes_config(
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

pub(in crate::cli) fn json_container_mut<'a>(
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

pub(in crate::cli) fn action_for_json(
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

pub(in crate::cli) fn copilot_studio_metadata(
    descriptor: &NativeMcpDescriptor,
) -> serde_json::Value {
    json!({
        "kind": "copilot_studio_manual_metadata",
        "stdio": descriptor.stdio_entry(true, false),
        "http": {
            "url": "http://127.0.0.1:8765/mcp",
            "start_command": [
                descriptor.command,
                "mcp",
                "http",
                "--config",
                descriptor.setup_config_path,
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
        },
        "notes": [
            "No local client configuration file is written for Copilot Studio.",
            "Remote Copilot Studio use requires user-managed endpoint exposure, bearer-token configuration, and TLS.",
        ],
    })
}

pub(in crate::cli) fn codex_toml_block(descriptor: &NativeMcpDescriptor) -> String {
    format!(
        "[mcp_servers.{}]\ncommand = {}\nargs = {}\nstartup_timeout_sec = {}\n",
        descriptor.name,
        toml_string(&descriptor.command),
        toml_array(&descriptor.args),
        descriptor.timeout
    )
}

pub(in crate::cli) fn toml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(in crate::cli) fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(in crate::cli) fn upsert_toml_block(
    existing: &str,
    server_name: &str,
    block: &str,
) -> (String, Option<String>) {
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

pub(in crate::cli) fn remove_toml_block(
    existing: &str,
    server_name: &str,
) -> (String, Option<String>) {
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

pub(in crate::cli) fn hermes_yaml_block(descriptor: &NativeMcpDescriptor) -> String {
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

pub(in crate::cli) fn yaml_scalar(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(in crate::cli) fn upsert_marked_block(existing: &str, block: &str) -> (String, Option<String>) {
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

pub(in crate::cli) fn remove_marked_block(existing: &str) -> (String, Option<String>) {
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
