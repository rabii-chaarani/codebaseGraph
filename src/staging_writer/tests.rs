use super::StagingAccumulator;
use crate::api::catalog::schema_statements_from_copy_statements;
use crate::db_writer::{write_database, LadybugWriteRequest};
use crate::partition_builder::GraphPartition;
use crate::protocol::ManifestEntry;
use crate::syntax_materializer::{GraphEdgeRow, GraphNodeRow};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn copy_path_normalizes_windows_separators_for_ladybug() {
    let path = Path::new(r#"C:\Users\runner\AppData\Local\Temp\thing "one".json"#);

    assert_eq!(
        super::files::copy_path(path),
        r#"C:/Users/runner/AppData/Local/Temp/thing \"one\".json"#
    );
}

#[test]
fn copy_path_strips_windows_extended_prefix_for_ladybug() {
    let path = Path::new(r#"\\?\C:\Users\runner\AppData\Local\Temp\thing.json"#);

    assert_eq!(
        super::files::copy_path(path),
        "C:/Users/runner/AppData/Local/Temp/thing.json"
    );
}

#[test]
fn csv_field_streaming_escapes_quotes_without_building_a_second_string() {
    let mut output = Vec::new();

    super::files::write_csv_field(&mut output, "naïve,\"value\"\n").unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "\"naïve,\"\"value\"\"\n\""
    );
}

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
fn writes_grammar_provenance_and_ordered_syntax_child_rows() {
    let staging_dir = temp_staging_dir("syntax_structure");
    let mut root = node("syntax:root", "SyntaxCapture", "module");
    root.grammar_version = Some("tree_sitter_rust@0.24.2".to_string());
    let mut child = node("syntax:child", "SyntaxCapture", "function_item");
    child.grammar_version = Some("tree_sitter_rust@0.24.2".to_string());
    let mut syntax_child = edge(
        "edge:syntax-child",
        "SyntaxChild",
        "syntax:root",
        "syntax:child",
    );
    syntax_child.kind = "syntax_child".to_string();
    syntax_child.field_name = Some("body".to_string());
    syntax_child.child_index = Some(0);

    let mut staging = StagingAccumulator::new(&staging_dir.to_string_lossy());
    staging.add_partition(&partition("hash-1", vec![root, child], vec![syntax_child]));
    let result = staging.finish().unwrap();

    assert_eq!(result.edge_rows, 1);
    assert_eq!(result.connector_rows, 2);
    let syntax_rows = read_json_array(&staging_dir.join("syntaxcapture.json"));
    assert!(syntax_rows
        .iter()
        .all(|row| { row["grammar_version"] == "tree_sitter_rust@0.24.2" }));
    let edge_rows = read_json_array(&staging_dir.join("syntaxchild.json"));
    assert_eq!(edge_rows[0]["field_name"], "body");
    assert_eq!(edge_rows[0]["child_index"], 0);
    assert!(staging_dir
        .join("from_syntaxchild__syntaxcapture__syntaxchild.csv")
        .exists());
    assert!(staging_dir
        .join("to_syntaxchild__syntaxchild__syntaxcapture.csv")
        .exists());
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
            node("ref:z", "Reference", "a"),
            node("sym:a", "Symbol", "a"),
            node("file:a", "File", "a.py"),
        ],
        vec![
            edge("edge:b", "Contains", "file:z", "sym:b"),
            edge("edge:a", "Contains", "file:a", "sym:a"),
            edge("edge:r", "References", "ref:z", "sym:a"),
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
            "COPY `Reference`",
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
        .join("from_references__reference__references.csv")
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

#[test]
fn small_budget_forces_spill_without_changing_deterministic_output() {
    let default_dir = temp_staging_dir("default_budget");
    let bounded_dir = temp_staging_dir("bounded_budget");
    let mut nodes = vec![node("file:one", "File", "file.py")];
    let mut edges = Vec::new();
    for index in (0..400).rev() {
        let symbol_id = format!("sym:{index:03}");
        nodes.push(node(&symbol_id, "Symbol", &format!("symbol_{index:03}")));
        edges.push(edge(
            &format!("edge:{index:03}"),
            "Contains",
            "file:one",
            &symbol_id,
        ));
    }
    let graph = partition("hash-spill", nodes, edges);

    let mut default = StagingAccumulator::new(&default_dir.to_string_lossy());
    default.add_partition(&graph);
    let default_result = default.finish().unwrap();

    let mut bounded =
        StagingAccumulator::with_chunk_limit(&bounded_dir.to_string_lossy(), 8_192).unwrap();
    bounded.add_partition(&graph);
    let bounded_result = bounded.finish().unwrap();

    assert_eq!(bounded_result.node_rows, 401);
    assert_eq!(bounded_result.edge_rows, 400);
    assert_eq!(bounded_result.unique_node_count, 401);
    assert_eq!(bounded_result.unique_edge_count, 400);
    assert_eq!(bounded_result.connector_rows, 800);
    assert!(bounded_result.spill_bytes > 0);
    assert!(bounded_result.high_water_bytes > 0);
    assert_eq!(
        unique_statement_tables(&bounded_result.copy_statements),
        unique_statement_tables(&default_result.copy_statements)
    );
    assert_eq!(
        staged_json_rows(&bounded_dir),
        staged_json_rows(&default_dir)
    );
    assert_eq!(staged_csv_rows(&bounded_dir), staged_csv_rows(&default_dir));
    assert!(bounded_dir.join("symbol__000001.json").is_file());
    assert!(fs::read_dir(&bounded_dir)
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with("__000001.csv")
        }));
    for entry in fs::read_dir(&bounded_dir).unwrap().filter_map(Result::ok) {
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            assert!(entry.metadata().unwrap().len() <= 8_192);
        }
    }
    let database_path = bounded_dir.join("chunked-copy.lbdb");
    write_database(LadybugWriteRequest {
        db_path: database_path.to_string_lossy().into_owned(),
        worker_memory_bytes: 768 * 1024 * 1024,
        buffer_pool_bytes: 384 * 1024 * 1024,
        max_num_threads: 1,
        defer_hash_indexes: false,
        include_fts: false,
        schema_statements: schema_statements_from_copy_statements(
            false,
            &bounded_result.copy_statements,
        ),
        copy_statements: bounded_result.copy_statements,
    })
    .unwrap();
}

