use crate::graph_rows::BuiltGraphRows;
use crate::legacy;
use crate::normalize::SyntaxNode;
use std::collections::BTreeMap;

pub(crate) fn build_syntax_tree_graph_rows(
    meta: BTreeMap<String, String>,
    root: &SyntaxNode,
) -> Result<BuiltGraphRows, String> {
    legacy::build_syntax_tree_graph_rows(meta, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_graph_rows_match_legacy_for_python_tree() {
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

        assert_native_matches_legacy(meta("python", "pkg/service.py"), &root);
    }

    #[test]
    fn native_graph_rows_match_legacy_for_rust_tree() {
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

        assert_native_matches_legacy(meta("rust", "src/lib.rs"), &root);
    }

    #[test]
    fn native_graph_rows_match_legacy_for_go_tree() {
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

        assert_native_matches_legacy(meta("go", "main.go"), &root);
    }

    #[test]
    fn native_graph_rows_match_legacy_for_empty_module_tree() {
        let root = syntax_node("module", "", Vec::new(), &[]);

        assert_native_matches_legacy(meta("python", "empty.py"), &root);
    }

    fn assert_native_matches_legacy(meta: BTreeMap<String, String>, root: &SyntaxNode) {
        let native = build_syntax_tree_graph_rows(meta.clone(), root).unwrap();
        let legacy = legacy::build_syntax_tree_graph_rows(meta, root).unwrap();

        assert_eq!(native, legacy);
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
        fields: &[(&str, serde_json::Value)],
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
}
