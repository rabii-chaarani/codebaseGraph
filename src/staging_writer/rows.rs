use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub(super) type NodeRowsById = HashMap<String, NodeStagedRow>;
pub(super) type EdgeRowsById = HashMap<String, EdgeStagedRow>;

#[derive(Clone, Debug, Serialize)]
pub(super) struct NodeStagedRow {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) kind: String,
    pub(super) language: String,
    pub(super) path: String,
    pub(super) qualified_name: String,
    pub(super) scope_id: String,
    pub(super) line_start: Option<i64>,
    pub(super) line_end: Option<i64>,
    pub(super) byte_start: Option<i64>,
    pub(super) byte_end: Option<i64>,
    pub(super) tree_sitter_node_type: String,
    pub(super) capture_name: String,
    pub(super) summary: String,
    pub(super) metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) content_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EdgeStagedRow {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) source_id: String,
    pub(super) target_id: String,
    pub(super) confidence: f64,
    pub(super) line_start: Option<i64>,
    pub(super) line_end: Option<i64>,
    pub(super) byte_start: Option<i64>,
    pub(super) byte_end: Option<i64>,
    pub(super) metadata: Value,
}

pub(super) fn node_fields(node: &GraphNodeRow, content_hash: Option<&str>) -> NodeStagedRow {
    NodeStagedRow {
        id: node.id.clone(),
        label: node.label.clone(),
        kind: node.kind.clone(),
        language: node.language.clone(),
        path: node.path.clone(),
        qualified_name: node.qualified_name.clone(),
        scope_id: node.scope_id.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        tree_sitter_node_type: node.tree_sitter_node_type.clone(),
        capture_name: node.capture_name.clone(),
        summary: node.summary.clone(),
        metadata: node.metadata.clone(),
        content_hash: content_hash.map(str::to_string),
    }
}

pub(super) fn edge_fields(edge: &GraphEdgeRow) -> EdgeStagedRow {
    EdgeStagedRow {
        id: edge.id.clone(),
        kind: edge.kind.clone(),
        source_id: edge.source_id.clone(),
        target_id: edge.target_id.clone(),
        confidence: edge.confidence,
        line_start: edge.line_start,
        line_end: edge.line_end,
        byte_start: edge.byte_start,
        byte_end: edge.byte_end,
        metadata: edge.metadata.clone(),
    }
}
