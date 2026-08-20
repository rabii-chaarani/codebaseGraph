use super::files::{copy_path, stage_file_stem, write_csv_field};
use super::merge::{merge_edge_value, merge_node_value};
use super::result::StagingResult;
use super::rows::{edge_fields, node_fields, EdgeStagedRow, NodeStagedRow};
use super::spill::{
    encode_bounded, encode_output_bounded, RecordFileReader, RecordFileWriter, SortedSpool,
    SpillMetrics, SpillRecord,
};
use crate::error::{MemoryBudgetExceeded, NativeError};
use crate::partition_builder::GraphPartition;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
const DEFAULT_SPILL_CHUNK_BYTES: usize = 32 * 1024 * 1024;
static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct StagingAccumulator {
    staging_dir: PathBuf,
    run_workspace: RunWorkspace,
    chunk_limit: usize,
    output_chunk_limit: usize,
    raw_records: Option<SortedSpool<RawRecord>>,
    next_source_order: u64,
    pending_error: Option<NativeError>,
    relation_constraints: RelationConstraints,
    metrics: SpillMetrics,
}

#[derive(Debug, Default)]
struct RelationConstraints {
    pairs_by_relation: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
}

impl StagingAccumulator {
    #[cfg(test)]
    pub(crate) fn new(staging_dir: &str) -> Self {
        match Self::with_chunk_limit(staging_dir, DEFAULT_SPILL_CHUNK_BYTES) {
            Ok(accumulator) => accumulator,
            Err(error) => Self::failed(staging_dir, DEFAULT_SPILL_CHUNK_BYTES, error),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_chunk_limit(
        staging_dir: &str,
        chunk_limit: usize,
    ) -> Result<Self, NativeError> {
        Self::with_limits(staging_dir, chunk_limit, chunk_limit)
    }

    pub(crate) fn with_limits(
        staging_dir: &str,
        chunk_limit: usize,
        output_chunk_limit: usize,
    ) -> Result<Self, NativeError> {
        let staging_dir = PathBuf::from(staging_dir);
        let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let run_root = staging_dir.join(format!(".runs-{}-{sequence}", std::process::id()));
        let metrics = SpillMetrics::default();
        let raw_records = SortedSpool::new(&run_root, "staged", chunk_limit, metrics.clone())?;
        Ok(Self {
            staging_dir,
            run_workspace: RunWorkspace::new(run_root),
            chunk_limit,
            output_chunk_limit,
            raw_records: Some(raw_records),
            next_source_order: 0,
            pending_error: None,
            relation_constraints: RelationConstraints::from_declared_schema(),
            metrics,
        })
    }

    #[cfg(test)]
    fn failed(staging_dir: &str, chunk_limit: usize, error: NativeError) -> Self {
        let staging_dir = PathBuf::from(staging_dir);
        let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            run_workspace: RunWorkspace::new(
                staging_dir.join(format!(".runs-{}-{sequence}", std::process::id())),
            ),
            staging_dir,
            chunk_limit,
            output_chunk_limit: chunk_limit,
            raw_records: None,
            next_source_order: 0,
            pending_error: Some(error),
            relation_constraints: RelationConstraints::from_declared_schema(),
            metrics: SpillMetrics::default(),
        }
    }

    pub(crate) fn add_partition(&mut self, partition: &GraphPartition) {
        self.add_partition_filtered(partition, &BTreeSet::new(), &BTreeSet::new());
    }

    pub(crate) fn add_partition_filtered(
        &mut self,
        partition: &GraphPartition,
        retained_nodes: &BTreeSet<String>,
        retained_edges: &BTreeSet<String>,
    ) {
        if self.pending_error.is_some() {
            return;
        }
        for node in &partition.nodes {
            let source_order = match self.take_source_order() {
                Ok(order) => order,
                Err(error) => {
                    self.pending_error = Some(error);
                    return;
                }
            };
            if let Err(error) = self.push_raw(&RawRecord::NodeType {
                id: node.id.clone(),
                table: node.table.clone(),
                source_order,
            }) {
                self.pending_error = Some(error);
                return;
            }
            if retained_nodes.contains(&node.id) {
                continue;
            }
            let row = node_fields(
                node,
                (node.table == "File").then_some(partition.entry.content_hash.as_str()),
            );
            if let Err(error) = self.push_raw(&RawRecord::Node {
                table: node.table.clone(),
                id: node.id.clone(),
                source_order,
                row,
            }) {
                self.pending_error = Some(error);
                return;
            }
        }
        for edge in &partition.edges {
            if retained_edges.contains(&edge.id) {
                continue;
            }
            let source_order = match self.take_source_order() {
                Ok(order) => order,
                Err(error) => {
                    self.pending_error = Some(error);
                    return;
                }
            };
            if let Err(error) = self.push_raw(&RawRecord::Edge {
                table: edge.edge_type.clone(),
                id: edge.id.clone(),
                source_order,
                row: edge_fields(edge),
            }) {
                self.pending_error = Some(error);
                return;
            }
        }
    }

    pub(crate) fn finish(mut self) -> Result<StagingResult, NativeError> {
        if let Some(error) = self.pending_error.take() {
            return Err(error);
        }
        fs::create_dir_all(&self.staging_dir)?;
        let raw_records = self.raw_records.take().ok_or_else(|| {
            NativeError::InvalidInput("staging record spool was not initialized".to_string())
        })?;
        let mut raw_stream = raw_records.finish()?;
        let run_root = self.run_workspace.path();
        let merged_edges_path = run_root.join("merged-edges.bin");
        let node_types_path = run_root.join("node-types.bin");
        let mut merged_edges = RecordFileWriter::create(
            &merged_edges_path,
            self.chunk_limit,
            "staging_edges",
            self.metrics.clone(),
        )?;
        let mut node_types = RecordFileWriter::create(
            &node_types_path,
            self.chunk_limit,
            "staging_node_types",
            self.metrics.clone(),
        )?;
        let mut endpoints = SortedSpool::new(
            run_root,
            "endpoints",
            self.chunk_limit,
            self.metrics.clone(),
        )?;
        let mut nodes = JsonTableSink::new(
            &self.staging_dir,
            self.chunk_limit,
            self.output_chunk_limit,
            self.metrics.clone(),
        );

        let mut node_group: Option<(String, String, NodeStagedRow)> = None;
        let mut edge_group: Option<(String, String, EdgeStagedRow)> = None;
        let mut type_group: Option<(String, String)> = None;
        let mut unique_node_count = 0_usize;
        while let Some(record) = raw_stream.next()? {
            match record {
                RawRecord::Node { table, id, row, .. } => {
                    if let Some((current_table, current_id, current_row)) = node_group.as_mut() {
                        if *current_table == table && *current_id == id {
                            merge_node_value(current_row, row);
                            continue;
                        }
                    }
                    if let Some((table, _, row)) = node_group.take() {
                        nodes.push(&table, &row)?;
                    }
                    node_group = Some((table, id, row));
                }
                RawRecord::Edge { table, id, row, .. } => {
                    if let Some((table, _, row)) = node_group.take() {
                        nodes.push(&table, &row)?;
                    }
                    if let Some((current_table, current_id, current_row)) = edge_group.as_mut() {
                        if *current_table == table && *current_id == id {
                            merge_edge_value(current_row, row);
                            continue;
                        }
                    }
                    if let Some((table, id, row)) = edge_group.take() {
                        write_merged_edge(&mut merged_edges, &mut endpoints, table, id, row)?;
                    }
                    edge_group = Some((table, id, row));
                }
                RawRecord::NodeType { id, table, .. } => {
                    if let Some((table, id, row)) = edge_group.take() {
                        write_merged_edge(&mut merged_edges, &mut endpoints, table, id, row)?;
                    }
                    if type_group
                        .as_ref()
                        .is_some_and(|(current_id, _)| *current_id == id)
                    {
                        continue;
                    }
                    if let Some((id, table)) = type_group.take() {
                        node_types.push(&ResolvedNodeType { id, table })?;
                        unique_node_count = unique_node_count.checked_add(1).ok_or_else(|| {
                            NativeError::InvalidInput("unique node count overflow".to_string())
                        })?;
                    }
                    type_group = Some((id, table));
                }
            }
        }
        if let Some((table, _, row)) = node_group.take() {
            nodes.push(&table, &row)?;
        }
        if let Some((table, id, row)) = edge_group.take() {
            write_merged_edge(&mut merged_edges, &mut endpoints, table, id, row)?;
        }
        if let Some((id, table)) = type_group.take() {
            node_types.push(&ResolvedNodeType { id, table })?;
            unique_node_count = unique_node_count.checked_add(1).ok_or_else(|| {
                NativeError::InvalidInput("unique node count overflow".to_string())
            })?;
        }
        let (node_copy_statements, node_rows) = nodes.finish()?;
        merged_edges.finish()?;
        node_types.finish()?;

        let mut endpoint_stream = endpoints.finish()?;
        let mut node_type_reader = RecordFileReader::<ResolvedNodeType>::open(&node_types_path)?;
        let mut current_node_type = node_type_reader.next()?;
        let mut resolved_endpoints = SortedSpool::new(
            run_root,
            "resolved-endpoints",
            self.chunk_limit,
            self.metrics.clone(),
        )?;
        while let Some(endpoint) = endpoint_stream.next()? {
            while current_node_type
                .as_ref()
                .is_some_and(|node_type| node_type.id < endpoint.node_id)
            {
                current_node_type = node_type_reader.next()?;
            }
            let node_type = current_node_type
                .as_ref()
                .filter(|node_type| node_type.id == endpoint.node_id)
                .ok_or_else(|| endpoint.missing_error())?;
            resolved_endpoints.push(&ResolvedEndpoint {
                relation: endpoint.relation,
                edge_id: endpoint.edge_id,
                side: endpoint.side,
                node_type: node_type.table.clone(),
            })?;
        }
        drop(node_type_reader);
        fs::remove_file(&node_types_path)?;

        let mut resolved_stream = resolved_endpoints.finish()?;
        let mut merged_edge_reader = RecordFileReader::<MergedEdge>::open(&merged_edges_path)?;
        let mut edges = JsonTableSink::new(
            &self.staging_dir,
            self.chunk_limit,
            self.output_chunk_limit,
            self.metrics.clone(),
        );
        let mut connectors = SortedSpool::new(
            run_root,
            "connectors",
            self.chunk_limit,
            self.metrics.clone(),
        )?;
        let mut accepted_edge_ids = SortedSpool::new(
            run_root,
            "accepted-edge-ids",
            self.chunk_limit,
            self.metrics.clone(),
        )?;
        while let Some(edge) = merged_edge_reader.next()? {
            let source = take_resolved_endpoint(
                &mut resolved_stream,
                &edge.table,
                &edge.id,
                EndpointSide::Source,
            )?;
            let target = take_resolved_endpoint(
                &mut resolved_stream,
                &edge.table,
                &edge.id,
                EndpointSide::Target,
            )?;
            if !self
                .relation_constraints
                .allows(&edge.table, &source.node_type, &target.node_type)
            {
                continue;
            }
            edges.push(&edge.table, &edge.row)?;
            accepted_edge_ids.push(&EdgeIdentity {
                id: edge.id.clone(),
            })?;
            connectors.push(&ConnectorRecord {
                relation: edge.table.clone(),
                side: EndpointSide::Source,
                from_type: source.node_type,
                to_type: edge.table.clone(),
                from_id: edge.row.source_id.clone(),
                to_id: edge.id.clone(),
                role: "source".to_string(),
            })?;
            connectors.push(&ConnectorRecord {
                relation: edge.table.clone(),
                side: EndpointSide::Target,
                from_type: edge.table.clone(),
                to_type: target.node_type,
                from_id: edge.id,
                to_id: edge.row.target_id.clone(),
                role: "target".to_string(),
            })?;
        }
        drop(merged_edge_reader);
        fs::remove_file(&merged_edges_path)?;
        let (edge_copy_statements, edge_rows) = edges.finish()?;
        let mut accepted_edge_stream = accepted_edge_ids.finish()?;
        let mut previous_edge_id = None;
        let mut unique_edge_count = 0_usize;
        while let Some(edge) = accepted_edge_stream.next()? {
            if previous_edge_id.as_ref() == Some(&edge.id) {
                continue;
            }
            unique_edge_count = unique_edge_count.checked_add(1).ok_or_else(|| {
                NativeError::InvalidInput("unique edge count overflow".to_string())
            })?;
            previous_edge_id = Some(edge.id);
        }

        let mut connector_sink = ConnectorSink::new(
            &self.staging_dir,
            self.chunk_limit,
            self.output_chunk_limit,
            self.metrics.clone(),
        );
        let mut connector_stream = connectors.finish()?;
        let mut previous_connector_key = None;
        while let Some(connector) = connector_stream.next()? {
            let key = connector.sort_key();
            if previous_connector_key.as_ref() == Some(&key) {
                continue;
            }
            connector_sink.push(&connector)?;
            previous_connector_key = Some(key);
        }
        let (connector_copy_statements, connector_rows) = connector_sink.finish()?;

        let mut copy_statements = node_copy_statements;
        copy_statements.extend(edge_copy_statements);
        copy_statements.extend(connector_copy_statements);
        let (spill_bytes, high_water_bytes) = self.metrics.snapshot();
        Ok(StagingResult {
            copy_calls: copy_statements.len(),
            copy_statements,
            node_rows,
            edge_rows,
            connector_rows,
            unique_node_count,
            unique_edge_count,
            spill_bytes,
            high_water_bytes,
        })
    }

    fn take_source_order(&mut self) -> Result<u64, NativeError> {
        let order = self.next_source_order;
        self.next_source_order = self.next_source_order.checked_add(1).ok_or_else(|| {
            NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
                "staging_order",
                u64::MAX,
                u64::MAX,
                0,
            ))
        })?;
        Ok(order)
    }

    fn push_raw(&mut self, record: &RawRecord) -> Result<(), NativeError> {
        self.raw_records
            .as_mut()
            .ok_or_else(|| {
                NativeError::InvalidInput("staging record spool was not initialized".to_string())
            })?
            .push(record)
    }
}

