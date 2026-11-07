use super::*;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn syntax_materializer_builds_python_class_and_method_rows() {
    let root = syntax_node(
        "module",
        "class Service:\n    def handle(self):\n        return call()\n",
        vec![syntax_node(
            "class_definition",
            "class Service:\n    def handle(self):\n        return call()",
            vec![syntax_node(
                "function_definition",
                "def handle(self):\n        return call()",
                Vec::new(),
                &[("name", json!("handle"))],
            )],
            &[("name", json!("Service"))],
        )],
        &[],
    );

    let rows = build_syntax_tree_graph_rows(meta("python", "pkg/service.py"), &root).unwrap();
    assert!(rows
        .nodes
        .iter()
        .any(|node| node.table == "Class" && node.label == "Service"));
    assert!(rows.nodes.iter().any(|node| node.label.contains("handle")));
    assert!(rows.edges.iter().any(|edge| edge.edge_type == "Defines"));
}

#[test]
fn syntax_materializer_builds_rust_function_rows() {
    let root = syntax_node(
        "source_file",
        "fn handle() { call(); }",
        vec![syntax_node(
            "function_item",
            "fn handle() { call(); }",
            Vec::new(),
            &[("name", json!("handle"))],
        )],
        &[],
    );

    let rows = build_syntax_tree_graph_rows(meta("rust", "src/lib.rs"), &root).unwrap();
    assert!(rows
        .nodes
        .iter()
        .any(|node| node.table == "Function" && node.label == "handle"));
}

#[test]
fn syntax_materializer_builds_go_function_rows() {
    let root = syntax_node(
        "source_file",
        "package main\nfunc Handle() { Call() }\n",
        vec![syntax_node(
            "function_declaration",
            "func Handle() { Call() }",
            Vec::new(),
            &[("name", json!("Handle"))],
        )],
        &[],
    );

    let rows = build_syntax_tree_graph_rows(meta("go", "main.go"), &root).unwrap();
    assert!(rows
        .nodes
        .iter()
        .any(|node| node.table == "Function" && node.label == "Handle"));
}

#[test]
fn syntax_materializer_builds_empty_module_tree() {
    let root = syntax_node("module", "", Vec::new(), &[]);

    let rows = build_syntax_tree_graph_rows(meta("python", "empty.py"), &root).unwrap();
    assert!(rows
        .nodes
        .iter()
        .any(|node| node.table == "Module" && node.path == "empty.py"));
}

#[test]
fn syntax_materializer_deduplicates_repeated_nodes() {
    let duplicate = syntax_node(
        "function_definition",
        "def same():\n    pass",
        Vec::new(),
        &[("name", json!("same"))],
    );
    let root = syntax_node(
        "module",
        "def same():\n    pass\ndef same():\n    pass\n",
        vec![duplicate.clone(), duplicate],
        &[],
    );

    let native = build_syntax_tree_graph_rows(meta("python", "pkg/dupe.py"), &root).unwrap();

    assert_eq!(
        native
            .nodes
            .iter()
            .filter(|node| node.table == "Function" && node.label == "same")
            .count(),
        1
    );
}

#[test]
fn syntax_materializer_manifest_ids_remain_stable() {
    let root = syntax_node(
        "module",
        "import os\nVALUE = call()\n",
        vec![
            syntax_node(
                "import_statement",
                "import os",
                Vec::new(),
                &[("module", json!("os"))],
            ),
            syntax_node(
                "assignment",
                "VALUE = call()",
                vec![syntax_node("call", "call()", Vec::new(), &[])],
                &[("target", json!("VALUE"))],
            ),
        ],
        &[],
    );

    let rows = build_syntax_tree_graph_rows(meta("python", "pkg/stable.py"), &root).unwrap();
    let ids = rows
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();

    assert!(ids.contains(&"Module:e1d78e658a62137527fd"));
    assert!(ids.contains(&"ImportDeclaration:0b00e4257b4bd2af9e92"));
    assert!(ids.contains(&"Constant:f30f45c3854762d38187"));
}

