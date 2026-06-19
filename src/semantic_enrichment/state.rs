use super::metadata::{metadata_string, metadata_string_map};
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::partition_builder::GraphPartition;
use crate::protocol::OntologyRelationType;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone)]
pub(super) struct RelationSpec {
    pub(super) source_types: HashSet<String>,
    pub(super) target_types: HashSet<String>,
}

#[derive(Clone)]
pub(super) struct SemNode {
    pub(super) graph_index: usize,
    pub(super) id: String,
    pub(super) table: String,
    pub(super) label: String,
    pub(super) language: String,
    pub(super) path: String,
    pub(super) qualified_name: String,
    pub(super) scope_id: String,
    pub(super) imported_name: String,
    pub(super) owner_node_id: String,
    pub(super) typed_node_id: String,
}

#[derive(Clone)]
pub(super) struct SemEdge {
    pub(super) graph_index: usize,
    pub(super) id: String,
    pub(super) edge_type: String,
    pub(super) source_id: String,
    pub(super) target_id: String,
    pub(super) kind: String,
    pub(super) confidence: f64,
    pub(super) metadata: BTreeMap<String, String>,
}

pub(super) struct SemanticState {
    pub(super) relation_specs: HashMap<String, RelationSpec>,
    pub(super) nodes: HashMap<String, SemNode>,
    pub(super) node_order: Vec<String>,
    pub(super) edges: HashMap<String, SemEdge>,
    pub(super) edge_order: Vec<String>,
    pub(super) derived_from_targets_by_source: HashMap<String, Vec<String>>,
    pub(super) file_node_ids_by_path: HashMap<String, Vec<String>>,
    pub(super) type_annotation_owner_ids_by_type: HashMap<String, Vec<String>>,
}

impl SemanticState {
    pub(super) fn from_partitions(
        partitions: &[GraphPartition],
        relation_types: &[OntologyRelationType],
    ) -> Self {
        let mut state = Self {
            relation_specs: relation_specs(relation_types),
            nodes: HashMap::new(),
            node_order: Vec::new(),
            edges: HashMap::new(),
            edge_order: Vec::new(),
            derived_from_targets_by_source: HashMap::new(),
            file_node_ids_by_path: HashMap::new(),
            type_annotation_owner_ids_by_type: HashMap::new(),
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
        state.rebuild_lookup_indexes();
        state
    }

    fn rebuild_lookup_indexes(&mut self) {
        self.derived_from_targets_by_source.clear();
        self.file_node_ids_by_path.clear();
        self.type_annotation_owner_ids_by_type.clear();

        for node_id in &self.node_order {
            let Some(node) = self.nodes.get(node_id) else {
                continue;
            };
            if node.table == "File" && !node.path.is_empty() {
                self.file_node_ids_by_path
                    .entry(node.path.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }

        let edges: Vec<SemEdge> = self
            .edge_order
            .iter()
            .filter_map(|edge_id| self.edges.get(edge_id).cloned())
            .collect();
        for edge in edges {
            self.index_edge(&edge);
        }
    }

    pub(super) fn index_edge(&mut self, edge: &SemEdge) {
        if edge.edge_type == "DerivedFrom" {
            self.derived_from_targets_by_source
                .entry(edge.source_id.clone())
                .or_default()
                .push(edge.target_id.clone());
            return;
        }

        if edge.edge_type != "HasTypeAnnotation" {
            return;
        }
        let Some(owner) = self.nodes.get(&edge.source_id) else {
            return;
        };
        if self.is_type_annotation_owner(owner) {
            self.type_annotation_owner_ids_by_type
                .entry(edge.target_id.clone())
                .or_default()
                .push(owner.id.clone());
        }
    }

    pub(super) fn is_type_annotation_owner(&self, node: &SemNode) -> bool {
        self.relation_specs
            .get("HasTypeAnnotation")
            .is_some_and(|spec| spec.source_types.contains(&node.table))
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

    pub(super) fn to_row(&self) -> GraphEdgeRow {
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

pub(super) fn relation_specs(
    relation_types: &[OntologyRelationType],
) -> HashMap<String, RelationSpec> {
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
