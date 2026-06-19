use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

pub(super) fn tree_sitter_fields(node: Node<'_>, source_bytes: &[u8]) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    let mut field_types = Map::new();
    let mut field_descendant_types = Map::new();

    for field_name in [
        "name",
        "module",
        "path",
        "function",
        "type",
        "return_type",
        "declarator",
        "left",
        "right",
        "object",
        "attribute",
    ] {
        let Some(child) = node.child_by_field_name(field_name) else {
            continue;
        };
        field_types.insert(field_name.to_string(), json!(child.kind()));
        field_descendant_types.insert(
            field_name.to_string(),
            json!(node_types(child).into_iter().collect::<Vec<_>>()),
        );
        if field_name != "declarator" {
            if let Some(label) = node_text(child, source_bytes).map(|value| clean_label(&value)) {
                if !label.is_empty() {
                    fields.insert(field_name.to_string(), json!(label));
                }
            }
        }
    }

    augment_field_metadata(
        node,
        source_bytes,
        &mut fields,
        &mut field_types,
        &mut field_descendant_types,
    );
    if !field_types.is_empty() {
        fields.insert("_field_types".to_string(), Value::Object(field_types));
    }
    if !field_descendant_types.is_empty() {
        fields.insert(
            "_field_descendant_types".to_string(),
            Value::Object(field_descendant_types),
        );
    }
    fields
}

pub(super) fn augment_field_metadata(
    node: Node<'_>,
    source_bytes: &[u8],
    fields: &mut BTreeMap<String, Value>,
    field_types: &mut Map<String, Value>,
    field_descendant_types: &mut Map<String, Value>,
) {
    let node_type = node.kind();
    if !fields.contains_key("name") {
        let name = derived_name(node, source_bytes);
        if !name.is_empty() {
            fields.insert("name".to_string(), json!(name));
        }
    }
    if matches!(
        node_type,
        "use_declaration" | "import_declaration" | "preproc_include" | "use_statement"
    ) {
        let module = import_module(node, source_bytes);
        if !module.is_empty() {
            fields.insert("module".to_string(), json!(module));
        }
    }
    if matches!(node_type, "call_expression" | "subroutine_call")
        && !fields.contains_key("function")
    {
        let function = first_descendant_text(node, source_bytes, &["identifier", "name"]);
        if !function.is_empty() {
            fields.insert("function".to_string(), json!(function));
        }
    }
    if node_type == "type_declaration" {
        let type_child = first_descendant(node, &["type_spec"])
            .and_then(|type_spec| type_spec.child_by_field_name("type"));
        if let Some(type_child) = type_child {
            field_types.insert("type".to_string(), json!(type_child.kind()));
            field_descendant_types.insert(
                "type".to_string(),
                json!(node_types(type_child).into_iter().collect::<Vec<_>>()),
            );
        }
    }
    if matches!(node_type, "import_statement" | "import_from_statement") {
        add_python_import_fields(node, source_bytes, fields);
    }
    if node_type == "call" {
        if let Some(function) = node.child_by_field_name("function") {
            let label = syntax_label(function, source_bytes);
            if !label.is_empty() {
                fields.insert("func".to_string(), json!(label));
            }
        }
    }
    if node_type == "assignment" {
        if let Some(left) = node.child_by_field_name("left") {
            let label = syntax_label(left, source_bytes);
            if !label.is_empty() {
                fields.insert("target".to_string(), json!(label));
            }
        }
        if let Some(right) = node.child_by_field_name("right") {
            let label = syntax_label(right, source_bytes);
            if !label.is_empty() {
                fields.insert("value".to_string(), json!(label));
            }
        }
    }
    if node_type == "attribute" {
        if let Some(object) = node.child_by_field_name("object") {
            let label = syntax_label(object, source_bytes);
            if !label.is_empty() {
                fields.insert("value".to_string(), json!(label));
            }
        }
        if let Some(attribute) = node.child_by_field_name("attribute") {
            let label = syntax_label(attribute, source_bytes);
            if !label.is_empty() {
                fields.insert("attr".to_string(), json!(label));
            }
        }
    }
    if matches!(
        node_type,
        "string" | "integer" | "float" | "true" | "false" | "none"
    ) {
        let label = node_text(node, source_bytes)
            .map(|value| strip_literal_delimiters(&value))
            .unwrap_or_default();
        if !label.is_empty() {
            fields.insert("value".to_string(), json!(label));
        }
    }
    if matches!(node_type, "typed_parameter" | "default_parameter") {
        if let Some(type_node) = node.child_by_field_name("type") {
            let label = syntax_label(type_node, source_bytes);
            if !label.is_empty() {
                fields.insert("annotation".to_string(), json!(label));
            }
        } else if let Some(label) = parameter_annotation_from_text(node, source_bytes) {
            fields.insert("annotation".to_string(), json!(label));
        }
    }
}

