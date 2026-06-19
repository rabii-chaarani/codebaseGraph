use super::{NativeSyntaxArena, Owner, TreeNodeRef};
use serde_json::{Map, Value};

#[derive(Clone)]
pub(super) struct Capture {
    pub(super) capture_name: String,
    pub(super) node_type: String,
    pub(super) label: String,
    pub(super) text: String,
    pub(super) line_start: Option<i64>,
    pub(super) line_end: Option<i64>,
    pub(super) byte_start: Option<i64>,
    pub(super) byte_end: Option<i64>,
    pub(super) fields: Vec<String>,
}
pub(super) fn tree_capture(node: TreeNodeRef<'_>) -> Capture {
    Capture {
        capture_name: node.capture_name().to_string(),
        node_type: node.node_type().to_string(),
        label: tree_label(node),
        text: node.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: node.field_keys(),
    }
}

pub(super) fn tree_label(node: TreeNodeRef<'_>) -> String {
    for key in ["name", "id", "arg", "attr", "module", "path", "function"] {
        if let Some(label) = node.field_label(key) {
            if !label.is_empty() {
                return label;
            }
        }
    }
    if let Some(label) = node.field_label("value") {
        if !label.is_empty() {
            return label;
        }
    }
    let text = node.text().trim();
    if text.is_empty() {
        node.node_type().to_string()
    } else {
        text.to_string()
    }
}

pub(super) fn should_derive_root_module(language: &str, root_node_type: &str) -> bool {
    !(matches!(root_node_type, "source_file")
        || (language == "python" && root_node_type == "module")
        || (language == "markdown" && root_node_type == "Module"))
}

