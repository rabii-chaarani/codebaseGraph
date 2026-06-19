use super::*;
use serde_json::json;

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

fn meta(language: &str, path: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("path".to_string(), path.to_string()),
        ("language".to_string(), language.to_string()),
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
        children,
        fields: fields
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    }
}
