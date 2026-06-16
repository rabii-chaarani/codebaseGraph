use crate::error::NativeError;
use crate::graph::{hex, unhex, GraphPartition};
use crate::legacy;
use crate::protocol::NativeSyntaxMaterializationRequest;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub(crate) struct StagingResult {
    pub(crate) copy_statements: Vec<String>,
    pub(crate) node_rows: usize,
    pub(crate) edge_rows: usize,
    pub(crate) connector_rows: usize,
    pub(crate) copy_calls: usize,
}

#[derive(Debug, Clone)]
struct NodeRecord {
    id: String,
    table: String,
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct EdgeRecord {
    id: String,
    edge_type: String,
    source_id: String,
    target_id: String,
    fields: BTreeMap<String, String>,
}

pub(crate) fn write_partitions(
    request: &NativeSyntaxMaterializationRequest,
    partitions: &[GraphPartition],
) -> Result<StagingResult, NativeError> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for partition in partitions {
        let (mut partition_nodes, partition_edges) = decode_graph_output(&partition.graph_output)?;
        for node in &mut partition_nodes {
            if node.table == "File" {
                node.fields.insert(
                    "content_hash".to_string(),
                    json_string(&partition.entry.content_hash),
                );
            }
        }
        nodes.extend(partition_nodes);
        edges.extend(partition_edges);
    }
    let payload = encode_bulk_payload(&request.staging_dir, &nodes, &edges);
    let output = legacy::write_bulk_staging_output(&payload).map_err(NativeError::Legacy)?;
    Ok(StagingResult {
        copy_calls: output.copy_statements.len(),
        copy_statements: output.copy_statements,
        node_rows: output.node_rows,
        edge_rows: output.edge_rows,
        connector_rows: output.connector_rows,
    })
}

fn decode_graph_output(output: &str) -> Result<(Vec<NodeRecord>, Vec<EdgeRecord>), NativeError> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts = line.split('\t').collect::<Vec<_>>();
        match parts.first().copied() {
            Some("NODE") if parts.len() == 17 => nodes.push(decode_node(&parts)?),
            Some("EDGE") if parts.len() == 12 => edges.push(decode_edge(&parts)?),
            Some("META") => {}
            Some(kind) => {
                return Err(NativeError::InvalidInput(format!(
                    "invalid graph output record: {kind}"
                )))
            }
            None => {}
        }
    }
    Ok((nodes, edges))
}

fn decode_node(parts: &[&str]) -> Result<NodeRecord, NativeError> {
    let id = unhex(parts[1])?;
    let table = unhex(parts[2])?;
    let mut fields = BTreeMap::new();
    fields.insert("id".to_string(), json_string(&id));
    fields.insert("label".to_string(), json_string(&unhex(parts[3])?));
    fields.insert("kind".to_string(), json_string(&unhex(parts[4])?));
    fields.insert("language".to_string(), json_string(&unhex(parts[5])?));
    fields.insert("path".to_string(), json_string(&unhex(parts[6])?));
    fields.insert("qualified_name".to_string(), json_string(&unhex(parts[7])?));
    fields.insert("scope_id".to_string(), json_string(&unhex(parts[8])?));
    fields.insert("line_start".to_string(), json_optional_i64(parts[9]));
    fields.insert("line_end".to_string(), json_optional_i64(parts[10]));
    fields.insert("byte_start".to_string(), json_optional_i64(parts[11]));
    fields.insert("byte_end".to_string(), json_optional_i64(parts[12]));
    fields.insert(
        "tree_sitter_node_type".to_string(),
        json_string(&unhex(parts[13])?),
    );
    fields.insert("capture_name".to_string(), json_string(&unhex(parts[14])?));
    fields.insert("summary".to_string(), json_string(&unhex(parts[15])?));
    fields.insert(
        "metadata".to_string(),
        json_object_or_empty(&unhex(parts[16])?),
    );
    Ok(NodeRecord { id, table, fields })
}

fn decode_edge(parts: &[&str]) -> Result<EdgeRecord, NativeError> {
    let id = unhex(parts[1])?;
    let edge_type = unhex(parts[2])?;
    let source_id = unhex(parts[3])?;
    let target_id = unhex(parts[4])?;
    let mut fields = BTreeMap::new();
    fields.insert("id".to_string(), json_string(&id));
    fields.insert("kind".to_string(), json_string(&unhex(parts[5])?));
    fields.insert("source_id".to_string(), json_string(&source_id));
    fields.insert("target_id".to_string(), json_string(&target_id));
    fields.insert("confidence".to_string(), parts[6].to_string());
    fields.insert("line_start".to_string(), json_optional_i64(parts[7]));
    fields.insert("line_end".to_string(), json_optional_i64(parts[8]));
    fields.insert("byte_start".to_string(), json_optional_i64(parts[9]));
    fields.insert("byte_end".to_string(), json_optional_i64(parts[10]));
    fields.insert(
        "metadata".to_string(),
        json_object_or_empty(&unhex(parts[11])?),
    );
    Ok(EdgeRecord {
        id,
        edge_type,
        source_id,
        target_id,
        fields,
    })
}

fn encode_bulk_payload(staging_dir: &str, nodes: &[NodeRecord], edges: &[EdgeRecord]) -> String {
    let mut node_tables = BTreeSet::new();
    let mut edge_tables = BTreeSet::new();
    let mut node_types_by_id = BTreeMap::new();
    for node in nodes {
        node_tables.insert(node.table.clone());
        node_types_by_id.insert(node.id.clone(), node.table.clone());
    }
    for edge in edges {
        edge_tables.insert(edge.edge_type.clone());
    }

    let mut lines = vec![format!("BULK\t{}", hex(staging_dir))];
    for table in node_tables {
        lines.push(["TABLE".to_string(), hex("node"), hex(&table)].join("\t"));
    }
    for table in edge_tables {
        lines.push(["TABLE".to_string(), hex("edge"), hex(&table)].join("\t"));
    }
    for node in nodes {
        lines.push(encode_bulk_row(
            "NROW",
            &node.table,
            &node.id,
            &node.fields,
            &[],
        ));
    }
    for edge in edges {
        let source_type = node_types_by_id
            .get(&edge.source_id)
            .cloned()
            .unwrap_or_default();
        let target_type = node_types_by_id
            .get(&edge.target_id)
            .cloned()
            .unwrap_or_default();
        lines.push(encode_bulk_row(
            "EROW",
            &edge.edge_type,
            &edge.id,
            &edge.fields,
            &[
                edge.source_id.as_str(),
                edge.target_id.as_str(),
                source_type.as_str(),
                target_type.as_str(),
            ],
        ));
    }
    lines.join("\n") + "\n"
}

fn encode_bulk_row(
    record_type: &str,
    table: &str,
    row_id: &str,
    row: &BTreeMap<String, String>,
    connector_fields: &[&str],
) -> String {
    let mut fields = vec![record_type.to_string(), hex(table), hex(row_id)];
    fields.extend(connector_fields.iter().map(|value| hex(value)));
    fields.push(row.len().to_string());
    for (key, value) in row {
        fields.push(hex(key));
        fields.push(hex(value));
    }
    fields.join("\t")
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn json_optional_i64(value: &str) -> String {
    if value.is_empty() {
        "null".to_string()
    } else {
        value.to_string()
    }
}

fn json_object_or_empty(value: &str) -> String {
    match serde_json::from_str::<Value>(value) {
        Ok(Value::Object(_)) => value.to_string(),
        _ => "{}".to_string(),
    }
}
