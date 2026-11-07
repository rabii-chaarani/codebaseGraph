use super::captures::mark_captures;
use super::fields::{
    first_field_label, named_children_with_indices, node_text, tree_sitter_fields,
};
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
        ("tree_sitter_css", _) | (_, "css") => Some(tree_sitter_css::LANGUAGE.into()),
        ("tree_sitter_fortran", _) | (_, "fortran") => Some(tree_sitter_fortran::LANGUAGE.into()),
        ("tree_sitter_go", _) | (_, "go") => Some(tree_sitter_go::LANGUAGE.into()),
        ("tree_sitter_html", _) | (_, "html") => Some(tree_sitter_html::LANGUAGE.into()),
        ("tree_sitter_javascript", _) | (_, "javascript") => {
            Some(tree_sitter_javascript::LANGUAGE.into())
        }
        ("tree_sitter_python", _) | (_, "python") => Some(tree_sitter_python::LANGUAGE.into()),
        ("tree_sitter_rust", _) | (_, "rust") => Some(tree_sitter_rust::LANGUAGE.into()),
        (_, "tsx") => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        ("tree_sitter_typescript", _) | (_, "typescript") => {
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        }
        ("tree_sitter_wat", _) | (_, "webassembly") => Some(super::wat::LANGUAGE.into()),
        _ => None,
    }
}

pub(super) fn grammar_node_types(profile: &LanguageProfile) -> Option<&'static str> {
    match (profile.grammar_package.as_str(), profile.language.as_str()) {
        ("tree_sitter_c", _) | (_, "c") => Some(tree_sitter_c::NODE_TYPES),
        ("tree_sitter_cpp", _) | (_, "cpp") => Some(tree_sitter_cpp::NODE_TYPES),
        ("tree_sitter_css", _) | (_, "css") => Some(tree_sitter_css::NODE_TYPES),
        ("tree_sitter_fortran", _) | (_, "fortran") => Some(tree_sitter_fortran::NODE_TYPES),
        ("tree_sitter_go", _) | (_, "go") => Some(tree_sitter_go::NODE_TYPES),
        ("tree_sitter_html", _) | (_, "html") => Some(tree_sitter_html::NODE_TYPES),
        ("tree_sitter_javascript", _) | (_, "javascript") => {
            Some(tree_sitter_javascript::NODE_TYPES)
        }
        ("tree_sitter_python", _) | (_, "python") => Some(tree_sitter_python::NODE_TYPES),
        ("tree_sitter_rust", _) | (_, "rust") => Some(tree_sitter_rust::NODE_TYPES),
        (_, "tsx") => Some(tree_sitter_typescript::TSX_NODE_TYPES),
        ("tree_sitter_typescript", _) | (_, "typescript") => {
            Some(tree_sitter_typescript::TYPESCRIPT_NODE_TYPES)
        }
        ("tree_sitter_wat", _) | (_, "webassembly") => Some(super::wat::NODE_TYPES),
        _ => None,
    }
}

fn normalize_tree_sitter_node(node: Node<'_>, source_bytes: &[u8]) -> SyntaxNode {
    let fields = tree_sitter_fields(node, source_bytes);
    let children = named_children_with_indices(node)
        .into_iter()
        .map(|(index, child)| {
            let mut normalized = normalize_tree_sitter_node(child, source_bytes);
            normalized.field_name = u32::try_from(index)
                .ok()
                .and_then(|index| node.field_name_for_child(index))
                .map(str::to_string);
            normalized
        })
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
        field_name: None,
        children,
        fields,
    }
}

/// Parse immutable Rust bytes with native cooperative progress reporting.
/// Flat normalization and materialization are separate phases.
pub(super) fn parse_guarded_tree(
    source: &[u8],
    profile: &LanguageProfile,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<tree_sitter::Tree, crate::extraction_control::ExtractionError> {
    use crate::extraction_control::ExtractionError;
    use tree_sitter::ParseOptions;
    control.check()?;
    let language = grammar_language(profile)
        .ok_or_else(|| ExtractionError::new("reconstruction_unavailable", "unsupported grammar"))?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ExtractionError::new("reconstruction_unavailable", "unsupported grammar"))?;
    let mut failure = None;
    let mut progress = |_: &tree_sitter::ParseState| match control.check() {
        Ok(()) => false,
        Err(error) => {
            failure = Some(error);
            true
        }
    };
    let mut read = |offset: usize, _: tree_sitter::Point| source.get(offset..).unwrap_or_default();
    let tree = parser.parse_with_options(
        &mut read,
        None,
        Some(ParseOptions::new().progress_callback(&mut progress)),
    );
    if let Some(error) = failure {
        return Err(error);
    }
    control.check()?;
    let tree = tree.ok_or_else(|| {
        ExtractionError::new("reconstruction_unavailable", "parser returned no tree")
    })?;
    if tree.root_node().has_error() {
        return Err(ExtractionError::new(
            "reconstruction_unavailable",
            "source contains parse errors",
        ));
    }
    Ok(tree)
}

