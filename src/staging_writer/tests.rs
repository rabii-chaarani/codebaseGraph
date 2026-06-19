use super::StagingAccumulator;
use crate::graph_rows::{GraphEdgeRow, GraphNodeRow};
use crate::partition_builder::GraphPartition;
use crate::protocol::ManifestEntry;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
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
    let to_csv = fs::read_to_string(staging_dir.join("to_contains__contains__symbol.csv")).unwrap();
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
        confidence: 1.0,
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        metadata: json!({}),
    }
}

fn temp_staging_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("codebase_graph_staging_{name}_{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn read_json_array(path: &Path) -> Vec<Value> {
    let content = fs::read_to_string(path).unwrap();
    serde_json::from_str(&content).unwrap()
}