#[test]
fn syntax_materializer_materializes_ordered_grammar_tree_with_provenance() {
    let mut function = syntax_node(
        "function_definition",
        "def handle():\n    pass",
        Vec::new(),
        &[("name", json!("handle"))],
    );
    function.field_name = Some("body".to_string());
    let mut class = syntax_node(
        "class_definition",
        "class Service:\n    def handle():\n        pass",
        vec![function],
        &[("name", json!("Service"))],
    );
    class.field_name = Some("body".to_string());
    let root = syntax_node(
        "module",
        "class Service:\n    def handle():\n        pass",
        vec![class],
        &[],
    );

    let rows = build_syntax_tree_graph_rows(meta("python", "pkg/service.py"), &root).unwrap();
    let syntax_nodes = rows
        .nodes
        .iter()
        .filter(|node| node.table == "SyntaxCapture")
        .collect::<Vec<_>>();
    assert_eq!(syntax_nodes.len(), 3);
    assert!(syntax_nodes
        .iter()
        .all(|node| { node.grammar_version.as_deref() == Some("tree_sitter_python@test") }));

    let root_syntax = syntax_nodes
        .iter()
        .find(|node| node.tree_sitter_node_type == "module")
        .unwrap();
    let class_syntax = syntax_nodes
        .iter()
        .find(|node| node.tree_sitter_node_type == "class_definition")
        .unwrap();
    let function_syntax = syntax_nodes
        .iter()
        .find(|node| node.tree_sitter_node_type == "function_definition")
        .unwrap();

    assert!(rows.edges.iter().any(|edge| {
        edge.edge_type == "EvidencedBy"
            && edge.kind == "syntax_root"
            && edge.target_id == root_syntax.id
    }));
    assert!(rows.edges.iter().any(|edge| {
        edge.edge_type == "SyntaxChild"
            && edge.source_id == root_syntax.id
            && edge.target_id == class_syntax.id
            && edge.field_name.as_deref() == Some("body")
            && edge.child_index == Some(0)
    }));
    assert!(rows.edges.iter().any(|edge| {
        edge.edge_type == "SyntaxChild"
            && edge.source_id == class_syntax.id
            && edge.target_id == function_syntax.id
            && edge.field_name.as_deref() == Some("body")
            && edge.child_index == Some(0)
    }));
    assert!(rows.edges.iter().any(|edge| {
        edge.edge_type == "DerivedFrom"
            && (edge.source_id.starts_with("Function:") || edge.source_id.starts_with("Method:"))
            && edge.target_id == function_syntax.id
    }));
}

fn meta(language: &str, path: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("path".to_string(), path.to_string()),
        ("language".to_string(), language.to_string()),
        (
            "grammar_version".to_string(),
            format!("tree_sitter_{language}@test"),
        ),
        ("source_root".to_string(), "/repo".to_string()),
        ("repository_label".to_string(), "repo".to_string()),
    ])
}

fn syntax_node(
    node_type: &str,
    text: &str,
    children: Vec<SyntaxNode>,
    fields: &[(&str, Value)],
) -> SyntaxNode {
    SyntaxNode {
        node_type: node_type.to_string(),
        text: text.to_string(),
        line_start: Some(1),
        line_end: Some(text.lines().count().max(1) as i64),
        byte_start: Some(0),
        byte_end: Some(text.len() as i64),
        capture_name: String::new(),
        field_name: None,
        children,
        fields: fields
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    }
}

#[test]
fn flat_arena_rust_rows_match_legacy_rows() {
    use crate::{
        extraction_control::ExtractionControl, parser, profiles::ProfileSet,
        protocol::SourceSnapshot,
    };
    let source="use std::fmt; struct Thing { value: i32 } impl Thing { fn get(&self) -> i32 { self.value } } fn main() { Thing::get(); }";
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let snapshot = SourceSnapshot {
        path: "src/lib.rs".into(),
        absolute_path: String::new(),
        content_hash: String::new(),
        language: Some("rust".into()),
        byte_len: source.len() as u64,
        source: Some(source.into()),
    };
    let legacy = parser::parse_file(&snapshot, &profile).unwrap();
    let expected = build_syntax_tree_graph_rows(meta("rust", "src/lib.rs"), &legacy.root).unwrap();
    let mut check = || Ok(());
    let mut control = ExtractionControl::new(&mut check, 1000, 1_000_000, 1_000_000, 1_000_000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut control).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut control).unwrap();
    let actual = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut control).unwrap();
    assert_eq!(actual.nodes, expected.nodes);
    assert_eq!(actual.edges, expected.edges);
    assert!(actual.nodes.iter().any(|n| n.table == "Function"
        && n.byte_start.is_some()
        && n.byte_end.is_some()
        && !n.qualified_name.is_empty()));
    assert!(actual.edges.iter().any(|e| e.edge_type == "Contains"
        && actual
            .nodes
            .iter()
            .any(|n| n.id == e.source_id && n.table == "Scope")));
}

