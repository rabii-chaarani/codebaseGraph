use crate::error::NativeError;
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::partition_builder::GraphPartition;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct StagingResult {
    pub(crate) copy_statements: Vec<String>,
    pub(crate) node_rows: usize,
    pub(crate) edge_rows: usize,
    pub(crate) connector_rows: usize,
    pub(crate) copy_calls: usize,
}

type NodeRowsById = HashMap<String, NodeStagedRow>;
type EdgeRowsById = HashMap<String, EdgeStagedRow>;
type ConnectorTypePair = (String, String);
type ConnectorRowKey = (String, String, String);
type ConnectorRowsByTypePair = HashMap<ConnectorTypePair, HashMap<ConnectorRowKey, ConnectorRow>>;
type ConnectorBucketsByTable = HashMap<String, ConnectorRowsByTypePair>;

#[derive(Clone, Debug, Serialize)]
struct NodeStagedRow {
    id: String,
    label: String,
    kind: String,
    language: String,
    path: String,
    qualified_name: String,
    scope_id: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
    tree_sitter_node_type: String,
    capture_name: String,
    summary: String,
    metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct EdgeStagedRow {
    id: String,
    kind: String,
    source_id: String,
    target_id: String,
    confidence: f64,
    line_start: Option<i64>,
    line_end: Option<i64>,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
    metadata: Value,
}

#[derive(Serialize)]
struct ConnectorRow {
    from_id: String,
    to_id: String,
    role: String,
}

struct EdgeConnector {
    id: String,
    edge_type: String,
    source_id: String,
    target_id: String,
}

pub(crate) struct StagingAccumulator {
    staging_dir: PathBuf,
    nodes: HashMap<String, NodeRowsById>,
    edges: HashMap<String, EdgeRowsById>,
    node_types_by_id: HashMap<String, String>,
    edge_connectors: Vec<EdgeConnector>,
    connectors: ConnectorBucketsByTable,
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

