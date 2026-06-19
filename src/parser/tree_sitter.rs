use super::captures::mark_captures;
use super::fields::{first_field_label, named_children, node_text, tree_sitter_fields};
use super::ParseOutput;
use crate::error::NativeError;
use crate::normalize::SyntaxNode;
use crate::protocol::LanguageProfile;
use tree_sitter::{Language, Node, Parser};

pub(super) fn parse_tree_sitter_source(
    source: &str,
    profile: &LanguageProfile,
) -> Result<ParseOutput, NativeError> {
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
