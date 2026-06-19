use crate::graph_rows::{BuiltGraphRows, GraphEdgeRow, GraphNodeRow};
use crate::normalize::SyntaxNode;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

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

use arena::*;
use builder::*;
use capture::*;
use ids::*;
use relations::*;

#[cfg(test)]
mod tests;