    fn materialize_connectors(&mut self) -> Result<(), NativeError> {
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

fn node_fields(node: &GraphNodeRow, content_hash: Option<&str>) -> NodeStagedRow {
    NodeStagedRow {
        id: node.id.clone(),
        label: node.label.clone(),
        kind: node.kind.clone(),
        language: node.language.clone(),
        path: node.path.clone(),
        qualified_name: node.qualified_name.clone(),
        scope_id: node.scope_id.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        tree_sitter_node_type: node.tree_sitter_node_type.clone(),
        capture_name: node.capture_name.clone(),
        summary: node.summary.clone(),
        metadata: node.metadata.clone(),
        content_hash: content_hash.map(str::to_string),
    }
}

fn edge_fields(edge: &GraphEdgeRow) -> EdgeStagedRow {
    EdgeStagedRow {
        id: edge.id.clone(),
        kind: edge.kind.clone(),
        source_id: edge.source_id.clone(),
        target_id: edge.target_id.clone(),
        confidence: 1.0,
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        metadata: edge.metadata.clone(),
    }
}

fn merge_node_row(rows: &mut NodeRowsById, row_id: String, incoming: NodeStagedRow) {
    let Some(existing) = rows.get_mut(&row_id) else {
        rows.insert(row_id, incoming);
        return;
    };

    merge_string(&mut existing.id, incoming.id);
    merge_string(&mut existing.label, incoming.label);
    merge_string(&mut existing.kind, incoming.kind);
    merge_string(&mut existing.language, incoming.language);
    merge_string(&mut existing.path, incoming.path);
    merge_string(&mut existing.qualified_name, incoming.qualified_name);
    merge_string(&mut existing.scope_id, incoming.scope_id);
    merge_optional_i64(&mut existing.line_start, incoming.line_start);
    merge_optional_i64(&mut existing.line_end, incoming.line_end);
    merge_optional_i64(&mut existing.byte_start, incoming.byte_start);
    merge_optional_i64(&mut existing.byte_end, incoming.byte_end);
    merge_string(
        &mut existing.tree_sitter_node_type,
        incoming.tree_sitter_node_type,
    );
    merge_string(&mut existing.capture_name, incoming.capture_name);
    merge_string(&mut existing.summary, incoming.summary);
    merge_value(&mut existing.metadata, incoming.metadata);
    merge_optional_string(&mut existing.content_hash, incoming.content_hash);
}

fn merge_edge_row(rows: &mut EdgeRowsById, row_id: String, incoming: EdgeStagedRow) {
    let Some(existing) = rows.get_mut(&row_id) else {
        rows.insert(row_id, incoming);
        return;
    };

    merge_string(&mut existing.id, incoming.id);
    merge_string(&mut existing.kind, incoming.kind);
    merge_string(&mut existing.source_id, incoming.source_id);
    merge_string(&mut existing.target_id, incoming.target_id);
    merge_optional_i64(&mut existing.line_start, incoming.line_start);
    merge_optional_i64(&mut existing.line_end, incoming.line_end);
    merge_optional_i64(&mut existing.byte_start, incoming.byte_start);
    merge_optional_i64(&mut existing.byte_end, incoming.byte_end);
    merge_value(&mut existing.metadata, incoming.metadata);
}

fn merge_string(existing: &mut String, incoming: String) {
    if existing.is_empty() && !incoming.is_empty() {
        *existing = incoming;
    }
}

fn merge_optional_string(existing: &mut Option<String>, incoming: Option<String>) {
    if let Some(incoming) = incoming {
        if !incoming.is_empty() && existing.as_ref().is_none_or(|current| current.is_empty()) {
            *existing = Some(incoming);
        }
    }
}

fn merge_optional_i64(existing: &mut Option<i64>, incoming: Option<i64>) {
    if existing.is_none() && incoming.is_some() {
        *existing = incoming;
    }
}

fn merge_value(existing: &mut Value, incoming: Value) {
    if json_value_is_empty(existing) && !json_value_is_empty(&incoming) {
        *existing = incoming;
    }
}

fn json_value_is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

fn sorted_keys<V>(values: &HashMap<String, V>) -> Vec<&String> {
    let mut keys = values.keys().collect::<Vec<_>>();
    keys.sort();
    keys
}

fn sorted_row_values<V>(rows: &HashMap<String, V>) -> Vec<&V> {
    let mut entries = rows.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries.into_iter().map(|(_, value)| value).collect()
}

fn sorted_connector_type_buckets(
    buckets: &ConnectorRowsByTypePair,
) -> Vec<(&ConnectorTypePair, &HashMap<ConnectorRowKey, ConnectorRow>)> {
    let mut buckets = buckets.iter().collect::<Vec<_>>();
    buckets.sort_by(|left, right| left.0.cmp(right.0));
    buckets
}

fn sorted_connector_rows(rows: &HashMap<ConnectorRowKey, ConnectorRow>) -> Vec<&ConnectorRow> {
    let mut entries = rows.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries.into_iter().map(|(_, value)| value).collect()
}

fn write_json_rows<'a, T: Serialize + 'a>(
    path: &Path,
    rows: impl IntoIterator<Item = &'a T>,
) -> Result<(), NativeError> {
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(b"[")?;
    for (row_index, row) in rows.into_iter().enumerate() {
        if row_index > 0 {
            writer.write_all(b",")?;
        }
        serde_json::to_writer(&mut writer, row)?;
    }
    writer.write_all(b"]\n")?;
    Ok(())
}

fn write_csv_rows<'a>(
    path: &Path,
    rows: impl IntoIterator<Item = &'a ConnectorRow>,
) -> Result<(), NativeError> {
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(b"from_id,to_id,role\r\n")?;
    for row in rows {
        writer.write_all(csv_field(&row.from_id).as_bytes())?;
        writer.write_all(b",")?;
        writer.write_all(csv_field(&row.to_id).as_bytes())?;
        writer.write_all(b",")?;
        writer.write_all(csv_field(&row.role).as_bytes())?;
        writer.write_all(b"\r\n")?;
    }
    Ok(())
}

fn csv_field(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn stage_file_stem(name: &str) -> String {
    let stem = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if stem.is_empty() {
        "table".to_string()
    } else {
        stem
    }
}

fn copy_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ManifestEntry;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_typed_rows_and_connectors_without_bulk_protocol() {
        let staging_dir = temp_staging_dir("typed_rows_and_connectors");
        let partition = partition(
            "hash-1",
            vec![
                node("file:one", "File", "file.py"),
                node("sym:one", "Symbol", "foo"),
            ],
            vec![edge("edge:one", "Contains", "file:one", "sym:one")],
        );

        let mut staging = StagingAccumulator::new(&staging_dir.to_string_lossy());
        staging.add_partition(&partition);
        let result = staging.finish().unwrap();

        assert_eq!(result.node_rows, 2);
        assert_eq!(result.edge_rows, 1);
        assert_eq!(result.connector_rows, 2);
        assert_eq!(result.copy_calls, 5);
        assert!(result
            .copy_statements
            .iter()
            .any(|statement| statement.starts_with("COPY `File` FROM ")));
        assert!(staging_dir.join("file.json").exists());
        assert!(staging_dir.join("symbol.json").exists());
        assert!(staging_dir.join("contains.json").exists());
        assert!(staging_dir
            .join("from_contains__file__contains.csv")
            .exists());
        assert!(staging_dir
            .join("to_contains__contains__symbol.csv")
            .exists());

        let file_rows = read_json_array(&staging_dir.join("file.json"));
        assert_eq!(file_rows[0]["content_hash"], "hash-1");
    }

    #[test]
    fn duplicate_typed_rows_keep_first_non_empty_fields() {
        let staging_dir = temp_staging_dir("duplicate_merge");
        let mut first = node("sym:one", "Symbol", "");
        first.label.clear();
        first.line_start = None;
        first.summary = "first-summary".to_string();
        let mut second = node("sym:one", "Symbol", "foo");
        second.label = "second-label".to_string();
        second.line_start = Some(42);
        second.summary = "second-summary".to_string();
        second.metadata = json!({"source": "later"});
        let first_file = node("file:one", "File", "file.py");
        let second_file = node("file:one", "File", "file.py");
        let first_partition = partition("", vec![first, first_file], Vec::new());
        let second_partition = partition("hash-2", vec![second, second_file], Vec::new());

        let mut staging = StagingAccumulator::new(&staging_dir.to_string_lossy());
        staging.add_partition(&first_partition);
        staging.add_partition(&second_partition);
        let result = staging.finish().unwrap();

        assert_eq!(result.node_rows, 2);
        let symbol_rows = read_json_array(&staging_dir.join("symbol.json"));
        assert_eq!(symbol_rows[0]["label"], "second-label");
        assert_eq!(symbol_rows[0]["summary"], "first-summary");
        assert_eq!(symbol_rows[0]["line_start"], 42);
        assert_eq!(symbol_rows[0]["metadata"], json!({"source": "later"}));
        let file_rows = read_json_array(&staging_dir.join("file.json"));
        assert_eq!(file_rows[0]["content_hash"], "hash-2");
    }

    #[test]
    fn deterministic_output_sorts_rows_connectors_and_copy_statements() {
        let staging_dir = temp_staging_dir("deterministic_output");
        let partition = partition(
            "hash-1",
            vec![
                node("sym:b", "Symbol", "b"),
                node("file:z", "File", "z.py"),
                node("sym:a", "Symbol", "a"),
                node("file:a", "File", "a.py"),
            ],
            vec![
                edge("edge:b", "Contains", "file:z", "sym:b"),
                edge("edge:a", "Contains", "file:a", "sym:a"),
                edge("edge:r", "References", "file:z", "sym:a"),
            ],
        );

        let mut staging = StagingAccumulator::new(&staging_dir.to_string_lossy());
        staging.add_partition(&partition);
        let result = staging.finish().unwrap();

        let statement_tables = result
            .copy_statements
            .iter()
            .map(|statement| statement.split(" FROM ").next().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            statement_tables,
            vec![
                "COPY `File`",
                "COPY `Symbol`",
                "COPY `Contains`",
                "COPY `References`",
                "COPY `FROM_Contains`",
                "COPY `TO_Contains`",
                "COPY `FROM_References`",
                "COPY `TO_References`",
            ]
        );

        let symbol_rows = read_json_array(&staging_dir.join("symbol.json"));
        assert_eq!(symbol_rows[0]["id"], "sym:a");
        assert_eq!(symbol_rows[1]["id"], "sym:b");
        let edge_rows = read_json_array(&staging_dir.join("contains.json"));
        assert_eq!(edge_rows[0]["id"], "edge:a");
        assert_eq!(edge_rows[1]["id"], "edge:b");
        let reference_rows = read_json_array(&staging_dir.join("references.json"));
        assert_eq!(reference_rows[0]["id"], "edge:r");

        let from_csv =
            fs::read_to_string(staging_dir.join("from_contains__file__contains.csv")).unwrap();
        assert_eq!(
            from_csv.lines().collect::<Vec<_>>(),
            vec![
                "from_id,to_id,role",
                "file:a,edge:a,source",
                "file:z,edge:b,source",
            ]
        );
        let to_csv =
            fs::read_to_string(staging_dir.join("to_contains__contains__symbol.csv")).unwrap();
        assert_eq!(
            to_csv.lines().collect::<Vec<_>>(),
            vec![
                "from_id,to_id,role",
                "edge:a,sym:a,target",
                "edge:b,sym:b,target",
            ]
        );
        assert!(staging_dir
            .join("from_references__file__references.csv")
            .exists());
        assert!(staging_dir
            .join("to_references__references__symbol.csv")
            .exists());
    }

    #[test]
    fn connector_generation_requires_existing_endpoints() {
        let staging_dir = temp_staging_dir("missing_endpoint");
        let partition = partition(
            "hash-1",
            vec![node("file:one", "File", "file.py")],
            vec![edge("edge:one", "Contains", "file:one", "sym:missing")],
        );

        let mut staging = StagingAccumulator::new(&staging_dir.to_string_lossy());
        staging.add_partition(&partition);
        let error = staging.finish().unwrap_err();

        assert!(error
            .to_string()
            .contains("edge edge:one references missing target node sym:missing"));
    }

    #[test]
    fn connector_generation_allows_target_in_later_partition() {
        let staging_dir = temp_staging_dir("deferred_connector");
        let first = partition(
            "hash-1",
            vec![node("file:one", "File", "file.py")],
            vec![edge("edge:one", "Contains", "file:one", "sym:later")],
        );
        let second = partition(
            "hash-2",
            vec![node("sym:later", "Symbol", "foo")],
            Vec::new(),
        );

        let mut staging = StagingAccumulator::new(&staging_dir.to_string_lossy());
        staging.add_partition(&first);
        staging.add_partition(&second);
        let result = staging.finish().unwrap();

        assert_eq!(result.connector_rows, 2);
        assert!(staging_dir
            .join("from_contains__file__contains.csv")
            .exists());
        assert!(staging_dir
            .join("to_contains__contains__symbol.csv")
            .exists());
    }

    fn partition(
        content_hash: &str,
        nodes: Vec<GraphNodeRow>,
        edges: Vec<GraphEdgeRow>,
    ) -> GraphPartition {
        GraphPartition {
            entry: ManifestEntry {
                path: "file.py".to_string(),
                content_hash: content_hash.to_string(),
                language: "python".to_string(),
                partition_id: "partition".to_string(),
                node_ids: nodes.iter().map(|node| node.id.clone()).collect(),
                edge_ids: edges.iter().map(|edge| edge.id.clone()).collect(),
                node_types: nodes
                    .iter()
                    .map(|node| (node.id.clone(), node.table.clone()))
                    .collect(),
                edge_types: edges
                    .iter()
                    .map(|edge| (edge.id.clone(), edge.edge_type.clone()))
                    .collect(),
                materialized_at: "now".to_string(),
            },
            nodes,
            edges,
        }
    }

    fn node(id: &str, table: &str, label: &str) -> GraphNodeRow {
        GraphNodeRow {
            id: id.to_string(),
            table: table.to_string(),
            label: label.to_string(),
            kind: label.to_string(),
            language: "python".to_string(),
            path: "file.py".to_string(),
            qualified_name: label.to_string(),
            scope_id: String::new(),
            line_start: Some(1),
            line_end: Some(1),
            byte_start: Some(0),
            byte_end: Some(1),
            tree_sitter_node_type: "identifier".to_string(),
            capture_name: "name".to_string(),
            summary: String::new(),
            metadata: json!({}),
        }
    }

    fn edge(id: &str, edge_type: &str, source_id: &str, target_id: &str) -> GraphEdgeRow {
        GraphEdgeRow {
            id: id.to_string(),
            edge_type: edge_type.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            kind: "contains".to_string(),
            metadata: json!({}),
        }
    }

    fn temp_staging_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("codebase_graph_native_staging_{name}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_json_array(path: &Path) -> Vec<Value> {
        let content = fs::read_to_string(path).unwrap();
        serde_json::from_str(&content).unwrap()
    }
}
