use crate::normalize::{mapping_for_syntax_node, SyntaxNode};
use crate::protocol::{CaptureMapping, LanguageProfile};

pub(super) fn mark_captures(
    node: &mut SyntaxNode,
    profile: &LanguageProfile,
    ancestors: &[String],
) {
    let mut child_ancestors = ancestors.to_vec();
    child_ancestors.push(node.node_type.clone());
    for child in &mut node.children {
        mark_captures(child, profile, &child_ancestors);
    }
    if let Some(mapping) = mapping_for_syntax_node(node, &profile.capture_mappings, ancestors) {
        node.capture_name = mapping.capture_name.clone();
    }
}

pub(super) fn mapping_for_capture_name<'a>(
    mappings: &'a [CaptureMapping],
    capture_name: &str,
) -> Option<&'a CaptureMapping> {
    mappings
        .iter()
        .find(|mapping| mapping.capture_name == capture_name)
}