#[test]
fn rust_source_known_node_and_scope_identity_vector() {
    use crate::{extraction_control::ExtractionControl, parser, profiles::ProfileSet};
    let source = "fn hello() {}";
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let mut check = || Ok(());
    let mut control = ExtractionControl::new(&mut check, 100, 10_000, 10_000, 10_000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut control).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut control).unwrap();
    let arena = NativeSyntaxArena::from_flat(&flat, &mut control).unwrap();
    let mut builder = NativeBuilder::new(meta("rust", "src/lib.rs")).unwrap();
    builder.build_tree(&arena, arena.root_id).unwrap();
    let rows = builder.into_rows();
    let function = rows
        .nodes
        .iter()
        .find(|n| n.id == "Function:216f9df7cf9055f46285")
        .unwrap();
    assert_eq!(
        (
            function.table.as_str(),
            function.qualified_name.as_str(),
            function.path.as_str(),
            function.byte_start,
            function.byte_end
        ),
        ("Function", "src.lib.hello", "src/lib.rs", Some(0), Some(13))
    );
    assert!(rows.edges.iter().any(|e| e.edge_type == "Contains"
        && e.source_id == "Scope:41046104907937f3c6e1"
        && e.target_id == function.id));
}

#[test]
fn flat_materializer_enforces_vertex_edge_and_cancellation_limits() {
    use crate::{
        extraction_control::{ExtractionControl, ExtractionError},
        parser,
        profiles::ProfileSet,
    };
    let source = "fn hello() {}";
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let mut check = || Ok(());
    let mut parser_control = ExtractionControl::new(&mut check, 100, 10_000, 10_000, 10_000);
    let tree =
        parser::parse_guarded_tree(source.as_bytes(), &profile, &mut parser_control).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut parser_control)
            .unwrap();
    for (vertices, edges) in [(1, 10_000), (10_000, 1)] {
        let mut check = || Ok(());
        let mut control = ExtractionControl::new(&mut check, 100, 10_000, vertices, edges);
        let result = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut control);
        assert_eq!(result.err().unwrap().code, "resource_limit");
    }
    // Arena admission consumes one visit per flat node; this limit permits
    // admission and forces failure inside the subsequent materializer work.
    let mut check = || Ok(());
    let mut traversal =
        ExtractionControl::new(&mut check, 100, flat.nodes.len() as u64 + 5, 10_000, 10_000);
    assert_eq!(
        build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut traversal)
            .err()
            .unwrap()
            .code,
        "resource_limit"
    );
    let mut check = || Err(ExtractionError::new("cancelled", "cancelled"));
    let mut control = ExtractionControl::new(&mut check, 100, 10_000, 10_000, 10_000);
    assert_eq!(
        build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut control)
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
}

#[test]
fn flat_materializer_exact_vertex_and_edge_limits() {
    use crate::{extraction_control::ExtractionControl, parser, profiles::ProfileSet};
    let source = "fn hello() {}";
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let mut check = || Ok(());
    let mut parse_control = ExtractionControl::new(&mut check, 100, 1_000_000, 10_000, 10_000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut parse_control).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut parse_control)
            .unwrap();
    let mut check = || Ok(());
    let mut full = ExtractionControl::new(&mut check, 100, 1_000_000, 10_000, 10_000);
    let rows = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut full).unwrap();
    let vertices = rows.nodes.len() as u64;
    let edges = rows.edges.len() as u64;
    let mut check = || Ok(());
    let mut exact = ExtractionControl::new(&mut check, 100, 1_000_000, vertices, edges);
    let exact_rows = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut exact).unwrap();
    assert_eq!(exact_rows.nodes, rows.nodes);
    assert_eq!(exact_rows.edges, rows.edges);
    for (v, e) in [(vertices - 1, edges), (vertices, edges - 1)] {
        let mut check = || Ok(());
        let mut control = ExtractionControl::new(&mut check, 100, 1_000_000, v, e);
        assert_eq!(
            build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut control)
                .err()
                .unwrap()
                .code,
            "resource_limit"
        );
    }
}

