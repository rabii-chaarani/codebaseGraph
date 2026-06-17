use crate::error::NativeError;
use crate::graph::GraphPartition;
use crate::legacy::{GraphEdgeRow, GraphNodeRow};
use crate::protocol::NativeSyntaxMaterializationRequest;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
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

type StagedRow = BTreeMap<String, Value>;
type RowsById = BTreeMap<String, StagedRow>;
type ConnectorKey = (String, String, String);
type ConnectorRowKey = (String, String, String);

#[derive(Serialize)]
struct ConnectorRow {
    from_id: String,
    to_id: String,
    role: String,
}

struct StagingTables {
    staging_dir: PathBuf,
    node_tables: BTreeSet<String>,
    edge_tables: BTreeSet<String>,
    nodes: BTreeMap<String, RowsById>,
    edges: BTreeMap<String, RowsById>,
    node_types_by_id: BTreeMap<String, String>,
    connectors: BTreeMap<ConnectorKey, BTreeMap<ConnectorRowKey, ConnectorRow>>,
}

pub(crate) fn write_partitions(
    request: &NativeSyntaxMaterializationRequest,
    partitions: &[GraphPartition],
) -> Result<StagingResult, NativeError> {
    let mut tables = StagingTables::new(&request.staging_dir);
    for partition in partitions {
        for node in &partition.nodes {
            tables.add_node(
                node,
                (node.table == "File").then_some(partition.entry.content_hash.as_str()),
            );
        }
    }
    for partition in partitions {
        for edge in &partition.edges {
            tables.add_edge(edge)?;
        }
    }
    tables.write()
}

impl StagingTables {
    fn new(staging_dir: &str) -> Self {
        Self {
            staging_dir: PathBuf::from(staging_dir),
            node_tables: BTreeSet::new(),
            edge_tables: BTreeSet::new(),
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            node_types_by_id: BTreeMap::new(),
            connectors: BTreeMap::new(),
        }
    }

    fn add_node(&mut self, node: &GraphNodeRow, content_hash: Option<&str>) {
        self.node_tables.insert(node.table.clone());
        self.node_types_by_id
            .entry(node.id.clone())
            .or_insert_with(|| node.table.clone());
        merge_staged_row(
            self.nodes.entry(node.table.clone()).or_default(),
            node.id.clone(),
            node_fields(node, content_hash),
        );
    }

