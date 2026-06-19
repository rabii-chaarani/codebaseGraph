use super::connectors::{ConnectorBucketsByTable, EdgeConnector};
use super::files::{copy_path, stage_file_stem, write_csv_rows, write_json_rows};
use super::merge::{merge_edge_row, merge_node_row};
use super::ordering::{
    sorted_connector_rows, sorted_connector_type_buckets, sorted_keys, sorted_row_values,
};
use super::result::StagingResult;
use super::rows::{edge_fields, node_fields, EdgeRowsById, NodeRowsById};
use crate::error::NativeError;
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::partition_builder::GraphPartition;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub(crate) struct StagingAccumulator {
    staging_dir: PathBuf,
    nodes: HashMap<String, NodeRowsById>,
    edges: HashMap<String, EdgeRowsById>,
    pub(super) node_types_by_id: HashMap<String, String>,
    pub(super) edge_connectors: Vec<EdgeConnector>,
    pub(super) connectors: ConnectorBucketsByTable,
}

impl StagingAccumulator {
    pub(crate) fn new(staging_dir: &str) -> Self {
        Self {
            staging_dir: PathBuf::from(staging_dir),
            nodes: HashMap::new(),
            edges: HashMap::new(),
            node_types_by_id: HashMap::new(),
            edge_connectors: Vec::new(),
            connectors: HashMap::new(),
        }
    }

    pub(crate) fn add_partition(&mut self, partition: &GraphPartition) {
        for node in &partition.nodes {
            self.add_node(
                node,
                (node.table == "File").then_some(partition.entry.content_hash.as_str()),
            );
        }
        for edge in &partition.edges {
            self.add_edge(edge);
        }
    }

    pub(crate) fn finish(mut self) -> Result<StagingResult, NativeError> {
        self.materialize_connectors()?;
        self.write()
    }

    fn add_node(&mut self, node: &GraphNodeRow, content_hash: Option<&str>) {
        self.node_types_by_id
            .entry(node.id.clone())
            .or_insert_with(|| node.table.clone());
        merge_node_row(
            self.nodes.entry(node.table.clone()).or_default(),
            node.id.clone(),
            node_fields(node, content_hash),
        );
    }

    fn add_edge(&mut self, edge: &GraphEdgeRow) {
        merge_edge_row(
            self.edges.entry(edge.edge_type.clone()).or_default(),
            edge.id.clone(),
            edge_fields(edge),
        );
        self.edge_connectors.push(EdgeConnector {
            id: edge.id.clone(),
            edge_type: edge.edge_type.clone(),
            source_id: edge.source_id.clone(),
            target_id: edge.target_id.clone(),
        });
    }

    fn write(&self) -> Result<StagingResult, NativeError> {
        fs::create_dir_all(&self.staging_dir)?;

        let mut copy_statements = Vec::new();
        let mut node_rows = 0;
        let mut edge_rows = 0;
        let mut connector_rows = 0;

        for table in sorted_keys(&self.nodes) {
            let Some(rows) = self.nodes.get(table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            let path = self
                .staging_dir
                .join(format!("{}.json", stage_file_stem(table)));
            write_json_rows(&path, sorted_row_values(rows))?;
            copy_statements.push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
            node_rows += rows.len();
        }

        for table in sorted_keys(&self.edges) {
            let Some(rows) = self.edges.get(table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            let path = self
                .staging_dir
                .join(format!("{}.json", stage_file_stem(table)));
            write_json_rows(&path, sorted_row_values(rows))?;
            copy_statements.push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
            edge_rows += rows.len();
        }

        for relation in sorted_keys(&self.edges) {
            for connector_table in [format!("FROM_{relation}"), format!("TO_{relation}")] {
                let Some(buckets) = self.connectors.get(&connector_table) else {
                    continue;
                };
                for ((from_type, to_type), rows) in sorted_connector_type_buckets(buckets) {
                    if rows.is_empty() {
                        continue;
                    }
                    let path = self.staging_dir.join(format!(
                        "{}__{}__{}.csv",
                        stage_file_stem(&connector_table),
                        stage_file_stem(from_type),
                        stage_file_stem(to_type)
                    ));
                    write_csv_rows(&path, sorted_connector_rows(rows))?;
                    copy_statements.push(format!(
                        "COPY `{}` FROM \"{}\" (header=true, from=\"{}\", to=\"{}\");",
                        connector_table,
                        copy_path(&path),
                        from_type,
                        to_type
                    ));
                    connector_rows += rows.len();
                }
            }
        }

        Ok(StagingResult {
            copy_calls: copy_statements.len(),
            copy_statements,
            node_rows,
            edge_rows,
            connector_rows,
        })
    }
}
