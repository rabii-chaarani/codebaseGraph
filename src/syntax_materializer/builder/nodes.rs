use super::NativeBuilder;
use crate::graph_rows::{BuiltGraphRows, GraphNodeRow};
use crate::syntax_materializer::{
    empty_metadata, graph_id, imported_name, kind_for, qualified_name, stable_optional_i64,
    symbol_key, Capture,
};
use serde_json::{Map, Value};

impl NativeBuilder {
    pub(in crate::syntax_materializer) fn support_node(
        &mut self,
        table: &str,
        stable_key: &str,
        label: &str,
        path: &str,
    ) -> GraphNodeRow {
        let mut metadata = empty_metadata();
        metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.to_string()),
        );
        let node = GraphNodeRow {
            id: graph_id(table, stable_key),
            table: table.to_string(),
            label: label.to_string(),
            kind: table.to_lowercase(),
            language: String::new(),
            path: path.to_string(),
            qualified_name: String::new(),
            scope_id: String::new(),
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            tree_sitter_node_type: String::new(),
            capture_name: String::new(),
            summary: label.to_string(),
            metadata: Value::Object(metadata),
        };
        self.add_node(node)
    }

    pub(in crate::syntax_materializer) fn semantic_node(
        &mut self,
        table: &str,
        capture: &Capture,
        label: &str,
        owner_id: &str,
        owner_qualified_name: &str,
        metadata: Option<Map<String, Value>>,
    ) -> GraphNodeRow {
        let qualified_name = qualified_name(owner_qualified_name, label);
        let stable_key = [
            self.path.as_str(),
            table,
            qualified_name.as_str(),
            capture.node_type.as_str(),
            &stable_optional_i64(capture.line_start),
            &stable_optional_i64(capture.byte_start),
            label,
        ]
        .join("|");
        let mut node_metadata = empty_metadata();
        node_metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.clone()),
        );
        if let Some(extra) = metadata {
            for (key, value) in extra {
                node_metadata.insert(key, value);
            }
        }
        let summary = if matches!(table, "DocumentationSource" | "DocumentationChunk")
            && !capture.text.trim().is_empty()
        {
            capture.text.trim().to_string()
        } else {
            label.to_string()
        };
        let node = GraphNodeRow {
            id: graph_id(table, &stable_key),
            table: table.to_string(),
            label: label.to_string(),
            kind: kind_for(table, &capture.node_type),
            language: self.language.clone(),
            path: self.path.clone(),
            qualified_name,
            scope_id: owner_id.to_string(),
            line_start: capture.line_start,
            line_end: capture.line_end,
            byte_start: capture.byte_start,
            byte_end: capture.byte_end,
            tree_sitter_node_type: capture.node_type.clone(),
            capture_name: capture.capture_name.clone(),
            summary,
            metadata: Value::Object(node_metadata),
        };
        self.add_node(node)
    }

    pub(in crate::syntax_materializer) fn symbol_node(&mut self, label: &str) -> GraphNodeRow {
        let stable_key = format!("{}|Symbol|{}", self.path, label.trim());
        let mut metadata = empty_metadata();
        metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.clone()),
        );
        metadata.insert(
            "resolution".to_string(),
            Value::String("name_placeholder".to_string()),
        );
        let node = GraphNodeRow {
            id: graph_id("Symbol", &stable_key),
            table: "Symbol".to_string(),
            label: label.trim().to_string(),
            kind: "symbol_reference".to_string(),
            language: self.language.clone(),
            path: self.path.clone(),
            qualified_name: label.trim().to_string(),
            scope_id: String::new(),
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            tree_sitter_node_type: String::new(),
            capture_name: String::new(),
            summary: label.trim().to_string(),
            metadata: Value::Object(metadata),
        };
        self.add_node(node)
    }

    pub(in crate::syntax_materializer) fn scope_for(
        &mut self,
        owner: &GraphNodeRow,
    ) -> GraphNodeRow {
        let stable_key = format!("{}|{}|scope", self.path, owner.id);
        let mut metadata = empty_metadata();
        metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.clone()),
        );
        let node = GraphNodeRow {
            id: graph_id("Scope", &stable_key),
            table: "Scope".to_string(),
            label: format!("{} scope", owner.label),
            kind: format!("{}_scope", owner.table.to_lowercase()),
            language: owner.language.clone(),
            path: owner.path.clone(),
            qualified_name: format!(
                "{}.<scope>",
                if owner.qualified_name.is_empty() {
                    &owner.label
                } else {
                    &owner.qualified_name
                }
            ),
            scope_id: owner.id.clone(),
            line_start: owner.line_start,
            line_end: owner.line_end,
            byte_start: owner.byte_start,
            byte_end: owner.byte_end,
            tree_sitter_node_type: String::new(),
            capture_name: String::new(),
            summary: format!("Scope for {}", owner.label),
            metadata: Value::Object(metadata),
        };
        self.add_node(node)
    }

    pub(in crate::syntax_materializer) fn syntax_capture(&mut self, capture: &Capture) -> String {
        let stable_key = [
            self.path.as_str(),
            capture.node_type.as_str(),
            &stable_optional_i64(capture.line_start),
            &stable_optional_i64(capture.byte_start),
            capture.label.as_str(),
        ]
        .join("|");
        let syntax_id = graph_id("SyntaxCapture", &stable_key);
        if self.nodes.contains_key(&syntax_id) {
            return syntax_id;
        }
        let mut metadata = empty_metadata();
        metadata.insert("canonical_key".to_string(), Value::String(stable_key));
        metadata.insert(
            "fields".to_string(),
            Value::Array(
                capture
                    .fields
                    .iter()
                    .map(|field| Value::String(field.clone()))
                    .collect(),
            ),
        );
        let summary = capture.text.chars().take(160).collect();
        let node = GraphNodeRow {
            id: syntax_id.clone(),
            table: "SyntaxCapture".to_string(),
            label: if capture.capture_name.is_empty() {
                capture.node_type.clone()
            } else {
                capture.capture_name.clone()
            },
            kind: capture.node_type.clone(),
            language: self.language.clone(),
            path: self.path.clone(),
            qualified_name: String::new(),
            scope_id: String::new(),
            line_start: capture.line_start,
            line_end: capture.line_end,
            byte_start: capture.byte_start,
            byte_end: capture.byte_end,
            tree_sitter_node_type: capture.node_type.clone(),
            capture_name: capture.capture_name.clone(),
            summary,
            metadata: Value::Object(metadata),
        };
        self.add_node(node);
        syntax_id
    }

    pub(in crate::syntax_materializer) fn add_node(&mut self, node: GraphNodeRow) -> GraphNodeRow {
        self.nodes
            .entry(node.id.clone())
            .or_insert_with(|| node.clone());
        let added = self.nodes.get(&node.id).cloned().unwrap_or(node);
        self.register_resolvable(&added);
        added
    }

    pub(in crate::syntax_materializer) fn register_resolvable(&mut self, node: &GraphNodeRow) {
        if !matches!(
            node.table.as_str(),
            "Symbol"
                | "Module"
                | "Class"
                | "Function"
                | "Method"
                | "Variable"
                | "Constant"
                | "Dependency"
        ) {
            return;
        }
        for key in [
            node.label.as_str(),
            node.qualified_name.as_str(),
            imported_name(node).as_str(),
        ] {
            let normalized = symbol_key(key);
            if normalized.is_empty() {
                continue;
            }
            let ids = self.symbols_by_name.entry(normalized).or_default();
            if !ids.contains(&node.id) {
                ids.push(node.id.clone());
            }
        }
    }

    pub(in crate::syntax_materializer) fn into_rows(self) -> BuiltGraphRows {
        let mut nodes = self.nodes.into_values().collect::<Vec<_>>();
        nodes.sort_by(|left, right| {
            (left.table.as_str(), left.id.as_str()).cmp(&(right.table.as_str(), right.id.as_str()))
        });
        let mut edges = self.edges.into_values().collect::<Vec<_>>();
        edges.sort_by(|left, right| {
            (left.edge_type.as_str(), left.id.as_str())
                .cmp(&(right.edge_type.as_str(), right.id.as_str()))
        });
        BuiltGraphRows { nodes, edges }
    }
}
