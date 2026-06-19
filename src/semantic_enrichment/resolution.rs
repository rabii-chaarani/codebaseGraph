use super::ids::semantic_stable_id;
use super::state::{SemEdge, SemanticState};
use super::symbols::{candidate_semantic_symbol_keys, SemSymbol};
use std::collections::{BTreeMap, HashMap};

const REFERENCE_TABLES: &[&str] = &[
    "Reference",
    "ImportDeclaration",
    "CallExpression",
    "TypeAnnotation",
    "Decorator",
];

pub(super) struct SemReference {
    pub(super) graph_index: usize,
    pub(super) reference_node_id: String,
    pub(super) name: String,
    pub(super) scope_id: String,
    pub(super) language: String,
    pub(super) source_path: String,
}

pub(super) struct SemResolutionCandidate {
    pub(super) target_node_id: String,
    pub(super) score: f64,
    pub(super) source: String,
    pub(super) rationale: String,
}

pub(super) fn collect_semantic_references(state: &SemanticState) -> Vec<SemReference> {
    let mut references = Vec::new();
    for node_id in &state.node_order {
        let Some(node) = state.nodes.get(node_id) else {
            continue;
        };
        if !REFERENCE_TABLES.contains(&node.table.as_str()) {
            continue;
        }
        let name = if node.imported_name.trim().is_empty() {
            node.label.trim().to_string()
        } else {
            node.imported_name.trim().to_string()
        };
        if name.is_empty() {
            continue;
        }
        references.push(SemReference {
            graph_index: node.graph_index,
            reference_node_id: node.id.clone(),
            name,
            scope_id: node.scope_id.clone(),
            language: node.language.clone(),
            source_path: node.path.clone(),
        });
    }
    references.sort_by(|left, right| {
        (left.source_path.as_str(), left.reference_node_id.as_str())
            .cmp(&(right.source_path.as_str(), right.reference_node_id.as_str()))
    });
    references
}

pub(super) fn resolve_semantic_reference(
    reference: &SemReference,
    by_name: &HashMap<String, Vec<SemSymbol>>,
) -> Option<SemResolutionCandidate> {
    for key in candidate_semantic_symbol_keys(&reference.name) {
        let Some(candidates) = by_name.get(&key) else {
            continue;
        };
        let mut symbols = candidates.clone();
        symbols.sort_by(|left, right| {
            (
                left.scope_id != reference.scope_id,
                left.language != reference.language,
                !matches!(left.visibility.as_str(), "local" | "public" | "exported"),
                left.qualified_name.as_str(),
                left.node_id.as_str(),
            )
                .cmp(&(
                    right.scope_id != reference.scope_id,
                    right.language != reference.language,
                    !matches!(right.visibility.as_str(), "local" | "public" | "exported"),
                    right.qualified_name.as_str(),
                    right.node_id.as_str(),
                ))
        });
        let symbol = symbols.first()?;
        let mut score: f64 = 0.72;
        if symbol.scope_id == reference.scope_id {
            score += 0.13;
        }
        if symbol.language == reference.language {
            score += 0.05;
        }
        return Some(SemResolutionCandidate {
            target_node_id: symbol.node_id.clone(),
            score: score.min(1.0),
            source: "symbol_table".to_string(),
            rationale: format!("symbol_table matched {}", reference.name),
        });
    }
    None
}

pub(super) fn semantic_resolves_to_edge(
    state: &mut SemanticState,
    reference: &SemReference,
    candidate: &SemResolutionCandidate,
    output_edges: &mut Vec<SemEdge>,
) -> Option<SemEdge> {
    let source_id = state.nodes.get(&reference.reference_node_id)?.id.clone();
    let target_id = state.nodes.get(&candidate.target_node_id)?.id.clone();
    if source_id == target_id {
        return None;
    }
    let mut metadata = BTreeMap::new();
    metadata.insert("resolver".to_string(), "semantic".to_string());
    metadata.insert("resolution_source".to_string(), candidate.source.clone());
    metadata.insert("rationale".to_string(), candidate.rationale.clone());
    metadata.insert("label".to_string(), reference.name.clone());
    let primary = add_semantic_edge_if_allowed(
        state,
        output_edges,
        reference.graph_index,
        "ResolvesTo",
        &source_id,
        &target_id,
        "semantic_resolution",
        candidate.score,
        metadata.clone(),
    );
    add_semantic_edge_if_allowed(
        state,
        output_edges,
        reference.graph_index,
        "References",
        &source_id,
        &target_id,
        "semantic_reference",
        candidate.score.min(0.9),
        metadata,
    );
    primary
}

