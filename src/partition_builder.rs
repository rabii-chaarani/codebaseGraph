use crate::error::NativeError;
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::hash;
use crate::parser::ParseOutput;
use crate::protocol::{ManifestEntry, NativeSyntaxMaterializationRequest, SourceSnapshot};
use crate::syntax_materializer;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub(crate) struct GraphPartition {
    pub(crate) entry: ManifestEntry,
    pub(crate) nodes: Vec<GraphNodeRow>,
    pub(crate) edges: Vec<GraphEdgeRow>,
}

pub(crate) fn build_partition(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
    parse: ParseOutput,
) -> Result<GraphPartition, NativeError> {
    let rows = syntax_materializer::build_syntax_tree_graph_rows(
        graph_meta(request, snapshot),
        &parse.root,
    )
    .map_err(NativeError::InvalidInput)?;
    let entry = manifest_entry(snapshot, &rows.nodes, &rows.edges);
    Ok(GraphPartition {
        entry,
        nodes: rows.nodes,
        edges: rows.edges,
    })
}

fn graph_meta(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();
    meta.insert("path".to_string(), snapshot.path.clone());
    meta.insert(
        "language".to_string(),
        snapshot.language.clone().unwrap_or_default(),
    );
    meta.insert("source_root".to_string(), request.source_root.clone());
    meta.insert(
        "repository_label".to_string(),
        request.repository_label.clone(),
    );
    if !request.ontology_schema.relation_types.is_empty() {
        let relation_types =
            serde_json::to_string(&request.ontology_schema.relation_types).unwrap_or_default();
        meta.insert("ontology_relations".to_string(), relation_types);
    }
    meta
}

fn manifest_entry(
    snapshot: &SourceSnapshot,
    nodes: &[GraphNodeRow],
    edges: &[GraphEdgeRow],
) -> ManifestEntry {
    let mut node_ids = Vec::new();
    let mut edge_ids = Vec::new();
    let mut node_types = BTreeMap::new();
    let mut edge_types = BTreeMap::new();
    for node in nodes {
        node_types.insert(node.id.clone(), node.table.clone());
        node_ids.push(node.id.clone());
    }
    for edge in edges {
        edge_types.insert(edge.id.clone(), edge.edge_type.clone());
        edge_ids.push(edge.id.clone());
    }
    node_ids.sort();
    edge_ids.sort();
    ManifestEntry {
        path: snapshot.path.clone(),
        content_hash: snapshot.content_hash.clone(),
        language: snapshot.language.clone().unwrap_or_default(),
        partition_id: hash::partition_id(&snapshot.path),
        node_ids,
        edge_ids,
        node_types,
        edge_types,
        materialized_at: materialized_at(),
    }
}

fn materialized_at() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("unix:{}", duration.as_secs()),
        Err(_) => "unix:0".to_string(),
    }
}
