use super::NativeBuilder;
use crate::syntax_materializer::{
    empty_metadata, graph_id, imported_name, kind_for, qualified_name, stable_optional_i64,
    symbol_key, BuiltGraphRows, Capture, GraphNodeRow,
};
use serde_json::{Map, Value};

impl NativeBuilder<'_, '_> {
    pub(in crate::syntax_materializer) fn support_node(
        &mut self,
        table: &str,
        stable_key: &str,
        label: &str,
        path: &str,
    ) -> GraphNodeRow {
        if let Some(guard) = &mut self.guard {
            return match guarded_support(table, stable_key, label, path, guard) {
                Ok(node) => self.add_node(node),
                Err(error) => {
                    self.guard_failure = Some(error);
                    failed_node()
                }
            };
        }
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
            grammar_version: None,
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
        if let Some(guard) = &mut self.guard {
            return match guarded_semantic(
                SemanticArgs {
                    table,
                    capture,
                    label,
                    owner_id,
                    owner_name: owner_qualified_name,
                    path: &self.path,
                    language: &self.language,
                    metadata,
                },
                guard,
            ) {
                Ok(node) => self.add_node(node),
                Err(error) => {
                    self.guard_failure = Some(error);
                    failed_node()
                }
            };
        }
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
            grammar_version: None,
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
        if let Some(guard) = &mut self.guard {
            return match guarded_symbol(&self.path, &self.language, label, guard) {
                Ok(node) => self.add_node(node),
                Err(error) => {
                    self.guard_failure = Some(error);
                    failed_node()
                }
            };
        }
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
            grammar_version: None,
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
        if let Some(guard) = &mut self.guard {
            return match guarded_scope(&self.path, owner, guard) {
                Ok(node) => self.add_node(node),
                Err(error) => {
                    self.guard_failure = Some(error);
                    failed_node()
                }
            };
        }
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
            grammar_version: None,
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
        if let Some(guard) = &mut self.guard {
            return match guarded_syntax(
                capture,
                &self.path,
                &self.language,
                self.grammar_version.as_deref(),
                guard,
            ) {
                Ok(node) => {
                    let id = match crate::extraction_control::copy_text(&node.id, guard) {
                        Ok(id) => id,
                        Err(error) => {
                            self.guard_failure = Some(error);
                            return String::new();
                        }
                    };
                    if !self.nodes.contains_key(&id) {
                        self.add_node(node);
                    }
                    id
                }
                Err(error) => {
                    self.guard_failure = Some(error);
                    String::new()
                }
            };
        }
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
            grammar_version: self.grammar_version.clone(),
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
        if self.guard_failure.is_some() {
            return node;
        }
        if !self.nodes.contains_key(&node.id) {
            if let Some(guard) = &mut self.guard {
                if let Err(error) = guard.vertex() {
                    self.guard_failure = Some(error);
                    return node;
                }
                if self.nodes.try_reserve(1).is_err() {
                    self.guard_failure = Some(crate::extraction_control::ExtractionError::new(
                        "resource_limit",
                        "allocation failed",
                    ));
                    return node;
                }
            }
        }
        if let Some(guard) = &mut self.guard {
            let source = self.nodes.get(&node.id).unwrap_or(&node);
            let added = match copy_node(source, guard) {
                Ok(value) => value,
                Err(error) => {
                    self.guard_failure = Some(error);
                    return node;
                }
            };
            if !self.nodes.contains_key(&node.id) {
                let key = match crate::extraction_control::copy_text(&node.id, guard) {
                    Ok(value) => value,
                    Err(error) => {
                        self.guard_failure = Some(error);
                        return node;
                    }
                };
                self.nodes.insert(key, node);
            }
            self.register_resolvable(&added);
            return added;
        }
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
        if self.guard.is_none() {
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
            return;
        }
        let imported = node
            .metadata
            .get("imported_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for key in [node.label.as_str(), node.qualified_name.as_str(), imported] {
            let guard = self.guard.as_mut().expect("guarded branch");
            let normalized = match crate::extraction_control::normalized_symbol(key, guard) {
                Ok(value) => value,
                Err(error) => {
                    self.guard_failure = Some(error);
                    return;
                }
            };
            if normalized.is_empty() {
                continue;
            }
            if self.symbols_by_name.try_reserve(1).is_err() {
                self.guard_failure = Some(crate::extraction_control::ExtractionError::new(
                    "resource_limit",
                    "allocation failed",
                ));
                return;
            }
            let ids = self.symbols_by_name.entry(normalized).or_default();
            let mut found = false;
            for id in ids.iter() {
                if let Err(error) = guard.visit(0) {
                    self.guard_failure = Some(error);
                    return;
                }
                if id == &node.id {
                    found = true;
                    break;
                }
            }
            if !found {
                if ids.try_reserve(1).is_err() {
                    self.guard_failure = Some(crate::extraction_control::ExtractionError::new(
                        "resource_limit",
                        "allocation failed",
                    ));
                    return;
                }
                let id = match crate::extraction_control::copy_text(&node.id, guard) {
                    Ok(value) => value,
                    Err(error) => {
                        self.guard_failure = Some(error);
                        return;
                    }
                };
                ids.push(id);
            }
        }
    }

