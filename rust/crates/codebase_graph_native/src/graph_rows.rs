use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphNodeRow {
    pub(crate) id: String,
    pub(crate) table: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) language: String,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphEdgeRow {
    pub(crate) id: String,
    pub(crate) edge_type: String,
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) kind: String,
    pub(crate) metadata: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BuiltGraphRows {
    pub(crate) nodes: Vec<GraphNodeRow>,
    pub(crate) edges: Vec<GraphEdgeRow>,
}