#[test]
fn materializer_observes_cancellation_during_copy() {
    use crate::{
        extraction_control::{ExtractionControl, ExtractionError},
        parser,
        profiles::ProfileSet,
    };
    let source = format!("fn {}() {{}}", "a".repeat(10_000));
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let mut check = || Ok(());
    let mut parse_control = ExtractionControl::new(&mut check, 100, 1_000_000, 1000, 1000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut parse_control).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut parse_control)
            .unwrap();
    let mut calls = 0;
    let mut cancel = || {
        calls += 1;
        if calls > 500 {
            Err(ExtractionError::new("cancelled", "copy cancelled"))
        } else {
            Ok(())
        }
    };
    let mut control = ExtractionControl::new(&mut cancel, 100, 1_000_000, 1000, 1000);
    assert_eq!(
        build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut control)
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
}

#[test]
fn guarded_real_traversal_reserve_failure_is_fatal() {
    use crate::{extraction_control::ExtractionControl, parser, profiles::ProfileSet};
    let source = "fn hello() {}";
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let mut check = || Ok(());
    let mut parse = ExtractionControl::new(&mut check, 100, 100_000, 1000, 1000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut parse).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut parse).unwrap();
    let mut check = || Ok(());
    let mut guard = ExtractionControl::new(&mut check, 100, 100_000, 1000, 1000);
    guard.inject_reserve_failure_after(0);
    let error = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut guard).unwrap_err();
    assert_eq!(error.code, "resource_limit");
}

#[test]
fn guarded_sha1_preserves_vectors_and_interrupts_hash_blocks() {
    use crate::extraction_control::{ExtractionControl, ExtractionError};
    for (len, expected) in [
        (0, "da39a3ee5e6b4b0d3255"),
        (55, "c1c8bbdc22796e28c0e1"),
        (56, "c2db330f6083854c99d4"),
        (63, "03f09f5b158a7a8cdad9"),
        (64, "0098ba824b5c16427bd7"),
        (65, "11655326c708d70319be"),
        (4096, "8c51fb6a0b587ec95ca7"),
    ] {
        let input = "a".repeat(len);
        let mut check = || Ok(());
        let mut control = ExtractionControl::new(&mut check, 10, 100_000, 10, 10);
        assert_eq!(
            graph_id_guarded("Node", &input, &mut control).unwrap(),
            format!("Node:{expected}")
        );
        assert_eq!(graph_id("Node", &input), format!("Node:{expected}"));
    }
    let input = "a".repeat(4096);
    let mut check = || Ok(());
    let mut control = ExtractionControl::new(&mut check, 10, 2, 10, 10);
    assert_eq!(
        graph_id_guarded("Node", &input, &mut control)
            .unwrap_err()
            .code,
        "resource_limit"
    );
    let mut calls = 0;
    let mut check = || {
        calls += 1;
        if calls >= 3 {
            Err(ExtractionError::new("cancelled", "hash cancelled"))
        } else {
            Ok(())
        }
    };
    let mut control = ExtractionControl::new(&mut check, 10, 100_000, 10, 10);
    assert_eq!(
        graph_id_guarded("Node", &input, &mut control)
            .unwrap_err()
            .code,
        "cancelled"
    );
}

#[test]
fn large_valid_root_uses_same_guarded_rows_as_legacy() {
    use crate::{
        extraction_control::ExtractionControl, parser, profiles::ProfileSet,
        protocol::SourceSnapshot,
    };
    let source = format!("{}\nfn hello() {{}}", "//x\n".repeat(1024));
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let snapshot = SourceSnapshot {
        path: "src/lib.rs".into(),
        absolute_path: String::new(),
        content_hash: String::new(),
        language: Some("rust".into()),
        byte_len: source.len() as u64,
        source: Some(source.clone()),
    };
    let legacy = parser::parse_file(&snapshot, &profile).unwrap();
    let expected = build_syntax_tree_graph_rows(meta("rust", "src/lib.rs"), &legacy.root).unwrap();
    let mut check = || Ok(());
    let mut control = ExtractionControl::new(&mut check, 1000, 10_000_000, 100_000, 100_000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut control).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut control).unwrap();
    let actual = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut control).unwrap();
    assert_eq!(actual.nodes, expected.nodes);
    assert_eq!(actual.edges, expected.edges);
}