pub(super) fn add_python_import_fields(
    node: Node<'_>,
    source_bytes: &[u8],
    fields: &mut BTreeMap<String, Value>,
) {
    let text = node_text(node, source_bytes)
        .map(|value| clean_label(&value))
        .unwrap_or_default();
    if node.kind() == "import_from_statement" {
        if let Some(rest) = text.strip_prefix("from ") {
            if let Some((module, names)) = rest.split_once(" import ") {
                let names = names
                    .split(',')
                    .filter_map(import_alias_json)
                    .collect::<Vec<_>>();
                if !module.trim().is_empty() {
                    fields.insert("module".to_string(), json!(module.trim()));
                }
                if !names.is_empty() {
                    fields.insert("names".to_string(), Value::Array(names));
                }
            }
        }
    } else if let Some(names) = text.strip_prefix("import ") {
        let names = names
            .split(',')
            .filter_map(import_alias_json)
            .collect::<Vec<_>>();
        if !names.is_empty() {
            fields.insert("names".to_string(), Value::Array(names));
        }
    }
}

pub(super) fn import_alias_json(raw_name: &str) -> Option<Value> {
    let name = raw_name
        .trim()
        .split_once(" as ")
        .map(|(left, _)| left)
        .unwrap_or_else(|| raw_name.trim())
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(json!({"type": "alias", "name": name}))
    }
}

pub(super) fn syntax_label(node: Node<'_>, source_bytes: &[u8]) -> String {
    if let Some(name) = node.child_by_field_name("name") {
        return node_text(name, source_bytes)
            .map(|value| clean_label(&value))
            .unwrap_or_default();
    }
    if node.kind() == "attribute" {
        let object = node
            .child_by_field_name("object")
            .map(|child| syntax_label(child, source_bytes))
            .unwrap_or_default();
        let attribute = node
            .child_by_field_name("attribute")
            .and_then(|child| node_text(child, source_bytes))
            .map(|value| clean_label(&value))
            .unwrap_or_default();
        if !object.is_empty() && !attribute.is_empty() {
            return format!("{object}.{attribute}");
        }
        if !attribute.is_empty() {
            return attribute;
        }
    }
    node_text(node, source_bytes)
        .map(|value| clean_label(&value))
        .unwrap_or_default()
}

pub(super) fn derived_name(node: Node<'_>, source_bytes: &[u8]) -> String {
    match node.kind() {
        "function_definition" | "function_declaration" | "field_declaration" | "declaration" => {
            declarator_name(node.child_by_field_name("declarator"), source_bytes)
        }
        "function_declarator" => declarator_name(Some(node), source_bytes),
        "type_declaration" => first_descendant(node, &["type_spec"])
            .and_then(|type_spec| type_spec.child_by_field_name("name"))
            .and_then(|name| node_text(name, source_bytes))
            .map(|value| clean_label(&value))
            .unwrap_or_default(),
        "module" | "subroutine" | "function" => {
            let statement_type = match node.kind() {
                "module" => "module_statement",
                "subroutine" => "subroutine_statement",
                _ => "function_statement",
            };
            first_descendant(node, &[statement_type])
                .and_then(|statement| {
                    statement
                        .child_by_field_name("name")
                        .or_else(|| first_descendant(statement, &["name"]))
                })
                .and_then(|name| node_text(name, source_bytes))
                .map(|value| clean_label(&value))
                .unwrap_or_default()
        }
        "package_clause" => {
            first_descendant_text(node, source_bytes, &["package_identifier", "identifier"])
        }
        _ => String::new(),
    }
}

