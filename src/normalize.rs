use crate::protocol::CaptureMapping;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct SyntaxNode {
    pub(crate) node_type: String,
    pub(crate) text: String,
    pub(crate) line_start: Option<i64>,
    pub(crate) line_end: Option<i64>,
    pub(crate) byte_start: Option<i64>,
    pub(crate) byte_end: Option<i64>,
    pub(crate) capture_name: String,
    pub(crate) children: Vec<SyntaxNode>,
    pub(crate) fields: BTreeMap<String, Value>,
}

pub(crate) fn mapping_for_syntax_node<'a>(
    node: &SyntaxNode,
    mappings: &'a [CaptureMapping],
    ancestors: &[String],
) -> Option<&'a CaptureMapping> {
    let candidates = mappings
        .iter()
        .filter(|mapping| {
            mapping
                .parser_node_types
                .iter()
                .any(|node_type| node_type == &node.node_type)
        })
        .collect::<Vec<_>>();
    for mapping in &candidates {
        if !mapping.context_rule.is_empty()
            && context_rule_matches(&mapping.context_rule, node, ancestors)
        {
            return Some(mapping);
        }
    }
    candidates
        .into_iter()
        .find(|mapping| mapping.context_rule.is_empty())
}

fn context_rule_matches(rule: &str, node: &SyntaxNode, ancestors: &[String]) -> bool {
    let normalized = rule.trim().to_lowercase();
    if let Some(expected) = normalized.strip_prefix("inside ") {
        return ancestors
            .iter()
            .any(|ancestor| context_name_matches(ancestor, expected));
    }
    if let Some(expected_type) = normalized.strip_prefix("type is ") {
        return field_type_matches(node, "type", expected_type);
    }
    if normalized == "qualified declarator" {
        return field_descendant_has(node, "declarator", "qualified_identifier");
    }
    if normalized == "function declarator" {
        return field_type_matches(node, "declarator", "function_declarator")
            || field_descendant_has(node, "declarator", "function_declarator");
    }
    false
}

fn context_name_matches(node_type: &str, expected: &str) -> bool {
    match expected {
        "impl" => node_type == "impl_item",
        "class" => matches!(node_type, "class_specifier" | "struct_specifier"),
        value => node_type == value,
    }
}

fn field_type_matches(node: &SyntaxNode, field_name: &str, expected_type: &str) -> bool {
    node.fields
        .get("_field_types")
        .and_then(Value::as_object)
        .and_then(|values| values.get(field_name))
        .and_then(Value::as_str)
        .is_some_and(|value| value == expected_type)
}

fn field_descendant_has(node: &SyntaxNode, field_name: &str, expected_type: &str) -> bool {
    node.fields
        .get("_field_descendant_types")
        .and_then(Value::as_object)
        .and_then(|values| values.get(field_name))
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .any(|value| value == expected_type)
        })
}
