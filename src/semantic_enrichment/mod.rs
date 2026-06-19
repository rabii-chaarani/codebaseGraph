mod evidence;
mod ids;
mod metadata;
mod resolution;
mod state;
mod symbols;

use crate::error::NativeError;
use crate::partition_builder::GraphPartition;
use crate::protocol::NativeSyntaxMaterializationRequest;
use evidence::{build_semantic_evidence_links, SemEvidence, SemEvidenceLink, SemFallback};
use ids::semantic_stable_id;
use metadata::{refresh_manifest_entries, update_file_metadata};
use resolution::{
    collect_semantic_references, enrich_semantic_call_and_type_relations,
    resolve_semantic_reference, semantic_resolves_to_edge,
};
use state::{SemEdge, SemanticState};
use std::collections::BTreeMap;
use std::time::Instant;
use symbols::{build_semantic_symbols, index_symbols_by_name};

#[derive(Debug, Clone, Default)]
pub(crate) struct SemanticEnrichmentStats {
    pub(crate) phase_timings: BTreeMap<String, f64>,
}

struct SemanticOutput {
    symbol_count: usize,
    call_type_relations: usize,
    edges: Vec<SemEdge>,
    evidence: Vec<SemEvidence>,
    evidence_links: Vec<SemEvidenceLink>,
    fallbacks: Vec<SemFallback>,
}

pub(crate) fn enrich_partitions(
    partitions: &mut [GraphPartition],
    request: &NativeSyntaxMaterializationRequest,
) -> Result<SemanticEnrichmentStats, NativeError> {
    if !request.semantic_enrichment {
        return Ok(SemanticEnrichmentStats::default());
    }
    if request.semantic_provider_mode != "local_only" {
        return Err(NativeError::Unsupported(format!(
            "native semantic enrichment only supports local_only provider mode, got {}",
            request.semantic_provider_mode
        )));
    }

    let metadata_started = Instant::now();
    let mut state =
        SemanticState::from_partitions(partitions, &request.ontology_schema.relation_types);
    let mut phase_timings = BTreeMap::new();
    phase_timings.insert(
        "semantic_metadata_seconds".to_string(),
        metadata_started.elapsed().as_secs_f64(),
    );

    let output = execute_semantic_batch(&mut state, &mut phase_timings);
    for edge in &output.edges {
        if let Some(partition) = partitions.get_mut(edge.graph_index) {
            partition.edges.push(edge.to_row());
        }
    }
    update_file_metadata(partitions, request, &output);
    refresh_manifest_entries(partitions);

    Ok(SemanticEnrichmentStats { phase_timings })
}

fn execute_semantic_batch(
    state: &mut SemanticState,
    phase_timings: &mut BTreeMap<String, f64>,
) -> SemanticOutput {
    let symbol_started = Instant::now();
    let symbols = build_semantic_symbols(state);
    let symbol_count = symbols.len();
    let by_name = index_symbols_by_name(&symbols);
    phase_timings.insert(
        "semantic_symbol_index_seconds".to_string(),
        symbol_started.elapsed().as_secs_f64(),
    );

    let resolution_started = Instant::now();
    let mut output_edges = Vec::new();
    let mut evidence = Vec::new();
    for reference in collect_semantic_references(state) {
        let Some(decision) = resolve_semantic_reference(&reference, &by_name) else {
            evidence.push(SemEvidence {
                evidence_id: semantic_stable_id(
                    "evidence",
                    &format!("unresolved:{}", reference.reference_node_id),
                ),
                source: "local".to_string(),
                confidence: 0.0,
                diagnostics: vec![format!("Unresolved reference: {}", reference.name)],
                provider: String::new(),
                metadata: BTreeMap::new(),
            });
            continue;
        };
        if let Some(primary) =
            semantic_resolves_to_edge(state, &reference, &decision, &mut output_edges)
        {
            let mut metadata = BTreeMap::new();
            metadata.insert("edge_id".to_string(), primary.id.clone());
            metadata.insert(
                "target_node_id".to_string(),
                decision.target_node_id.clone(),
            );
            evidence.push(SemEvidence {
                evidence_id: semantic_stable_id("evidence", &primary.id),
                source: decision.source,
                confidence: decision.score,
                diagnostics: Vec::new(),
                provider: String::new(),
                metadata,
            });
        }
    }
    phase_timings.insert(
        "semantic_resolution_seconds".to_string(),
        resolution_started.elapsed().as_secs_f64(),
    );

    let promotion_started = Instant::now();
    let call_type_relations = enrich_semantic_call_and_type_relations(state, &mut output_edges);
    let (evidence_links, fallbacks) =
        build_semantic_evidence_links(state, &evidence, &mut output_edges);
    phase_timings.insert(
        "semantic_edge_promotion_seconds".to_string(),
        promotion_started.elapsed().as_secs_f64(),
    );

    SemanticOutput {
        symbol_count,
        call_type_relations,
        edges: output_edges,
        evidence,
        evidence_links,
        fallbacks,
    }
}

#[cfg(test)]
mod tests {
    use super::ids::semantic_stable_id;
    use super::resolution::add_semantic_edge_if_allowed;
    use super::state::{relation_specs, SemNode, SemanticState};
    use super::*;
    use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
    use crate::partition_builder::GraphPartition;
    use crate::protocol::NativeSyntaxMaterializationRequest;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn semantic_ids_match_python_truncated_sha1_shape() {
        assert_eq!(
            semantic_stable_id(
                "edge",
                "Calls|call:helper|function:helper|semantic_call_target"
            ),
            "edge:8ce01f098422bc48a80d"
        );
    }

