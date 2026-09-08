use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_codebase-graph")
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

fn run_cli(cwd: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn native_request(root: &Path, db: &Path, staging: &Path) -> PathBuf {
    let request_path = root.join("native-request.json");
    let payload = json!({
        "source_root": root,
        "repository_label": "native-request-identity",
        "mode": "full",
        "parser_version": "native-test",
        "manifest_schema_version": 1,
        "ontology": "code_ontology_v1",
        "previous_manifest": null,
        "profiles": [],
        "excluded_parts": [],
        "db_path": db,
        "include_fts": false,
        "semantic_enrichment": false,
        "semantic_provider_mode": "local_only",
        "schema_statements": [],
        "staging_dir": staging,
        "atomic_rebuild": true,
        "strict": true,
    });
    fs::write(&request_path, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    request_path
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn standalone_native_request_uses_its_source_root_from_an_unrelated_cwd() {
    let root = unique_temp_dir("codebase-graph-native-request-source");
    let unrelated = unique_temp_dir("codebase-graph-native-request-cwd");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(root.join("expected.rs"), "pub fn expected() {}\n").unwrap();
    fs::write(unrelated.join("decoy.rs"), "pub fn decoy() {}\n").unwrap();
    let db = root.join("graph.ldb");
    let manifest = root.join("manifest.json");
    let staging = root.join("staging");
    let request = native_request(&root, &db, &staging);

    let output = run_cli(
        &unrelated,
        &[
            "build",
            "--native-request",
            request.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--no-git",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
    );
    assert_success(&output);
    assert!(manifest.exists());
    assert!(db.exists());
    assert!(!unrelated.join(".codebaseGraph/manifest.json").exists());
    assert!(!unrelated
        .join(".codebaseGraph/storage/active.json")
        .exists());
    let manifest_value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert!(manifest_value["files"]["expected.rs"].is_object());
    assert!(manifest_value["files"].get("decoy.rs").is_none());

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(unrelated);
}

#[test]
fn bound_native_request_source_mismatch_is_rejected_before_direct_publication() {
    let source_root = unique_temp_dir("codebase-graph-native-request-bound-source");
    let bound_root = unique_temp_dir("codebase-graph-native-request-bound-repo");
    let unrelated = unique_temp_dir("codebase-graph-native-request-bound-cwd");
    fs::create_dir_all(&source_root).unwrap();
    fs::create_dir_all(&bound_root).unwrap();
    fs::create_dir_all(&unrelated).unwrap();
    let db = bound_root.join("graph.ldb");
    let manifest = bound_root.join("manifest.json");
    let staging = source_root.join("staging");
    let request = native_request(&source_root, &db, &staging);
    fs::write(&db, b"existing-db").unwrap();
    fs::write(&manifest, b"existing-manifest").unwrap();

    let output = run_cli(
        &unrelated,
        &[
            "build",
            "--native-request",
            request.to_str().unwrap(),
            "--repo-root",
            bound_root.to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--no-git",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
    );
    assert!(!output.status.success());
    let error = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(error.contains("source root") && error.contains("does not match"));
    assert_eq!(fs::read(&db).unwrap(), b"existing-db");
    assert_eq!(fs::read(&manifest).unwrap(), b"existing-manifest");
    assert!(!bound_root
        .join(".codebaseGraph/storage/active.json")
        .exists());

    let _ = fs::remove_dir_all(source_root);
    let _ = fs::remove_dir_all(bound_root);
    let _ = fs::remove_dir_all(unrelated);
}
