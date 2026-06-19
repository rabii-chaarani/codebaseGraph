use super::state::{SemNode, SemanticState};
use std::collections::{BTreeSet, HashMap, HashSet};

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

#[derive(Clone)]
pub(super) struct SemSymbol {
    pub(super) name: String,
    pub(super) qualified_name: String,
    pub(super) node_id: String,
    pub(super) table: String,
    pub(super) language: String,
    pub(super) scope_id: String,
    pub(super) visibility: String,
}

pub(super) fn build_semantic_symbols(state: &SemanticState) -> Vec<SemSymbol> {
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

pub(super) fn index_symbols_by_name(symbols: &[SemSymbol]) -> HashMap<String, Vec<SemSymbol>> {
    let mut by_name: HashMap<String, Vec<SemSymbol>> = HashMap::with_capacity(symbols.len() * 2);
    for symbol in symbols {
        for key in semantic_symbol_keys(&symbol.name, &symbol.qualified_name) {
            by_name.entry(key).or_default().push(symbol.clone());
        }
    }
    by_name
}

pub(super) fn semantic_symbol_keys(name: &str, qualified_name: &str) -> Vec<String> {
    let mut keys: BTreeSet<String> = candidate_semantic_symbol_keys(name).into_iter().collect();
    keys.extend(candidate_semantic_symbol_keys(qualified_name));
    keys.into_iter().collect()
}

pub(super) fn candidate_semantic_symbol_keys(label: &str) -> Vec<String> {
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

pub(super) fn semantic_visibility(node: &SemNode) -> String {
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
