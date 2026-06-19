use super::cypher::{cypher_string_list, quote_identifier};
use crate::protocol::{ManifestDiff, NativeManifest};
use std::collections::{BTreeMap, BTreeSet};

const DELETE_BATCH_SIZE: usize = 500;

pub fn partition_delete_statements(
    previous_manifest: Option<&NativeManifest>,
    diff: &ManifestDiff,
) -> Vec<String> {
    let Some(manifest) = previous_manifest else {
        return Vec::new();
    };
    if diff.force_rebuild {
        return Vec::new();
    }
    let touched_paths = diff
        .deleted
        .iter()
        .chain(diff.rebuild_paths().iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    if touched_paths.is_empty() {
        return Vec::new();
    }
    let mut retained_nodes = BTreeSet::new();
    let mut retained_edges = BTreeSet::new();
    for (path, entry) in &manifest.files {
        if touched_paths.contains(path) {
            continue;
        }
        retained_nodes.extend(entry.node_ids.iter().cloned());
        retained_edges.extend(entry.edge_ids.iter().cloned());
    }
    let mut edge_deletes = Vec::new();
    let mut node_deletes = Vec::new();
    for path in touched_paths {
        let Some(entry) = manifest.files.get(&path) else {
            continue;
        };
        edge_deletes.extend(delete_edge_statements(
            &entry.edge_ids,
            &entry.edge_types,
            &retained_edges,
        ));
        node_deletes.extend(delete_node_statements(
            &entry.node_ids,
            &entry.node_types,
            &retained_nodes,
        ));
    }
    edge_deletes.extend(node_deletes);
    edge_deletes
}

fn delete_edge_statements(
    edge_ids: &[String],
    edge_types: &BTreeMap<String, String>,
    retained_edges: &BTreeSet<String>,
) -> Vec<String> {
    let mut ids_by_type: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge_id in edge_ids {
        if retained_edges.contains(edge_id) {
            continue;
        }
        let Some(edge_type) = edge_types.get(edge_id) else {
            continue;
        };
        ids_by_type
            .entry(edge_type.clone())
            .or_default()
            .push(edge_id.clone());
    }
    let mut statements = Vec::new();
    for (edge_type, mut ids) in ids_by_type {
        ids.sort();
        let edge_table = quote_identifier(&edge_type);
        let from_table = quote_identifier(&format!("FROM_{edge_type}"));
        let to_table = quote_identifier(&format!("TO_{edge_type}"));
        for chunk in ids.chunks(DELETE_BATCH_SIZE) {
            let id_list = cypher_string_list(chunk);
            statements.push(format!(
                "MATCH ()-[r:{from_table}]->(edge:{edge_table}) WHERE edge.id IN [{id_list}] DELETE r"
            ));
            statements.push(format!(
                "MATCH (edge:{edge_table})-[r:{to_table}]->() WHERE edge.id IN [{id_list}] DELETE r"
            ));
            statements.push(format!(
                "MATCH (edge:{edge_table}) WHERE edge.id IN [{id_list}] DELETE edge"
            ));
        }
    }
    statements
}

fn delete_node_statements(
    node_ids: &[String],
    node_types: &BTreeMap<String, String>,
    retained_nodes: &BTreeSet<String>,
) -> Vec<String> {
    let mut ids_by_type: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node_id in node_ids {
        if retained_nodes.contains(node_id) {
            continue;
        }
        let Some(node_type) = node_types.get(node_id) else {
            continue;
        };
        ids_by_type
            .entry(node_type.clone())
            .or_default()
            .push(node_id.clone());
    }
    let mut statements = Vec::new();
    for (node_type, mut ids) in ids_by_type {
        ids.sort();
        let node_table = quote_identifier(&node_type);
        for chunk in ids.chunks(DELETE_BATCH_SIZE) {
            statements.push(format!(
                "MATCH (node:{node_table}) WHERE node.id IN [{}] DELETE node",
                cypher_string_list(chunk)
            ));
        }
    }
    statements
}
