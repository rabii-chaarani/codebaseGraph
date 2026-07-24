use super::{block::serialize_error_block, options::McpServeOptions};
use crate::api::normalization::required_fields;
use crate::api::{
    contracts::{
        ApiError, ContextRequest, HealthRequest, OperationRequest, OperationResponse, OutputFormat,
        QueryRequest, RepoSelector, SearchRequest,
    },
    CodebaseGraphApi, OperationDescriptor,
};
use serde_json::json;
use serde_json::Map;

pub(in crate::adapters) fn generate_mcp_specs() -> Result<serde_json::Value, String> {
    let tools = CodebaseGraphApi::new()
        .operation_descriptors()
        .into_iter()
        .filter(|operation| operation.surfaces.contains(&"mcp"))
        .filter_map(operation_spec_from_descriptor)
        .collect::<Vec<_>>();
    Ok(json!({"tools": tools}))
}

fn operation_spec_from_descriptor(descriptor: OperationDescriptor) -> Option<serde_json::Value> {
    let name = descriptor.mcp_tool_name?;
    let properties = (descriptor.request_schema)();
    let mut property_map: Map<String, serde_json::Value> =
        properties.as_object().cloned().unwrap_or_default();
    property_map.insert(
        "output_format".to_string(),
        json!({"type":"string","enum":["json","block"],"default":"block"}),
    );
    property_map.insert(
        "include_structured_content".to_string(),
        json!({"type":"boolean","default":false,"description":"Include the MCP structuredContent payload alongside the text result."}),
    );

    Some(json!({
        "name": name,
        "description": descriptor.summary,
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": property_map,
            "required": required_fields(descriptor.id),
        },
    }))
}

fn map_error_to_transport(tool_name: &str, error: &ApiError) -> serde_json::Value {
    json!({
        "error": {
            "tool": tool_name,
            "type": "ValueError",
            "code": error.code,
            "message": error.message,
            "details": error.details,
            "retryable": error.retryable,
        }
    })
}

pub(in crate::adapters) fn mcp_call_tool_result(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    let output_format = arguments
        .get("output_format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("block");
    let output_format = match output_format {
        "json" => "json",
        "block" => "block",
        _ => return Err("graph tool output_format must be \"json\" or \"block\"".to_string()),
    };
    let response = mcp_tool_payload(tool_name, arguments, options);
    let include_structured = arguments
        .get("include_structured_content")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match response {
        Ok(response) => {
            let payload = response.payload;
            let (text, structured) = if output_format == "json" {
                (
                    serde_json::to_string(&payload).map_err(|error| error.to_string())?,
                    payload.clone(),
                )
            } else {
                let text = payload
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "block response did not contain text".to_string())?
                    .to_string();
                let structured = payload
                    .get("structured")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                (text, structured)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": false,
            });
            if include_structured {
                result["structuredContent"] = structured;
            }
            Ok(result)
        }
        Err(error)
            if tool_name.is_empty()
                || error.message.starts_with("Unknown codebaseGraph MCP tool") =>
        {
            Err(error.message)
        }
        Err(error) => {
            let payload = map_error_to_transport(tool_name, &error);
            let text = if output_format == "json" {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            } else {
                serialize_error_block(&payload)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": true,
            });
            if include_structured {
                result["structuredContent"] = payload;
            }
            Ok(result)
        }
    }
}

