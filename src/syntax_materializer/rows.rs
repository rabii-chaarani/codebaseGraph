use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct GraphNodeRow {
    pub(crate) id: String,
    pub(crate) table: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) grammar_version: Option<String>,
    pub(crate) path: String,
    pub(crate) qualified_name: String,
    pub(crate) scope_id: String,
    pub(crate) line_start: Option<i64>,
    pub(crate) line_end: Option<i64>,
    pub(crate) byte_start: Option<i64>,
    pub(crate) byte_end: Option<i64>,
    pub(crate) tree_sitter_node_type: String,
    pub(crate) capture_name: String,
    pub(crate) summary: String,
    pub(crate) metadata: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct GraphEdgeRow {
    pub(crate) id: String,
    pub(crate) edge_type: String,
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) kind: String,
    pub(crate) confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) field_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) child_index: Option<i64>,
    pub(crate) line_start: Option<i64>,
    pub(crate) line_end: Option<i64>,
    pub(crate) byte_start: Option<i64>,
    pub(crate) byte_end: Option<i64>,
    pub(crate) metadata: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct BuiltGraphRows {
    pub(crate) nodes: Vec<GraphNodeRow>,
    pub(crate) edges: Vec<GraphEdgeRow>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn graph_rows_roundtrip_through_json() {
        let node = GraphNodeRow {
            id: "node:1".to_string(),
            table: "Function".to_string(),
            label: "main".to_string(),
            kind: "definition.function".to_string(),
            language: "rust".to_string(),
            grammar_version: Some("tree_sitter_rust@0.24.2".to_string()),
            path: "src/main.rs".to_string(),
            qualified_name: "crate::main".to_string(),
            scope_id: "scope:root".to_string(),
            line_start: Some(1),
            line_end: Some(3),
            byte_start: Some(0),
            byte_end: Some(42),
            tree_sitter_node_type: "function_item".to_string(),
            capture_name: "definition.function".to_string(),
            summary: "fn main()".to_string(),
            metadata: json!({"role":"entrypoint"}),
        };
        let edge = GraphEdgeRow {
            id: "edge:1".to_string(),
            edge_type: "Calls".to_string(),
            source_id: "node:1".to_string(),
            target_id: "node:2".to_string(),
            kind: "reference.call".to_string(),
            confidence: 0.9,
            field_name: Some("function".to_string()),
            child_index: Some(0),
            line_start: Some(2),
            line_end: Some(2),
            byte_start: Some(10),
            byte_end: Some(18),
            metadata: json!({"via":"call_expression"}),
        };

        let encoded = serde_json::to_string(&(node.clone(), edge.clone())).unwrap();
        let decoded: (GraphNodeRow, GraphEdgeRow) = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.0, node);
        assert_eq!(decoded.1, edge);
    }
}
