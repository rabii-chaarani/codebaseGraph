use crate::normalize::SyntaxNode;
use std::collections::BTreeMap;

pub(crate) use rows::{BuiltGraphRows, GraphEdgeRow, GraphNodeRow};

pub(crate) fn build_syntax_tree_graph_rows(
    meta: BTreeMap<String, String>,
    root: &SyntaxNode,
) -> Result<BuiltGraphRows, String> {
    let mut builder = NativeBuilder::new(meta)?;
    let nodes = NativeSyntaxArena::new(root);
    builder.build_tree(&nodes, nodes.root_id)?;
    Ok(builder.into_rows())
}

mod arena;
mod builder;
mod capture;
mod ids;
mod relations;
mod rows;

use arena::*;
use builder::*;
use capture::*;
use ids::*;
use relations::*;

#[cfg(test)]
mod tests;

/// Materialize a flat Rust syntax tree under the invocation's shared limits.
pub(crate) fn build_flat_syntax_rows(
    meta: BTreeMap<String, String>,
    flat: &crate::normalize::FlatSyntax,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<BuiltGraphRows, crate::extraction_control::ExtractionError> {
    use crate::extraction_control::ExtractionError;
    let arena = NativeSyntaxArena::from_flat(flat, control)?;
    let mut builder = NativeBuilder::with_guard(meta, control).map_err(|_| {
        ExtractionError::new(
            "reconstruction_unavailable",
            "materializer configuration invalid",
        )
    })?;
    let result = builder.build_tree(&arena, arena.root_id);
    if let Some(error) = builder.guard_failure.take() {
        return Err(error);
    }
    result.map_err(|_| {
        ExtractionError::new("reconstruction_unavailable", "materialization failed")
    })?;
    builder.guard_visit().map_err(|_| {
        builder.guard_failure.take().unwrap_or_else(|| {
            ExtractionError::new("architecture_internal_error", "materializer guard failed")
        })
    })?;
    builder.into_rows_guarded()
}
