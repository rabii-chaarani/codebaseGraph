use crate::error::NativeError;
use crate::normalize::{mapping_for_syntax_node, SyntaxNode};
use crate::protocol::{CaptureMapping, LanguageProfile, SourceSnapshot};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use tree_sitter::{Language, Node, Parser};

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
        return Ok(parse_markdown_source(source, profile));
    }

    let Some(language) = grammar_language(profile) else {
        return Err(NativeError::Unsupported(format!(
            "Unsupported native grammar for language {} ({})",
            profile.language, profile.grammar_package
        )));
    };
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| NativeError::Unsupported(error.to_string()))?;
    let source_bytes = source.as_bytes();
    let tree = parser.parse(source_bytes, None).ok_or_else(|| {
        NativeError::InvalidInput("tree-sitter parser returned no tree".to_string())
    })?;
    let mut root = normalize_tree_sitter_node(tree.root_node(), source_bytes);
    let mut diagnostics = Vec::new();
    if !profile.root_node_types.is_empty()
        && !profile
            .root_node_types
            .iter()
            .any(|node_type| node_type == &root.node_type)
    {
        diagnostics.push(format!(
            "Unexpected root node {} for {}",
            root.node_type, profile.language
        ));
    }
    mark_captures(&mut root, profile, &[]);
    Ok(ParseOutput { root, diagnostics })
}

fn grammar_language(profile: &LanguageProfile) -> Option<Language> {
    match (profile.grammar_package.as_str(), profile.language.as_str()) {
        ("tree_sitter_c", _) | (_, "c") => Some(tree_sitter_c::LANGUAGE.into()),
        ("tree_sitter_cpp", _) | (_, "cpp") => Some(tree_sitter_cpp::LANGUAGE.into()),
        ("tree_sitter_fortran", _) | (_, "fortran") => Some(tree_sitter_fortran::LANGUAGE.into()),
        ("tree_sitter_go", _) | (_, "go") => Some(tree_sitter_go::LANGUAGE.into()),
        ("tree_sitter_python", _) | (_, "python") => Some(tree_sitter_python::LANGUAGE.into()),
        ("tree_sitter_rust", _) | (_, "rust") => Some(tree_sitter_rust::LANGUAGE.into()),
        _ => None,
    }
}

fn normalize_tree_sitter_node(node: Node<'_>, source_bytes: &[u8]) -> SyntaxNode {
    let fields = tree_sitter_fields(node, source_bytes);
    let children = named_children(node)
        .into_iter()
        .map(|child| normalize_tree_sitter_node(child, source_bytes))
        .collect();
    let text = node_text(node, source_bytes)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| first_field_label(&fields));
    SyntaxNode {
        node_type: node.kind().to_string(),
        text,
        line_start: Some((node.start_position().row + 1) as i64),
        line_end: Some((node.end_position().row + 1) as i64),
        byte_start: Some(node.start_byte() as i64),
        byte_end: Some(node.end_byte() as i64),
        capture_name: String::new(),
        children,
        fields,
    }
}

