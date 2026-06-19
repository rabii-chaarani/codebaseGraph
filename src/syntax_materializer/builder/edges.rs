use super::{NativeBuilder, Owner};
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::syntax_materializer::{empty_metadata, graph_id};
use serde_json::{Map, Value};

impl NativeBuilder {
    pub(in crate::syntax_materializer) fn connect_owner(
        &mut self,
        owner: &Owner,
        semantic: &GraphNodeRow,
    ) -> Result<(), String> {
        self.edge(
            "Contains",
            &owner.node_id,
            &semantic.id,
            &format!("contains_{}", semantic.table.to_lowercase()),
            empty_metadata(),
        )?;
        if !owner.scope_id.is_empty() {
            self.edge(
                "Contains",
                &owner.scope_id,
                &semantic.id,
                &format!("scope_contains_{}", semantic.table.to_lowercase()),
                empty_metadata(),
            )?;
        }
        Ok(())
    }

    pub(in crate::syntax_materializer) fn derived_from(
        &mut self,
        semantic_id: &str,
        syntax_id: &str,
    ) -> Result<(), String> {
        if self.nodes.contains_key(syntax_id) {
            self.edge(
                "DerivedFrom",
                semantic_id,
                syntax_id,
                "parser_capture",
                empty_metadata(),
            )?;
        }
        Ok(())
    }

    pub(in crate::syntax_materializer) fn edge_if_allowed(
        &mut self,
        edge_type: &str,
        source_id: &str,
        target_id: &str,
        kind: &str,
        metadata: Map<String, Value>,
    ) -> Result<(), String> {
        let Some(source) = self.nodes.get(source_id) else {
            return Ok(());
        };
        let Some(target) = self.nodes.get(target_id) else {
            return Ok(());
        };
        if self
            .relation_allowlist
            .allows(edge_type, &source.table, &target.table)
        {
            self.edge(edge_type, source_id, target_id, kind, metadata)?;
        }
        Ok(())
    }

    pub(in crate::syntax_materializer) fn edge(
        &mut self,
        edge_type: &str,
        source_id: &str,
        target_id: &str,
        kind: &str,
        metadata: Map<String, Value>,
    ) -> Result<GraphEdgeRow, String> {
        let source_table = self.nodes.get(source_id).map(|node| node.table.clone());
        let target_table = self.nodes.get(target_id).map(|node| node.table.clone());
        if let (Some(source_table), Some(target_table)) = (&source_table, &target_table) {
            if !self
                .relation_allowlist
                .allows(edge_type, source_table, target_table)
            {
                return Err(format!(
                    "relation {edge_type} does not allow {source_table} -> {target_table}"
                ));
            }
        }
        let canonical_key = format!("{}|{}|{}|{}", edge_type, source_id, target_id, kind);
        let mut edge_metadata = empty_metadata();
        edge_metadata.insert(
            "canonical_key".to_string(),
            Value::String(canonical_key.clone()),
        );
        for (key, value) in metadata {
            edge_metadata.insert(key, value);
        }
        let edge = GraphEdgeRow {
            id: graph_id("edge", &canonical_key),
            edge_type: edge_type.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            kind: kind.to_string(),
            confidence: 1.0,
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            metadata: Value::Object(edge_metadata),
        };
        self.edges
            .entry(edge.id.clone())
            .or_insert_with(|| edge.clone());
        Ok(self.edges.get(&edge.id).cloned().unwrap_or(edge))
    }
}
