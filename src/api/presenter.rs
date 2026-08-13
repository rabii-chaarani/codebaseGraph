use crate::api::contracts::{OperationResponse, OutputFormat};
#[path = "presentation_support.rs"]
mod support;
use self::support::{
    serialize_architecture_queries_block, serialize_context_block, serialize_health_block,
    serialize_plan_block, serialize_query_block, serialize_query_helpers_block,
    serialize_schema_block, serialize_search_block, serialize_syntax_block,
    serialize_uninstall_block,
};

fn present_block(operation: &str, payload: &serde_json::Value) -> String {
    match operation {
        "health" => serialize_health_block(payload),
        "search" => serialize_search_block(payload),
        "context" => {
            if payload.get("context").is_some() {
                serialize_context_block(payload)
            } else {
                serialize_search_block(payload)
            }
        }
        "query" => serialize_query_block(payload),
        "schema" => serialize_schema_block(payload),
        "syntax" => serialize_syntax_block(payload),
        "query-helpers" => serialize_query_helpers_block(payload),
        "architecture-queries" => serialize_architecture_queries_block(payload),
        "plan" => serialize_plan_block(payload),
        "uninstall" => serialize_uninstall_block(payload),
        _ => serde_json::to_string_pretty(payload).unwrap_or_else(|_| String::new()),
    }
}

pub fn present_operation_response(
    mut response: OperationResponse,
    output_format: OutputFormat,
) -> OperationResponse {
    response.output_format = output_format;
    if output_format == OutputFormat::Block {
        let text = present_block(&response.operation, &response.payload);
        response.payload = serde_json::json!({
            "text": text,
            "structured": response.payload,
        });
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selected_output_format_presents_typed_or_block_response() {
        let payload = json!({
            "statement": "MATCH (n) RETURN n",
            "row_count": 1,
            "rows": [{"n": "node-1"}],
            "truncated": false,
        });
        let typed = present_operation_response(
            OperationResponse::from_payload("query", OutputFormat::Typed, payload.clone()),
            OutputFormat::Typed,
        );
        assert_eq!(typed.payload, payload);
        assert_eq!(typed.output_format, OutputFormat::Typed);

        let block = present_operation_response(
            OperationResponse::from_payload("query", OutputFormat::Typed, payload.clone()),
            OutputFormat::Block,
        );
        assert_eq!(block.output_format, OutputFormat::Block);
        assert_eq!(block.payload["structured"], payload);
        assert!(block.payload["text"]
            .as_str()
            .expect("block text should be present")
            .starts_with("query rows=1"));
    }
}