    fn add_edge(&mut self, edge: &GraphEdgeRow) -> Result<(), NativeError> {
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

        self.edge_tables.insert(edge.edge_type.clone());
        merge_staged_row(
            self.edges.entry(edge.edge_type.clone()).or_default(),
            edge.id.clone(),
            edge_fields(edge),
        );
        self.add_connector(
            format!("FROM_{}", edge.edge_type),
            source_type,
            edge.edge_type.clone(),
            edge.source_id.clone(),
            edge.id.clone(),
            "source".to_string(),
        );
        self.add_connector(
            format!("TO_{}", edge.edge_type),
            edge.edge_type.clone(),
            target_type,
            edge.id.clone(),
            edge.target_id.clone(),
            "target".to_string(),
        );
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
            .entry((table, from_type, to_type))
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

        for table in &self.node_tables {
            let Some(rows) = self.nodes.get(table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            let path = self
                .staging_dir
                .join(format!("{}.json", stage_file_stem(table)));
            write_json_rows(&path, rows.values())?;
            copy_statements.push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
            node_rows += rows.len();
        }

        for table in &self.edge_tables {
            let Some(rows) = self.edges.get(table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            let path = self
                .staging_dir
                .join(format!("{}.json", stage_file_stem(table)));
            write_json_rows(&path, rows.values())?;
            copy_statements.push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
            edge_rows += rows.len();
        }

        for relation in &self.edge_tables {
            for connector_table in [format!("FROM_{relation}"), format!("TO_{relation}")] {
                for ((table, from_type, to_type), rows) in &self.connectors {
                    if table != &connector_table || rows.is_empty() {
                        continue;
                    }
                    let path = self.staging_dir.join(format!(
                        "{}__{}__{}.csv",
                        stage_file_stem(table),
                        stage_file_stem(from_type),
                        stage_file_stem(to_type)
                    ));
                    write_csv_rows(&path, rows.values())?;
                    copy_statements.push(format!(
                        "COPY `{}` FROM \"{}\" (header=true, from=\"{}\", to=\"{}\");",
                        table,
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

fn node_fields(node: &GraphNodeRow, content_hash: Option<&str>) -> StagedRow {
    let mut fields = BTreeMap::from([
        ("id".to_string(), Value::String(node.id.clone())),
        ("label".to_string(), Value::String(node.label.clone())),
        ("kind".to_string(), Value::String(node.kind.clone())),
        ("language".to_string(), Value::String(node.language.clone())),
        ("path".to_string(), Value::String(node.path.clone())),
        (
            "qualified_name".to_string(),
            Value::String(node.qualified_name.clone()),
        ),
        ("scope_id".to_string(), Value::String(node.scope_id.clone())),
        ("line_start".to_string(), optional_i64(node.line_start)),
        ("line_end".to_string(), optional_i64(node.line_end)),
        ("byte_start".to_string(), optional_i64(node.byte_start)),
        ("byte_end".to_string(), optional_i64(node.byte_end)),
        (
            "tree_sitter_node_type".to_string(),
            Value::String(node.tree_sitter_node_type.clone()),
        ),
        (
            "capture_name".to_string(),
            Value::String(node.capture_name.clone()),
        ),
        ("summary".to_string(), Value::String(node.summary.clone())),
        ("metadata".to_string(), metadata_object(&node.metadata)),
    ]);
    if let Some(hash) = content_hash {
        fields.insert("content_hash".to_string(), Value::String(hash.to_string()));
    }
    fields
}

fn edge_fields(edge: &GraphEdgeRow) -> StagedRow {
    BTreeMap::from([
        ("id".to_string(), Value::String(edge.id.clone())),
        ("kind".to_string(), Value::String(edge.kind.clone())),
        (
            "source_id".to_string(),
            Value::String(edge.source_id.clone()),
        ),
        (
            "target_id".to_string(),
            Value::String(edge.target_id.clone()),
        ),
        ("confidence".to_string(), json!(1.0)),
        ("line_start".to_string(), Value::Null),
        ("line_end".to_string(), Value::Null),
        ("byte_start".to_string(), Value::Null),
        ("byte_end".to_string(), Value::Null),
        ("metadata".to_string(), metadata_object(&edge.metadata)),
    ])
}

fn metadata_object(value: &str) -> Value {
    match serde_json::from_str::<Value>(value) {
        Ok(Value::Object(object)) => Value::Object(object),
        _ => json!({}),
    }
}

fn optional_i64(value: Option<i64>) -> Value {
    value.map(Value::from).unwrap_or(Value::Null)
}

fn merge_staged_row(rows: &mut RowsById, row_id: String, incoming: StagedRow) {
    let Some(existing) = rows.get_mut(&row_id) else {
        rows.insert(row_id, incoming);
        return;
    };

    for (key, value) in incoming {
        if json_value_is_empty(&value) {
            continue;
        }
        let should_replace = match existing.get(&key) {
            Some(current) => json_value_is_empty(current),
            None => true,
        };
        if should_replace {
            existing.insert(key, value);
        }
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

fn write_json_rows<'a>(
    path: &Path,
    rows: impl Iterator<Item = &'a StagedRow>,
) -> Result<(), NativeError> {
    let mut writer = BufWriter::new(File::create(path)?);
    writer.write_all(b"[")?;
    for (row_index, row) in rows.enumerate() {
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
    rows: impl Iterator<Item = &'a ConnectorRow>,
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
    use crate::protocol::{ManifestEntry, OntologySchema};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_typed_rows_and_connectors_without_bulk_protocol() {
        let staging_dir = temp_staging_dir("typed_rows_and_connectors");
        let request = request(&staging_dir);
        let partition = partition(
            "hash-1",
            vec![
                node("file:one", "File", "file.py"),
                node("sym:one", "Symbol", "foo"),
            ],
            vec![edge("edge:one", "Contains", "file:one", "sym:one")],
        );

        let result = write_partitions(&request, &[partition]).unwrap();

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
        let request = request(&staging_dir);
        let mut first = node("sym:one", "Symbol", "");
        first.label.clear();
        first.summary = "first-summary".to_string();
        let mut second = node("sym:one", "Symbol", "foo");
        second.label = "second-label".to_string();
        second.summary = "second-summary".to_string();
        let partition = partition("hash-1", vec![first, second], Vec::new());

        let result = write_partitions(&request, &[partition]).unwrap();

        assert_eq!(result.node_rows, 1);
        let symbol_rows = read_json_array(&staging_dir.join("symbol.json"));
        assert_eq!(symbol_rows[0]["label"], "second-label");
        assert_eq!(symbol_rows[0]["summary"], "first-summary");
    }

    #[test]
    fn connector_generation_requires_existing_endpoints() {
        let staging_dir = temp_staging_dir("missing_endpoint");
        let request = request(&staging_dir);
        let partition = partition(
            "hash-1",
            vec![node("file:one", "File", "file.py")],
            vec![edge("edge:one", "Contains", "file:one", "sym:missing")],
        );

        let error = write_partitions(&request, &[partition]).unwrap_err();

        assert!(error
            .to_string()
            .contains("edge edge:one references missing target node sym:missing"));
    }

    fn request(staging_dir: &Path) -> NativeSyntaxMaterializationRequest {
        NativeSyntaxMaterializationRequest {
            source_root: ".".to_string(),
            repository_label: "repo".to_string(),
            mode: "full".to_string(),
            parser_version: "test".to_string(),
            manifest_schema_version: 1,
            ontology: "default".to_string(),
            ontology_schema: OntologySchema::default(),
            previous_manifest: None,
            profiles: Vec::new(),
            excluded_parts: Vec::new(),
            db_path: ":memory:".to_string(),
            include_fts: false,
            schema_statements: Vec::new(),
            staging_dir: staging_dir.to_string_lossy().to_string(),
            atomic_rebuild: false,
            strict: false,
        }
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
            metadata: "{}".to_string(),
        }
    }

    fn edge(id: &str, edge_type: &str, source_id: &str, target_id: &str) -> GraphEdgeRow {
        GraphEdgeRow {
            id: id.to_string(),
            edge_type: edge_type.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            kind: "contains".to_string(),
            metadata: "{}".to_string(),
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
