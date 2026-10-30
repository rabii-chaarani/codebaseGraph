#![cfg(debug_assertions)]

use codebase_graph::api::{
    CodebaseGraphApi, OperationRequest, OutputFormat, QueryRequest, RepoSelector, SearchRequest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")))
}

struct Fixture {
    root: PathBuf,
    direct: bool,
}

impl Fixture {
    fn new(direct: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "copy-phase-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        if !direct {
            fs::create_dir(root.join(".codebaseGraph")).unwrap();
            fs::write(
                root.join(".codebaseGraph/config.json"),
                serde_json::to_vec(&json!({"schema_version":2,"repo_root":root})).unwrap(),
            )
            .unwrap();
        } else {
            fs::create_dir(root.join("output")).unwrap();
        }
        Self { root, direct }
    }

    fn selector(&self) -> RepoSelector {
        RepoSelector {
            repo_root: Some(self.root.clone()),
            db_path: self.direct.then(|| self.root.join("output/graph.ldb")),
            manifest_path: self.direct.then(|| self.root.join("output/manifest.json")),
            ..RepoSelector::default()
        }
    }

    fn source(&self, label: &str) {
        fs::write(
            self.root.join("src/lib.rs"),
            format!("pub fn {label}() {{}}\n"),
        )
        .unwrap();
    }

    fn build(&self, trace: &Path, spill_mib: u64) -> Output {
        let mut command = Command::new(binary());
        command
            .current_dir(&self.root)
            .args(["build", "--repo-root"])
            .arg(&self.root)
            .args([
                "--mode",
                "full",
                "--no-git",
                "--single-thread",
                "--spill-chunk-mib",
            ])
            .arg(spill_mib.to_string())
            .arg("--json")
            .env("CODEBASE_GRAPH_TEST_COPY_RESULT_DIR", trace);
        if self.direct {
            command
                .arg("--db")
                .arg(self.root.join("output/graph.ldb"))
                .arg("--manifest")
                .arg(self.root.join("output/manifest.json"));
        }
        command.output().unwrap()
    }

    fn paths(&self) -> (PathBuf, PathBuf) {
        if self.direct {
            return (
                self.root.join("output/graph.ldb"),
                self.root.join("output/manifest.json"),
            );
        }
        let pointer: Value = serde_json::from_slice(
            &fs::read(self.root.join(".codebaseGraph/storage/active.json")).unwrap(),
        )
        .unwrap();
        let generation = self
            .root
            .join(".codebaseGraph/storage/generations")
            .join(format!(
                "gen-{}",
                pointer["generation_id"].as_str().unwrap()
            ));
        (
            generation.join("graph.ldb"),
            generation.join("manifest.json"),
        )
    }

    fn query(&self, statement: &str) -> Vec<Vec<Value>> {
        let result = CodebaseGraphApi::new()
            .execute_operation(&OperationRequest::Query(QueryRequest {
                repo: self.selector(),
                statement: statement.into(),
                parameters: json!({}),
                limit: 1000,
                output_format: OutputFormat::Typed,
            }))
            .unwrap();
        serde_json::from_value(result.payload["rows"].clone()).unwrap()
    }

    fn search(&self, label: &str) -> Vec<String> {
        let result = CodebaseGraphApi::new()
            .execute_operation(&OperationRequest::Search(SearchRequest {
                repo: self.selector(),
                query: label.into(),
                layer: "semantic".into(),
                profile: "brief".into(),
                limit: 10,
                budget: 0,
                context_limit: 0,
                max_depth: None,
                detail: "slim".into(),
                output_format: OutputFormat::Typed,
            }))
            .unwrap();
        result.payload["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|row| row["label"].as_str().map(str::to_owned))
            .collect()
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "CLI failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn traces(directory: &Path) -> Vec<Value> {
    // The writer waits for each child; successful phase records append in execution order.
    fs::read_to_string(directory.join("phases.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn process_instance(event: &Value) -> Result<&str, &'static str> {
    if !event["pid"]
        .as_u64()
        .is_some_and(|pid| pid > 0 && pid <= u32::MAX as u64)
    {
        return Err("phase trace has no valid diagnostic PID");
    }
    let instance = event["process_instance"]
        .as_str()
        .ok_or("phase trace has no process instance")?;
    if instance.len() != 64
        || !instance
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("phase trace has an invalid process instance");
    }
    Ok(instance)
}

fn distinct_phase_processes(events: &[&Value]) -> Result<(), &'static str> {
    if events.len() < 2 {
        return Err("process isolation needs multiple phase records");
    }
    let mut instances = BTreeSet::new();
    for event in events {
        if !instances.insert(process_instance(event)?) {
            return Err("multiple phases executed in one process instance");
        }
    }
    Ok(())
}

#[test]
fn phase_process_oracle_accepts_reused_pid_but_rejects_reused_process() {
    let first = json!({"pid": 42, "process_instance": "a".repeat(64)});
    let next = json!({"pid": 42, "process_instance": "b".repeat(64)});
    assert!(distinct_phase_processes(&[&first, &next]).is_ok());
    assert!(distinct_phase_processes(&[&first, &first]).is_err());
    let changed_pid = json!({"pid": 43, "process_instance": "a".repeat(64)});
    assert!(distinct_phase_processes(&[&first, &changed_pid]).is_err());
}

#[test]
fn phase_process_oracle_rejects_missing_or_invalid_evidence() {
    let valid = json!({"pid": 42, "process_instance": "a".repeat(64)});
    assert!(distinct_phase_processes(&[]).is_err());
    assert!(distinct_phase_processes(&[&valid]).is_err());
    for invalid in [
        json!({"pid": 43}),
        json!({"process_instance": "b".repeat(64)}),
        json!({"pid": 0, "process_instance": "b".repeat(64)}),
        json!({"pid": "43", "process_instance": "b".repeat(64)}),
        json!({"pid": 43, "process_instance": ""}),
        json!({"pid": 43, "process_instance": "B".repeat(64)}),
        json!({"pid": 43, "process_instance": "g".repeat(64)}),
    ] {
        assert!(distinct_phase_processes(&[&valid, &invalid]).is_err());
    }
}

fn bundle_hashes(db: &Path, manifest: &Path) -> BTreeMap<PathBuf, String> {
    let mut paths = vec![db.to_path_buf(), manifest.to_path_buf()];
    let value: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
    if let Some(files) = value["search_backend"]["files"].as_object() {
        paths.extend(
            files
                .keys()
                .map(|suffix| PathBuf::from(format!("{}.{}", db.display(), suffix))),
        );
    }
    paths
        .into_iter()
        .map(|path| {
            let digest = format!("{:x}", Sha256::digest(fs::read(&path).unwrap()));
            (path, digest)
        })
        .collect()
}

#[test]
fn native_copy_acknowledgement_failure_preserves_managed_and_direct_bundles() {
    for direct in [false, true] {
        let fixture = Fixture::new(direct);
        fixture.source("stable_marker");
        assert_success(&fixture.build(&fixture.root.join("initial-trace"), 32));
        let (db, manifest) = fixture.paths();
        let before = bundle_hashes(&db, &manifest);
        assert!(fixture
            .search("stable_marker")
            .contains(&"stable_marker".to_owned()));

        fixture.source("failed_marker");
        let failed_trace = fixture.root.join("failed-trace");
        fs::create_dir(&failed_trace).unwrap();
        fs::write(
            failed_trace.join("FAIL_AFTER_COPY"),
            b"fault only after real COPY and checkpoint",
        )
        .unwrap();
        let failed = fixture.build(&failed_trace, 32);
        assert!(!failed.status.success());
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&failed.stdout),
            String::from_utf8_lossy(&failed.stderr)
        );
        assert!(
            diagnostic.contains("COPY result acknowledgement failed"),
            "{diagnostic}"
        );
        assert!(
            !diagnostic.contains("non-empty primary-key node table"),
            "first error was replaced: {diagnostic}"
        );
        let events = traces(&failed_trace);
        assert_eq!(
            events
                .iter()
                .map(|event| event["phase_kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["schema", "copy"]
        );
        assert!(events.iter().all(|event| event["success"] == true));
        assert_eq!(events[1]["copy_index"], 0);
        assert!(events[1].get("buffer_pool_bytes").is_none());
        assert!(events[1].get("worker_memory_bytes").is_none());
        assert!(events[1]["staged_file_bytes"].as_u64().unwrap() > 0);
        assert_eq!(distinct_phase_processes(&[&events[0], &events[1]]), Ok(()));
        assert_eq!(fixture.paths(), (db.clone(), manifest.clone()));
        assert_eq!(bundle_hashes(&db, &manifest), before);
        assert!(fixture
            .search("stable_marker")
            .contains(&"stable_marker".to_owned()));
        assert!(!fixture
            .search("failed_marker")
            .contains(&"failed_marker".to_owned()));

        fixture.source("recovered_marker");
        let recovery_trace = fixture.root.join("recovery-trace");
        assert_success(&fixture.build(&recovery_trace, 32));
        assert!(fixture
            .search("recovered_marker")
            .contains(&"recovered_marker".to_owned()));
        assert!(!fixture
            .search("failed_marker")
            .contains(&"failed_marker".to_owned()));
        let recovered_events = traces(&recovery_trace);
        assert_eq!(
            distinct_phase_processes(&[&recovered_events[0], &events[0]]),
            Ok(())
        );
        fs::remove_dir_all(&fixture.root).unwrap();
    }
}

fn string_rows(connection: &lbug::Connection<'_>, statement: &str) -> Vec<Vec<String>> {
    connection
        .query(statement)
        .unwrap()
        .map(|row| {
            row.into_iter()
                .map(|value| match value {
                    lbug::Value::String(value) => value,
                    other => panic!("expected string result, got {other:?}"),
                })
                .collect()
        })
        .collect()
}

#[test]
fn jumbo_staged_row_survives_real_copy_and_fresh_process_read() {
    let fixture = Fixture::new(true);
    let padding = "x".repeat(2 * 1024 * 1024);
    fs::write(
        fixture.root.join("src/lib.rs"),
        format!("/*{padding}*/\npub fn jumbo_record_marker() {{}}\n"),
    )
    .unwrap();
    let trace = fixture.root.join("jumbo-trace");
    assert_success(&fixture.build(&trace, 1));
    assert!(traces(&trace)
        .iter()
        .any(|event| event["phase_kind"] == "copy"
            && event["staged_file_bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 1024 * 1024)));
    let (db_path, _) = fixture.paths();
    let database = lbug::Database::new(
        &db_path,
        lbug::SystemConfig::default()
            .read_only(true)
            .max_num_threads(1),
    )
    .unwrap();
    let connection = lbug::Connection::new(&database).unwrap();
    let rows = string_rows(&connection, "MATCH (n:SyntaxCapture) WHERE n.tree_sitter_node_type='block_comment' RETURN CAST(n.metadata AS STRING)");
    let metadata: Value = serde_json::from_str(&rows[0][0]).unwrap();
    assert!(metadata["canonical_key"]
        .as_str()
        .unwrap()
        .contains(&padding));
    drop(connection);
    drop(database);
    assert!(fixture
        .search("jumbo_record_marker")
        .contains(&"jumbo_record_marker".to_owned()));
    fs::remove_dir_all(fixture.root).unwrap();
}

fn logical_snapshot(connection: &lbug::Connection<'_>) -> BTreeMap<String, Vec<Vec<String>>> {
    [
        (
            "node rows and JSON",
            "MATCH (n) RETURN label(n), n.id, CAST(n AS STRING) ORDER BY label(n), n.id",
        ),
        (
            "connectors",
            "MATCH (a)-[r]->(b) RETURN label(r), a.id, b.id ORDER BY label(r), a.id, b.id",
        ),
    ]
    .into_iter()
    .map(|(name, statement)| (name.to_owned(), string_rows(connection, statement)))
    .collect()
}

#[test]
fn bounded_production_chunks_reopen_with_primary_keys_connectors_and_search() {
    let fixture = Fixture::new(true);
    let mut body = String::new();
    for index in 0..1400 {
        body.push_str(&format!(
            "pub fn chunk_symbol_{index:04}() -> usize {{ {index} }}\n"
        ));
    }
    fs::write(fixture.root.join("src/lib.rs"), body).unwrap();
    let trace = fixture.root.join("chunk-trace");
    let built = fixture.build(&trace, 1);
    assert_success(&built);
    let response: Value = serde_json::from_slice(&built.stdout).unwrap();
    eprintln!(
        "chunked native build resources: {}",
        response["phase_high_water_marks"]
    );
    let events = traces(&trace);
    let copies = events
        .iter()
        .filter(|event| event["phase_kind"] == "copy")
        .collect::<Vec<_>>();
    let mut repeated = BTreeMap::<String, Vec<usize>>::new();
    for (position, event) in events.iter().enumerate() {
        assert_eq!(event["max_num_threads"], 1);
        if event["phase_kind"] != "copy" {
            continue;
        }
        let target = event["target"]
            .as_str()
            .unwrap()
            .trim_matches('`')
            .to_owned();
        let bytes = event["staged_file_bytes"].as_u64().unwrap();
        assert!(bytes > 0 && bytes <= 1024 * 1024, "{event}");
        assert!(
            event["staged_file_path"]
                .as_str()
                .unwrap()
                .ends_with(".json")
                || event["staged_file_path"]
                    .as_str()
                    .unwrap()
                    .ends_with(".csv")
        );
        if !target.starts_with("FROM_") && !target.starts_with("TO_") {
            repeated.entry(target).or_default().push(position);
        }
    }
    assert!(
        repeated
            .values()
            .filter(|positions| positions.len() >= 2)
            .count()
            >= 2,
        "production must rotate multiple node/edge-node tables"
    );
    for (table, positions) in &repeated {
        if positions.len() < 2 {
            continue;
        }
        assert!(
            events[positions[0] + 1..positions[1]]
                .iter()
                .any(|event| event["phase_kind"] == "index" && event["target"] == *table),
            "no early PK index for {table}"
        );
    }
    let first_connector = events
        .iter()
        .position(|event| {
            event["target"]
                .as_str()
                .is_some_and(|table| table.starts_with("`FROM_") || table.starts_with("`TO_"))
        })
        .unwrap();
    assert!(!events[first_connector..]
        .iter()
        .any(|event| event["phase_kind"] == "index"));
    assert_eq!(distinct_phase_processes(&copies), Ok(()));

    let (db_path, _) = fixture.paths();
    let database = lbug::Database::new(
        &db_path,
        lbug::SystemConfig::default()
            .read_only(true)
            .max_num_threads(1),
    )
    .unwrap();
    let connection = lbug::Connection::new(&database).unwrap();
    let functions = string_rows(
        &connection,
        "MATCH (n:Function) RETURN n.id, n.label ORDER BY n.label",
    );
    assert_eq!(functions.len(), 1400);
    assert_eq!(
        functions
            .iter()
            .map(|row| row[1].clone())
            .collect::<BTreeSet<_>>(),
        (0..1400)
            .map(|index| format!("chunk_symbol_{index:04}"))
            .collect()
    );
    let mut lookup = connection
        .prepare("MATCH (n:Function {id:$id}) RETURN n.id, n.label")
        .unwrap();
    for expected in &functions {
        let mut rows = connection
            .execute(
                &mut lookup,
                vec![("id", lbug::Value::String(expected[0].clone()))],
            )
            .unwrap();
        assert_eq!(
            rows.next().unwrap(),
            vec![
                lbug::Value::String(expected[0].clone()),
                lbug::Value::String(expected[1].clone())
            ]
        );
        assert!(rows.next().is_none());
    }
    for (table, positions) in &repeated {
        if positions.len() < 2 {
            continue;
        }
        let ids = string_rows(
            &connection,
            &format!("MATCH (n:`{table}`) RETURN n.id ORDER BY n.id"),
        );
        assert!(!ids.is_empty());
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
        let mut keyed = connection
            .prepare(&format!("MATCH (n:`{table}` {{id:$id}}) RETURN n.id"))
            .unwrap();
        for expected in ids {
            let mut result = connection
                .execute(
                    &mut keyed,
                    vec![("id", lbug::Value::String(expected[0].clone()))],
                )
                .unwrap();
            assert_eq!(
                result.next().unwrap(),
                vec![lbug::Value::String(expected[0].clone())]
            );
            assert!(result.next().is_none());
        }
    }
    let plan = format!(
        "{:?}",
        connection
            .query("EXPLAIN MATCH (n:Function {id:'missing'}) RETURN n.id")
            .unwrap()
    );
    eprintln!("PK lookup plan: {plan}");
    assert!(
        plan.contains("PRIMARY_KEY_SCAN_NODE_TABLE"),
        "lookup did not use the primary-key operator: {plan}"
    );
    let endpoints = string_rows(&connection, "MATCH (m:Module)-[:FROM_Defines]->(e:Defines)-[:TO_Defines]->(f:Function) RETURN m.label, f.id, f.label ORDER BY f.label");
    assert_eq!(endpoints.len(), 1400);
    assert!(endpoints.iter().all(|row| row[0] == "src.lib"));
    assert_eq!(
        endpoints
            .iter()
            .map(|row| row[1..].to_vec())
            .collect::<Vec<_>>(),
        functions
    );
    let chunked_snapshot = logical_snapshot(&connection);
    drop(lookup);
    drop(connection);
    drop(database);
    let labels = ["chunk_symbol_0000", "chunk_symbol_1399"];
    let chunked_search = labels
        .iter()
        .map(|label| fixture.search(label))
        .collect::<Vec<_>>();
    for (label, found) in labels.iter().zip(&chunked_search) {
        assert!(found.contains(&(*label).to_owned()));
    }
    assert_eq!(
        fixture.query("MATCH (n:Function) RETURN count(n)")[0][0],
        1400
    );

    // Same source paths and content, fresh candidate, only the existing spill setting differs.
    let control_trace = fixture.root.join("control-trace");
    assert_success(&fixture.build(&control_trace, 32));
    let (control_db_path, _) = fixture.paths();
    let control_database = lbug::Database::new(
        &control_db_path,
        lbug::SystemConfig::default()
            .read_only(true)
            .max_num_threads(1),
    )
    .unwrap();
    let control_connection = lbug::Connection::new(&control_database).unwrap();
    assert_eq!(logical_snapshot(&control_connection), chunked_snapshot);
    drop(control_connection);
    drop(control_database);
    for (label, expected) in labels.iter().zip(chunked_search) {
        assert_eq!(fixture.search(label), expected);
    }
    fs::remove_dir_all(fixture.root).unwrap();
}
