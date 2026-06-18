use crate::error::NativeError;
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::partition_builder::GraphPartition;
use crate::protocol::{NativeSyntaxMaterializationRequest, OntologyRelationType};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

const DECLARATION_TABLES: &[&str] = &[
    "Symbol",
    "Class",
    "Function",
    "Method",
    "Parameter",
    "ReturnType",
    "TypeAnnotation",
    "TypeAlias",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
    "Decorator",
    "Assignment",
    "APIEndpoint",
    "Component",
    "Route",
    "Query",
    "SecretRef",
    "Dependency",
    "Module",
];
const REFERENCE_TABLES: &[&str] = &[
    "Reference",
    "ImportDeclaration",
    "CallExpression",
    "TypeAnnotation",
    "Decorator",
];

#[derive(Debug, Clone, Default)]
pub(crate) struct SemanticEnrichmentStats {
    pub(crate) phase_timings: BTreeMap<String, f64>,
}

#[derive(Clone)]
struct RelationSpec {
    source_types: HashSet<String>,
    target_types: HashSet<String>,
}

#[derive(Clone)]
struct SemNode {
    graph_index: usize,
    id: String,
    table: String,
    label: String,
    language: String,
    path: String,
    qualified_name: String,
    scope_id: String,
    imported_name: String,
    owner_node_id: String,
    typed_node_id: String,
}

#[derive(Clone)]
struct SemEdge {
    graph_index: usize,
    id: String,
    edge_type: String,
    source_id: String,
    target_id: String,
    kind: String,
    confidence: f64,
    metadata: BTreeMap<String, String>,
}

#[derive(Clone)]
struct SemSymbol {
    name: String,
    qualified_name: String,
    node_id: String,
    table: String,
    language: String,
    scope_id: String,
    visibility: String,
}

struct SemReference {
    graph_index: usize,
    reference_node_id: String,
    name: String,
    scope_id: String,
    language: String,
    source_path: String,
}

struct SemResolutionCandidate {
    target_node_id: String,
    score: f64,
    source: String,
    rationale: String,
}

#[derive(Clone)]
struct SemEvidence {
    evidence_id: String,
    source: String,
    confidence: f64,
    diagnostics: Vec<String>,
    provider: String,
    metadata: BTreeMap<String, String>,
}

struct SemEvidenceLink {
    graph_index: usize,
    semantic_relation_id: String,
    evidence_node_id: String,
    evidence_kind: String,
    confidence: f64,
    metadata_fallback: bool,
}

struct SemFallback {
    graph_index: usize,
    semantic_relation_id: String,
    source_node_id: String,
    evidence_id: String,
    metadata: BTreeMap<String, String>,
}

struct SemanticState {
    relation_specs: HashMap<String, RelationSpec>,
    nodes: HashMap<String, SemNode>,
    node_order: Vec<String>,
    edges: HashMap<String, SemEdge>,
    edge_order: Vec<String>,
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

impl SemanticState {
    fn from_partitions(
        partitions: &[GraphPartition],
        relation_types: &[OntologyRelationType],
    ) -> Self {
        let mut state = Self {
            relation_specs: relation_specs(relation_types),
            nodes: HashMap::new(),
            node_order: Vec::new(),
            edges: HashMap::new(),
            edge_order: Vec::new(),
        };
        for (graph_index, partition) in partitions.iter().enumerate() {
            for node in &partition.nodes {
                let sem_node = SemNode::from_row(graph_index, node);
                state.node_order.push(sem_node.id.clone());
                state.nodes.insert(sem_node.id.clone(), sem_node);
            }
            for edge in &partition.edges {
                let sem_edge = SemEdge::from_row(graph_index, edge);
                state.edge_order.push(sem_edge.id.clone());
                state.edges.insert(sem_edge.id.clone(), sem_edge);
            }
        }
        state
    }
}

impl SemNode {
    fn from_row(graph_index: usize, row: &GraphNodeRow) -> Self {
        Self {
            graph_index,
            id: row.id.clone(),
            table: row.table.clone(),
            label: row.label.clone(),
            language: row.language.clone(),
            path: row.path.clone(),
            qualified_name: row.qualified_name.clone(),
            scope_id: row.scope_id.clone(),
            imported_name: metadata_string(&row.metadata, "imported_name"),
            owner_node_id: metadata_string(&row.metadata, "owner_node_id"),
            typed_node_id: metadata_string(&row.metadata, "typed_node_id"),
        }
    }
}

impl SemEdge {
    fn from_row(graph_index: usize, row: &GraphEdgeRow) -> Self {
        Self {
            graph_index,
            id: row.id.clone(),
            edge_type: row.edge_type.clone(),
            source_id: row.source_id.clone(),
            target_id: row.target_id.clone(),
            kind: row.kind.clone(),
            confidence: row.confidence,
            metadata: metadata_string_map(&row.metadata),
        }
    }

