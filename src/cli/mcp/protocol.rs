use super::{options::McpServeOptions, tools::mcp_call_tool_result};
use crate::cli::{
    constants::{LATEST_PROTOCOL_VERSION, MCP_TOOL_SPECS_JSON},
    format::metadata_payload,
};
use serde_json::json;

pub(in crate::cli) fn handle_mcp_message(
    message: serde_json::Value,
    session: &mut McpSession,
    options: &McpServeOptions,
) -> Option<serde_json::Value> {
    let request_id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if method == "notifications/initialized" {
        session.initialized = true;
        return None;
    }
    if method.starts_with("notifications/") {
        return None;
    }
    if matches!(method, "tools/list" | "tools/call") && session.protocol_version.is_none() {
        return Some(rpc_error(
            request_id,
            -32002,
            "MCP session is not initialized",
        ));
    }
    let result = match method {
        "initialize" => {
            let requested = message
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let protocol_version = negotiate_protocol_version(requested);
            session.protocol_version = Some(protocol_version.to_string());
            Ok(json!({
                "protocolVersion": protocol_version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "codebase_graph", "version": env!("CARGO_PKG_VERSION")},
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => metadata_payload(MCP_TOOL_SPECS_JSON),
        "tools/call" => {
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            mcp_call_tool_result(tool_name, &arguments, options)
        }
        _ => {
            return Some(rpc_error(
                request_id,
                -32601,
                &format!("Unsupported MCP method: {method}"),
            ));
        }
    };
    match result {
        Ok(result) => Some(json!({"jsonrpc": "2.0", "id": request_id, "result": result})),
        Err(error) => Some(rpc_error(request_id, -32602, &error)),
    }
}

pub(in crate::cli) fn parse_mcp_payload(data: &[u8]) -> Result<serde_json::Value, String> {
    let payload: serde_json::Value =
        serde_json::from_slice(data).map_err(|error| error.to_string())?;
    if !payload.is_object() {
        return Err("JSON-RPC payload must be an object".to_string());
    }
    Ok(payload)
}

pub(in crate::cli) fn negotiate_protocol_version(requested: &str) -> String {
    match requested {
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05" => requested.to_string(),
        _ => LATEST_PROTOCOL_VERSION.to_string(),
    }
}

pub(in crate::cli) fn rpc_error(
    request_id: serde_json::Value,
    code: i64,
    message: &str,
) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

pub(in crate::cli) fn is_supported_protocol_version(version: &str) -> bool {
    matches!(
        version,
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05"
    )
}

#[derive(Debug, Default)]
pub(in crate::cli) struct McpSession {
    pub(in crate::cli) protocol_version: Option<String>,
    pub(in crate::cli) initialized: bool,
}
