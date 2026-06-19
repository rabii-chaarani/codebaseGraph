use super::accumulator::StagingAccumulator;
use crate::error::NativeError;
use serde::Serialize;
use std::collections::HashMap;

pub(super) type ConnectorTypePair = (String, String);
pub(super) type ConnectorRowKey = (String, String, String);
pub(super) type ConnectorRowsByTypePair =
    HashMap<ConnectorTypePair, HashMap<ConnectorRowKey, ConnectorRow>>;
pub(super) type ConnectorBucketsByTable = HashMap<String, ConnectorRowsByTypePair>;

#[derive(Serialize)]
pub(super) struct ConnectorRow {
    pub(super) from_id: String,
    pub(super) to_id: String,
    pub(super) role: String,
}

pub(super) struct EdgeConnector {
    pub(super) id: String,
    pub(super) edge_type: String,
    pub(super) source_id: String,
    pub(super) target_id: String,
}

impl StagingAccumulator {
    pub(super) fn materialize_connectors(&mut self) -> Result<(), NativeError> {
        for edge in std::mem::take(&mut self.edge_connectors) {
            let source_type = self
                .node_types_by_id
                .get(&edge.source_id)
                .cloned()
                .ok_or_else(|| {
                    NativeError::InvalidInput(format!(
                        "edge {} references missing source node {}",
                        edge.id, edge.source_id
                    ))
                })?;
            let target_type = self
                .node_types_by_id
                .get(&edge.target_id)
                .cloned()
                .ok_or_else(|| {
                    NativeError::InvalidInput(format!(
                        "edge {} references missing target node {}",
                        edge.id, edge.target_id
                    ))
                })?;

            self.add_connector(
                format!("FROM_{}", edge.edge_type),
                source_type,
                edge.edge_type.clone(),
                edge.source_id,
                edge.id.clone(),
                "source".to_string(),
            );
            self.add_connector(
                format!("TO_{}", edge.edge_type),
                edge.edge_type,
                target_type,
                edge.id,
                edge.target_id,
                "target".to_string(),
            );
        }
        Ok(())
    }

    fn add_connector(
        &mut self,
        table: String,
        from_type: String,
        to_type: String,
        from_id: String,
        to_id: String,
        role: String,
    ) {
        let rows = self
            .connectors
            .entry(table)
            .or_default()
            .entry((from_type, to_type))
            .or_default();
        rows.entry((from_id.clone(), to_id.clone(), role.clone()))
            .or_insert(ConnectorRow {
                from_id,
                to_id,
                role,
            });
    }
}
