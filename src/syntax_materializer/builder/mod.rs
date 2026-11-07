mod edges;
mod nodes;
mod semantic;
mod traversal;

use super::RelationAllowlist;
use crate::syntax_materializer::{GraphEdgeRow, GraphNodeRow};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
pub(in crate::syntax_materializer) struct Owner {
    pub(in crate::syntax_materializer) node_id: String,
    pub(in crate::syntax_materializer) table: String,
    pub(in crate::syntax_materializer) qualified_name: String,
    pub(in crate::syntax_materializer) scope_id: String,
}

pub(in crate::syntax_materializer) struct NativeBuilder<'a, 'b> {
    pub(in crate::syntax_materializer) guard:
        Option<&'a mut crate::extraction_control::ExtractionControl<'b>>,
    pub(in crate::syntax_materializer) guard_failure:
        Option<crate::extraction_control::ExtractionError>,
    pub(in crate::syntax_materializer) path: String,
    pub(in crate::syntax_materializer) language: String,
    pub(in crate::syntax_materializer) grammar_version: Option<String>,
    pub(in crate::syntax_materializer) source_root: String,
    pub(in crate::syntax_materializer) repository_label: String,
    pub(in crate::syntax_materializer) nodes: HashMap<String, GraphNodeRow>,
    pub(in crate::syntax_materializer) edges: HashMap<String, GraphEdgeRow>,
    pub(in crate::syntax_materializer) symbols_by_name: HashMap<String, Vec<String>>,
    pub(in crate::syntax_materializer) relation_allowlist: RelationAllowlist,
}

impl<'a, 'b> NativeBuilder<'a, 'b> {
    pub(in crate::syntax_materializer) fn new(
        meta: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let relation_allowlist = RelationAllowlist::from_meta(&meta)?;
        Ok(Self {
            guard: None,
            guard_failure: None,
            path: meta.get("path").cloned().unwrap_or_default(),
            language: meta.get("language").cloned().unwrap_or_default(),
            grammar_version: meta
                .get("grammar_version")
                .filter(|version| !version.trim().is_empty())
                .cloned(),
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
    pub(in crate::syntax_materializer) fn with_guard(
        meta: BTreeMap<String, String>,
        guard: &'a mut crate::extraction_control::ExtractionControl<'b>,
    ) -> Result<Self, String> {
        let mut builder = Self::new(meta)?;
        builder.guard = Some(guard);
        Ok(builder)
    }
    pub(in crate::syntax_materializer) fn guard_visit(&mut self) -> Result<(), String> {
        if let Some(failure) = &self.guard_failure {
            return Err(failure.message.to_string());
        }
        if let Some(guard) = &mut self.guard {
            if let Err(error) = guard.visit(0) {
                let message = error.message.to_string();
                self.guard_failure = Some(error);
                return Err(message);
            }
        }
        Ok(())
    }
    pub(in crate::syntax_materializer) fn capture(
        &mut self,
        node: crate::syntax_materializer::TreeNodeRef<'_>,
    ) -> Result<crate::syntax_materializer::Capture, String> {
        if let Some(guard) = &mut self.guard {
            match crate::syntax_materializer::tree_capture_guarded(node, guard) {
                Ok(capture) => Ok(capture),
                Err(error) => {
                    let message = error.message.to_string();
                    self.guard_failure = Some(error);
                    Err(message)
                }
            }
        } else {
            Ok(crate::syntax_materializer::tree_capture(node))
        }
    }
    pub(in crate::syntax_materializer) fn copy_text(
        &mut self,
        value: &str,
    ) -> Result<String, String> {
        if let Some(guard) = &mut self.guard {
            match crate::extraction_control::copy_text(value, guard) {
                Ok(value) => Ok(value),
                Err(error) => {
                    let message = error.message.to_string();
                    self.guard_failure = Some(error);
                    Err(message)
                }
            }
        } else {
            Ok(value.to_owned())
        }
    }
}