    pub(in crate::syntax_materializer) fn into_rows_guarded(
        mut self,
    ) -> Result<BuiltGraphRows, crate::extraction_control::ExtractionError> {
        use crate::extraction_control::{sort_by, ExtractionError};
        if let Some(error) = self.guard_failure.take() {
            return Err(error);
        }
        let guard = self.guard.take().ok_or_else(|| {
            ExtractionError::new("architecture_internal_error", "missing materializer guard")
        })?;
        let mut nodes = Vec::new();
        nodes
            .try_reserve(self.nodes.len())
            .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
        for value in self.nodes.into_values() {
            guard.visit(0)?;
            nodes.push(value);
        }
        sort_by(&mut nodes, guard, |a, b| {
            (a.table.as_str(), a.id.as_str()).cmp(&(b.table.as_str(), b.id.as_str()))
        })?;
        let mut edges = Vec::new();
        edges
            .try_reserve(self.edges.len())
            .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
        for value in self.edges.into_values() {
            guard.visit(0)?;
            edges.push(value);
        }
        sort_by(&mut edges, guard, |a, b| {
            (a.edge_type.as_str(), a.id.as_str()).cmp(&(b.edge_type.as_str(), b.id.as_str()))
        })?;
        Ok(BuiltGraphRows { nodes, edges })
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

// Node metadata here is created by the materializer itself: a fixed-depth
// object of scalar values and the grammar's field-name array. Candidate text
// occurs in scalar strings, which are copied cooperatively.
pub(super) fn copy_value(
    value: &Value,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<Value, crate::extraction_control::ExtractionError> {
    use crate::extraction_control::{copy_text, ExtractionError};
    guard.visit(0)?;
    Ok(match value {
        Value::Null => Value::Null,
        Value::Bool(v) => Value::Bool(*v),
        Value::Number(v) => Value::Number(v.clone()),
        Value::String(v) => Value::String(copy_text(v, guard)?),
        Value::Array(values) => {
            let mut result = Vec::new();
            result
                .try_reserve(values.len())
                .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
            for v in values {
                guard.visit(0)?;
                result.push(copy_value(v, guard)?);
            }
            Value::Array(result)
        }
        Value::Object(values) => {
            let mut result = Map::new();
            for (key, v) in values {
                guard.visit(0)?;
                result.insert(copy_text(key, guard)?, copy_value(v, guard)?);
            }
            Value::Object(result)
        }
    })
}
pub(super) fn copy_node(
    node: &GraphNodeRow,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<GraphNodeRow, crate::extraction_control::ExtractionError> {
    use crate::extraction_control::copy_text;
    Ok(GraphNodeRow {
        id: copy_text(&node.id, guard)?,
        table: copy_text(&node.table, guard)?,
        label: copy_text(&node.label, guard)?,
        kind: copy_text(&node.kind, guard)?,
        language: copy_text(&node.language, guard)?,
        grammar_version: node
            .grammar_version
            .as_deref()
            .map(|v| copy_text(v, guard))
            .transpose()?,
        path: copy_text(&node.path, guard)?,
        qualified_name: copy_text(&node.qualified_name, guard)?,
        scope_id: copy_text(&node.scope_id, guard)?,
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        tree_sitter_node_type: copy_text(&node.tree_sitter_node_type, guard)?,
        capture_name: copy_text(&node.capture_name, guard)?,
        summary: copy_text(&node.summary, guard)?,
        metadata: copy_value(&node.metadata, guard)?,
    })
}

fn guarded_join(
    parts: &[&str],
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<String, crate::extraction_control::ExtractionError> {
    use crate::extraction_control::ExtractionError;
    let mut size = parts.len().saturating_sub(1);
    for part in parts {
        control.visit(0)?;
        size = size
            .checked_add(part.len())
            .ok_or_else(|| ExtractionError::new("resource_limit", "key length overflow"))?;
    }
    let mut result = String::new();
    result
        .try_reserve(size)
        .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
    for (index, part) in parts.iter().enumerate() {
        if index != 0 {
            result.push('|');
        }
        for ch in part.chars() {
            control.visit(0)?;
            result.push(ch);
        }
    }
    Ok(result)
}
fn guarded_join_dot(
    left: &str,
    right: &str,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<String, crate::extraction_control::ExtractionError> {
    use crate::extraction_control::{copy_text, ExtractionError};
    let mut equal = left.len() == right.len();
    if equal {
        for (a, b) in left.bytes().zip(right.bytes()) {
            control.visit(0)?;
            if a != b {
                equal = false;
                break;
            }
        }
    }
    if left.is_empty() || equal {
        return copy_text(right, control);
    }
    if right.is_empty() {
        return copy_text(left, control);
    }
    let size = left
        .len()
        .checked_add(right.len())
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| ExtractionError::new("resource_limit", "name length overflow"))?;
    let mut out = String::new();
    out.try_reserve(size)
        .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
    for ch in left.chars() {
        control.visit(0)?;
        out.push(ch);
    }
    out.push('.');
    for ch in right.chars() {
        control.visit(0)?;
        out.push(ch);
    }
    Ok(out)
}
fn guarded_support(
    table: &str,
    key: &str,
    label: &str,
    path: &str,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<GraphNodeRow, crate::extraction_control::ExtractionError> {
    use crate::{extraction_control::copy_text, syntax_materializer::graph_id_guarded};
    let mut metadata = empty_metadata();
    metadata.insert(
        "canonical_key".into(),
        Value::String(copy_text(key, guard)?),
    );
    Ok(GraphNodeRow {
        id: graph_id_guarded(table, key, guard)?,
        table: copy_text(table, guard)?,
        label: copy_text(label, guard)?,
        kind: table.to_lowercase(),
        language: String::new(),
        grammar_version: None,
        path: copy_text(path, guard)?,
        qualified_name: String::new(),
        scope_id: String::new(),
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        tree_sitter_node_type: String::new(),
        capture_name: String::new(),
        summary: copy_text(label, guard)?,
        metadata: Value::Object(metadata),
    })
}
struct SemanticArgs<'a> {
    table: &'a str,
    capture: &'a Capture,
    label: &'a str,
    owner_id: &'a str,
    owner_name: &'a str,
    path: &'a str,
    language: &'a str,
    metadata: Option<Map<String, Value>>,
}
fn guarded_semantic(
    args: SemanticArgs<'_>,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<GraphNodeRow, crate::extraction_control::ExtractionError> {
    let SemanticArgs {
        table,
        capture,
        label,
        owner_id,
        owner_name,
        path,
        language,
        metadata,
    } = args;
    use crate::{extraction_control::copy_text, syntax_materializer::graph_id_guarded};
    let qualified_name = guarded_join_dot(owner_name, label, guard)?;
    let line = stable_optional_i64(capture.line_start);
    let byte = stable_optional_i64(capture.byte_start);
    let key = guarded_join(
        &[
            path,
            table,
            &qualified_name,
            &capture.node_type,
            &line,
            &byte,
            label,
        ],
        guard,
    )?;
    let id = graph_id_guarded(table, &key, guard)?;
    let mut node_metadata = empty_metadata();
    node_metadata.insert("canonical_key".into(), Value::String(key));
    if let Some(extra) = metadata {
        for (key, value) in extra {
            guard.visit(0)?;
            node_metadata.insert(key, value);
        }
    }
    let summary = if matches!(table, "DocumentationSource" | "DocumentationChunk")
        && !capture.text.trim().is_empty()
    {
        copy_text(capture.text.trim(), guard)?
    } else {
        copy_text(label, guard)?
    };
    let kind = match table {
        "Method" => "method",
        "Function" => "function",
        "Class" => "class",
        _ => capture.node_type.as_str(),
    };
    Ok(GraphNodeRow {
        id,
        table: copy_text(table, guard)?,
        label: copy_text(label, guard)?,
        kind: copy_text(kind, guard)?,
        language: copy_text(language, guard)?,
        grammar_version: None,
        path: copy_text(path, guard)?,
        qualified_name,
        scope_id: copy_text(owner_id, guard)?,
        line_start: capture.line_start,
        line_end: capture.line_end,
        byte_start: capture.byte_start,
        byte_end: capture.byte_end,
        tree_sitter_node_type: copy_text(&capture.node_type, guard)?,
        capture_name: copy_text(&capture.capture_name, guard)?,
        summary,
        metadata: Value::Object(node_metadata),
    })
}

fn guarded_symbol(
    path: &str,
    language: &str,
    label: &str,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<GraphNodeRow, crate::extraction_control::ExtractionError> {
    use crate::{extraction_control::copy_text, syntax_materializer::graph_id_guarded};
    let mut first = None;
    let mut last = 0usize;
    for (index, ch) in label.char_indices() {
        guard.visit(0)?;
        if !ch.is_whitespace() {
            if first.is_none() {
                first = Some(index);
            }
            last = index + ch.len_utf8();
        }
    }
    let label = first.map_or("", |index| &label[index..last]);
    let key = guarded_join(&[path, "Symbol", label], guard)?;
    let id = graph_id_guarded("Symbol", &key, guard)?;
    let mut metadata = empty_metadata();
    metadata.insert("canonical_key".into(), Value::String(key));
    metadata.insert(
        "resolution".into(),
        Value::String("name_placeholder".into()),
    );
    Ok(GraphNodeRow {
        id,
        table: "Symbol".into(),
        label: copy_text(label, guard)?,
        kind: "symbol_reference".into(),
        language: copy_text(language, guard)?,
        grammar_version: None,
        path: copy_text(path, guard)?,
        qualified_name: copy_text(label, guard)?,
        scope_id: String::new(),
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        tree_sitter_node_type: String::new(),
        capture_name: String::new(),
        summary: copy_text(label, guard)?,
        metadata: Value::Object(metadata),
    })
}
fn guarded_scope(
    path: &str,
    owner: &GraphNodeRow,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<GraphNodeRow, crate::extraction_control::ExtractionError> {
    use crate::{extraction_control::copy_text, syntax_materializer::graph_id_guarded};
    let key = guarded_join(&[path, &owner.id, "scope"], guard)?;
    let id = graph_id_guarded("Scope", &key, guard)?;
    let mut metadata = empty_metadata();
    metadata.insert("canonical_key".into(), Value::String(key));
    // The original formats concatenate directly; build the input-scaled pieces
    // with fallible guarded copying and only append fixed literals.
    let mut label = copy_text(&owner.label, guard)?;
    label.try_reserve(6).map_err(|_| {
        crate::extraction_control::ExtractionError::new("resource_limit", "allocation failed")
    })?;
    label.push_str(" scope");
    let base = if owner.qualified_name.is_empty() {
        &owner.label
    } else {
        &owner.qualified_name
    };
    let mut qualified_name = copy_text(base, guard)?;
    qualified_name.try_reserve(8).map_err(|_| {
        crate::extraction_control::ExtractionError::new("resource_limit", "allocation failed")
    })?;
    qualified_name.push_str(".<scope>");
    let mut summary = String::new();
    summary
        .try_reserve(10usize.checked_add(owner.label.len()).ok_or_else(|| {
            crate::extraction_control::ExtractionError::new("resource_limit", "summary overflow")
        })?)
        .map_err(|_| {
            crate::extraction_control::ExtractionError::new("resource_limit", "allocation failed")
        })?;
    summary.push_str("Scope for ");
    for ch in owner.label.chars() {
        guard.visit(0)?;
        summary.push(ch);
    }
    Ok(GraphNodeRow {
        id,
        table: "Scope".into(),
        label,
        kind: format!("{}_scope", owner.table.to_lowercase()),
        language: copy_text(&owner.language, guard)?,
        grammar_version: None,
        path: copy_text(&owner.path, guard)?,
        qualified_name,
        scope_id: copy_text(&owner.id, guard)?,
        line_start: owner.line_start,
        line_end: owner.line_end,
        byte_start: owner.byte_start,
        byte_end: owner.byte_end,
        tree_sitter_node_type: String::new(),
        capture_name: String::new(),
        summary,
        metadata: Value::Object(metadata),
    })
}
fn failed_node() -> GraphNodeRow {
    GraphNodeRow {
        id: String::new(),
        table: String::new(),
        label: String::new(),
        kind: String::new(),
        language: String::new(),
        grammar_version: None,
        path: String::new(),
        qualified_name: String::new(),
        scope_id: String::new(),
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        tree_sitter_node_type: String::new(),
        capture_name: String::new(),
        summary: String::new(),
        metadata: Value::Null,
    }
}

fn guarded_syntax(
    capture: &Capture,
    path: &str,
    language: &str,
    grammar: Option<&str>,
    guard: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<GraphNodeRow, crate::extraction_control::ExtractionError> {
    use crate::{
        extraction_control::{copy_text, ExtractionError},
        syntax_materializer::graph_id_guarded,
    };
    let line = stable_optional_i64(capture.line_start);
    let byte = stable_optional_i64(capture.byte_start);
    let key = guarded_join(
        &[path, &capture.node_type, &line, &byte, &capture.label],
        guard,
    )?;
    let id = graph_id_guarded("SyntaxCapture", &key, guard)?;
    let mut fields = Vec::new();
    fields
        .try_reserve(capture.fields.len())
        .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
    for field in &capture.fields {
        guard.visit(0)?;
        fields.push(Value::String(copy_text(field, guard)?));
    }
    let mut metadata = empty_metadata();
    metadata.insert("canonical_key".into(), Value::String(key));
    metadata.insert("fields".into(), Value::Array(fields));
    let mut summary = String::new();
    for ch in capture.text.chars().take(160) {
        guard.visit(0)?;
        summary
            .try_reserve(ch.len_utf8())
            .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
        summary.push(ch);
    }
    let label = if capture.capture_name.is_empty() {
        &capture.node_type
    } else {
        &capture.capture_name
    };
    Ok(GraphNodeRow {
        id,
        table: "SyntaxCapture".into(),
        label: copy_text(label, guard)?,
        kind: copy_text(&capture.node_type, guard)?,
        language: copy_text(language, guard)?,
        grammar_version: grammar.map(|s| copy_text(s, guard)).transpose()?,
        path: copy_text(path, guard)?,
        qualified_name: String::new(),
        scope_id: String::new(),
        line_start: capture.line_start,
        line_end: capture.line_end,
        byte_start: capture.byte_start,
        byte_end: capture.byte_end,
        tree_sitter_node_type: copy_text(&capture.node_type, guard)?,
        capture_name: copy_text(&capture.capture_name, guard)?,
        summary,
        metadata: Value::Object(metadata),
    })
}

#[cfg(test)]
mod guarded_key_tests {
    use super::*;
    use crate::extraction_control::{ExtractionControl, ExtractionError};
    #[test]
    fn candidate_scaled_key_copy_is_interruptible() {
        let value = "x".repeat(10_000);
        let mut calls = 0;
        let mut check = || {
            calls += 1;
            if calls > 100 {
                Err(ExtractionError::new("cancelled", "key cancelled"))
            } else {
                Ok(())
            }
        };
        let mut guard = ExtractionControl::new(&mut check, 100, 100_000, 100, 100);
        assert_eq!(
            guarded_join(&["prefix", &value], &mut guard)
                .unwrap_err()
                .code,
            "cancelled"
        );
    }
}
