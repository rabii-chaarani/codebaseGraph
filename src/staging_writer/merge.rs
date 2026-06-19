use super::rows::{EdgeRowsById, EdgeStagedRow, NodeRowsById, NodeStagedRow};
use serde_json::Value;

pub(super) fn merge_node_row(rows: &mut NodeRowsById, row_id: String, incoming: NodeStagedRow) {
    let Some(existing) = rows.get_mut(&row_id) else {
        rows.insert(row_id, incoming);
        return;
    };

    merge_string(&mut existing.id, incoming.id);
    merge_string(&mut existing.label, incoming.label);
    merge_string(&mut existing.kind, incoming.kind);
    merge_string(&mut existing.language, incoming.language);
    merge_string(&mut existing.path, incoming.path);
    merge_string(&mut existing.qualified_name, incoming.qualified_name);
    merge_string(&mut existing.scope_id, incoming.scope_id);
    merge_optional_i64(&mut existing.line_start, incoming.line_start);
    merge_optional_i64(&mut existing.line_end, incoming.line_end);
    merge_optional_i64(&mut existing.byte_start, incoming.byte_start);
    merge_optional_i64(&mut existing.byte_end, incoming.byte_end);
    merge_string(
        &mut existing.tree_sitter_node_type,
        incoming.tree_sitter_node_type,
    );
    merge_string(&mut existing.capture_name, incoming.capture_name);
    merge_string(&mut existing.summary, incoming.summary);
    merge_value(&mut existing.metadata, incoming.metadata);
    merge_optional_string(&mut existing.content_hash, incoming.content_hash);
}

pub(super) fn merge_edge_row(rows: &mut EdgeRowsById, row_id: String, incoming: EdgeStagedRow) {
    let Some(existing) = rows.get_mut(&row_id) else {
        rows.insert(row_id, incoming);
        return;
    };

    merge_string(&mut existing.id, incoming.id);
    merge_string(&mut existing.kind, incoming.kind);
    merge_string(&mut existing.source_id, incoming.source_id);
    merge_string(&mut existing.target_id, incoming.target_id);
    if (existing.confidence - 1.0).abs() < f64::EPSILON
        && (incoming.confidence - 1.0).abs() >= f64::EPSILON
    {
        existing.confidence = incoming.confidence;
    }
    merge_optional_i64(&mut existing.line_start, incoming.line_start);
    merge_optional_i64(&mut existing.line_end, incoming.line_end);
    merge_optional_i64(&mut existing.byte_start, incoming.byte_start);
    merge_optional_i64(&mut existing.byte_end, incoming.byte_end);
    merge_value(&mut existing.metadata, incoming.metadata);
}

fn merge_string(existing: &mut String, incoming: String) {
    if existing.is_empty() && !incoming.is_empty() {
        *existing = incoming;
    }
}

fn merge_optional_string(existing: &mut Option<String>, incoming: Option<String>) {
    if let Some(incoming) = incoming {
        if !incoming.is_empty() && existing.as_ref().is_none_or(|current| current.is_empty()) {
            *existing = Some(incoming);
        }
    }
}

fn merge_optional_i64(existing: &mut Option<i64>, incoming: Option<i64>) {
    if existing.is_none() && incoming.is_some() {
        *existing = incoming;
    }
}

fn merge_value(existing: &mut Value, incoming: Value) {
    if json_value_is_empty(existing) && !json_value_is_empty(&incoming) {
        *existing = incoming;
    }
}

fn json_value_is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}
