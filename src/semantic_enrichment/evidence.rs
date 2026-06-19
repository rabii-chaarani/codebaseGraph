use super::ids::{dedupe_strings, sha1_hex_20};
use super::resolution::semantic_edge_if_allowed_with_id;
use super::state::{SemEdge, SemanticState};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct SemEvidence {
    pub(super) evidence_id: String,
    pub(super) source: String,
    pub(super) confidence: f64,
    pub(super) diagnostics: Vec<String>,
    pub(super) provider: String,
    pub(super) metadata: BTreeMap<String, String>,
}

pub(super) struct SemEvidenceLink {
    pub(super) graph_index: usize,
    pub(super) semantic_relation_id: String,
    pub(super) evidence_node_id: String,
    pub(super) evidence_kind: String,
    pub(super) confidence: f64,
    pub(super) metadata_fallback: bool,
}

pub(super) struct SemFallback {
    pub(super) graph_index: usize,
    pub(super) semantic_relation_id: String,
    pub(super) source_node_id: String,
    pub(super) evidence_id: String,
    pub(super) metadata: BTreeMap<String, String>,
}

pub(super) fn build_semantic_evidence_links(
    state: &mut SemanticState,
    evidence: &[SemEvidence],
    output_edges: &mut Vec<SemEdge>,
) -> (Vec<SemEvidenceLink>, Vec<SemFallback>) {
    let mut links = Vec::new();
    let mut fallbacks = Vec::new();
    for item in evidence {
        let semantic_relation_id = item.metadata.get("edge_id").cloned().unwrap_or_default();
        let Some(semantic_edge) = state.edges.get(&semantic_relation_id).cloned() else {
            continue;
        };
        let evidence_node_ids = semantic_evidence_node_ids(state, &semantic_edge.source_id, item);
        if evidence_node_ids.is_empty() {
            fallbacks.push(SemFallback {
                graph_index: semantic_edge.graph_index,
                semantic_relation_id,
                source_node_id: semantic_edge.source_id,
                evidence_id: item.evidence_id.clone(),
                metadata: item.metadata.clone(),
            });
            continue;
        }
        for evidence_node_id in evidence_node_ids {
            let Some(evidence_node) = state.nodes.get(&evidence_node_id).cloned() else {
                continue;
            };
            let mut metadata = BTreeMap::new();
            metadata.insert("resolver".to_string(), "semantic".to_string());
            metadata.insert(
                "semantic_relation_id".to_string(),
                semantic_relation_id.clone(),
            );
            metadata.insert("evidence_id".to_string(), item.evidence_id.clone());
            metadata.insert("source".to_string(), item.source.clone());
            metadata.insert("provider".to_string(), item.provider.clone());
            let edge = semantic_evidence_edge_if_allowed(
                state,
                output_edges,
                semantic_edge.graph_index,
                &semantic_edge.source_id,
                &evidence_node_id,
                item.confidence,
                metadata,
            );
            if edge.is_none() {
                continue;
            }
            links.push(SemEvidenceLink {
                graph_index: semantic_edge.graph_index,
                semantic_relation_id: semantic_relation_id.clone(),
                evidence_node_id,
                evidence_kind: evidence_node.table,
                confidence: item.confidence,
                metadata_fallback: false,
            });
        }
    }
    (links, fallbacks)
}

fn semantic_evidence_node_ids(
    state: &SemanticState,
    source_node_id: &str,
    evidence: &SemEvidence,
) -> Vec<String> {
    let mut node_ids = Vec::new();
    if let Some(explicit_id) = evidence.metadata.get("evidence_node_id") {
        if semantic_is_valid_evidence_target(state, explicit_id) {
            node_ids.push(explicit_id.clone());
        }
    }
    if let Some(target_ids) = state.derived_from_targets_by_source.get(source_node_id) {
        for target_id in target_ids {
            if semantic_is_valid_evidence_target(state, target_id) {
                node_ids.push(target_id.clone());
            }
        }
    }
    if let Some(source_node) = state.nodes.get(source_node_id) {
        if !source_node.path.is_empty() {
            if let Some(file_node_ids) = state.file_node_ids_by_path.get(&source_node.path) {
                for node_id in file_node_ids {
                    if semantic_is_valid_evidence_target(state, node_id) {
                        node_ids.push(node_id.clone());
                    }
                }
            }
        }
    }
    dedupe_strings(node_ids)
}

fn semantic_is_valid_evidence_target(state: &SemanticState, node_id: &str) -> bool {
    state.nodes.get(node_id).is_some_and(|node| {
        matches!(
            node.table.as_str(),
            "SyntaxCapture" | "File" | "DocumentationChunk"
        )
    })
}

fn semantic_evidence_edge_if_allowed(
    state: &mut SemanticState,
    output_edges: &mut Vec<SemEdge>,
    graph_index: usize,
    source_id: &str,
    target_id: &str,
    confidence: f64,
    metadata: BTreeMap<String, String>,
) -> Option<SemEdge> {
    let relation_id = metadata
        .get("semantic_relation_id")
        .cloned()
        .unwrap_or_default();
    let edge_id = format!(
        "edge:semantic-evidence:{}",
        sha1_hex_20(format!("{source_id}|{target_id}|{relation_id}").as_bytes())
    );
    semantic_edge_if_allowed_with_id(
        state,
        output_edges,
        graph_index,
        edge_id,
        "EvidencedBy",
        source_id,
        target_id,
        "semantic_evidence",
        confidence,
        metadata,
    )
}