    #[test]
    fn edge_legality_rejects_invalid_relation_endpoints() {
        let mut state = SemanticState {
            relation_specs: relation_specs(&[]),
            nodes: HashMap::from([
                (
                    "type:User".to_string(),
                    SemNode {
                        graph_index: 0,
                        id: "type:User".to_string(),
                        table: "TypeAnnotation".to_string(),
                        label: "User".to_string(),
                        language: "rust".to_string(),
                        path: "src/lib.rs".to_string(),
                        qualified_name: String::new(),
                        scope_id: String::new(),
                        imported_name: String::new(),
                        owner_node_id: String::new(),
                        typed_node_id: String::new(),
                    },
                ),
                (
                    "function:helper".to_string(),
                    SemNode {
                        graph_index: 0,
                        id: "function:helper".to_string(),
                        table: "Function".to_string(),
                        label: "helper".to_string(),
                        language: "rust".to_string(),
                        path: "src/lib.rs".to_string(),
                        qualified_name: "helper".to_string(),
                        scope_id: String::new(),
                        imported_name: String::new(),
                        owner_node_id: String::new(),
                        typed_node_id: String::new(),
                    },
                ),
            ]),
            node_order: vec!["type:User".to_string(), "function:helper".to_string()],
            edges: HashMap::new(),
            edge_order: Vec::new(),
            derived_from_targets_by_source: HashMap::new(),
            file_node_ids_by_path: HashMap::new(),
            type_annotation_owner_ids_by_type: HashMap::new(),
        };
        let mut output = Vec::new();

        let edge = add_semantic_edge_if_allowed(
            &mut state,
            &mut output,
            0,
            "Calls",
            "type:User",
            "function:helper",
            "semantic_call_target",
            0.9,
            BTreeMap::new(),
        );

        assert!(edge.is_none());
        assert!(output.is_empty());
    }

    #[test]
    fn local_enrichment_resolves_call_and_type_rows() {
        let mut partitions = vec![GraphPartition {
            entry: crate::protocol::ManifestEntry {
                path: "src/lib.rs".to_string(),
                content_hash: "hash".to_string(),
                language: "rust".to_string(),
                partition_id: "partition".to_string(),
                node_ids: Vec::new(),
                edge_ids: Vec::new(),
                node_types: BTreeMap::new(),
                edge_types: BTreeMap::new(),
                materialized_at: "unix:0".to_string(),
            },
            nodes: vec![
                node("file:lib", "File", "lib"),
                node("function:main", "Function", "main"),
                node("function:helper", "Function", "helper"),
                node("parameter:user", "Parameter", "user"),
                node("type:User", "TypeAnnotation", "User"),
                node("class:User", "Class", "User"),
                node("call:helper", "CallExpression", "helper"),
                node("syntax:call", "SyntaxCapture", "helper()"),
            ],
            edges: vec![
                edge(
                    "edge:parameter-type",
                    "HasTypeAnnotation",
                    "parameter:user",
                    "type:User",
                ),
                edge(
                    "edge:call-syntax",
                    "DerivedFrom",
                    "call:helper",
                    "syntax:call",
                ),
            ],
        }];
        partitions[0].nodes[4].scope_id = "parameter:user".to_string();
        let request = NativeSyntaxMaterializationRequest {
            source_root: ".".to_string(),
            repository_label: "repo".to_string(),
            mode: "full".to_string(),
            parser_version: "parser+semantic".to_string(),
            manifest_schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            ontology_schema: Default::default(),
            previous_manifest: None,
            profiles: Vec::new(),
            excluded_parts: Vec::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            ignore_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            db_path: "graph.ladybug".to_string(),
            include_fts: false,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: "staging".to_string(),
            atomic_rebuild: true,
            strict: true,
            parallel: false,
            progress: false,
        };

        let stats = enrich_partitions(&mut partitions, &request).unwrap();

        assert!(stats
            .phase_timings
            .contains_key("semantic_symbol_index_seconds"));
        assert!(partitions[0]
            .edges
            .iter()
            .any(|edge| edge.edge_type == "ResolvesTo" && edge.kind == "semantic_resolution"));
        assert!(partitions[0]
            .edges
            .iter()
            .any(|edge| edge.edge_type == "Calls" && edge.kind == "semantic_call_target"));
        assert!(partitions[0]
            .edges
            .iter()
            .any(|edge| edge.edge_type == "References" && edge.kind == "semantic_reference"));
    }

    fn node(id: &str, table: &str, label: &str) -> GraphNodeRow {
        GraphNodeRow {
            id: id.to_string(),
            table: table.to_string(),
            label: label.to_string(),
            kind: label.to_string(),
            language: "rust".to_string(),
            path: "src/lib.rs".to_string(),
            qualified_name: label.to_string(),
            scope_id: String::new(),
            line_start: Some(1),
            line_end: Some(1),
            byte_start: Some(0),
            byte_end: Some(1),
            tree_sitter_node_type: "identifier".to_string(),
            capture_name: "name".to_string(),
            summary: String::new(),
            metadata: json!({}),
        }
    }

    fn edge(id: &str, edge_type: &str, source_id: &str, target_id: &str) -> GraphEdgeRow {
        GraphEdgeRow {
            id: id.to_string(),
            edge_type: edge_type.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            kind: edge_type.to_lowercase(),
            confidence: 1.0,
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            metadata: json!({}),
        }
    }
}