pub(super) fn enrich_semantic_call_and_type_relations(
    state: &mut SemanticState,
    output_edges: &mut Vec<SemEdge>,
) -> usize {
    let edge_ids = state.edge_order.clone();
    let mut resolutions = 0;
    for edge_id in edge_ids {
        let Some(edge) = state.edges.get(&edge_id).cloned() else {
            continue;
        };
        if edge.edge_type != "ResolvesTo" {
            continue;
        }
        let Some(source) = state.nodes.get(&edge.source_id).cloned() else {
            continue;
        };
        let Some(target) = state.nodes.get(&edge.target_id).cloned() else {
            continue;
        };
        if source.table == "CallExpression" {
            if !matches!(
                target.table.as_str(),
                "Function" | "Method" | "Class" | "APIEndpoint"
            ) {
                continue;
            }
            let mut metadata = BTreeMap::new();
            metadata.insert("resolver".to_string(), "semantic".to_string());
            metadata.insert("source_edge".to_string(), edge.id.clone());
            add_semantic_edge_if_allowed(
                state,
                output_edges,
                source.graph_index,
                "Calls",
                &source.id,
                &target.id,
                "semantic_call_target",
                edge.confidence,
                metadata,
            );
            resolutions += 1;
        } else if source.table == "TypeAnnotation" {
            let mut fallback_metadata = BTreeMap::new();
            fallback_metadata.insert("resolver".to_string(), "semantic".to_string());
            fallback_metadata.insert("source_edge".to_string(), edge.id.clone());
            add_semantic_edge_if_allowed(
                state,
                output_edges,
                source.graph_index,
                "References",
                &source.id,
                &target.id,
                "semantic_type_reference",
                edge.confidence,
                fallback_metadata.clone(),
            );
            if let Some(owner_id) = semantic_type_annotation_owner_id(state, &source.id) {
                let mut metadata = fallback_metadata;
                metadata.insert("target_node_id".to_string(), target.id.clone());
                add_semantic_edge_if_allowed(
                    state,
                    output_edges,
                    source.graph_index,
                    "HasTypeAnnotation",
                    &owner_id,
                    &source.id,
                    "semantic_type_annotation",
                    edge.confidence,
                    metadata,
                );
            }
            resolutions += 1;
        }
    }
    resolutions
}

fn semantic_type_annotation_owner_id(state: &SemanticState, type_node_id: &str) -> Option<String> {
    if let Some(owner_ids) = state.type_annotation_owner_ids_by_type.get(type_node_id) {
        for owner_id in owner_ids {
            let Some(owner) = state.nodes.get(owner_id) else {
                continue;
            };
            if state.is_type_annotation_owner(owner) {
                return Some(owner.id.clone());
            }
        }
    }

    let type_node = state.nodes.get(type_node_id)?;
    if !type_node.scope_id.is_empty() {
        if let Some(owner) = state.nodes.get(&type_node.scope_id) {
            if state.is_type_annotation_owner(owner) {
                return Some(owner.id.clone());
            }
        }
    }
    for owner_id in [&type_node.owner_node_id, &type_node.typed_node_id] {
        if owner_id.is_empty() {
            continue;
        }
        if let Some(owner) = state.nodes.get(owner_id) {
            if state.is_type_annotation_owner(owner) {
                return Some(owner.id.clone());
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub(super) fn add_semantic_edge_if_allowed(
    state: &mut SemanticState,
    output_edges: &mut Vec<SemEdge>,
    graph_index: usize,
    edge_type: &str,
    source_id: &str,
    target_id: &str,
    kind: &str,
    confidence: f64,
    metadata: BTreeMap<String, String>,
) -> Option<SemEdge> {
    let edge_id = semantic_stable_id(
        "edge",
        &format!("{edge_type}|{source_id}|{target_id}|{kind}"),
    );
    semantic_edge_if_allowed_with_id(
        state,
        output_edges,
        graph_index,
        edge_id,
        edge_type,
        source_id,
        target_id,
        kind,
        confidence,
        metadata,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn semantic_edge_if_allowed_with_id(
    state: &mut SemanticState,
    output_edges: &mut Vec<SemEdge>,
    graph_index: usize,
    edge_id: String,
    edge_type: &str,
    source_id: &str,
    target_id: &str,
    kind: &str,
    confidence: f64,
    metadata: BTreeMap<String, String>,
) -> Option<SemEdge> {
    let source = state.nodes.get(source_id)?;
    let target = state.nodes.get(target_id)?;
    let spec = state.relation_specs.get(edge_type)?;
    if !spec.source_types.contains(&source.table) || !spec.target_types.contains(&target.table) {
        return None;
    }
    let mut full_metadata = BTreeMap::new();
    full_metadata.insert(
        "canonical_key".to_string(),
        format!("{edge_type}|{source_id}|{target_id}|{kind}"),
    );
    for (key, value) in metadata {
        full_metadata.insert(key, value);
    }
    let edge = SemEdge {
        graph_index,
        id: edge_id.clone(),
        edge_type: edge_type.to_string(),
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        kind: kind.to_string(),
        confidence,
        metadata: full_metadata,
    };
    if !state.edges.contains_key(&edge_id) {
        state.edge_order.push(edge_id.clone());
        state.index_edge(&edge);
        state.edges.insert(edge_id, edge.clone());
        output_edges.push(edge.clone());
    }
    Some(state.edges.get(&edge.id).cloned().unwrap_or(edge))
}