pub(in crate::adapters) fn mcp_tool_payload(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<OperationResponse, ApiError> {
    let _refresh_read_guard = if matches!(
        tool_name,
        "graph_health" | "graph_search" | "graph_context" | "graph_query"
    ) {
        options
            .refresh
            .as_ref()
            .map(|refresh| refresh.read_guard())
            .transpose()
            .map_err(|error| ApiError::new("refresh_lock_failed", error))?
    } else {
        None
    };
    let output_format = match arguments
        .get("output_format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("block")
    {
        "json" => OutputFormat::Typed,
        "block" => OutputFormat::Block,
        _ => {
            return Err(ApiError::new(
                "invalid_output_format",
                "graph tool output_format must be \"json\" or \"block\"",
            ))
        }
    };
    match tool_name {
        "graph_health" => execute_api_request(OperationRequest::Health(HealthRequest {
            repo: repo_selector_from_mcp_options(options),
            refresh_status: options
                .refresh
                .as_ref()
                .map(|refresh| serde_json::to_value(refresh.as_json()).unwrap_or_default()),
            output_format,
        })),
        "graph_schema" => execute_api_request(OperationRequest::Catalog {
            kind: "schema".to_string(),
            group: None,
            output_format,
        }),
        "graph_query_helpers" => execute_api_request(OperationRequest::Catalog {
            kind: "query-helpers".to_string(),
            group: None,
            output_format,
        }),
        "graph_architecture_queries" => execute_api_request(OperationRequest::Catalog {
            kind: "architecture-queries".to_string(),
            group: arguments
                .get("group")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            output_format,
        }),
        "graph_search" => {
            let search = search_request_from_mcp(arguments, options, output_format);
            execute_api_request(OperationRequest::Search(search))
        }
        "graph_context" => {
            let context = context_request_from_mcp(arguments, options, output_format);
            execute_api_request(OperationRequest::Context(context))
        }
        "graph_query" => {
            let statement = arguments
                .get("statement")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let parameters = arguments
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let limit = arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(100) as usize;
            execute_api_request(OperationRequest::Query(QueryRequest {
                repo: repo_selector_from_mcp_options(options),
                statement,
                parameters,
                limit,
                output_format,
            }))
        }
        _ => Err(ApiError::new(
            "unknown_tool",
            format!("Unknown codebaseGraph MCP tool: {tool_name}"),
        )),
    }
}

fn execute_api_request(request: OperationRequest) -> Result<OperationResponse, ApiError> {
    CodebaseGraphApi::new().execute_operation(&request)
}

fn repo_selector_from_mcp_options(options: &McpServeOptions) -> RepoSelector {
    RepoSelector {
        repo_root: options.repo_root.clone(),
        config_path: options.config.clone(),
        db_path: options.db.clone(),
        manifest_path: options.manifest.clone(),
    }
}

fn search_request_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
    output_format: OutputFormat,
) -> SearchRequest {
    let query = arguments
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let detail = arguments
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("standard");
    SearchRequest {
        repo: repo_selector_from_mcp_options(options),
        query,
        limit: json_usize(arguments, "limit", 3),
        profile: arguments
            .get("profile")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("brief")
            .to_string(),
        budget: json_usize(arguments, "budget", 600),
        context_limit: json_usize(arguments, "context_limit", 3),
        max_depth: arguments
            .get("max_depth")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize),
        detail: detail.to_string(),
        output_format,
    }
}

fn context_request_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
    output_format: OutputFormat,
) -> ContextRequest {
    let node_id = arguments
        .get("node_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let node_type = arguments
        .get("node_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let search = search_request_from_mcp(arguments, options, output_format);
    ContextRequest {
        repo: search.repo,
        query: if search.query.is_empty() {
            None
        } else {
            Some(search.query)
        },
        profile: search.profile,
        limit: search.limit,
        budget: search.budget,
        context_limit: search.context_limit,
        max_depth: search.max_depth,
        detail: search.detail,
        node_id,
        node_type,
        output_format,
    }
}

pub(in crate::adapters) fn json_usize(
    arguments: &serde_json::Value,
    key: &str,
    default: usize,
) -> usize {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_errors_map_to_mcp_failures_without_changing_codes() {
        let error = ApiError::new("graph_busy", "graph is busy")
            .with_details(json!({"operation": "query"}))
            .retryable(true);

        let payload = map_error_to_transport("graph_query", &error);

        assert_eq!(payload["error"]["tool"], "graph_query");
        assert_eq!(payload["error"]["code"], "graph_busy");
        assert_eq!(payload["error"]["message"], "graph is busy");
        assert_eq!(payload["error"]["details"]["operation"], "query");
        assert_eq!(payload["error"]["retryable"], true);
    }

    #[test]
    fn mcp_specs_are_generated_from_registered_operation_metadata() {
        let specs = generate_mcp_specs().expect("MCP specs should generate");
        let tools = specs["tools"].as_array().expect("tools should be an array");
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "graph_architecture_queries",
                "graph_context",
                "graph_health",
                "graph_query",
                "graph_query_helpers",
                "graph_schema",
                "graph_search",
            ]
        );
        let search = tools
            .iter()
            .find(|tool| tool["name"] == "graph_search")
            .expect("search tool should be advertised");
        assert_eq!(
            search["inputSchema"]["properties"]["detail"]["enum"],
            json!(["standard", "slim"])
        );
        assert_eq!(search["inputSchema"]["required"], json!(["query"]));
    }
}
