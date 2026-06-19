use super::{partition_delete_statements, write_database, LadybugWriteRequest};
use crate::protocol::{ManifestDiff, ManifestEntry, NativeManifest};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn native_writer_loads_json_staging_through_ladybug_copy() {
    let root = unique_temp_dir("codebase-graph-native-lbug");
    fs::create_dir_all(&root).unwrap();
    let db_path = root.join("graph.lbug");
    let json_path = root.join("thing.json");
    fs::write(
        &json_path,
        r#"[{"id":"one","label":"One","metadata":{"answer":42}}]"#,
    )
    .unwrap();

    let result = write_database(LadybugWriteRequest {
        db_path: db_path.to_string_lossy().to_string(),
        include_fts: false,
        schema_statements: vec![
            "INSTALL json".to_string(),
            "LOAD json".to_string(),
            "CREATE NODE TABLE IF NOT EXISTS `Thing`(
  `id` STRING PRIMARY KEY,
  `label` STRING,
  `metadata` JSON
)"
            .to_string(),
        ],
        replace_database: false,
        delete_statements: Vec::new(),
        copy_statements: vec![format!("COPY `Thing` FROM \"{}\";", copy_path(&json_path))],
    });
    let _ = fs::remove_dir_all(&root);
    result.expect("native writer should execute JSON COPY through Ladybug");
}

#[test]
fn partition_delete_statements_skip_retained_shared_ids() {
    let mut files = BTreeMap::new();
    files.insert(
        "changed.py".to_string(),
        entry(
            &["Function:changed", "Symbol:shared"],
            &[
                ("Function:changed", "Function"),
                ("Symbol:shared", "Symbol"),
            ],
            &["Contains:changed", "References:shared"],
            &[
                ("Contains:changed", "Contains"),
                ("References:shared", "References"),
            ],
        ),
    );
    files.insert(
        "unchanged.py".to_string(),
        entry(
            &["Function:unchanged", "Symbol:shared"],
            &[
                ("Function:unchanged", "Function"),
                ("Symbol:shared", "Symbol"),
            ],
            &["References:shared"],
            &[("References:shared", "References")],
        ),
    );
    let manifest = NativeManifest {
        schema_version: 1,
        ontology: "code_ontology_v1".to_string(),
        parser_version: "test".to_string(),
        files,
    };
    let statements = partition_delete_statements(
        Some(&manifest),
        &ManifestDiff {
            added: Vec::new(),
            modified: vec!["changed.py".to_string()],
            unchanged: vec!["unchanged.py".to_string()],
            deleted: Vec::new(),
            force_rebuild: false,
        },
    );
    let joined = statements.join("\n");

    assert!(joined.contains("Function:changed"));
    assert!(joined.contains("Contains:changed"));
    assert!(!joined.contains("Symbol:shared"));
    assert!(!joined.contains("References:shared"));
}

fn entry(
    node_ids: &[&str],
    node_types: &[(&str, &str)],
    edge_ids: &[&str],
    edge_types: &[(&str, &str)],
) -> ManifestEntry {
    ManifestEntry {
        path: "path.py".to_string(),
        content_hash: "hash".to_string(),
        language: "python".to_string(),
        partition_id: "partition".to_string(),
        node_ids: node_ids.iter().map(|value| value.to_string()).collect(),
        edge_ids: edge_ids.iter().map(|value| value.to_string()).collect(),
        node_types: node_types
            .iter()
            .map(|(id, table)| (id.to_string(), table.to_string()))
            .collect(),
        edge_types: edge_types
            .iter()
            .map(|(id, table)| (id.to_string(), table.to_string()))
            .collect(),
        materialized_at: "now".to_string(),
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn copy_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"")
}
