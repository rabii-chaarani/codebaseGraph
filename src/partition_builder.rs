use crate::error::NativeError;
use crate::hash;
use crate::parser::ParseOutput;
use crate::protocol::{ManifestEntry, NativeSyntaxMaterializationRequest, SourceSnapshot};
use crate::syntax_materializer::{self, GraphEdgeRow, GraphNodeRow};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct GraphPartition {
    pub(crate) entry: ManifestEntry,
    pub(crate) nodes: Vec<GraphNodeRow>,
    pub(crate) edges: Vec<GraphEdgeRow>,
}

impl GraphPartition {
    pub(crate) fn set_artifact_key(&mut self, artifact_key: impl Into<String>) {
        self.entry.artifact_key = Some(artifact_key.into());
    }

    pub(crate) fn validate_raw_rows(&self) -> Result<(), String> {
        if self.entry.partition_id != hash::partition_id(&self.entry.path) {
            return Err(format!(
                "partition {} does not match path {}",
                self.entry.partition_id, self.entry.path
            ));
        }

        let mut expected_node_ids = Vec::with_capacity(self.nodes.len());
        let mut expected_edge_ids = Vec::with_capacity(self.edges.len());
        let mut expected_node_types = BTreeMap::new();
        let mut expected_edge_types = BTreeMap::new();

        for node in &self.nodes {
            expected_node_ids.push(node.id.clone());
            expected_node_types.insert(node.id.clone(), node.table.clone());
        }

        for edge in &self.edges {
            expected_edge_ids.push(edge.id.clone());
            expected_edge_types.insert(edge.id.clone(), edge.edge_type.clone());
        }

        expected_node_ids.sort();
        expected_edge_ids.sort();

        if self.entry.node_ids != expected_node_ids {
            return Err("entry node_ids do not match raw node rows".to_string());
        }
        if self.entry.edge_ids != expected_edge_ids {
            return Err("entry edge_ids do not match raw edge rows".to_string());
        }
        if self.entry.node_types != expected_node_types {
            return Err("entry node_types do not match raw node rows".to_string());
        }
        if self.entry.edge_types != expected_edge_types {
            return Err("entry edge_types do not match raw edge rows".to_string());
        }

        Ok(())
    }
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
        artifact_key: None,
        node_ids,
        edge_ids,
        node_types,
        edge_types,
        // Raw partitions are content-addressed. Keep their payload deterministic;
        // generation metadata owns wall-clock publication timestamps.
        materialized_at: "unix:0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn graph_partition_roundtrips_through_json() {
        let partition = GraphPartition {
            entry: ManifestEntry {
                path: "src/lib.rs".to_string(),
                content_hash: "hash".to_string(),
                language: "rust".to_string(),
                partition_id: hash::partition_id("src/lib.rs"),
                artifact_key: Some("artifact-key".to_string()),
                node_ids: vec!["node:1".to_string()],
                edge_ids: vec!["edge:1".to_string()],
                node_types: BTreeMap::from([("node:1".to_string(), "Function".to_string())]),
                edge_types: BTreeMap::from([("edge:1".to_string(), "Calls".to_string())]),
                materialized_at: "unix:0".to_string(),
            },
            nodes: vec![GraphNodeRow {
                id: "node:1".to_string(),
                table: "Function".to_string(),
                label: "main".to_string(),
                kind: "definition.function".to_string(),
                language: "rust".to_string(),
                path: "src/lib.rs".to_string(),
                qualified_name: "crate::main".to_string(),
                scope_id: "scope:root".to_string(),
                line_start: Some(1),
                line_end: Some(3),
                byte_start: Some(0),
                byte_end: Some(42),
                tree_sitter_node_type: "function_item".to_string(),
                capture_name: "definition.function".to_string(),
                summary: "fn main()".to_string(),
                metadata: json!({"kind":"function"}),
            }],
            edges: vec![GraphEdgeRow {
                id: "edge:1".to_string(),
                edge_type: "Calls".to_string(),
                source_id: "node:1".to_string(),
                target_id: "node:2".to_string(),
                kind: "reference.call".to_string(),
                confidence: 0.9,
                line_start: Some(2),
                line_end: Some(2),
                byte_start: Some(10),
                byte_end: Some(18),
                metadata: json!({"kind":"call"}),
            }],
        };

        let encoded = serde_json::to_string(&partition).unwrap();
        let decoded: GraphPartition = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.entry.artifact_key.as_deref(), Some("artifact-key"));
        decoded.validate_raw_rows().unwrap();
    }
}