pub(super) fn declarator_name(node: Option<Node<'_>>, source_bytes: &[u8]) -> String {
    let Some(node) = node else {
        return String::new();
    };
    for field_name in ["name", "declarator"] {
        let Some(child) = node.child_by_field_name(field_name) else {
            continue;
        };
        let label = if field_name == "declarator" {
            declarator_name(Some(child), source_bytes)
        } else {
            node_text(child, source_bytes)
                .map(|value| clean_label(&value))
                .unwrap_or_default()
        };
        if !label.is_empty() {
            return label;
        }
    }
    if matches!(
        node.kind(),
        "identifier"
            | "field_identifier"
            | "type_identifier"
            | "qualified_identifier"
            | "namespace_identifier"
    ) {
        return node_text(node, source_bytes)
            .map(|value| clean_label(&value))
            .unwrap_or_default();
    }
    for child in named_children(node) {
        let label = declarator_name(Some(child), source_bytes);
        if !label.is_empty() {
            return label;
        }
    }
    String::new()
}

pub(super) fn import_module(node: Node<'_>, source_bytes: &[u8]) -> String {
    match node.kind() {
        "preproc_include" => node
            .child_by_field_name("path")
            .and_then(|path| node_text(path, source_bytes))
            .map(|value| strip_import_delimiters(&value))
            .unwrap_or_default(),
        "use_declaration" => named_children(node)
            .into_iter()
            .next()
            .and_then(|child| node_text(child, source_bytes))
            .map(|value| clean_label(&value))
            .unwrap_or_default(),
        "import_declaration" => {
            for candidate_type in [
                "interpreted_string_literal_content",
                "raw_string_literal_content",
                "interpreted_string_literal",
                "raw_string_literal",
                "string_literal",
            ] {
                let label = first_descendant_text(node, source_bytes, &[candidate_type]);
                if !label.is_empty() {
                    return strip_import_delimiters(&label);
                }
            }
            String::new()
        }
        "use_statement" => first_descendant_text(node, source_bytes, &["module_name", "name"]),
        _ => String::new(),
    }
}

pub(super) fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    (0..node.named_child_count())
        .filter_map(|index| {
            let index = u32::try_from(index).ok()?;
            node.named_child(index)
        })
        .collect()
}

pub(super) fn node_types(node: Node<'_>) -> BTreeSet<String> {
    let mut types = BTreeSet::new();
    collect_node_types(node, &mut types);
    types
}

pub(super) fn collect_node_types(node: Node<'_>, types: &mut BTreeSet<String>) {
    let kind = node.kind();
    if !kind.is_empty() {
        types.insert(kind.to_string());
    }
    for child in named_children(node) {
        collect_node_types(child, types);
    }
}

pub(super) fn first_descendant<'a>(node: Node<'a>, node_types: &[&str]) -> Option<Node<'a>> {
    for child in named_children(node) {
        if node_types
            .iter()
            .any(|node_type| child.kind() == *node_type)
        {
            return Some(child);
        }
        if let Some(found) = first_descendant(child, node_types) {
            return Some(found);
        }
    }
    None
}

pub(super) fn first_descendant_text(
    node: Node<'_>,
    source_bytes: &[u8],
    node_types: &[&str],
) -> String {
    first_descendant(node, node_types)
        .and_then(|descendant| node_text(descendant, source_bytes))
        .map(|value| clean_label(&value))
        .unwrap_or_default()
}

pub(super) fn node_text(node: Node<'_>, source_bytes: &[u8]) -> Option<String> {
    node.utf8_text(source_bytes)
        .ok()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

pub(super) fn first_field_label(fields: &BTreeMap<String, Value>) -> String {
    for key in ["name", "id", "module", "path", "function"] {
        let Some(value) = fields.get(key).and_then(Value::as_str) else {
            continue;
        };
        if !value.is_empty() {
            return value.to_string();
        }
    }
    String::new()
}

pub(super) fn strip_import_delimiters(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('<')
        .trim_matches('>')
        .trim()
        .to_string()
}

pub(super) fn strip_literal_delimiters(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

pub(super) fn parameter_annotation_from_text(
    node: Node<'_>,
    source_bytes: &[u8],
) -> Option<String> {
    let text = node_text(node, source_bytes)?;
    let annotation = text
        .split_once(':')?
        .1
        .split_once('=')
        .map(|(left, _)| left)
        .unwrap_or_else(|| text.split_once(':').map(|(_, right)| right).unwrap_or(""))
        .trim();
    if annotation.is_empty() {
        None
    } else {
        Some(annotation.to_string())
    }
}

pub(super) fn clean_label(value: &str) -> String {
    value.trim().replace('\n', " ")
}
