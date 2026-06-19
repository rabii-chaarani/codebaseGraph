mod captures;
mod fields;
mod markdown;
mod tree_sitter;

use crate::error::NativeError;
use crate::normalize::SyntaxNode;
use crate::protocol::{LanguageProfile, SourceSnapshot};
use std::fs;

#[derive(Debug, Clone)]
pub(crate) struct ParseOutput {
    pub(crate) root: SyntaxNode,
    pub(crate) diagnostics: Vec<String>,
}

pub(crate) fn parse_file(
    snapshot: &SourceSnapshot,
    profile: &LanguageProfile,
) -> Result<ParseOutput, NativeError> {
    let source = fs::read_to_string(&snapshot.absolute_path)?;
    parse_source(&source, profile)
}

fn parse_source(source: &str, profile: &LanguageProfile) -> Result<ParseOutput, NativeError> {
    if profile.language == "markdown" {
        return Ok(markdown::parse_markdown_source(source, profile));
    }
    tree_sitter::parse_tree_sitter_source(source, profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::ProfileSet;
    use crate::protocol::LanguageProfile;
    use serde_json::Value;

    fn profile(language: &str) -> LanguageProfile {
        ProfileSet::new(&[])
            .profile_for_language(language)
            .unwrap_or_else(|| panic!("{language} profile should exist"))
            .clone()
    }

    #[test]
    fn rust_tree_sitter_parser_marks_profile_captures() {
        let output = parse_source(
            "use std::fmt;\nstruct Service;\nimpl Service { fn new() -> Self { Service } }\nfn helper() { Service::new(); }\n",
            &profile("rust"),
        )
        .expect("rust parsing should succeed");

        let captures = marked_captures(&output.root);

        assert!(captures.contains(&("reference.use".to_string(), "std::fmt".to_string())));
        assert!(captures.contains(&("definition.struct".to_string(), "Service".to_string())));
        assert!(captures.contains(&("definition.method".to_string(), "new".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "helper".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "Service::new".to_string())));
        assert_eq!(output.root.node_type, "source_file");
    }

    #[test]
    fn go_tree_sitter_parser_derives_import_and_call_labels() {
        let output = parse_source(
            "package main\nimport \"fmt\"\nfunc helper() { fmt.Println(1) }\n",
            &profile("go"),
        )
        .expect("go parsing should succeed");

        let captures = marked_captures(&output.root);

        assert!(captures.contains(&("definition.package".to_string(), "main".to_string())));
        assert!(captures.contains(&("reference.import".to_string(), "fmt".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "helper".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "fmt.Println".to_string())));
    }

    #[test]
    fn c_tree_sitter_parser_marks_profile_captures() {
        let output = parse_source(
            "#include <stdio.h>\nstruct Service { int id; };\nint helper() { printf(\"ok\"); return 1; }\n",
            &profile("c"),
        )
        .expect("c parsing should succeed");

        let captures = marked_captures(&output.root);

        assert!(captures.contains(&("reference.include".to_string(), "stdio.h".to_string())));
        assert!(captures.contains(&("definition.struct".to_string(), "Service".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "helper".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "printf".to_string())));
    }

    #[test]
    fn cpp_tree_sitter_parser_marks_profile_captures() {
        let output = parse_source(
            "#include <iostream>\nclass Service { public: void run() { helper(); } };\nint helper() { return 1; }\n",
            &profile("cpp"),
        )
        .expect("cpp parsing should succeed");

        let captures = marked_captures(&output.root);

        assert!(captures.contains(&("reference.include".to_string(), "iostream".to_string())));
        assert!(captures.contains(&("definition.class".to_string(), "Service".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "helper".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "helper".to_string())));
    }

    #[test]
    fn fortran_tree_sitter_parser_marks_profile_captures() {
        let output = parse_source(
            "module service_mod\ncontains\nsubroutine helper()\nuse iso_fortran_env\ncall run()\nend subroutine helper\nend module service_mod\n",
            &profile("fortran"),
        )
        .expect("fortran parsing should succeed");

        let captures = marked_captures(&output.root);

        assert!(captures.contains(&("definition.module".to_string(), "service_mod".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "helper".to_string())));
        assert!(captures.contains(&("reference.use".to_string(), "iso_fortran_env".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "run".to_string())));
    }

    fn marked_captures(root: &SyntaxNode) -> Vec<(String, String)> {
        let mut captures = Vec::new();
        collect_marked_captures(root, &mut captures);
        captures
    }

    fn collect_marked_captures(node: &SyntaxNode, captures: &mut Vec<(String, String)>) {
        if !node.capture_name.is_empty() {
            captures.push((node.capture_name.clone(), test_label(node)));
        }
        for child in &node.children {
            collect_marked_captures(child, captures);
        }
    }

    fn test_label(node: &SyntaxNode) -> String {
        for key in ["name", "id", "arg", "attr", "module", "path", "function"] {
            let Some(value) = node.fields.get(key).and_then(Value::as_str) else {
                continue;
            };
            if !value.is_empty() {
                return value.to_string();
            }
        }
        if node.text.trim().is_empty() {
            node.node_type.clone()
        } else {
            node.text.trim().to_string()
        }
    }
}
