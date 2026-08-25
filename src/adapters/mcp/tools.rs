use super::{block::serialize_error_block, options::McpServeOptions};
use crate::api::{
    ApiError, CodebaseGraphApi, OperationDescriptor, OperationInvocation, OperationResponse,
    OutputFormat,
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
            "required": descriptor.required_fields(),
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
    let output_format = parse_output_format(arguments).map_err(|error| error.message)?;
    let response = mcp_tool_payload(tool_name, arguments, options, output_format);
    let include_structured = arguments
        .get("include_structured_content")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match response {
        Ok(response) => {
            let payload = response.payload;
            let (text, structured) = if output_format == OutputFormat::Typed {
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
        Err(error) if tool_name.is_empty() || error.code == "unknown_tool" => Err(error.message),
        Err(error) => {
            let payload = map_error_to_transport(tool_name, &error);
            let text = if output_format == OutputFormat::Typed {
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
    output_format: OutputFormat,
) -> Result<OperationResponse, ApiError> {
    let operation = CodebaseGraphApi::new()
        .resolve_mcp_operation(tool_name)
        .ok_or_else(|| {
            ApiError::new(
                "unknown_tool",
                format!("Unknown codebaseGraph MCP tool: {tool_name}"),
            )
        })?;
    let invocation = OperationInvocation {
        repo: options.repo_selector(),
        arguments: arguments.clone(),
        output_format,
    };
    match options.api.as_ref() {
        Some(api) => api.execute_invocation(operation.id, &invocation),
        None => CodebaseGraphApi::new().execute_invocation(operation.id, &invocation),
    }
}

fn parse_output_format(arguments: &serde_json::Value) -> Result<OutputFormat, ApiError> {
    match arguments
        .get("output_format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("block")
    {
        "json" => Ok(OutputFormat::Typed),
        "block" => Ok(OutputFormat::Block),
        _ => Err(ApiError::new(
            "invalid_output_format",
            "graph tool output_format must be \"json\" or \"block\"",
        )),
    }
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
                "graph_syntax",
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
        assert_eq!(
            search["inputSchema"]["properties"]["profile"]["default"],
            "brief"
        );
        assert_eq!(search["inputSchema"]["properties"]["limit"]["default"], 3);
        assert_eq!(
            search["inputSchema"]["properties"]["layer"],
            json!({"type":"string","enum":["semantic","syntax","hybrid"],"default":"semantic"})
        );
        assert_eq!(search["inputSchema"]["required"], json!(["query"]));
        let context = tools
            .iter()
            .find(|tool| tool["name"] == "graph_context")
            .expect("context tool should be advertised");
        assert_eq!(
            context["inputSchema"]["properties"]["layer"],
            json!({"type":"string","enum":["semantic","syntax","hybrid"],"default":"semantic"})
        );
        let syntax = tools
            .iter()
            .find(|tool| tool["name"] == "graph_syntax")
            .expect("syntax tool should be advertised");
        assert_eq!(syntax["inputSchema"]["required"], json!(["language"]));
        assert_eq!(
            syntax["inputSchema"]["properties"]["language"]["enum"],
            json!([
                "c",
                "cpp",
                "css",
                "fortran",
                "go",
                "javascript",
                "markdown",
                "python",
                "rust",
                "tsx",
                "typescript"
            ])
        );
    }
}