/// Convert a completed Rust tree into flat records without recursive ownership.
pub(super) fn normalize_rust_flat(
    tree: &tree_sitter::Tree,
    source: &[u8],
    profile: &LanguageProfile,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<crate::normalize::FlatSyntax, crate::extraction_control::ExtractionError> {
    use crate::{
        extraction_control::ExtractionError,
        normalize::{mapping_for_syntax_node, FlatSyntax},
    };
    enum Job<'a> {
        Enter(Node<'a>, Option<usize>, Option<&'static str>, u64),
        Exit,
    }
    fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), ExtractionError> {
        v.try_reserve(n)
            .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))
    }
    fn copy(
        value: &str,
        control: &mut crate::extraction_control::ExtractionControl<'_>,
    ) -> Result<String, ExtractionError> {
        let mut s = String::new();
        s.try_reserve(value.len())
            .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
        for ch in value.chars() {
            control.visit(0)?;
            s.push(ch);
        }
        Ok(s)
    }
    let mut result = FlatSyntax {
        nodes: Vec::new(),
        parents: Vec::new(),
        children: Vec::new(),
    };
    let mut ancestors: Vec<String> = Vec::new();
    let mut stack = Vec::new();
    reserve(&mut stack, 1)?;
    stack.push(Job::Enter(tree.root_node(), None, None, 1));
    while let Some(job) = stack.pop() {
        match job {
            Job::Exit => {
                control.check()?;
                ancestors.pop();
            }
            Job::Enter(node, parent, field, depth) => {
                control.visit(depth)?;
                let fields = super::fields::rust_fields_guarded(node, source, depth, control)?;
                let text = match node.utf8_text(source).ok().filter(|s| !s.is_empty()) {
                    Some(value) => copy(value, control)?,
                    None => {
                        let label = first_field_label(&fields);
                        copy(&label, control)?
                    }
                };
                let mut syntax = SyntaxNode {
                    node_type: copy(node.kind(), control)?,
                    text,
                    line_start: Some((node.start_position().row + 1) as i64),
                    line_end: Some((node.end_position().row + 1) as i64),
                    byte_start: Some(node.start_byte() as i64),
                    byte_end: Some(node.end_byte() as i64),
                    capture_name: String::new(),
                    field_name: match field {
                        Some(value) => Some(copy(value, control)?),
                        None => None,
                    },
                    children: Vec::new(),
                    fields,
                };
                if let Some(mapping) =
                    mapping_for_syntax_node(&syntax, &profile.capture_mappings, &ancestors)
                {
                    syntax.capture_name = copy(&mapping.capture_name, control)?;
                }
                reserve(&mut result.nodes, 1)?;
                reserve(&mut result.parents, 1)?;
                reserve(&mut result.children, 1)?;
                let id = result.nodes.len();
                result.nodes.push(syntax);
                result.parents.push(parent);
                result.children.push(Vec::new());
                if let Some(parent) = parent {
                    reserve(&mut result.children[parent], 1)?;
                    result.children[parent].push(id);
                }
                reserve(&mut ancestors, 1)?;
                ancestors.push(copy(node.kind(), control)?);
                reserve(&mut stack, node.child_count().saturating_add(1))?;
                stack.push(Job::Exit);
                for index in (0..node.child_count()).rev() {
                    control.check()?;
                    if let Some(child) = node.child(index).filter(|c| c.is_named()) {
                        let field = u32::try_from(index)
                            .ok()
                            .and_then(|i| node.field_name_for_child(i));
                        stack.push(Job::Enter(
                            child,
                            Some(id),
                            field,
                            depth.checked_add(1).ok_or_else(|| {
                                ExtractionError::new("resource_limit", "depth overflow")
                            })?,
                        ));
                    }
                }
            }
        }
    }
    Ok(result)
}