pub(super) fn json_value_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Object(object) => {
            for key in ["id", "name", "arg", "attr", "value"] {
                if let Some(value) = object.get(key) {
                    let label = json_value_label(value)?;
                    if !label.is_empty() {
                        return Some(label);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub(super) fn semantic_child_ids(
    nodes: &NativeSyntaxArena<'_>,
    parent: TreeNodeRef<'_>,
    language: &str,
) -> Vec<usize> {
    let mut child_ids = Vec::new();
    for child_id in parent.children {
        let Some(child) = nodes.get_node(*child_id) else {
            continue;
        };
        if should_inline_child(child, language) {
            child_ids.extend(semantic_child_ids(nodes, child, language));
        } else if should_traverse_child(parent, child, language) {
            child_ids.push(*child_id);
        }
    }
    child_ids
}

pub(super) fn should_inline_child(child: TreeNodeRef<'_>, language: &str) -> bool {
    (language == "python" && child.node_type() == "block")
        || (language == "fortran" && child.node_type() == "variable_declaration")
}

pub(super) fn should_traverse_child(
    parent: TreeNodeRef<'_>,
    child: TreeNodeRef<'_>,
    language: &str,
) -> bool {
    if language == "python" {
        if parent.node_type() == "attribute" {
            return json_field_label(parent, "value")
                .is_some_and(|label| label == tree_label(child));
        }
        if matches!(child.node_type(), "parameters" | "decorator") {
            return false;
        }
        if matches!(
            parent.node_type(),
            "class_definition" | "function_definition"
        ) && matches!(child.node_type(), "identifier" | "type" | "type_identifier")
        {
            return false;
        }
        if matches!(child.node_type(), "identifier" | "type_identifier")
            && !matches!(parent.node_type(), "assignment" | "call" | "attribute")
        {
            return false;
        }
    }
    if child.node_type() == "block" {
        return true;
    }
    if matches!(
        parent.node_type(),
        "import_statement" | "import_from_statement" | "import_declaration" | "use_declaration"
    ) && matches!(
        child.node_type(),
        "identifier"
            | "dotted_name"
            | "aliased_import"
            | "import_list"
            | "string"
            | "interpreted_string_literal"
            | "raw_string_literal"
            | "string_literal"
    ) {
        if language == "python" && child.node_type() == "dotted_name" {
            return true;
        }
        return false;
    }
    true
}

pub(super) fn table_for_node_type(node_type: &str, owner: &Owner) -> Option<String> {
    Some(
        match node_type {
            "import_statement"
            | "import_from_statement"
            | "import_declaration"
            | "use_declaration"
            | "preproc_include"
            | "use_statement" => "ImportDeclaration",
            "export_statement" | "export_clause" | "export_declaration" => "ExportDeclaration",
            "class_definition"
            | "class_declaration"
            | "struct_item"
            | "interface_declaration"
            | "struct_specifier"
            | "union_specifier"
            | "enum_specifier"
            | "class_specifier"
            | "type_declaration" => "Class",
            "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "function_item"
            | "subroutine"
            | "function" => {
                if matches!(owner.table.as_str(), "Class" | "Component") {
                    "Method"
                } else {
                    "Function"
                }
            }
            "arg" => "Parameter",
            "return_type" | "returns" => "ReturnType",
            "type" | "type_identifier" | "qualified_type" | "type_annotation" | "annotation" => {
                "TypeAnnotation"
            }
            "assignment" | "assignment_expression" => "Assignment",
            "call"
            | "call_expression"
            | "invocation_expression"
            | "call_statement"
            | "subroutine_call" => "CallExpression",
            "identifier" | "field_identifier" | "attribute" => "Reference",
            "string" | "integer" | "float" | "true" | "false" | "null" | "none"
            | "intrinsic_type" => "Literal",
            "if_statement" | "for_statement" | "while_statement" | "match_statement"
            | "switch_statement" => "ControlFlowBlock",
            "try_statement" | "except_clause" | "catch_clause" | "raise_statement"
            | "throw_statement" => "ExceptionFlow",
            _ => return None,
        }
        .to_string(),
    )
}

pub(super) fn json_field_label(node: TreeNodeRef<'_>, field: &str) -> Option<String> {
    node.field_label(field)
}

pub(super) fn import_label(node: TreeNodeRef<'_>) -> Option<String> {
    let module = json_field_label(node, "module").unwrap_or_default();
    let names = node
        .field_value("names")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| json_value_label(&item))
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if !module.is_empty() && !names.is_empty() {
        Some(
            names
                .into_iter()
                .map(|name| format!("{module}.{name}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    } else if !module.is_empty() {
        Some(module)
    } else if !names.is_empty() {
        Some(names.join(", "))
    } else {
        None
    }
}

pub(super) fn assignment_target_label(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
) -> Option<String> {
    json_field_label(node, "target").or_else(|| {
        node.children
            .iter()
            .filter_map(|child_id| nodes.get_node(*child_id))
            .find(|child| !matches!(child.node_type(), "call" | "call_expression"))
            .map(tree_label)
            .filter(|label| !label.is_empty())
    })
}

pub(super) fn assignment_target_table(label: &str, owner: &Owner, node_type: &str) -> &'static str {
    if label.chars().any(|character| character.is_alphabetic())
        && label
            .chars()
            .filter(|character| character.is_alphabetic())
            .all(|character| character.is_uppercase())
    {
        return "Constant";
    }
    if owner.table == "Class" {
        return "ClassAttribute";
    }
    if label.contains('.') {
        return "InstanceAttribute";
    }
    if node_type == "AnnAssign" && owner.table == "Class" {
        return "ClassAttribute";
    }
    "Variable"
}

pub(super) fn call_value_child(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
) -> Option<usize> {
    node.children.iter().copied().find(|child_id| {
        nodes
            .get_node(*child_id)
            .is_some_and(|child| matches!(child.node_type(), "call" | "call_expression"))
    })
}

pub(super) fn parameter_child_ids(
    nodes: &NativeSyntaxArena<'_>,
    function_node: TreeNodeRef<'_>,
) -> Vec<usize> {
    function_node
        .children
        .iter()
        .filter_map(|child_id| nodes.get_node(*child_id))
        .filter(|child| child.node_type() == "parameters")
        .flat_map(|parameters| parameters.children.iter().copied())
        .filter(|child_id| {
            nodes.get_node(*child_id).is_some_and(|child| {
                matches!(
                    child.node_type(),
                    "identifier" | "typed_parameter" | "default_parameter" | "parameter"
                )
            })
        })
        .collect()
}

pub(super) fn parameter_label(node: TreeNodeRef<'_>) -> String {
    let label = tree_label(node);
    label
        .split_once(':')
        .map(|(left, _)| left)
        .unwrap_or(label.as_str())
        .split_once('=')
        .map(|(left, _)| left)
        .unwrap_or_else(|| {
            label
                .split_once(':')
                .map(|(left, _)| left)
                .unwrap_or(label.as_str())
        })
        .trim()
        .trim_start_matches('*')
        .to_string()
}

pub(super) fn parameter_capture(node: TreeNodeRef<'_>, language: &str) -> Capture {
    if language != "python" {
        let mut capture = tree_capture(node);
        capture.capture_name = "parameter".to_string();
        capture.label = parameter_label(node);
        return capture;
    }
    Capture {
        capture_name: String::new(),
        node_type: "arg".to_string(),
        label: parameter_label(node),
        text: node.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: vec!["arg".to_string()],
    }
}

pub(super) fn parameter_annotation_label(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
) -> Option<String> {
    json_field_label(node, "annotation").or_else(|| {
        node.children
            .iter()
            .filter_map(|child_id| nodes.get_node(*child_id))
            .find(|child| {
                matches!(
                    child.node_type(),
                    "type" | "type_identifier" | "qualified_type" | "annotation"
                )
            })
            .map(tree_label)
            .filter(|label| !label.is_empty())
    })
}

pub(super) fn parameter_annotation_capture(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
    language: &str,
) -> Option<Capture> {
    if language == "python" {
        if let Some(type_child) = first_child_with_type(nodes, node, &["type", "type_identifier"]) {
            return Some(tree_capture(type_child));
        }
        return json_field_label(node, "annotation").map(|label| Capture {
            capture_name: String::new(),
            node_type: "type".to_string(),
            label,
            text: node.text().to_string(),
            line_start: node.line_start(),
            line_end: node.line_end(),
            byte_start: node.byte_start(),
            byte_end: node.byte_end(),
            fields: vec!["id".to_string()],
        });
    }
    parameter_annotation_label(nodes, node).map(|label| Capture {
        capture_name: String::new(),
        node_type: "type_annotation".to_string(),
        label,
        text: node.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: Vec::new(),
    })
}

pub(super) fn return_type_capture(
    nodes: &NativeSyntaxArena<'_>,
    function_node: TreeNodeRef<'_>,
    language: &str,
) -> Option<Capture> {
    if language == "python" {
        return first_child_with_type(nodes, function_node, &["type", "type_identifier"])
            .map(tree_capture);
    }
    json_field_label(function_node, "return_type")
        .or_else(|| json_field_label(function_node, "returns"))
        .map(|label| Capture {
            capture_name: "return_type".to_string(),
            node_type: "return_type".to_string(),
            label,
            text: function_node.text().to_string(),
            line_start: function_node.line_start(),
            line_end: function_node.line_end(),
            byte_start: function_node.byte_start(),
            byte_end: function_node.byte_end(),
            fields: Vec::new(),
        })
}

pub(super) fn first_child_with_type<'a>(
    nodes: &'a NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
    node_types: &[&str],
) -> Option<TreeNodeRef<'a>> {
    node.children.iter().find_map(|child_id| {
        let child = nodes.get_node(*child_id)?;
        if node_types
            .iter()
            .any(|node_type| child.node_type() == *node_type)
        {
            Some(child)
        } else {
            None
        }
    })
}

pub(super) fn fortran_literal_capture(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
) -> Option<Capture> {
    if node.node_type() != "intrinsic_type" {
        return None;
    }
    let parent = node
        .parent_id
        .and_then(|parent_id| nodes.get_node(parent_id))?;
    if parent.node_type() != "variable_declaration" {
        return None;
    }
    Some(Capture {
        capture_name: String::new(),
        node_type: "integer".to_string(),
        label: parent.text().to_string(),
        text: parent.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: Vec::new(),
    })
}

pub(super) fn parser_like_metadata_capture(
    node: TreeNodeRef<'_>,
    field_name: &str,
) -> Option<Capture> {
    let metadata = node.field_value(field_name)?;
    let object = metadata.as_object()?;
    let node_type_value = object.get("type")?;
    let node_type = metadata_value_label(node_type_value)?;
    if node_type.is_empty() {
        return None;
    }
    let label = metadata_object_label(object).unwrap_or_else(|| node_type.clone());
    Some(Capture {
        capture_name: String::new(),
        node_type,
        text: label.clone(),
        label,
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        fields: object.keys().cloned().collect(),
    })
}

pub(super) fn metadata_object_label(object: &Map<String, Value>) -> Option<String> {
    for key in ["name", "id", "arg", "attr", "module"] {
        if let Some(label) = object.get(key).and_then(metadata_value_label) {
            if !label.is_empty() {
                return Some(label);
            }
        }
    }
    if let Some(label) = object.get("value").and_then(metadata_value_label) {
        if !label.is_empty() {
            return Some(label);
        }
    }
    for key in [
        "name",
        "module",
        "path",
        "function",
        "type",
        "return_type",
        "declarator",
    ] {
        if let Some(label) = object.get(key).and_then(metadata_value_label) {
            if !label.is_empty() {
                return Some(label);
            }
        }
    }
    None
}

pub(super) fn metadata_value_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Array(items) => Some(format!(
            "[{}]",
            items
                .iter()
                .filter_map(metadata_value_label)
                .map(|item| format!("'{item}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Value::Object(object) => metadata_object_label(object),
        Value::Null => None,
    }
}

pub(super) fn table_for_capture(capture: &str, owner: &Owner) -> Option<String> {
    let normalized = capture.trim_start_matches('@');
    Some(
        match normalized {
            "definition.class"
            | "definition.struct"
            | "definition.interface"
            | "definition.enum"
            | "definition.union" => "Class",
            "definition.module" | "definition.namespace" | "definition.package" => "Module",
            "definition.component" | "component" => "Component",
            "definition.method" => "Method",
            "definition.function" => {
                if matches!(owner.table.as_str(), "Class" | "Component") {
                    "Method"
                } else {
                    "Function"
                }
            }
            "definition.parameter" | "parameter" => "Parameter",
            "type.return" | "return_type" => "ReturnType",
            "type" | "type.annotation" | "reference.type" => "TypeAnnotation",
            "definition.type_alias" => "TypeAlias",
            "definition.macro" => "Symbol",
            "definition.constant" => "Constant",
            "definition.variable" => "Variable",
            "decorator" | "definition.decorator" => "Decorator",
            "reference.import" | "reference.include" | "reference.require" | "reference.use"
            | "import" => "ImportDeclaration",
            "export" | "definition.export" => "ExportDeclaration",
            "reference.call" | "call" => "CallExpression",
            "entrypoint.api" => "APIEndpoint",
            "endpoint" => "APIEndpoint",
            "route" => "Route",
            "doc.source" => "DocumentationSource",
            "literal" | "string" | "number" => "Literal",
            "control_flow" => "ControlFlowBlock",
            "exception" | "raises" | "handles" => "ExceptionFlow",
            value if value.starts_with("query.") => "Query",
            value if value.starts_with("secret.") => "SecretRef",
            value if value.starts_with("doc") => "DocumentationChunk",
            value if value.starts_with("reference") => "Reference",
            _ => return None,
        }
        .to_string(),
    )
}
