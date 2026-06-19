use super::{executable_in_path, McpInstallOptions, NativeMcpDescriptor};
use crate::cli::constants::LATEST_PROTOCOL_VERSION;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    process::Command,
};

pub(in crate::cli) fn attach_install_verification(
    mut payload: serde_json::Value,
    descriptor: &NativeMcpDescriptor,
    options: &McpInstallOptions,
) -> Result<serde_json::Value, String> {
    if options.verify && !options.dry_run {
        payload["verification"] = verify_mcp_install(descriptor, &options.client);
    }
    Ok(payload)
}

pub(in crate::cli) fn verify_mcp_install(
    descriptor: &NativeMcpDescriptor,
    client: &str,
) -> serde_json::Value {
    let stdio = verify_stdio(descriptor);
    let visibility = verify_client_visibility(client, &descriptor.name);
    json!({
        "ok": stdio.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false)
            && visibility.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(true),
        "stdio": stdio,
        "client_visibility": visibility,
    })
}

pub(in crate::cli) fn verify_stdio(descriptor: &NativeMcpDescriptor) -> serde_json::Value {
    let command = descriptor_command(descriptor);
    let payload = [
        stdio_json_rpc_message(
            "initialize",
            json!({"protocolVersion": LATEST_PROTOCOL_VERSION}),
            1,
        ),
        stdio_json_rpc_message("tools/list", json!({}), 2),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_health", "arguments": {"include_structured_content": true}}),
            3,
        ),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_search", "arguments": {"query": descriptor.name, "limit": 1}}),
            4,
        ),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_query", "arguments": {"statement": "MATCH (n) DELETE n"}}),
            5,
        ),
    ]
    .join("");
    let mut process = match Command::new(&command[0])
        .args(&command[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(process) => process,
        Err(error) => return json!({"ok": false, "command": command, "error": error.to_string()}),
    };
    if let Some(mut stdin) = process.stdin.take() {
        if let Err(error) = stdin.write_all(payload.as_bytes()) {
            return json!({"ok": false, "command": command, "error": error.to_string()});
        }
    }
    let output = match process.wait_with_output() {
        Ok(output) => output,
        Err(error) => return json!({"ok": false, "command": command, "error": error.to_string()}),
    };
    let responses = parse_stdio_json_lines(&output.stdout);
    if !output.status.success() {
        return json!({
            "ok": false,
            "command": command,
            "returncode": output.status.code().unwrap_or(1),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "responses": responses,
        });
    }
    let checks = stdio_checks(&responses);
    json!({
        "ok": checks.values().all(|value| *value),
        "command": command,
        "checks": checks,
        "responses": responses,
        "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub(in crate::cli) fn verify_client_visibility(
    client: &str,
    server_name: &str,
) -> serde_json::Value {
    let Some(command) = visibility_command(client) else {
        return json!({"ok": true, "skipped": true, "reason": format!("{client} has no CLI visibility check")});
    };
    if !executable_in_path(&command[0]) {
        return json!({"ok": true, "skipped": true, "reason": format!("{} executable not found", command[0])});
    }
    match Command::new(&command[0]).args(&command[1..]).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let found = stdout.contains(server_name) || stderr.contains(server_name);
            json!({
                "ok": output.status.success() && found,
                "command": command,
                "returncode": output.status.code().unwrap_or(1),
                "found": found,
                "stdout": stdout,
                "stderr": stderr,
            })
        }
        Err(error) => json!({"ok": false, "command": command, "error": error.to_string()}),
    }
}

pub(in crate::cli) fn descriptor_command(descriptor: &NativeMcpDescriptor) -> Vec<String> {
    let mut command = vec![descriptor.command.clone()];
    command.extend(descriptor.args.clone());
    command
}

pub(in crate::cli) fn stdio_json_rpc_message(
    method: &str,
    params: serde_json::Value,
    id: u64,
) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap_or_else(|_| "{}".to_string())
        + "\n"
}

pub(in crate::cli) fn parse_stdio_json_lines(data: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(data)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

pub(in crate::cli) fn stdio_checks(responses: &[serde_json::Value]) -> BTreeMap<String, bool> {
    let mut by_id = BTreeMap::new();
    for response in responses {
        if let Some(id) = response.get("id").and_then(serde_json::Value::as_u64) {
            by_id.insert(id, response);
        }
    }
    let tools = by_id
        .get(&2)
        .and_then(|value| value.pointer("/result/tools"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tool_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut checks = BTreeMap::new();
    checks.insert(
        "initialize".to_string(),
        by_id
            .get(&1)
            .and_then(|value| value.pointer("/result/protocolVersion"))
            .and_then(serde_json::Value::as_str)
            == Some(LATEST_PROTOCOL_VERSION),
    );
    checks.insert(
        "tools_list".to_string(),
        tool_names.contains("graph_health") && tool_names.contains("graph_search"),
    );
    checks.insert(
        "graph_health".to_string(),
        by_id
            .get(&3)
            .and_then(|value| value.pointer("/result/structuredContent/ok"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    );
    checks.insert(
        "graph_search".to_string(),
        by_id
            .get(&4)
            .is_some_and(|value| value.get("error").is_none()),
    );
    checks.insert(
        "tool_error_result".to_string(),
        by_id
            .get(&5)
            .and_then(|value| value.pointer("/result/isError"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    );
    checks
}

pub(in crate::cli) fn visibility_command(client: &str) -> Option<Vec<String>> {
    match client {
        "codex" => Some(vec![
            "codex".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        "claude" | "claude-project" => Some(vec![
            "claude".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        "openclaw" => Some(vec![
            "openclaw".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        _ => None,
    }
}
