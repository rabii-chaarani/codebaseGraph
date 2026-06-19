use super::options::McpServeOptions;
use crate::product_cli::{
    constants::{ARCHITECTURE_QUERIES_JSON, GRAPH_SCHEMA_JSON, QUERY_HELPERS_JSON},
    format::{
        filter_architecture_group, metadata_payload, serialize_architecture_queries_block,
        serialize_context_block, serialize_error_block, serialize_health_block,
        serialize_query_block, serialize_query_helpers_block, serialize_schema_block,
        serialize_search_block,
    },
    graph::{
        count_graph_nodes, execute_graph_context, execute_graph_search, execute_read_only_query,
        resolve_health_runtime, validate_read_only_statement, GraphContextOptions,
        GraphSearchOptions, MetadataOutputOptions,
    },
};
use serde_json::json;

pub(in crate::product_cli) fn mcp_call_tool_result(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    let payload = mcp_tool_payload(tool_name, arguments, options);
    let output_format = arguments
        .get("output_format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("block");
    let include_structured = arguments
        .get("include_structured_content")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match payload {
        Ok(payload) => {
            let text = if output_format == "json" {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            } else {
                mcp_block_text(tool_name, &payload)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": false,
            });
            if include_structured {
                result["structuredContent"] = payload;
            }
            Ok(result)
        }
        Err(error)
            if tool_name.is_empty() || error.starts_with("Unknown codebaseGraph MCP tool") =>
        {
            Err(error)
        }
        Err(error) => {
            let payload = json!({
                "error": {
                    "tool": tool_name,
                    "type": "ValueError",
                    "message": error,
                }
            });
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

pub(in crate::product_cli) fn mcp_tool_payload(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    match tool_name {
        "graph_health" => graph_health_payload(options),
        "graph_schema" => metadata_payload(GRAPH_SCHEMA_JSON),
        "graph_query_helpers" => metadata_payload(QUERY_HELPERS_JSON),
        "graph_architecture_queries" => {
            let mut payload = metadata_payload(ARCHITECTURE_QUERIES_JSON)?;
            if let Some(group) = arguments.get("group").and_then(serde_json::Value::as_str) {
                filter_architecture_group(&mut payload, group)?;
            }
            Ok(payload)
        }
        "graph_search" => {
            let search = graph_search_options_from_mcp(arguments, options, true)?;
            let runtime = resolve_health_runtime(&options.health_options())?;
            let results = execute_graph_search(&runtime.db_path, &search)?;
            Ok(json!({
                "query": search.query,
                "profile": search.profile,
                "limit": search.limit,
                "budget": search.budget,
                "results": results,
            }))
        }
        "graph_context" => {
            let context = graph_context_options_from_mcp(arguments, options)?;
            let runtime = resolve_health_runtime(&options.health_options())?;
            if let (Some(node_id), Some(node_type)) =
                (context.node_id.as_ref(), context.node_type.as_ref())
            {
                let rows =
                    execute_graph_context(&runtime.db_path, node_id, node_type, &context.search)?;
                Ok(json!({
                    "node_id": node_id,
                    "node_type": node_type,
                    "profile": context.search.profile,
                    "context": rows,
                }))
            } else {
                let results = execute_graph_search(&runtime.db_path, &context.search)?;
                Ok(json!({
                    "query": context.search.query,
                    "profile": context.search.profile,
                    "limit": context.search.limit,
                    "budget": context.search.budget,
                    "results": results,
                }))
            }
        }
        "graph_query" => {
            let statement = arguments
                .get("statement")
                .or_else(|| arguments.get("query"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();
            if statement.is_empty() {
                return Err("graph_query requires a non-empty statement".to_string());
            }
            validate_read_only_statement(statement)?;
            let parameters = arguments
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let parameters = parameters
                .as_object()
                .ok_or_else(|| "graph_query parameters must be a JSON object".to_string())?;
            let limit = arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(100) as usize;
            if limit == 0 || limit > 1000 {
                return Err("graph_query limit must be between 1 and 1000".to_string());
            }
            let runtime = resolve_health_runtime(&options.health_options())?;
            let (rows, truncated) =
                execute_read_only_query(&runtime.db_path, statement, parameters, limit)?;
            Ok(json!({
                "statement": statement,
                "row_count": rows.len(),
                "rows": rows,
                "truncated": truncated,
            }))
        }
        _ => Err(format!("Unknown codebaseGraph MCP tool: {tool_name}")),
    }
}

pub(in crate::product_cli) fn graph_health_payload(
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    let runtime = resolve_health_runtime(&options.health_options())?;
    let database_exists = runtime.db_path.exists();
    let manifest_exists = runtime.manifest_path.exists();
    let mut graph_readable = false;
    let mut total_nodes = 0_u64;
    let mut error_message = None;
    if database_exists {
        match count_graph_nodes(&runtime.db_path) {
            Ok(count) => {
                graph_readable = true;
                total_nodes = count;
            }
            Err(error) => error_message = Some(error),
        }
    }
    Ok(json!({
        "ok": database_exists && graph_readable,
        "repo_root": runtime.repo_root,
        "database_path": runtime.db_path,
        "manifest_path": runtime.manifest_path,
        "database_exists": database_exists,
        "manifest_exists": manifest_exists,
        "graph_readable": graph_readable,
        "total_nodes": total_nodes,
        "error": error_message,
    }))
}

pub(in crate::product_cli) fn graph_search_options_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
    require_query: bool,
) -> Result<GraphSearchOptions, String> {
    let query = arguments
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    if require_query && query.trim().is_empty() {
        return Err("Search query must not be empty".to_string());
    }
    let detail = arguments
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("standard");
    if detail != "standard" && detail != "slim" {
        return Err("--detail must be standard or slim".to_string());
    }
    Ok(GraphSearchOptions {
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
        repo_root: options.repo_root.clone(),
        config: options.config.clone(),
        db: options.db.clone(),
        manifest: options.manifest.clone(),
        output: MetadataOutputOptions {
            format: arguments
                .get("output_format")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("block")
                .to_string(),
            pretty: false,
            help: false,
        },
    })
}

pub(in crate::product_cli) fn graph_context_options_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<GraphContextOptions, String> {
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
    if node_id.is_some() != node_type.is_some() {
        return Err(
            "graph-context explicit lookup requires both --node-id and --node-type".to_string(),
        );
    }
    let search = graph_search_options_from_mcp(arguments, options, node_id.is_none())?;
    Ok(GraphContextOptions {
        search,
        node_id,
        node_type,
    })
}

pub(in crate::product_cli) fn json_usize(
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

pub(in crate::product_cli) fn mcp_block_text(
    tool_name: &str,
    payload: &serde_json::Value,
) -> String {
    match tool_name {
        "graph_health" => serialize_health_block(payload),
        "graph_schema" => serialize_schema_block(payload),
        "graph_query_helpers" => serialize_query_helpers_block(payload),
        "graph_architecture_queries" => serialize_architecture_queries_block(payload),
        "graph_search" => serialize_search_block(payload),
        "graph_context" => {
            if payload.get("context").is_some() {
                serialize_context_block(payload)
            } else {
                serialize_search_block(payload)
            }
        }
        "graph_query" => serialize_query_block(payload),
        _ => serde_json::to_string(payload).unwrap_or_default(),
    }
}
