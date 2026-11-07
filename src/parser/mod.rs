mod captures;
mod fields;
mod markdown;
mod tree_sitter;
mod wat;

use crate::error::NativeError;
use crate::normalize::SyntaxNode;
use crate::protocol::{LanguageProfile, SourceSnapshot};

#[derive(Debug, Clone)]
pub(crate) struct ParseOutput {
    pub(crate) root: SyntaxNode,
    pub(crate) diagnostics: Vec<String>,
}

pub(crate) fn parse_file(
    snapshot: &SourceSnapshot,
    profile: &LanguageProfile,
) -> Result<ParseOutput, NativeError> {
    let source = snapshot.source.as_deref().ok_or_else(|| {
        NativeError::InvalidInput(format!(
            "missing source content for scanned file: {}",
            snapshot.path
        ))
    })?;
    parse_source(source, profile)
}

fn parse_source(source: &str, profile: &LanguageProfile) -> Result<ParseOutput, NativeError> {
    if profile.language == "markdown" {
        return Ok(markdown::parse_markdown_source(source, profile));
    }
    tree_sitter::parse_tree_sitter_source(source, profile)
}

pub(crate) fn grammar_node_types(profile: &LanguageProfile) -> Option<&'static str> {
    tree_sitter::grammar_node_types(profile)
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
    fn tree_sitter_parser_preserves_named_child_order_and_field_names() {
        let output = parse_source(
            "fn greet(name: String) -> String { name }",
            &profile("rust"),
        )
        .expect("rust parsing should succeed");

        let function = output
            .root
            .children
            .iter()
            .find(|node| node.node_type == "function_item")
            .expect("function item should be retained");
        let children = function
            .children
            .iter()
            .map(|child| (child.node_type.as_str(), child.field_name.as_deref()))
            .collect::<Vec<_>>();

        assert_eq!(
            children,
            vec![
                ("identifier", Some("name")),
                ("parameters", Some("parameters")),
                ("type_identifier", Some("return_type")),
                ("block", Some("body")),
            ]
        );
    }

    #[test]
    fn markdown_parser_marks_children_without_grammar_field_names() {
        let output = parse_source("# Overview\n\nDetails", &profile("markdown"))
            .expect("markdown parsing should succeed");

        assert!(!output.root.children.is_empty());
        assert!(output
            .root
            .children
            .iter()
            .all(|child| child.field_name.is_none()));
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
    fn css_tree_sitter_parser_preserves_stylesheet_structure() {
        let output = parse_source(
            "@import 'theme.css';\n.card, #hero { color: red; width: calc(100% - 1rem); }\n",
            &profile("css"),
        )
        .expect("CSS parsing should succeed");

        assert_eq!(output.root.node_type, "stylesheet");
        assert!(output.diagnostics.is_empty());
        assert!(contains_node_type(&output.root, "import_statement"));
        assert!(contains_node_type(&output.root, "rule_set"));
        assert!(contains_node_type(&output.root, "class_selector"));
        assert!(contains_node_type(&output.root, "id_selector"));
        assert!(contains_node_type(&output.root, "declaration"));
        assert!(marked_captures(&output.root).is_empty());
    }

    #[test]
    fn typescript_tree_sitter_parser_marks_profile_captures() {
        let output = parse_source(
            "import { value } from './dep';\ninterface Service { run(): void }\ntype Id = string;\nclass Worker implements Service { run() { value(); } }\nfunction helper(): Id { return 'ok'; }\n",
            &profile("typescript"),
        )
        .expect("TypeScript parsing should succeed");

        let captures = marked_captures(&output.root);
        assert!(captures
            .iter()
            .any(|(capture, _)| capture == "reference.import"));
        assert!(captures.contains(&("definition.interface".to_string(), "Service".to_string())));
        assert!(captures.contains(&("definition.type_alias".to_string(), "Id".to_string())));
        assert!(captures.contains(&("definition.class".to_string(), "Worker".to_string())));
        assert!(captures.contains(&("definition.method".to_string(), "run".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "helper".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "value".to_string())));
        assert_eq!(output.root.node_type, "program");
        assert!(output.diagnostics.is_empty());
    }

    #[test]
    fn tsx_tree_sitter_parser_uses_the_tsx_grammar() {
        let output = parse_source(
            "export function Card() { return <section><h1>Hello</h1></section>; }\n",
            &profile("tsx"),
        )
        .expect("TSX parsing should succeed");

        assert_eq!(output.root.node_type, "program");
        assert!(output.diagnostics.is_empty());
        assert!(contains_node_type(&output.root, "jsx_element"));
    }

    #[test]
    fn javascript_tree_sitter_parser_marks_profile_captures_and_jsx() {
        let output = parse_source(
            "import { value } from './dep.js';\nclass Worker { run() { value(); } }\nfunction Card() { return <section>Hello</section>; }\n",
            &profile("javascript"),
        )
        .expect("JavaScript parsing should succeed");

        let captures = marked_captures(&output.root);
        assert!(captures
            .iter()
            .any(|(capture, _)| capture == "reference.import"));
        assert!(captures.contains(&("definition.class".to_string(), "Worker".to_string())));
        assert!(captures.contains(&("definition.method".to_string(), "run".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "Card".to_string())));
        assert!(captures.contains(&("reference.call".to_string(), "value".to_string())));
        assert_eq!(output.root.node_type, "program");
        assert!(output.diagnostics.is_empty());
        assert!(contains_node_type(&output.root, "jsx_element"));
    }

    #[test]
    fn html_tree_sitter_parser_preserves_document_structure() {
        let output = parse_source(
            "<!doctype html><html><head><style>.card { color: red; }</style></head><body><main>Welcome</main><script>boot();</script></body></html>",
            &profile("html"),
        )
        .expect("HTML parsing should succeed");

        assert_eq!(output.root.node_type, "document");
        assert!(output.diagnostics.is_empty());
        assert!(contains_node_type(&output.root, "doctype"));
        assert!(contains_node_type(&output.root, "element"));
        assert!(contains_node_type(&output.root, "style_element"));
        assert!(contains_node_type(&output.root, "script_element"));
        assert!(marked_captures(&output.root).is_empty());
    }

    #[test]
    fn webassembly_tree_sitter_parser_marks_named_semantics() {
        let output = parse_source(
            r#"(module $math
  (type $binary (func (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32)))
  (func $add (export "add") (param i32 i32) (result i32)
    local.get 0
    call $log
    return_call $log)
  (memory 1)
  (export "memory" (memory 0)))
"#,
            &profile("webassembly"),
        )
        .expect("WebAssembly parsing should succeed");

        let captures = marked_captures(&output.root);

        assert_eq!(output.root.node_type, "root");
        assert!(output.diagnostics.is_empty());
        assert!(captures.contains(&("definition.module".to_string(), "$math".to_string())));
        assert!(captures.contains(&("definition.type_alias".to_string(), "$binary".to_string())));
        assert!(captures.contains(&("definition.function".to_string(), "$add".to_string())));
        assert!(captures.contains(&("reference.import".to_string(), "env".to_string())));
        assert!(captures.contains(&("definition.export".to_string(), "add".to_string())));
        assert!(captures.contains(&("definition.export".to_string(), "memory".to_string())));
        assert_eq!(
            captures
                .iter()
                .filter(|capture| capture.0 == "reference.call" && capture.1 == "$log")
                .count(),
            2
        );
    }

    #[test]
    fn webassembly_tree_sitter_parser_keeps_anonymous_constructs_syntax_only() {
        let output = parse_source(
            "(module (type (func)) (func (param i32) local.get 0))\n",
            &profile("webassembly"),
        )
        .expect("anonymous WebAssembly constructs should parse");

        let captures = marked_captures(&output.root);

        assert_eq!(output.root.node_type, "root");
        assert!(output.diagnostics.is_empty());
        assert!(captures.iter().all(|(capture, _)| !matches!(
            capture.as_str(),
            "definition.module" | "definition.function" | "definition.type_alias"
        )));
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

    fn contains_node_type(node: &SyntaxNode, node_type: &str) -> bool {
        node.node_type == node_type
            || node
                .children
                .iter()
                .any(|child| contains_node_type(child, node_type))
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

pub(crate) fn parse_guarded_tree(
    source: &[u8],
    profile: &LanguageProfile,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<::tree_sitter::Tree, crate::extraction_control::ExtractionError> {
    tree_sitter::parse_guarded_tree(source, profile, control)
}

#[cfg(test)]
mod guarded_tests {
    use super::*;
    use crate::{
        extraction_control::{ExtractionControl, ExtractionError},
        profiles::ProfileSet,
    };
    #[test]
    fn native_parse_progress_observes_cancellation_and_rejects_parse_errors() {
        let profile = ProfileSet::new(&[])
            .profile_for_language("rust")
            .unwrap()
            .clone();
        let mut calls = 0usize;
        let mut callback = || {
            calls += 1;
            if calls >= 2 {
                Err(ExtractionError::new("cancelled", "test cancellation"))
            } else {
                Ok(())
            }
        };
        let mut control =
            ExtractionControl::new(&mut callback, 100_000, 1_000_000, 1_000_000, 1_000_000);
        let source = "fn f() {}\n".repeat(100_000);
        let err = match parse_guarded_tree(source.as_bytes(), &profile, &mut control) {
            Ok(_) => panic!("parse should cancel"),
            Err(error) => error,
        };
        assert_eq!(err.code, "cancelled");
        let mut callback = || Ok(());
        let mut control = ExtractionControl::new(&mut callback, 100, 100, 100, 100);
        let err = match parse_guarded_tree(b"fn {", &profile, &mut control) {
            Ok(_) => panic!("parse errors must be unknown"),
            Err(error) => error,
        };
        assert_eq!(err.code, "reconstruction_unavailable");
    }
}

pub(crate) fn normalize_guarded_rust(
    tree: &::tree_sitter::Tree,
    source: &[u8],
    profile: &LanguageProfile,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<crate::normalize::FlatSyntax, crate::extraction_control::ExtractionError> {
    tree_sitter::normalize_rust_flat(tree, source, profile, control)
}

#[cfg(test)]
mod flat_tests {
    use super::*;
    use crate::{extraction_control::ExtractionControl, profiles::ProfileSet};
    #[test]
    fn flat_rust_nodes_preserve_legacy_fields_and_guard_depth() {
        let profile = ProfileSet::new(&[])
            .profile_for_language("rust")
            .unwrap()
            .clone();
        let source="use std::fmt; struct Thing { value: i32 } impl Thing { fn get(&self) -> i32 { self.value } } fn main() { Thing::get(); }";
        let legacy = super::parse_source(source, &profile).unwrap();
        let mut check = || Ok(());
        let mut ctl = ExtractionControl::new(&mut check, 1000, 1_000_000, 1_000_000, 1_000_000);
        let tree = parse_guarded_tree(source.as_bytes(), &profile, &mut ctl).unwrap();
        let flat = normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut ctl).unwrap();
        let mut stack = vec![(&legacy.root, 0usize)];
        let mut count = 0;
        while let Some((original, id)) = stack.pop() {
            let actual = &flat.nodes[id];
            count += 1;
            assert_eq!(
                (
                    &actual.node_type,
                    &actual.text,
                    &actual.capture_name,
                    &actual.field_name,
                    &actual.fields,
                    actual.byte_start,
                    actual.byte_end
                ),
                (
                    &original.node_type,
                    &original.text,
                    &original.capture_name,
                    &original.field_name,
                    &original.fields,
                    original.byte_start,
                    original.byte_end
                )
            );
            assert_eq!(flat.children[id].len(), original.children.len());
            for (child, &child_id) in original.children.iter().zip(&flat.children[id]).rev() {
                assert_eq!(flat.parents[child_id], Some(id));
                stack.push((child, child_id));
            }
        }
        assert_eq!(count, flat.nodes.len());
        let deep = format!("fn f() {{ {} 0 {} }}", "(".repeat(500), ")".repeat(500));
        let mut check = || Ok(());
        let mut ctl = ExtractionControl::new(&mut check, 64, 1_000_000, 1_000_000, 1_000_000);
        let tree = parse_guarded_tree(deep.as_bytes(), &profile, &mut ctl).unwrap();
        let error = match normalize_guarded_rust(&tree, deep.as_bytes(), &profile, &mut ctl) {
            Ok(_) => panic!("depth must be bounded"),
            Err(e) => e,
        };
        assert_eq!(error.code, "resource_limit");
        let mut check = || Ok(());
        let mut control =
            ExtractionControl::new(&mut check, 2_000, 10_000_000, 10_000_000, 10_000_000);
        let tree = parse_guarded_tree(deep.as_bytes(), &profile, &mut control).unwrap();
        let flat = normalize_guarded_rust(&tree, deep.as_bytes(), &profile, &mut control).unwrap();
        assert!(flat.nodes.len() > 500);
        drop(flat);
    }
}

#[cfg(test)]
mod guarded_copy_tests {
    use super::*;
    use crate::{
        extraction_control::{ExtractionControl, ExtractionError},
        profiles::ProfileSet,
    };
    #[test]
    fn cancellation_interrupts_field_and_normalization_copy_work() {
        let profile = ProfileSet::new(&[])
            .profile_for_language("rust")
            .unwrap()
            .clone();
        let source = format!("fn {}() {{}}", "a".repeat(20_000));
        let mut ok = || Ok(());
        let mut initial = ExtractionControl::new(&mut ok, 100, 1_000_000, 1000, 1000);
        let tree = parse_guarded_tree(source.as_bytes(), &profile, &mut initial).unwrap();
        let mut calls = 0usize;
        let mut cancel = || {
            calls += 1;
            if calls > 500 {
                Err(ExtractionError::new("cancelled", "copy cancelled"))
            } else {
                Ok(())
            }
        };
        let mut guarded = ExtractionControl::new(&mut cancel, 100, 1_000_000, 1000, 1000);
        let error = match normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut guarded) {
            Ok(_) => panic!("copy must stop"),
            Err(error) => error,
        };
        assert_eq!(error.code, "cancelled");
    }
}