#[test]
fn long_rust_reference_edge_copy_is_interruptible_and_preserves_rows() {
    use crate::{
        extraction_control::{ExtractionControl, ExtractionError},
        parser,
        profiles::ProfileSet,
        protocol::SourceSnapshot,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let name = format!("long_name_{}", "x".repeat(2048));
    let source = format!("fn {name}() {{}} fn use_it() {{ {name}(); }}");
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let snapshot = SourceSnapshot {
        path: "src/lib.rs".into(),
        absolute_path: String::new(),
        content_hash: String::new(),
        language: Some("rust".into()),
        byte_len: source.len() as u64,
        source: Some(source.clone()),
    };
    let legacy = parser::parse_file(&snapshot, &profile).unwrap();
    let expected = build_syntax_tree_graph_rows(meta("rust", "src/lib.rs"), &legacy.root).unwrap();
    assert!(expected
        .edges
        .iter()
        .any(|edge| edge.edge_type == "References"
            && edge.metadata.get("label").and_then(Value::as_str) == Some(name.as_str())));
    let mut check = || Ok(());
    let mut parse = ExtractionControl::new(&mut check, 1000, 10_000_000, 100_000, 100_000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut parse).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut parse).unwrap();
    let mut check = || Ok(());
    let mut guard = ExtractionControl::new(&mut check, 1000, 10_000_000, 100_000, 100_000);
    let actual = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut guard).unwrap();
    assert_eq!(actual.nodes, expected.nodes);
    assert_eq!(actual.edges, expected.edges);
    let flag = Arc::new(AtomicBool::new(false));
    let seen = flag.clone();
    let mut after = 0usize;
    let mut cancel = || {
        if seen.load(Ordering::Relaxed) {
            after += 1;
            if after > 20 {
                return Err(ExtractionError::new(
                    "cancelled",
                    "edge label copy cancelled",
                ));
            }
        }
        Ok(())
    };
    let mut guard = ExtractionControl::new(&mut cancel, 1000, 10_000_000, 100_000, 100_000);
    guard.watch_edge_copy(name, flag.clone());
    let error = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut guard).unwrap_err();
    assert!(flag.load(Ordering::Relaxed));
    assert_eq!(error.code, "cancelled");
}

#[test]
fn long_rust_assignment_target_copy_and_classification_are_interruptible() {
    use crate::{
        extraction_control::{ExtractionControl, ExtractionError},
        parser,
        profiles::ProfileSet,
        protocol::SourceSnapshot,
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let name = format!("long_target_{}", "x".repeat(2048));
    let source = format!("fn f() {{ let mut {name}=0; {name}=other; }}");
    let profile = ProfileSet::new(&[])
        .profile_for_language("rust")
        .unwrap()
        .clone();
    let snapshot = SourceSnapshot {
        path: "src/lib.rs".into(),
        absolute_path: String::new(),
        content_hash: String::new(),
        language: Some("rust".into()),
        byte_len: source.len() as u64,
        source: Some(source.clone()),
    };
    let legacy = parser::parse_file(&snapshot, &profile).unwrap();
    let expected = build_syntax_tree_graph_rows(meta("rust", "src/lib.rs"), &legacy.root).unwrap();
    assert!(expected
        .nodes
        .iter()
        .any(|node| node.table == "Variable" && node.label == name));
    let mut check = || Ok(());
    let mut parse = ExtractionControl::new(&mut check, 1000, 10_000_000, 100_000, 100_000);
    let tree = parser::parse_guarded_tree(source.as_bytes(), &profile, &mut parse).unwrap();
    let flat =
        parser::normalize_guarded_rust(&tree, source.as_bytes(), &profile, &mut parse).unwrap();
    let mut check = || Ok(());
    let mut guard = ExtractionControl::new(&mut check, 1000, 10_000_000, 100_000, 100_000);
    let actual = build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut guard).unwrap();
    assert_eq!(actual.nodes, expected.nodes);
    assert_eq!(actual.edges, expected.edges);
    for classify in [false, true] {
        let flag = Arc::new(AtomicBool::new(false));
        let seen = flag.clone();
        let mut after = 0usize;
        let mut cancel = || {
            if seen.load(Ordering::Relaxed) {
                after += 1;
                if after > 20 {
                    return Err(ExtractionError::new(
                        "cancelled",
                        "assignment work cancelled",
                    ));
                }
            }
            Ok(())
        };
        let mut guard = ExtractionControl::new(&mut cancel, 1000, 10_000_000, 100_000, 100_000);
        if classify {
            guard.watch_assignment_classification(name.clone(), flag.clone());
        } else {
            guard.watch_assignment_copy(name.clone(), flag.clone());
        }
        let error =
            build_flat_syntax_rows(meta("rust", "src/lib.rs"), &flat, &mut guard).unwrap_err();
        assert!(flag.load(Ordering::Relaxed));
        assert_eq!(error.code, "cancelled");
    }
}