fn tree_sitter_fields(node: Node<'_>, source_bytes: &[u8]) -> BTreeMap<String, Value> {
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

fn augment_field_metadata(
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
    if node_type == "subroutine_call" && !fields.contains_key("function") {
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

fn add_python_import_fields(
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

fn import_alias_json(raw_name: &str) -> Option<Value> {
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

fn syntax_label(node: Node<'_>, source_bytes: &[u8]) -> String {
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

fn derived_name(node: Node<'_>, source_bytes: &[u8]) -> String {
    match node.kind() {
        "function_definition" | "function_declaration" | "field_declaration" => {
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

fn declarator_name(node: Option<Node<'_>>, source_bytes: &[u8]) -> String {
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

fn import_module(node: Node<'_>, source_bytes: &[u8]) -> String {
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

fn mark_captures(node: &mut SyntaxNode, profile: &LanguageProfile, ancestors: &[String]) {
    let mut child_ancestors = ancestors.to_vec();
    child_ancestors.push(node.node_type.clone());
    for child in &mut node.children {
        mark_captures(child, profile, &child_ancestors);
    }
    if let Some(mapping) = mapping_for_syntax_node(node, &profile.capture_mappings, ancestors) {
        node.capture_name = mapping.capture_name.clone();
    }
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    (0..node.named_child_count())
        .filter_map(|index| node.named_child(index))
        .collect()
}

fn node_types(node: Node<'_>) -> BTreeSet<String> {
    let mut types = BTreeSet::new();
    collect_node_types(node, &mut types);
    types
}

fn collect_node_types(node: Node<'_>, types: &mut BTreeSet<String>) {
    let kind = node.kind();
    if !kind.is_empty() {
        types.insert(kind.to_string());
    }
    for child in named_children(node) {
        collect_node_types(child, types);
    }
}

fn first_descendant<'a>(node: Node<'a>, node_types: &[&str]) -> Option<Node<'a>> {
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

fn first_descendant_text(node: Node<'_>, source_bytes: &[u8], node_types: &[&str]) -> String {
    first_descendant(node, node_types)
        .and_then(|descendant| node_text(descendant, source_bytes))
        .map(|value| clean_label(&value))
        .unwrap_or_default()
}

fn node_text(node: Node<'_>, source_bytes: &[u8]) -> Option<String> {
    node.utf8_text(source_bytes)
        .ok()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn first_field_label(fields: &BTreeMap<String, Value>) -> String {
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

fn strip_import_delimiters(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('<')
        .trim_matches('>')
        .trim()
        .to_string()
}

fn strip_literal_delimiters(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parameter_annotation_from_text(node: Node<'_>, source_bytes: &[u8]) -> Option<String> {
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

fn clean_label(value: &str) -> String {
    value.trim().replace('\n', " ")
}

fn parse_markdown_source(source: &str, profile: &LanguageProfile) -> ParseOutput {
    let mut root = SyntaxNode {
        node_type: "Module".to_string(),
        text: String::new(),
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        capture_name: String::new(),
        children: Vec::new(),
        fields: BTreeMap::new(),
    };
    let total_lines = source.lines().count().max(1) as i64;
    push_markdown_capture(
        profile,
        "doc.source",
        markdown_node(
            "DocumentationSource",
            source,
            1,
            total_lines,
            0,
            source.len() as i64,
            [("name", ""), ("text", &summary(source))],
        ),
        &mut root,
    );
    for section in markdown_sections(source) {
        let heading = if section.heading.is_empty() {
            "section".to_string()
        } else {
            section.heading.clone()
        };
        push_markdown_capture(
            profile,
            "doc.chunk",
            markdown_node(
                "DocumentationChunk",
                &section.text,
                section.line_start,
                section.line_end,
                0,
                0,
                [
                    ("name", heading.as_str()),
                    ("heading", section.heading.as_str()),
                    ("text", summary(&section.text).as_str()),
                ],
            ),
            &mut root,
        );
    }
    ParseOutput {
        root,
        diagnostics: Vec::new(),
    }
}

fn push_markdown_capture(
    profile: &LanguageProfile,
    capture_name: &str,
    mut node: SyntaxNode,
    root: &mut SyntaxNode,
) {
    let Some(mapping) = mapping_for_capture_name(&profile.capture_mappings, capture_name) else {
        return;
    };
    node.capture_name = mapping.capture_name.clone();
    root.children.push(node);
}

fn mapping_for_capture_name<'a>(
    mappings: &'a [CaptureMapping],
    capture_name: &str,
) -> Option<&'a CaptureMapping> {
    mappings
        .iter()
        .find(|mapping| mapping.capture_name == capture_name)
}

fn markdown_node<const N: usize>(
    node_type: &str,
    text: &str,
    line_start: i64,
    line_end: i64,
    byte_start: i64,
    byte_end: i64,
    values: [(&str, &str); N],
) -> SyntaxNode {
    let mut fields = BTreeMap::new();
    for (key, value) in values {
        if !value.is_empty() {
            fields.insert(key.to_string(), json!(value));
        }
    }
    SyntaxNode {
        node_type: node_type.to_string(),
        text: text.to_string(),
        line_start: Some(line_start),
        line_end: Some(line_end),
        byte_start: if byte_end == 0 {
            None
        } else {
            Some(byte_start)
        },
        byte_end: if byte_end == 0 { None } else { Some(byte_end) },
        capture_name: String::new(),
        children: Vec::new(),
        fields,
    }
}

#[derive(Debug, Clone)]
struct MarkdownSection {
    heading: String,
    line_start: i64,
    line_end: i64,
    text: String,
}

fn markdown_sections(source: &str) -> Vec<MarkdownSection> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut headings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let level = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if (1..=6).contains(&level) && trimmed.chars().nth(level).is_some_and(char::is_whitespace) {
            headings.push((index + 1, trimmed[level..].trim().to_string()));
        }
    }
    if headings.is_empty() {
        let text = source.trim();
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![MarkdownSection {
                heading: String::new(),
                line_start: 1,
                line_end: lines.len().max(1) as i64,
                text: text.to_string(),
            }]
        };
    }
    headings
        .iter()
        .enumerate()
        .filter_map(|(index, (line_start, heading))| {
            let line_end = headings
                .get(index + 1)
                .map(|(next_line, _)| next_line.saturating_sub(1))
                .unwrap_or(lines.len());
            let text = lines[line_start.saturating_sub(1)..line_end]
                .join("\n")
                .trim()
                .to_string();
            if text.is_empty() {
                None
            } else {
                Some(MarkdownSection {
                    heading: heading.clone(),
                    line_start: *line_start as i64,
                    line_end: line_end as i64,
                    text,
                })
            }
        })
        .collect()
}

fn summary(text: &str) -> String {
    text.trim().chars().take(2000).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CaptureMapping;

    fn profile(language: &str) -> LanguageProfile {
        let (grammar_package, suffixes, root_node_types, mappings) = match language {
            "rust" => (
                "tree_sitter_rust",
                vec![".rs"],
                vec!["source_file"],
                vec![
                    mapping("definition.struct", &["struct_item"], "Class", ""),
                    mapping(
                        "definition.method",
                        &["function_item"],
                        "Method",
                        "inside impl",
                    ),
                    mapping("definition.function", &["function_item"], "Function", ""),
                    mapping(
                        "reference.use",
                        &["use_declaration"],
                        "ImportDeclaration",
                        "",
                    ),
                    mapping("reference.call", &["call_expression"], "CallExpression", ""),
                    mapping(
                        "reference.macro",
                        &["macro_invocation"],
                        "CallExpression",
                        "",
                    ),
                ],
            ),
            "go" => (
                "tree_sitter_go",
                vec![".go"],
                vec!["source_file"],
                vec![
                    mapping("definition.package", &["package_clause"], "Module", ""),
                    mapping(
                        "definition.function",
                        &["function_declaration"],
                        "Function",
                        "",
                    ),
                    mapping("definition.method", &["method_declaration"], "Method", ""),
                    mapping(
                        "reference.import",
                        &["import_declaration"],
                        "ImportDeclaration",
                        "",
                    ),
                    mapping("reference.call", &["call_expression"], "CallExpression", ""),
                ],
            ),
            other => panic!("unsupported test profile: {other}"),
        };
        LanguageProfile {
            language: language.to_string(),
            suffixes: suffixes.into_iter().map(str::to_string).collect(),
            grammar_package: grammar_package.to_string(),
            root_node_types: root_node_types.into_iter().map(str::to_string).collect(),
            capture_mappings: mappings,
        }
    }

    fn mapping(
        capture_name: &str,
        parser_node_types: &[&str],
        target_node_type: &str,
        context_rule: &str,
    ) -> CaptureMapping {
        CaptureMapping {
            capture_name: capture_name.to_string(),
            parser_node_types: parser_node_types
                .iter()
                .map(|item| item.to_string())
                .collect(),
            target_node_type: target_node_type.to_string(),
            relation_types: Vec::new(),
            context_rule: context_rule.to_string(),
            construct: String::new(),
        }
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