fn write_merged_edge(
    writer: &mut RecordFileWriter<MergedEdge>,
    endpoints: &mut SortedSpool<EndpointRequest>,
    table: String,
    id: String,
    row: EdgeStagedRow,
) -> Result<(), NativeError> {
    endpoints.push(&EndpointRequest {
        node_id: row.source_id.clone(),
        relation: table.clone(),
        edge_id: id.clone(),
        side: EndpointSide::Source,
    })?;
    endpoints.push(&EndpointRequest {
        node_id: row.target_id.clone(),
        relation: table.clone(),
        edge_id: id.clone(),
        side: EndpointSide::Target,
    })?;
    writer.push(&MergedEdge { table, id, row })
}

fn take_resolved_endpoint(
    stream: &mut super::spill::SortedStream<ResolvedEndpoint>,
    relation: &str,
    edge_id: &str,
    side: EndpointSide,
) -> Result<ResolvedEndpoint, NativeError> {
    let endpoint = stream.next()?.ok_or_else(|| {
        NativeError::InvalidInput(format!(
            "edge {edge_id} is missing its {} endpoint resolution",
            side.as_str()
        ))
    })?;
    if endpoint.relation != relation || endpoint.edge_id != edge_id || endpoint.side != side {
        return Err(NativeError::InvalidInput(format!(
            "edge {edge_id} has inconsistent {} endpoint resolution",
            side.as_str()
        )));
    }
    Ok(endpoint)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
enum RawRecord {
    Node {
        table: String,
        id: String,
        source_order: u64,
        row: NodeStagedRow,
    },
    Edge {
        table: String,
        id: String,
        source_order: u64,
        row: EdgeStagedRow,
    },
    NodeType {
        id: String,
        table: String,
        source_order: u64,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RawKey {
    Node(String, String, u64),
    Edge(String, String, u64),
    NodeType(String, u64),
}

impl SpillRecord for RawRecord {
    type Key = RawKey;

    fn sort_key(&self) -> Self::Key {
        match self {
            Self::Node {
                table,
                id,
                source_order,
                ..
            } => RawKey::Node(table.clone(), id.clone(), *source_order),
            Self::Edge {
                table,
                id,
                source_order,
                ..
            } => RawKey::Edge(table.clone(), id.clone(), *source_order),
            Self::NodeType {
                id, source_order, ..
            } => RawKey::NodeType(id.clone(), *source_order),
        }
    }

    fn key_bytes(key: &Self::Key) -> usize {
        match key {
            RawKey::Node(table, id, _) | RawKey::Edge(table, id, _) => {
                table.len().saturating_add(id.len())
            }
            RawKey::NodeType(id, _) => id.len(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct MergedEdge {
    table: String,
    id: String,
    row: EdgeStagedRow,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResolvedNodeType {
    id: String,
    table: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
enum EndpointSide {
    Source,
    Target,
}

impl EndpointSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct EndpointRequest {
    node_id: String,
    relation: String,
    edge_id: String,
    side: EndpointSide,
}

impl EndpointRequest {
    fn missing_error(&self) -> NativeError {
        NativeError::InvalidInput(format!(
            "edge {} references missing {} node {}",
            self.edge_id,
            self.side.as_str(),
            self.node_id
        ))
    }
}

impl SpillRecord for EndpointRequest {
    type Key = (String, String, String, EndpointSide);

    fn sort_key(&self) -> Self::Key {
        (
            self.node_id.clone(),
            self.relation.clone(),
            self.edge_id.clone(),
            self.side,
        )
    }

    fn key_bytes(key: &Self::Key) -> usize {
        key.0
            .len()
            .saturating_add(key.1.len())
            .saturating_add(key.2.len())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ResolvedEndpoint {
    relation: String,
    edge_id: String,
    side: EndpointSide,
    node_type: String,
}

impl SpillRecord for ResolvedEndpoint {
    type Key = (String, String, EndpointSide);

    fn sort_key(&self) -> Self::Key {
        (self.relation.clone(), self.edge_id.clone(), self.side)
    }

    fn key_bytes(key: &Self::Key) -> usize {
        key.0.len().saturating_add(key.1.len())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ConnectorRecord {
    relation: String,
    side: EndpointSide,
    from_type: String,
    to_type: String,
    from_id: String,
    to_id: String,
    role: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct EdgeIdentity {
    id: String,
}

impl SpillRecord for EdgeIdentity {
    type Key = String;

    fn sort_key(&self) -> Self::Key {
        self.id.clone()
    }

    fn key_bytes(key: &Self::Key) -> usize {
        key.len()
    }
}

impl SpillRecord for ConnectorRecord {
    type Key = (String, EndpointSide, String, String, String, String, String);

    fn sort_key(&self) -> Self::Key {
        (
            self.relation.clone(),
            self.side,
            self.from_type.clone(),
            self.to_type.clone(),
            self.from_id.clone(),
            self.to_id.clone(),
            self.role.clone(),
        )
    }

    fn key_bytes(key: &Self::Key) -> usize {
        key.0
            .len()
            .saturating_add(key.2.len())
            .saturating_add(key.3.len())
            .saturating_add(key.4.len())
            .saturating_add(key.5.len())
            .saturating_add(key.6.len())
    }
}

struct JsonTableSink {
    staging_dir: PathBuf,
    record_limit: usize,
    chunk_limit: usize,
    metrics: SpillMetrics,
    current_table: Option<String>,
    chunk_index: usize,
    writer: Option<BufWriter<File>>,
    first_row: bool,
    current_bytes: usize,
    copy_statements: Vec<String>,
    row_count: usize,
}

impl JsonTableSink {
    fn new(
        staging_dir: &Path,
        record_limit: usize,
        chunk_limit: usize,
        metrics: SpillMetrics,
    ) -> Self {
        Self {
            staging_dir: staging_dir.to_path_buf(),
            record_limit,
            chunk_limit,
            metrics,
            current_table: None,
            chunk_index: 0,
            writer: None,
            first_row: true,
            current_bytes: 0,
            copy_statements: Vec::new(),
            row_count: 0,
        }
    }

    fn push<T: Serialize>(&mut self, table: &str, row: &T) -> Result<(), NativeError> {
        let encoded = encode_bounded(row, self.record_limit, "staging_json_output")?;
        let _charge = self.metrics.charge(encoded.capacity());
        let single_row_bytes = encoded.len().checked_add(3).ok_or_else(|| {
            output_budget_error("staging_json_output", self.record_limit, usize::MAX)
        })?;
        if single_row_bytes > self.record_limit {
            return Err(output_budget_error(
                "staging_json_output",
                self.record_limit,
                single_row_bytes,
            ));
        }
        if self.current_table.as_deref() != Some(table) {
            self.close_chunk()?;
            self.current_table = Some(table.to_string());
            self.chunk_index = 0;
            self.open_chunk()?;
        }
        let separator_bytes = usize::from(!self.first_row);
        let projected_bytes = self
            .current_bytes
            .checked_add(separator_bytes)
            .and_then(|bytes| bytes.checked_add(encoded.len()))
            .and_then(|bytes| bytes.checked_add(2))
            .ok_or_else(|| {
                output_budget_error("staging_json_output", self.chunk_limit, usize::MAX)
            })?;
        if !self.first_row && projected_bytes > self.chunk_limit {
            self.close_chunk()?;
            self.chunk_index = self.chunk_index.checked_add(1).ok_or_else(|| {
                NativeError::InvalidInput("staging JSON chunk index overflow".to_string())
            })?;
            self.open_chunk()?;
        }
        let writer = self.writer.as_mut().ok_or_else(|| {
            NativeError::InvalidInput("staging JSON writer is not open".to_string())
        })?;
        if !self.first_row {
            writer.write_all(b",")?;
            self.current_bytes = self.current_bytes.checked_add(1).ok_or_else(|| {
                NativeError::InvalidInput("staging JSON byte count overflow".to_string())
            })?;
        }
        writer.write_all(&encoded)?;
        self.current_bytes = self
            .current_bytes
            .checked_add(encoded.len())
            .ok_or_else(|| {
                NativeError::InvalidInput("staging JSON byte count overflow".to_string())
            })?;
        self.first_row = false;
        self.row_count = self
            .row_count
            .checked_add(1)
            .ok_or_else(|| NativeError::InvalidInput("staging row count overflow".to_string()))?;
        Ok(())
    }

    fn finish(mut self) -> Result<(Vec<String>, usize), NativeError> {
        self.close_chunk()?;
        Ok((self.copy_statements, self.row_count))
    }

    fn open_chunk(&mut self) -> Result<(), NativeError> {
        let table = self.current_table.as_deref().ok_or_else(|| {
            NativeError::InvalidInput("staging JSON table is not selected".to_string())
        })?;
        let path = chunked_output_path(
            &self.staging_dir,
            &stage_file_stem(table),
            "json",
            self.chunk_index,
        );
        let mut writer = BufWriter::new(File::create(&path)?);
        writer.write_all(b"[")?;
        self.copy_statements
            .push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
        self.writer = Some(writer);
        self.first_row = true;
        self.current_bytes = 1;
        Ok(())
    }

    fn close_chunk(&mut self) -> Result<(), NativeError> {
        if let Some(mut writer) = self.writer.take() {
            writer.write_all(b"]\n")?;
            writer.flush()?;
        }
        self.current_bytes = 0;
        Ok(())
    }
}

struct ConnectorSink {
    staging_dir: PathBuf,
    record_limit: usize,
    chunk_limit: usize,
    metrics: SpillMetrics,
    current_group: Option<(String, EndpointSide, String, String)>,
    chunk_index: usize,
    writer: Option<BufWriter<File>>,
    current_bytes: usize,
    rows_in_chunk: usize,
    copy_statements: Vec<String>,
    row_count: usize,
}

impl ConnectorSink {
    fn new(
        staging_dir: &Path,
        record_limit: usize,
        chunk_limit: usize,
        metrics: SpillMetrics,
    ) -> Self {
        Self {
            staging_dir: staging_dir.to_path_buf(),
            record_limit,
            chunk_limit,
            metrics,
            current_group: None,
            chunk_index: 0,
            writer: None,
            current_bytes: 0,
            rows_in_chunk: 0,
            copy_statements: Vec::new(),
            row_count: 0,
        }
    }

    fn push(&mut self, connector: &ConnectorRecord) -> Result<(), NativeError> {
        let group = (
            connector.relation.clone(),
            connector.side,
            connector.from_type.clone(),
            connector.to_type.clone(),
        );
        if self.current_group.as_ref() != Some(&group) {
            self.close_chunk()?;
            self.current_group = Some(group);
            self.chunk_index = 0;
            self.open_chunk()?;
        }
        let encoded =
            encode_output_bounded(self.record_limit, "staging_connector_output", |writer| {
                write_csv_field(writer, &connector.from_id)?;
                writer.write_all(b",")?;
                write_csv_field(writer, &connector.to_id)?;
                writer.write_all(b",")?;
                write_csv_field(writer, &connector.role)?;
                writer.write_all(b"\r\n")
            })?;
        let _charge = self.metrics.charge(encoded.capacity());
        let single_row_bytes = CONNECTOR_HEADER
            .len()
            .checked_add(encoded.len())
            .ok_or_else(|| {
                output_budget_error("staging_connector_output", self.record_limit, usize::MAX)
            })?;
        if single_row_bytes > self.record_limit {
            return Err(output_budget_error(
                "staging_connector_output",
                self.record_limit,
                single_row_bytes,
            ));
        }
        let projected_bytes = self
            .current_bytes
            .checked_add(encoded.len())
            .ok_or_else(|| {
                output_budget_error("staging_connector_output", self.chunk_limit, usize::MAX)
            })?;
        if self.rows_in_chunk > 0 && projected_bytes > self.chunk_limit {
            self.close_chunk()?;
            self.chunk_index = self.chunk_index.checked_add(1).ok_or_else(|| {
                NativeError::InvalidInput("staging connector chunk index overflow".to_string())
            })?;
            self.open_chunk()?;
        }
        let writer = self.writer.as_mut().ok_or_else(|| {
            NativeError::InvalidInput("staging connector writer is not open".to_string())
        })?;
        writer.write_all(&encoded)?;
        self.current_bytes = self
            .current_bytes
            .checked_add(encoded.len())
            .ok_or_else(|| {
                NativeError::InvalidInput("staging connector byte count overflow".to_string())
            })?;
        self.rows_in_chunk = self.rows_in_chunk.checked_add(1).ok_or_else(|| {
            NativeError::InvalidInput("staging connector chunk row count overflow".to_string())
        })?;
        self.row_count = self.row_count.checked_add(1).ok_or_else(|| {
            NativeError::InvalidInput("staging connector row count overflow".to_string())
        })?;
        Ok(())
    }

    fn finish(mut self) -> Result<(Vec<String>, usize), NativeError> {
        self.close_chunk()?;
        Ok((self.copy_statements, self.row_count))
    }

    fn open_chunk(&mut self) -> Result<(), NativeError> {
        let (relation, side, from_type, to_type) =
            self.current_group.as_ref().ok_or_else(|| {
                NativeError::InvalidInput("staging connector group is not selected".to_string())
            })?;
        let connector_table = match side {
            EndpointSide::Source => format!("FROM_{relation}"),
            EndpointSide::Target => format!("TO_{relation}"),
        };
        let stem = format!(
            "{}__{}__{}",
            stage_file_stem(&connector_table),
            stage_file_stem(from_type),
            stage_file_stem(to_type)
        );
        let path = chunked_output_path(&self.staging_dir, &stem, "csv", self.chunk_index);
        let mut writer = BufWriter::new(File::create(&path)?);
        writer.write_all(CONNECTOR_HEADER)?;
        self.copy_statements.push(format!(
            "COPY `{}` FROM \"{}\" (header=true, from=\"{}\", to=\"{}\");",
            connector_table,
            copy_path(&path),
            from_type,
            to_type
        ));
        self.writer = Some(writer);
        self.current_bytes = CONNECTOR_HEADER.len();
        self.rows_in_chunk = 0;
        Ok(())
    }

    fn close_chunk(&mut self) -> Result<(), NativeError> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
        }
        self.current_bytes = 0;
        self.rows_in_chunk = 0;
        Ok(())
    }
}

const CONNECTOR_HEADER: &[u8] = b"from_id,to_id,role\r\n";

fn chunked_output_path(root: &Path, stem: &str, extension: &str, index: usize) -> PathBuf {
    if index == 0 {
        root.join(format!("{stem}.{extension}"))
    } else {
        root.join(format!("{stem}__{index:06}.{extension}"))
    }
}

fn output_budget_error(phase: &str, limit: usize, accounted: usize) -> NativeError {
    NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
        phase,
        u64::try_from(limit).unwrap_or(u64::MAX),
        u64::try_from(accounted).unwrap_or(u64::MAX),
        0,
    ))
}

struct RunWorkspace {
    path: PathBuf,
}

impl RunWorkspace {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunWorkspace {
    fn drop(&mut self) {
        if self.path.is_dir() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

impl RelationConstraints {
    fn from_declared_schema() -> Self {
        let Ok(schema) =
            serde_json::from_str::<Value>(include_str!("../../assets/graph_schema.json"))
        else {
            return Self::default();
        };
        let pairs_by_relation = schema
            .get("relation_types")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|relation| {
                let name = relation.get("name").and_then(Value::as_str)?.to_string();
                let source_types = json_string_set(relation, "source_types");
                let target_types = json_string_set(relation, "target_types");
                if source_types.is_empty() || target_types.is_empty() {
                    return None;
                }
                Some((name, (source_types, target_types)))
            })
            .collect();
        Self { pairs_by_relation }
    }

    fn allows(&self, relation: &str, source_type: &str, target_type: &str) -> bool {
        self.pairs_by_relation
            .get(relation)
            .is_none_or(|(sources, targets)| {
                sources.contains(source_type) && targets.contains(target_type)
            })
    }
}

fn json_string_set(value: &Value, key: &str) -> BTreeSet<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Serialize)]
    struct OutputRow {
        id: String,
    }

    #[test]
    fn json_output_rotates_at_the_byte_limit() {
        let root = temp_root("json-chunks");
        let row = OutputRow {
            id: "x".repeat(128),
        };
        let encoded_len = serde_json::to_vec(&row).unwrap().len();
        let chunk_limit = encoded_len + 3;
        let mut sink = JsonTableSink::new(&root, chunk_limit, chunk_limit, SpillMetrics::default());

        sink.push("Symbol", &row).unwrap();
        sink.push("Symbol", &row).unwrap();
        let (statements, rows) = sink.finish().unwrap();

        assert_eq!(rows, 2);
        assert_eq!(statements.len(), 2);
        assert!(root.join("symbol.json").is_file());
        assert!(root.join("symbol__000001.json").is_file());
        assert!(fs::metadata(root.join("symbol.json")).unwrap().len() <= chunk_limit as u64);
        assert!(
            fs::metadata(root.join("symbol__000001.json"))
                .unwrap()
                .len()
                <= chunk_limit as u64
        );
    }

    #[test]
    fn final_outputs_reject_single_rows_larger_than_the_chunk() {
        let root = temp_root("oversized-output");
        let row = OutputRow {
            id: "x".repeat(128),
        };
        let encoded_len = serde_json::to_vec(&row).unwrap().len();
        let mut json = JsonTableSink::new(
            &root,
            encoded_len + 2,
            encoded_len + 2,
            SpillMetrics::default(),
        );
        let error = json.push("Symbol", &row).unwrap_err();
        assert_budget_error(error, "staging_json_output", encoded_len + 2);

        let connector = ConnectorRecord {
            relation: "Contains".to_string(),
            side: EndpointSide::Source,
            from_type: "File".to_string(),
            to_type: "Contains".to_string(),
            from_id: "source".repeat(64),
            to_id: "edge:one".to_string(),
            role: "source".to_string(),
        };
        let mut connector_sink = ConnectorSink::new(&root, 64, 64, SpillMetrics::default());
        let error = connector_sink.push(&connector).unwrap_err();
        assert_budget_error(error, "staging_connector_output", 64);
    }

    fn assert_budget_error(error: NativeError, phase: &str, limit: usize) {
        let NativeError::MemoryBudgetExceeded(error) = error else {
            panic!("expected structured memory budget error");
        };
        assert_eq!(error.phase, phase);
        assert_eq!(error.limit_bytes, limit as u64);
        assert!(error.accounted_bytes > error.limit_bytes);
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codebase_graph_output_{name}_{}_{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
