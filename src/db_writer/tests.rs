use super::{
    is_transient_database_error, retry_transient_database, write_database, LadybugWriteRequest,
    WRITE_RETRY_POLICY,
};
use crate::error::NativeError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn native_writer_loads_json_staging_through_ladybug_copy() {
    let root = unique_temp_dir("codebase-graph-native-lbug");
    fs::create_dir_all(&root).unwrap();
    let db_path = root.join("graph.lbug");
    let json_path = root.join("thing.json");
    let relationship_path = root.join("links.csv");
    fs::write(
        &json_path,
        r#"[{"id":"one","label":"One","metadata":{"answer":42}},{"id":"two","label":"Two","metadata":{}}]"#,
    )
    .unwrap();
    fs::write(&relationship_path, "from_id,to_id\none,two\n").unwrap();

    let result = write_database(LadybugWriteRequest {
        db_path: db_path.to_string_lossy().to_string(),
        worker_memory_bytes: 768 * 1024 * 1024,
        buffer_pool_bytes: 128 * 1024 * 1024,
        max_num_threads: 1,
        defer_hash_indexes: true,
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
            "CREATE REL TABLE IF NOT EXISTS `FROM_Links`(FROM `Thing` TO `Thing`)".to_string(),
        ],
        copy_statements: vec![
            format!("COPY `Thing` FROM \"{}\";", copy_path(&json_path)),
            format!(
                "COPY `FROM_Links` FROM \"{}\" (header=true, from=\"from_id\", to=\"to_id\");",
                copy_path(&relationship_path)
            ),
        ],
    });
    let _ = fs::remove_dir_all(&root);
    result.expect("native writer should execute JSON COPY through Ladybug");
}

#[test]
fn transient_database_error_detection_matches_ladybug_lock_failures() {
    assert!(is_transient_database_error(
        "IO exception: Could not set lock on file graph.ldb (Lock is held by PID 123)"
    ));
    assert!(is_transient_database_error(
        "Couldn't replay shadow pages under read-only mode"
    ));
    assert!(!is_transient_database_error(
        "Copy exception: Found duplicated primary key value"
    ));
}

#[test]
fn transient_database_retry_replays_operation_until_success() {
    let attempts = AtomicUsize::new(0);
    retry_transient_database(WRITE_RETRY_POLICY, || {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(NativeError::Database(
                "IO exception: Could not set lock on file".to_string(),
            ))
        } else {
            Ok(())
        }
    })
    .unwrap();

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
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