#[test]
fn single_record_larger_than_budget_fails_structurally() {
    let staging_dir = temp_staging_dir("record_budget");
    let graph = partition(
        "hash-1",
        vec![node("file:one", "File", "file.py")],
        Vec::new(),
    );
    let mut staging =
        StagingAccumulator::with_chunk_limit(&staging_dir.to_string_lossy(), 64).unwrap();
    staging.add_partition(&graph);

    let error = staging.finish().unwrap_err();

    let crate::error::NativeError::MemoryBudgetExceeded(error) = error else {
        panic!("expected a structured memory budget error");
    };
    assert_eq!(error.phase, "staged");
    assert_eq!(error.limit_bytes, 64);
    assert!(error.accounted_bytes > error.limit_bytes);
}

fn partition(
    content_hash: &str,
    nodes: Vec<GraphNodeRow>,
    edges: Vec<GraphEdgeRow>,
) -> GraphPartition {
    let node_count = nodes.len();
    let edge_count = edges.len();
    GraphPartition {
        entry: ManifestEntry {
            path: "file.py".to_string(),
            content_hash: content_hash.to_string(),
            language: "python".to_string(),
            partition_id: "partition".to_string(),
            artifact_key: None,
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
            node_count,
            edge_count,
            materialized_at: "now".to_string(),
        },
        nodes,
        edges,
    }
}

fn unique_statement_tables(statements: &[String]) -> Vec<&str> {
    let mut tables = Vec::new();
    for table in statements
        .iter()
        .map(|statement| statement.split(" FROM ").next().unwrap())
    {
        if tables.last().copied() != Some(table) {
            tables.push(table);
        }
    }
    tables
}

fn staged_json_rows(root: &Path) -> BTreeMap<String, Vec<Value>> {
    let mut paths = fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut rows = BTreeMap::<String, Vec<Value>>::new();
    for path in paths {
        let stem = path.file_stem().unwrap().to_string_lossy();
        let table = unchunked_stem(&stem).to_string();
        rows.entry(table)
            .or_default()
            .extend(read_json_array(&path));
    }
    rows
}

fn staged_csv_rows(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut paths = fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "csv"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut rows = BTreeMap::<String, Vec<String>>::new();
    for path in paths {
        let stem = path.file_stem().unwrap().to_string_lossy();
        let table = unchunked_stem(&stem).to_string();
        rows.entry(table).or_default().extend(
            fs::read_to_string(path)
                .unwrap()
                .lines()
                .skip(1)
                .map(str::to_string),
        );
    }
    rows
}

fn unchunked_stem(stem: &str) -> &str {
    stem.rsplit_once("__")
        .filter(|(_, suffix)| suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or(stem, |(base, _)| base)
}

fn node(id: &str, table: &str, label: &str) -> GraphNodeRow {
    GraphNodeRow {
        id: id.to_string(),
        table: table.to_string(),
        label: label.to_string(),
        kind: label.to_string(),
        language: "python".to_string(),
        grammar_version: None,
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
        field_name: None,
        child_index: None,
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
