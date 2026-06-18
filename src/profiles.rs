use crate::protocol::{CaptureMapping, LanguageProfile};
use std::collections::BTreeMap;

pub(crate) struct ProfileSet {
    by_language: BTreeMap<String, LanguageProfile>,
    suffix_to_language: BTreeMap<String, String>,
}

impl ProfileSet {
    pub(crate) fn new(profiles: &[LanguageProfile]) -> Self {
        let mut by_language = BTreeMap::new();
        let mut suffix_to_language = BTreeMap::new();
        for profile in base_profiles().into_iter().chain(profiles.iter().cloned()) {
            for suffix in &profile.suffixes {
                suffix_to_language.insert(suffix.to_lowercase(), profile.language.clone());
            }
            by_language.insert(profile.language.clone(), profile);
        }
        Self {
            by_language,
            suffix_to_language,
        }
    }

    pub(crate) fn language_for_path(&self, path: &std::path::Path) -> Option<String> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(|extension| {
                let suffix = format!(".{}", extension.to_lowercase());
                self.suffix_to_language.get(&suffix)
            })
            .cloned()
    }

    pub(crate) fn profile_for_language(&self, language: &str) -> Option<&LanguageProfile> {
        self.by_language.get(language)
    }
}

fn base_profiles() -> Vec<LanguageProfile> {
    vec![
        LanguageProfile {
            language: "python".to_string(),
            suffixes: vec![".py".to_string()],
            grammar_package: "tree_sitter_python".to_string(),
            root_node_types: vec!["module".to_string()],
            capture_mappings: vec![
                mapping("definition.class", &["class_definition"], "Class"),
                mapping("definition.function", &["function_definition"], "Function"),
                mapping(
                    "reference.import",
                    &["import_statement", "import_from_statement"],
                    "ImportDeclaration",
                ),
                mapping("reference.call", &["call"], "CallExpression"),
            ],
        },
        LanguageProfile {
            language: "markdown".to_string(),
            suffixes: vec![".md".to_string(), ".mdx".to_string()],
            grammar_package: String::new(),
            root_node_types: vec!["Module".to_string()],
            capture_mappings: vec![
                mapping(
                    "doc.source",
                    &["DocumentationSource"],
                    "DocumentationSource",
                ),
                mapping("doc.chunk", &["DocumentationChunk"], "DocumentationChunk"),
            ],
        },
    ]
}

fn mapping(
    capture_name: &str,
    parser_node_types: &[&str],
    target_node_type: &str,
) -> CaptureMapping {
    CaptureMapping {
        capture_name: capture_name.to_string(),
        parser_node_types: parser_node_types
            .iter()
            .map(|item| item.to_string())
            .collect(),
        target_node_type: target_node_type.to_string(),
        relation_types: Vec::new(),
        context_rule: String::new(),
        construct: String::new(),
    }
}
