use std::process::Command;

#[test]
fn health_rejects_retired_confirmation_and_omits_it_from_help() {
    let binary = env!("CARGO_BIN_EXE_codebase-graph");
    let help = Command::new(binary)
        .args(["check-health", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(!text.contains("confirm-legacy-quiescence"), "{text}");

    let root = std::env::temp_dir().join(format!(
        "cgraph-restore-flags-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let rejected = Command::new(binary)
        .current_dir(&root)
        .args(["check-health", "--confirm-legacy-quiescence"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    let error = String::from_utf8_lossy(&rejected.stderr);
    assert!(error.contains("confirm-legacy-quiescence"), "{error}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_build_and_health_leave_unowned_historical_marker_untouched() {
    let binary = env!("CARGO_BIN_EXE_codebase-graph");
    let root = std::env::temp_dir().join(format!(
        "cgraph-restore-marker-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let state = root.join(".codebaseGraph");
    let storage = state.join("storage");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn retained() {}\n").unwrap();
    std::fs::write(
        state.join("config.json"),
        serde_json::to_vec(&serde_json::json!({"schema_version": 2, "repo_root": root})).unwrap(),
    )
    .unwrap();
    let marker = storage.join(".reference-scope.json");
    let original = b"unowned historical marker; leave unchanged";
    std::fs::write(&marker, original).unwrap();

    let build = Command::new(binary)
        .current_dir(&root)
        .args(["build", "--repo-root"])
        .arg(&root)
        .args(["--mode", "full", "--no-git", "--no-fts", "--json"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let health = Command::new(binary)
        .current_dir(&root)
        .args(["check-health", "--repo-root"])
        .arg(&root)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        health.status.success(),
        "{}",
        String::from_utf8_lossy(&health.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&health.stdout).unwrap();
    assert_eq!(value["graph_readable"], true);
    assert!(value.get("maintenance").is_none());
    assert_eq!(std::fs::read(&marker).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}
