use crate::error::NativeError;
use crate::hash;
use crate::legacy;
use crate::normalize::SyntaxNode;
use crate::parser::ParseOutput;
use crate::protocol::{ManifestEntry, NativeSyntaxMaterializationRequest, SourceSnapshot};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub(crate) struct GraphPartition {
    pub(crate) entry: ManifestEntry,
    pub(crate) graph_output: String,
}

pub(crate) fn build_partition(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
    parse: ParseOutput,
) -> Result<GraphPartition, NativeError> {
    let payload = encode_tree_graph_payload(request, snapshot, parse);
    let graph_output = legacy::build_tree_graph_output(&payload).map_err(NativeError::Legacy)?;
    let entry = manifest_entry(snapshot, &graph_output)?;
    Ok(GraphPartition {
        entry,
        graph_output,
    })
}

fn encode_tree_graph_payload(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
    parse: ParseOutput,
) -> String {
    let mut lines = vec![
        "TREEGRAPH".to_string(),
        format!("META\tpath\t{}", hex(&snapshot.path)),
        format!(
            "META\tlanguage\t{}",
            hex(snapshot.language.as_deref().unwrap_or(""))
        ),
        format!("META\tsource_root\t{}", hex(&request.source_root)),
        format!("META\trepository_label\t{}", hex(&request.repository_label)),
    ];
    if !request.ontology_schema.relation_types.is_empty() {
        let relation_types =
            serde_json::to_string(&request.ontology_schema.relation_types).unwrap_or_default();
        lines.push(format!(
            "META\tontology_relations\t{}",
            hex(&relation_types)
        ));
    }
    append_tree_node(&parse.root, None, &mut 0, &mut lines);
    lines.join("\n") + "\n"
}

fn append_tree_node(
    node: &SyntaxNode,
    parent_id: Option<usize>,
    next_id: &mut usize,
    lines: &mut Vec<String>,
) {
    let node_id = *next_id;
    *next_id += 1;
    let mut fields = vec![
        "NODE".to_string(),
        node_id.to_string(),
        parent_id.map(|value| value.to_string()).unwrap_or_default(),
        hex(&node.node_type),
        hex(&node.text),
        optional_i64(node.line_start),
        optional_i64(node.line_end),
        optional_i64(node.byte_start),
        optional_i64(node.byte_end),
        hex(&node.capture_name),
        node.fields.len().to_string(),
    ];
    for (key, value) in &node.fields {
        fields.push(hex(key));
        fields.push(hex(&json_token(value)));
    }
    lines.push(fields.join("\t"));
    for child in &node.children {
        append_tree_node(child, Some(node_id), next_id, lines);
    }
}

fn json_token(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn manifest_entry(
    snapshot: &SourceSnapshot,
    graph_output: &str,
) -> Result<ManifestEntry, NativeError> {
    let mut node_ids = Vec::new();
    let mut edge_ids = Vec::new();
    let mut node_types = BTreeMap::new();
    let mut edge_types = BTreeMap::new();
    for line in graph_output.lines() {
        let parts = line.split('\t').collect::<Vec<_>>();
        match parts.first().copied() {
            Some("NODE") if parts.len() >= 3 => {
                let id = unhex(parts[1])?;
                let table = unhex(parts[2])?;
                node_types.insert(id.clone(), table);
                node_ids.push(id);
            }
            Some("EDGE") if parts.len() >= 3 => {
                let id = unhex(parts[1])?;
                let edge_type = unhex(parts[2])?;
                edge_types.insert(id.clone(), edge_type);
                edge_ids.push(id);
            }
            _ => {}
        }
    }
    node_ids.sort();
    edge_ids.sort();
    Ok(ManifestEntry {
        path: snapshot.path.clone(),
        content_hash: snapshot.content_hash.clone(),
        language: snapshot.language.clone().unwrap_or_default(),
        partition_id: hash::partition_id(&snapshot.path),
        node_ids,
        edge_ids,
        node_types,
        edge_types,
        materialized_at: materialized_at(),
    })
}

fn materialized_at() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("unix:{}", duration.as_secs()),
        Err(_) => "unix:0".to_string(),
    }
}

pub(crate) fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

pub(crate) fn unhex(value: &str) -> Result<String, NativeError> {
    if !value.len().is_multiple_of(2) {
        return Err(NativeError::InvalidInput(
            "hex value has odd length".to_string(),
        ));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|error| NativeError::InvalidInput(error.to_string()))?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).map_err(|error| NativeError::InvalidInput(error.to_string()))
}

fn optional_i64(value: Option<i64>) -> String {
    value.map(|number| number.to_string()).unwrap_or_default()
}
