use super::{TransportError, TransportPayload};
use crate::api::{
    mcp_operation_descriptor, mcp_operation_descriptors, AccessMode, OkfWikiApi,
    WikiOperationExecutor, WikiOperationRequest,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

const SERVER_NAME: &str = "Knowledge Wiki";
const PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAccess {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolDefinition {
    pub identifier: &'static str,
    pub display_name: &'static str,
    pub access: ToolAccess,
    pub description: &'static str,
}

pub fn tools() -> Vec<ToolDefinition> {
    mcp_operation_descriptors()
        .into_iter()
        .map(|descriptor| ToolDefinition {
            identifier: descriptor
                .mcp_tool_name
                .expect("MCP descriptor must have a tool name"),
            display_name: descriptor
                .mcp_display_name
                .expect("MCP descriptor must have a display name"),
            access: match descriptor.access {
                AccessMode::Read => ToolAccess::Read,
                AccessMode::Write => ToolAccess::Write,
            },
            description: descriptor.summary,
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct McpSession {
    pub protocol_version: Option<String>,
    pub initialized: bool,
}

pub fn server_name() -> &'static str {
    SERVER_NAME
}

pub fn protocol_version() -> &'static str {
    PROTOCOL_VERSION
}

/// Converts one declared MCP tool call into one typed public API dispatch.
pub fn dispatch_api<E>(
    api: &OkfWikiApi<E>,
    tool_name: &str,
    arguments: &Value,
) -> Result<TransportPayload, TransportError>
where
    E: WikiOperationExecutor,
{
    let request = request_for_tool(tool_name, arguments)?;
    let response = api
        .execute_operation(&request)
        .map_err(|error| TransportError {
            code: error.code,
            message: error.message,
            details: error.details,
            retryable: error.retryable,
        })?;
    let structured = serde_json::to_value(&response)
        .map_err(|_| TransportError::new("invalid_request", "response could not be serialized"))?;
    let text = serde_json::to_string(&response)
        .map_err(|_| TransportError::new("invalid_request", "response could not be serialized"))?;
    Ok(TransportPayload::structured(text, structured))
}

pub fn generate_tool_specs<F>(mut schema_provider: F) -> Value
where
    F: FnMut(&'static str) -> Value,
{
    let tools = tools();
    json!({
        "tools": tools.iter().map(|tool| {
            json!({
                "name": tool.identifier,
                "title": tool.display_name,
                "description": tool.description,
                "inputSchema": schema_provider(tool.identifier),
                "annotations": {
                    "readOnlyHint": matches!(tool.access, ToolAccess::Read),
                    "wikiAccess": match tool.access {
                        ToolAccess::Read => "read",
                        ToolAccess::Write => "write",
                    },
                }
            })
        }).collect::<Vec<_>>()
    })
}

pub fn handle_message<F, G>(
    message: Value,
    session: &mut McpSession,
    schema_provider: &mut F,
    dispatch: &mut G,
) -> Option<Value>
where
    F: FnMut(&'static str) -> Value,
    G: FnMut(&str, &Value) -> Result<TransportPayload, TransportError>,
{
    let request_id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();

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
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = negotiate_protocol_version(requested);
            session.protocol_version = Some(version.to_string());
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION")
                }
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(generate_tool_specs(schema_provider)),
        "tools/call" => {
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_tool(tool_name, &arguments, dispatch)
        }
        _ => Err(TransportError::new(
            "unsupported_method",
            format!("Unsupported MCP method: {method}"),
        )),
    };

    match result {
        Ok(payload) => Some(json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": payload
        })),
        Err(error) if error.code == "unknown_tool" => {
            Some(rpc_error(request_id, -32602, &error.message))
        }
        Err(error) if error.code == "unsupported_method" => {
            Some(rpc_error(request_id, -32601, &error.message))
        }
        Err(error) => Some(rpc_error(request_id, -32602, &error.message)),
    }
}

pub fn negotiate_protocol_version(requested: &str) -> &'static str {
    match requested {
        "2025-11-25" => "2025-11-25",
        "2025-06-18" => "2025-06-18",
        "2025-03-26" => "2025-03-26",
        "2024-11-05" => "2024-11-05",
        _ => PROTOCOL_VERSION,
    }
}

