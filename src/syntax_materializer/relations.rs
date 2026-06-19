use super::{is_declaration, is_documentation, is_expression, is_symbol_target};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Deserialize)]
pub(super) struct RelationSpecPayload {
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) source_types: Vec<String>,
    #[serde(default)]
    pub(super) target_types: Vec<String>,
}

#[derive(Clone, Default)]
pub(super) struct RelationAllowlist {
    pub(super) enabled: bool,
    pub(super) pairs_by_relation: HashMap<String, HashSet<(String, String)>>,
}

impl RelationAllowlist {
    pub(super) fn from_meta(meta: &BTreeMap<String, String>) -> Result<Self, String> {
        let Some(encoded) = meta.get("ontology_relations") else {
            return Ok(Self::default());
        };
        let relation_specs: Vec<RelationSpecPayload> = serde_json::from_str(encoded)
            .map_err(|error| format!("invalid ontology_relations metadata: {error}"))?;
        let mut pairs_by_relation: HashMap<String, HashSet<(String, String)>> = HashMap::new();
        for relation in relation_specs {
            if relation.name.is_empty() {
                continue;
            }
            let pairs = pairs_by_relation.entry(relation.name).or_default();
            for source in &relation.source_types {
                for target in &relation.target_types {
                    pairs.insert((source.clone(), target.clone()));
                }
            }
        }
        Ok(Self {
            enabled: true,
            pairs_by_relation,
        })
    }

    pub(super) fn allows(&self, edge_type: &str, source: &str, target: &str) -> bool {
        if !self.enabled {
            return default_relation_allowed(edge_type, source, target);
        }
        self.pairs_by_relation
            .get(edge_type)
            .is_some_and(|pairs| pairs.contains(&(source.to_string(), target.to_string())))
    }
}
pub(super) fn default_relation_allowed(edge_type: &str, source: &str, target: &str) -> bool {
    match edge_type {
        "Imports" => {
            matches!(source, "File" | "Module" | "Scope")
                && matches!(
                    target,
                    "ImportDeclaration" | "Dependency" | "Module" | "Symbol"
                )
        }
        "References" => {
            matches!(
                source,
                "Reference"
                    | "Expression"
                    | "CallExpression"
                    | "Assignment"
                    | "ControlFlowBlock"
                    | "TypeAnnotation"
                    | "Decorator"
                    | "Query"
                    | "SecretRef"
            ) && (is_symbol_target(target) || matches!(target, "Module" | "Dependency"))
        }
        "Calls" => {
            matches!(
                source,
                "Function"
                    | "Method"
                    | "CallExpression"
                    | "Decorator"
                    | "APIEndpoint"
                    | "Route"
                    | "Component"
            ) && matches!(
                target,
                "CallExpression" | "Function" | "Method" | "Class" | "APIEndpoint"
            )
        }
        "ResolvesTo" => {
            matches!(
                source,
                "Reference"
                    | "ImportDeclaration"
                    | "CallExpression"
                    | "TypeAnnotation"
                    | "Decorator"
            ) && (is_symbol_target(target) || matches!(target, "Module" | "Dependency"))
        }
        "Documents" => {
            matches!(
                source,
                "DocumentationSource" | "DocumentationChunk" | "Literal"
            ) && (matches!(target, "Repository" | "File" | "Module") || is_declaration(target))
        }
        "EvidencedBy" => {
            (matches!(source, "Repository" | "File" | "Module" | "Dependency")
                || is_declaration(source)
                || is_expression(source)
                || is_documentation(source))
                && matches!(target, "SyntaxCapture" | "File" | "DocumentationChunk")
        }
        _ => true,
    }
}
