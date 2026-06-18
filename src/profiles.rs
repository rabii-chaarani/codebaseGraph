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
        LanguageProfile {
            language: "rust".to_string(),
            suffixes: vec![".rs".to_string()],
            grammar_package: "tree_sitter_rust".to_string(),
            root_node_types: vec!["source_file".to_string()],
            capture_mappings: vec![
                mapping("definition.struct", &["struct_item"], "Class"),
                mapping_with_context(
                    "definition.method",
                    &["function_item"],
                    "Method",
                    "inside impl",
                ),
                mapping("definition.function", &["function_item"], "Function"),
                mapping("reference.use", &["use_declaration"], "ImportDeclaration"),
                mapping("reference.call", &["call_expression"], "CallExpression"),
                mapping("reference.call", &["macro_invocation"], "CallExpression"),
            ],
        },
        LanguageProfile {
            language: "go".to_string(),
            suffixes: vec![".go".to_string()],
            grammar_package: "tree_sitter_go".to_string(),
            root_node_types: vec!["source_file".to_string()],
            capture_mappings: vec![
                mapping("definition.package", &["package_clause"], "Module"),
                mapping("definition.function", &["function_declaration"], "Function"),
                mapping("definition.method", &["method_declaration"], "Method"),
                mapping(
                    "reference.import",
                    &["import_declaration"],
                    "ImportDeclaration",
                ),
                mapping("reference.call", &["call_expression"], "CallExpression"),
            ],
        },
        LanguageProfile {
            language: "c".to_string(),
            suffixes: vec![".c".to_string(), ".h".to_string()],
            grammar_package: "tree_sitter_c".to_string(),
            root_node_types: vec!["translation_unit".to_string()],
            capture_mappings: c_family_mappings(),
        },
        LanguageProfile {
            language: "cpp".to_string(),
            suffixes: vec![
                ".cc".to_string(),
                ".cpp".to_string(),
                ".cxx".to_string(),
                ".hpp".to_string(),
                ".hh".to_string(),
            ],
            grammar_package: "tree_sitter_cpp".to_string(),
            root_node_types: vec!["translation_unit".to_string()],
            capture_mappings: c_family_mappings(),
        },
        LanguageProfile {
            language: "fortran".to_string(),
            suffixes: vec![
                ".f".to_string(),
                ".f90".to_string(),
                ".f95".to_string(),
                ".for".to_string(),
            ],
            grammar_package: "tree_sitter_fortran".to_string(),
            root_node_types: vec!["translation_unit".to_string()],
            capture_mappings: vec![
                mapping("definition.module", &["module"], "Module"),
                mapping("definition.function", &["program"], "Function"),
                mapping("definition.function", &["subroutine"], "Function"),
                mapping("definition.function", &["function"], "Function"),
                mapping("reference.use", &["use_statement"], "ImportDeclaration"),
                mapping("reference.call", &["subroutine_call"], "CallExpression"),
                mapping("reference.call", &["call_expression"], "CallExpression"),
            ],
        },
    ]
}

fn c_family_mappings() -> Vec<CaptureMapping> {
    vec![
        mapping("definition.function", &["function_definition"], "Function"),
        mapping_with_context(
            "definition.function",
            &["declaration"],
            "Function",
            "function declarator",
        ),
        mapping("definition.struct", &["struct_specifier"], "Class"),
        mapping("definition.union", &["union_specifier"], "Class"),
        mapping("definition.enum", &["enum_specifier"], "Class"),
        mapping("definition.class", &["class_specifier"], "Class"),
        mapping(
            "reference.include",
            &["preproc_include"],
            "ImportDeclaration",
        ),
        mapping("reference.call", &["call_expression"], "CallExpression"),
    ]
}

fn mapping(
    capture_name: &str,
    parser_node_types: &[&str],
    target_node_type: &str,
) -> CaptureMapping {
    mapping_with_context(capture_name, parser_node_types, target_node_type, "")
}

fn mapping_with_context(
    capture_name: &str,
    parser_node_types: &[&str],
    target_node_type: &str,
    context_rule: &str,
) -> CaptureMapping {
    CaptureMapping {
        capture_name: capture_name.to_string(),
        parser_node_types: parser_node_types
            .iter()
            .map(|item| item.to_string())
            .collect(),
        target_node_type: target_node_type.to_string(),
        relation_types: Vec::new(),
        context_rule: context_rule.to_string(),
        construct: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn base_profiles_recognize_documented_language_suffixes() {
        let profiles = ProfileSet::new(&[]);
        let cases = [
            ("service.py", "python"),
            ("README.md", "markdown"),
            ("README.mdx", "markdown"),
            ("src/lib.rs", "rust"),
            ("main.go", "go"),
            ("service.c", "c"),
            ("service.h", "c"),
            ("service.cc", "cpp"),
            ("service.cpp", "cpp"),
            ("service.cxx", "cpp"),
            ("service.hpp", "cpp"),
            ("service.hh", "cpp"),
            ("solver.f", "fortran"),
            ("solver.f90", "fortran"),
            ("solver.f95", "fortran"),
            ("solver.for", "fortran"),
        ];

        for (path, language) in cases {
            assert_eq!(
                profiles.language_for_path(Path::new(path)).as_deref(),
                Some(language),
                "{path} should resolve to {language}"
            );
        }
    }
}