pub fn rpc_error(request_id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn call_tool<G>(
    tool_name: &str,
    arguments: &Value,
    dispatch: &mut G,
) -> Result<Value, TransportError>
where
    G: FnMut(&str, &Value) -> Result<TransportPayload, TransportError>,
{
    let descriptor = mcp_operation_descriptor(tool_name).ok_or_else(|| {
        TransportError::new(
            "unknown_tool",
            format!("Unknown Knowledge Wiki MCP tool: {tool_name}"),
        )
    })?;
    let tool = ToolDefinition {
        identifier: descriptor
            .mcp_tool_name
            .expect("MCP descriptor must have a tool name"),
        display_name: descriptor
            .mcp_display_name
            .expect("MCP descriptor must have a display name"),
        access: match descriptor.access {
            AccessMode::Read => ToolAccess::Read,
            AccessMode::Write => ToolAccess::Write,
        },
        description: descriptor.summary,
    };
    let payload = dispatch(tool.identifier, arguments)?;
    let include_structured = arguments
        .get("include_structured_content")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut result = json!({
        "content": [{"type": "text", "text": payload.text}],
        "isError": false,
    });
    if include_structured {
        result["structuredContent"] = payload.structured;
    }
    if matches!(tool.access, ToolAccess::Write) {
        result["annotations"] = json!({"writeOperation": true});
    }
    Ok(result)
}

fn request_for_tool(
    tool_name: &str,
    arguments: &Value,
) -> Result<WikiOperationRequest, TransportError> {
    let descriptor = mcp_operation_descriptor(tool_name).ok_or_else(|| {
        TransportError::new(
            "unknown_tool",
            format!("Unknown Knowledge Wiki MCP tool: {tool_name}"),
        )
    })?;
    let arguments = tool_arguments(arguments);
    match descriptor.id {
        "validate_bundle" => deserialize(arguments).map(WikiOperationRequest::ValidateBundle),
        "check_links" => deserialize(arguments).map(WikiOperationRequest::CheckLinks),
        "list_bundles" => deserialize(arguments).map(WikiOperationRequest::ListBundles),
        "get_directory" => deserialize(arguments).map(WikiOperationRequest::GetDirectory),
        "get_concept" => deserialize(arguments).map(WikiOperationRequest::GetConcept),
        "search_concepts" => deserialize(arguments).map(WikiOperationRequest::SearchConcepts),
        "get_backlinks" => deserialize(arguments).map(WikiOperationRequest::GetBacklinks),
        "get_neighborhood" => deserialize(arguments).map(WikiOperationRequest::GetNeighborhood),
        "get_diagnostics" => deserialize(arguments).map(WikiOperationRequest::GetDiagnostics),
        "get_recent_changes" => deserialize(arguments).map(WikiOperationRequest::GetRecentChanges),
        "create_bundle" => deserialize(arguments).map(WikiOperationRequest::CreateBundle),
        "create_page" => deserialize(arguments).map(WikiOperationRequest::CreatePage),
        "populate_page" => deserialize(arguments).map(WikiOperationRequest::PopulatePage),
        "build_site" => deserialize(arguments).map(WikiOperationRequest::BuildSite),
        _ => Err(TransportError::new(
            "unknown_tool",
            format!("Unknown Knowledge Wiki MCP tool: {tool_name}"),
        )),
    }
}

fn tool_arguments(arguments: &Value) -> Value {
    let mut arguments = arguments.clone();
    if let Some(object) = arguments.as_object_mut() {
        object.remove("include_structured_content");
    }
    arguments
}

fn deserialize<T>(value: Value) -> Result<T, TransportError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value)
        .map_err(|_| TransportError::new("invalid_request", "tool arguments are invalid"))
}
