use super::evidence::{SemEvidenceLink, SemFallback};
use super::SemanticOutput;
use crate::partition_builder::GraphPartition;
use crate::protocol::NativeSyntaxMaterializationRequest;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

pub(super) fn update_file_metadata(
    partitions: &mut [GraphPartition],
    request: &NativeSyntaxMaterializationRequest,
    output: &SemanticOutput,
) {
    let evidence_by_graph = group_evidence_by_graph(output);
    let links_by_graph = group_links_by_graph(&output.evidence_links);
    let fallbacks_by_graph = group_fallbacks_by_graph(&output.fallbacks);
    for (graph_index, partition) in partitions.iter_mut().enumerate() {
        let Some(file_node) = partition.nodes.iter_mut().find(|node| node.table == "File") else {
            continue;
        };
        let metadata = ensure_metadata_object(&mut file_node.metadata);
        metadata.insert(
            "semantic_enrichment".to_string(),
            json!({
                "syntax_graph": true,
                "build_context": false,
                "symbol_table": output.symbol_count > 0,
                "local_resolution": !output.evidence.is_empty(),
                "provider_resolution": false,
                "diagnostics": [],
            }),
        );
        metadata.insert(
            "semantic_build_context".to_string(),
            json!({
                "ecosystem": "",
                "target": null,
                "source_root": request.source_root,
            }),
        );
        metadata.insert(
            "semantic_relations".to_string(),
            json!({
                "resolution_evidence": output.evidence.len(),
                "call_type_relations": output.call_type_relations,
                "evidence_links": output.evidence_links.len(),
            }),
        );
        metadata.insert(
            "semantic_resolution_evidence".to_string(),
            json!(evidence_by_graph
                .get(&graph_index)
                .cloned()
                .unwrap_or_default()),
        );
        if let Some(links) = links_by_graph.get(&graph_index) {
            metadata.insert("semantic_evidence_links".to_string(), json!(links));
        }
        if let Some(fallbacks) = fallbacks_by_graph.get(&graph_index) {
            metadata.insert("semantic_evidence_fallback".to_string(), json!(fallbacks));
        }
    }
}

fn group_evidence_by_graph(output: &SemanticOutput) -> HashMap<usize, Vec<Value>> {
    let edge_graphs = output
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge.graph_index))
        .collect::<HashMap<_, _>>();
    let mut grouped: HashMap<usize, Vec<Value>> = HashMap::new();
    for evidence in &output.evidence {
        let graph_index = evidence
            .metadata
            .get("edge_id")
            .and_then(|edge_id| edge_graphs.get(edge_id.as_str()).copied())
            .unwrap_or(0);
        grouped.entry(graph_index).or_default().push(json!({
            "evidence_id": evidence.evidence_id,
            "source": evidence.source,
            "confidence": evidence.confidence,
            "diagnostics": evidence.diagnostics,
            "provider": evidence.provider,
            "metadata": evidence.metadata,
        }));
    }
    grouped
}

fn group_links_by_graph(links: &[SemEvidenceLink]) -> HashMap<usize, Vec<Value>> {
    let mut grouped: HashMap<usize, Vec<Value>> = HashMap::new();
    for link in links {
        grouped.entry(link.graph_index).or_default().push(json!({
            "semantic_relation_id": link.semantic_relation_id,
            "evidence_node_id": link.evidence_node_id,
            "evidence_kind": link.evidence_kind,
            "confidence": link.confidence,
            "metadata_fallback": link.metadata_fallback,
        }));
    }
    grouped
}

fn group_fallbacks_by_graph(fallbacks: &[SemFallback]) -> HashMap<usize, Vec<Value>> {
    let mut grouped: HashMap<usize, Vec<Value>> = HashMap::new();
    for fallback in fallbacks {
        grouped
            .entry(fallback.graph_index)
            .or_default()
            .push(json!({
                "semantic_relation_id": fallback.semantic_relation_id,
                "source_node_id": fallback.source_node_id,
                "evidence_id": fallback.evidence_id,
                "metadata": fallback.metadata,
            }));
    }
    grouped
}

pub(super) fn refresh_manifest_entries(partitions: &mut [GraphPartition]) {
    for partition in partitions {
        let mut edge_ids = Vec::with_capacity(partition.edges.len());
        let mut edge_types = BTreeMap::new();
        for edge in &partition.edges {
            edge_ids.push(edge.id.clone());
            edge_types.insert(edge.id.clone(), edge.edge_type.clone());
        }
        edge_ids.sort();
        partition.entry.edge_ids = edge_ids;
        partition.entry.edge_types = edge_types;
    }
}

pub(super) fn metadata_string(metadata: &Value, key: &str) -> String {
    metadata
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn metadata_string_map(metadata: &Value) -> BTreeMap<String, String> {
    metadata
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|text| (key.clone(), text.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ensure_metadata_object(metadata: &mut Value) -> &mut Map<String, Value> {
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .expect("metadata object was just initialized")
}
