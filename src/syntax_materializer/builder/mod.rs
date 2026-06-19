mod edges;
mod nodes;
mod semantic;
mod traversal;

use super::RelationAllowlist;
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use std::collections::{BTreeMap, HashMap};

pub(in crate::syntax_materializer) struct Owner {
    pub(in crate::syntax_materializer) node_id: String,
    pub(in crate::syntax_materializer) table: String,
    pub(in crate::syntax_materializer) qualified_name: String,
    pub(in crate::syntax_materializer) scope_id: String,
}

pub(in crate::syntax_materializer) struct NativeBuilder {
    pub(in crate::syntax_materializer) path: String,
    pub(in crate::syntax_materializer) language: String,
    pub(in crate::syntax_materializer) source_root: String,
    pub(in crate::syntax_materializer) repository_label: String,
    pub(in crate::syntax_materializer) nodes: HashMap<String, GraphNodeRow>,
    pub(in crate::syntax_materializer) edges: HashMap<String, GraphEdgeRow>,
    pub(in crate::syntax_materializer) symbols_by_name: HashMap<String, Vec<String>>,
    pub(in crate::syntax_materializer) relation_allowlist: RelationAllowlist,
}

impl NativeBuilder {
    pub(in crate::syntax_materializer) fn new(
        meta: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let relation_allowlist = RelationAllowlist::from_meta(&meta)?;
        Ok(Self {
            path: meta.get("path").cloned().unwrap_or_default(),
            language: meta.get("language").cloned().unwrap_or_default(),
            source_root: meta.get("source_root").cloned().unwrap_or_default(),
            repository_label: meta
                .get("repository_label")
                .cloned()
                .unwrap_or_else(|| "repository".to_string()),
            nodes: HashMap::new(),
            edges: HashMap::new(),
            symbols_by_name: HashMap::new(),
            relation_allowlist,
        })
    }
}
