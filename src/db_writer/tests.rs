use super::{
    is_transient_database_error, retry_transient_database, write_database, LadybugWriteRequest,
    WRITE_RETRY_POLICY,
};
use crate::api::graph_read::count_graph_nodes;
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
    result.expect("native writer should execute JSON COPY through Ladybug");
    assert_eq!(count_graph_nodes(&db_path).unwrap(), 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn native_writer_loads_multiple_chunks_into_one_primary_key_table() {
    let root = unique_temp_dir("codebase-graph-chunked-lbug");
    fs::create_dir_all(&root).unwrap();
    let db_path = root.join("graph.lbug");
    let first_json_path = root.join("thing.json");
    let second_json_path = root.join("thing__000001.json");
    fs::write(&first_json_path, r#"[{"id":"one","label":"One"}]"#).unwrap();
    fs::write(&second_json_path, r#"[{"id":"two","label":"Two"}]"#).unwrap();

    let result = write_database(LadybugWriteRequest {
        db_path: db_path.to_string_lossy().to_string(),
        max_num_threads: 1,
        defer_hash_indexes: true,
        include_fts: false,
        schema_statements: vec![
            "INSTALL json".to_string(),
            "LOAD json".to_string(),
            "CREATE NODE TABLE IF NOT EXISTS `Thing`(\n  `id` STRING PRIMARY KEY,\n  `label` STRING\n)"
                .to_string(),
        ],
        copy_statements: vec![
            format!("COPY `Thing` FROM \"{}\";", copy_path(&first_json_path)),
            format!("COPY `Thing` FROM \"{}\";", copy_path(&second_json_path)),
        ],
    });
    result.expect("native writer should load sequential chunks into one indexed table");
    assert_eq!(count_graph_nodes(&db_path).unwrap(), 2);
    let _ = fs::remove_dir_all(&root);
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

#[test]
fn guarded_database_native_path_rejects_transforms_before_filesystem_effects() {
    let root = unique_temp_dir("guarded-native-path-rejection");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("sentinel"), b"unchanged").unwrap();
    let traversal = root.join("absent").join("..").join("graph.lbug");
    let tilde = PathBuf::from(format!(
        "~/../../{}",
        root.join("tilde-native")
            .join("graph.lbug")
            .strip_prefix("/")
            .unwrap_or(&root)
            .display()
    ));
    for invalid in [
        PathBuf::new(),
        PathBuf::from(":memory:"),
        PathBuf::from("~"),
        PathBuf::from("~other/graph.lbug"),
        PathBuf::from("graph\0.lbug"),
        traversal,
        tilde,
        PathBuf::from(format!("{}/absent/", root.display())),
        PathBuf::from(format!("{}/absent/.", root.display())),
    ] {
        assert!(
            matches!(
                super::open_ladybug_database(&invalid, false),
                Err(NativeError::InvalidInput(_))
            ),
            "unsupported native spelling must be rejected: {invalid:?}"
        );
    }
    assert_eq!(fs::read(root.join("sentinel")).unwrap(), b"unchanged");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn guarded_database_native_path_rejects_full_non_utf8_before_effects() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let root = unique_temp_dir("guarded-native-nonutf8");
    fs::create_dir_all(&root).unwrap();
    // No invalid-byte directory is created: Darwin rejects such a fixture at
    // the filesystem boundary. This exercises Rust-side rejection on Darwin
    // and Linux without pretending to establish a Linux native runtime result.
    let invalid = root
        .join(OsString::from_vec(b"invalid-\xff-parent".to_vec()))
        .join("graph.lbug");
    assert!(matches!(
        super::open_ladybug_database(&invalid, false),
        Err(NativeError::InvalidInput(_))
    ));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn guarded_database_native_path_rejects_unsupported_windows_prefixes() {
    for spelling in [
        r"C:graph.lbug",
        r"\graph.lbug",
        r"\\?\GLOBALROOT\Device\graph.lbug",
        r"\\.\pipe\graph.lbug",
        r"C:\absent\..\graph.lbug",
    ] {
        assert!(matches!(
            super::open_ladybug_database(Path::new(spelling), false),
            Err(NativeError::InvalidInput(_))
        ));
    }
}

#[test]
fn guarded_database_windows_encoding_policy_checks_the_complete_native_path() {
    use super::access::windows_native_encoding_matches;
    for code_page in [0, 437, 932, 1252, 65001] {
        for ascii in [
            r"C:\fixture\graph.lbug",
            r"\\?\C:\fixture\graph.lbug",
            r"\\server\share\fixture\graph.lbug",
            r"\\?\UNC\server\share\fixture\graph.lbug",
        ] {
            assert!(windows_native_encoding_matches(ascii, code_page));
        }
        for unicode in [
            r"C:\café\graph.lbug",
            r"\\?\C:\café\graph.lbug",
            r"\\server\share\café\graph.lbug",
            r"\\?\UNC\server\share\café\graph.lbug",
        ] {
            // Put Unicode in the parent, retaining an ASCII database basename.
            assert_eq!(
                windows_native_encoding_matches(unicode, code_page),
                code_page == 65001
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn guarded_database_windows_preserves_disk_unc_and_canonical_namespace_spelling() {
    for spelling in [
        r"C:\fixture\graph.lbug",
        r"\\server\share\fixture\graph.lbug",
        r"\\?\C:\fixture\graph.lbug",
        r"\\?\UNC\server\share\fixture\graph.lbug",
    ] {
        let path = Path::new(spelling);
        assert_eq!(
            super::access::guarded_native_path(path)
                .unwrap()
                .as_os_str(),
            path.as_os_str()
        );
    }
    for spelling in [
        r"\\?\C:\fixture\.\graph.lbug",
        r"\\?\C:\fixture\..\graph.lbug",
        r"\\?\C:\fixture\\graph.lbug",
        r"\\?\C:\fixture/graph.lbug",
        r"C:\fixture.\graph.lbug",
        r"C:\fixture \graph.lbug",
    ] {
        assert!(super::access::guarded_native_path(Path::new(spelling)).is_err());
    }
}

#[test]
fn relative_native_spelling_is_bound_once_to_an_absolute_path() {
    let relative = Path::new("native-path-fixture.lbug");
    let bound = super::access::guarded_native_path(relative).unwrap();
    assert_eq!(bound, std::env::current_dir().unwrap().join(relative));
    assert!(bound.is_absolute());
}