    fn to_row(&self) -> GraphEdgeRow {
        GraphEdgeRow {
            id: self.id.clone(),
            edge_type: self.edge_type.clone(),
            source_id: self.source_id.clone(),
            target_id: self.target_id.clone(),
            kind: self.kind.clone(),
            confidence: self.confidence,
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            metadata: json!(self.metadata),
        }
    }
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

fn build_semantic_symbols(state: &SemanticState) -> Vec<SemSymbol> {
    let exported_targets: HashSet<String> = state
        .edges
        .values()
        .filter(|edge| edge.edge_type == "Exports")
        .map(|edge| edge.target_id.clone())
        .collect();
    let mut symbols = Vec::new();
    for node_id in &state.node_order {
        let Some(node) = state.nodes.get(node_id) else {
            continue;
        };
        if !DECLARATION_TABLES.contains(&node.table.as_str()) {
            continue;
        }
        let name = node.label.trim();
        if name.is_empty() {
            continue;
        }
        let mut visibility = semantic_visibility(node);
        if exported_targets.contains(&node.id) {
            visibility = "exported".to_string();
        }
        symbols.push(SemSymbol {
            name: name.to_string(),
            qualified_name: if node.qualified_name.is_empty() {
                name.to_string()
            } else {
                node.qualified_name.clone()
            },
            node_id: node.id.clone(),
            table: node.table.clone(),
            language: node.language.clone(),
            scope_id: node.scope_id.clone(),
            visibility,
        });
    }
    symbols.sort_by(|left, right| {
        (
            left.qualified_name.as_str(),
            left.table.as_str(),
            left.node_id.as_str(),
        )
            .cmp(&(
                right.qualified_name.as_str(),
                right.table.as_str(),
                right.node_id.as_str(),
            ))
    });
    symbols
}

fn index_symbols_by_name(symbols: &[SemSymbol]) -> HashMap<String, Vec<SemSymbol>> {
    let mut by_name: HashMap<String, Vec<SemSymbol>> = HashMap::with_capacity(symbols.len() * 2);
    for symbol in symbols {
        for key in semantic_symbol_keys(&symbol.name, &symbol.qualified_name) {
            by_name.entry(key).or_default().push(symbol.clone());
        }
    }
    by_name
}

fn collect_semantic_references(state: &SemanticState) -> Vec<SemReference> {
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

fn resolve_semantic_reference(
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

fn semantic_resolves_to_edge(
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

fn enrich_semantic_call_and_type_relations(
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
    let typed_owner_types = state
        .relation_specs
        .get("HasTypeAnnotation")
        .map(|spec| spec.source_types.clone())
        .unwrap_or_default();
    for edge_id in &state.edge_order {
        let Some(edge) = state.edges.get(edge_id) else {
            continue;
        };
        if edge.edge_type != "HasTypeAnnotation" || edge.target_id != type_node_id {
            continue;
        }
        let Some(owner) = state.nodes.get(&edge.source_id) else {
            continue;
        };
        if typed_owner_types.contains(&owner.table) {
            return Some(owner.id.clone());
        }
    }
    let type_node = state.nodes.get(type_node_id)?;
    if !type_node.scope_id.is_empty() {
        if let Some(owner) = state.nodes.get(&type_node.scope_id) {
            if typed_owner_types.contains(&owner.table) {
                return Some(owner.id.clone());
            }
        }
    }
    for owner_id in [&type_node.owner_node_id, &type_node.typed_node_id] {
        if owner_id.is_empty() {
            continue;
        }
        if let Some(owner) = state.nodes.get(owner_id) {
            if typed_owner_types.contains(&owner.table) {
                return Some(owner.id.clone());
            }
        }
    }
    None
}

fn build_semantic_evidence_links(
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
    for edge_id in &state.edge_order {
        let Some(edge) = state.edges.get(edge_id) else {
            continue;
        };
        if edge.edge_type == "DerivedFrom"
            && edge.source_id == source_node_id
            && semantic_is_valid_evidence_target(state, &edge.target_id)
        {
            node_ids.push(edge.target_id.clone());
        }
    }
    if let Some(source_node) = state.nodes.get(source_node_id) {
        if !source_node.path.is_empty() {
            for node_id in &state.node_order {
                let Some(node) = state.nodes.get(node_id) else {
                    continue;
                };
                if node.table == "File"
                    && node.path == source_node.path
                    && semantic_is_valid_evidence_target(state, &node.id)
                {
                    node_ids.push(node.id.clone());
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

#[allow(clippy::too_many_arguments)]
fn add_semantic_edge_if_allowed(
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
fn semantic_edge_if_allowed_with_id(
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
        state.edges.insert(edge_id, edge.clone());
        output_edges.push(edge.clone());
    }
    Some(state.edges.get(&edge.id).cloned().unwrap_or(edge))
}

fn update_file_metadata(
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

fn refresh_manifest_entries(partitions: &mut [GraphPartition]) {
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

fn relation_specs(relation_types: &[OntologyRelationType]) -> HashMap<String, RelationSpec> {
    let mut specs = relation_types
        .iter()
        .filter(|relation| !relation.name.is_empty())
        .map(|relation| {
            (
                relation.name.clone(),
                RelationSpec {
                    source_types: relation.source_types.iter().cloned().collect(),
                    target_types: relation.target_types.iter().cloned().collect(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for (name, source_types, target_types) in fallback_relation_specs() {
        specs
            .entry(name.to_string())
            .or_insert_with(|| RelationSpec {
                source_types: source_types.iter().map(|value| value.to_string()).collect(),
                target_types: target_types.iter().map(|value| value.to_string()).collect(),
            });
    }
    specs
}

fn fallback_relation_specs() -> Vec<(&'static str, Vec<&'static str>, Vec<&'static str>)> {
    vec![
        (
            "References",
            vec![
                "Reference",
                "Expression",
                "CallExpression",
                "Assignment",
                "ControlFlowBlock",
                "TypeAnnotation",
                "Decorator",
                "Query",
                "SecretRef",
            ],
            vec![
                "Symbol",
                "Class",
                "Function",
                "Method",
                "Variable",
                "Constant",
                "ClassAttribute",
                "InstanceAttribute",
                "Property",
                "Parameter",
                "Module",
                "Dependency",
            ],
        ),
        (
            "Calls",
            vec![
                "Function",
                "Method",
                "CallExpression",
                "Decorator",
                "APIEndpoint",
                "Route",
                "Component",
            ],
            vec![
                "CallExpression",
                "Function",
                "Method",
                "Class",
                "APIEndpoint",
            ],
        ),
        (
            "ResolvesTo",
            vec![
                "Reference",
                "ImportDeclaration",
                "CallExpression",
                "TypeAnnotation",
                "Decorator",
            ],
            vec![
                "Symbol",
                "Module",
                "Class",
                "Function",
                "Method",
                "Variable",
                "Constant",
                "Dependency",
                "Parameter",
            ],
        ),
        (
            "HasTypeAnnotation",
            vec![
                "Symbol",
                "Parameter",
                "ReturnType",
                "TypeAlias",
                "Variable",
                "Constant",
                "ClassAttribute",
                "InstanceAttribute",
            ],
            vec!["TypeAnnotation", "Reference", "Literal"],
        ),
        (
            "EvidencedBy",
            vec![
                "Repository",
                "File",
                "Module",
                "Symbol",
                "Class",
                "Function",
                "Method",
                "Parameter",
                "ReturnType",
                "TypeAnnotation",
                "TypeAlias",
                "Variable",
                "Constant",
                "ClassAttribute",
                "InstanceAttribute",
                "Property",
                "Decorator",
                "Assignment",
                "APIEndpoint",
                "Component",
                "Route",
                "Query",
                "SecretRef",
                "CallExpression",
                "Reference",
                "Literal",
                "Expression",
                "ControlFlowBlock",
                "ExceptionFlow",
                "Dependency",
                "DocumentationSource",
                "DocumentationChunk",
            ],
            vec!["SyntaxCapture", "File", "DocumentationChunk"],
        ),
    ]
}

fn semantic_symbol_keys(name: &str, qualified_name: &str) -> Vec<String> {
    let mut keys: BTreeSet<String> = candidate_semantic_symbol_keys(name).into_iter().collect();
    keys.extend(candidate_semantic_symbol_keys(qualified_name));
    keys.into_iter().collect()
}

fn candidate_semantic_symbol_keys(label: &str) -> Vec<String> {
    let text = label.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut parts = BTreeSet::new();
    parts.insert(text.to_string());
    for delimiter in [".", "::", "->"] {
        if text.contains(delimiter) {
            if let Some((_, right)) = text.rsplit_once(delimiter) {
                parts.insert(right.to_string());
            }
        }
    }
    if text.contains('/') {
        if let Some((_, right)) = text.rsplit_once('/') {
            parts.insert(right.to_string());
        }
    }
    parts
        .into_iter()
        .filter_map(|part| {
            let normalized = part.trim().to_lowercase().replace('_', "");
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn semantic_visibility(node: &SemNode) -> String {
    if node.table == "Dependency" {
        "external".to_string()
    } else if node.label.starts_with('_') {
        "private".to_string()
    } else if node.label.chars().next().is_some_and(char::is_uppercase)
        || matches!(
            node.table.as_str(),
            "Module" | "Class" | "Function" | "Method" | "TypeAlias"
        )
    {
        "public".to_string()
    } else {
        "local".to_string()
    }
}

fn metadata_string(metadata: &Value, key: &str) -> String {
    metadata
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

fn metadata_string_map(metadata: &Value) -> BTreeMap<String, String> {
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

fn semantic_stable_id(prefix: &str, key: &str) -> String {
    format!("{prefix}:{}", sha1_hex_20(key.as_bytes()))
}

fn sha1_hex_20(bytes: &[u8]) -> String {
    let digest = sha1(bytes);
    digest[..10]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xefcdab89;
    let mut h2: u32 = 0x98badcfe;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xc3d2e1f0;

    let bit_len = (input.len() as u64) * 8;
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut words = [0u32; 80];
        for (index, word) in words.iter_mut().enumerate().take(16) {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut output = [0u8; 20];
    for (index, word) in [h0, h1, h2, h3, h4].iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            output.push(value);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            db_path: "graph.ladybug".to_string(),
            include_fts: false,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: "staging".to_string(),
            atomic_rebuild: true,
            strict: true,
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
