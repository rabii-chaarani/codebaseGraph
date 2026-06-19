use super::*;

#[test]
fn prints_top_level_help() {
    let mut output = Vec::new();
    run(["--help"], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph native CLI"));
    assert!(text.contains("materialize"));
}

#[test]
fn prints_top_level_help_without_args() {
    let mut output = Vec::new();
    run(std::iter::empty::<&str>(), &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph native CLI"));
    assert!(text.contains("mcp"));
}

#[test]
fn materialize_help_is_product_command_help() {
    let mut output = Vec::new();
    run(["materialize", "--help"], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph materialize"));
    assert!(text.contains("--native-request"));
    assert!(text.contains("local_only only"));
    assert!(!text.contains("opportunistic"));
    assert!(!text.contains("provider_first"));
}

#[test]
fn setup_help_is_product_command_help() {
    let mut output = Vec::new();
    run(["setup", "--help"], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph setup"));
    assert!(text.contains("--mcp-client"));
    assert!(text.contains("local_only only"));
    assert!(!text.contains("opportunistic"));
    assert!(!text.contains("provider_first"));
}

#[test]
fn materialize_rejects_provider_backed_semantic_modes() {
    let args = vec![
        "--semantic-provider-mode".to_string(),
        "provider_first".to_string(),
    ];
    let error = MaterializeOptions::parse(&args).unwrap_err();

    assert!(error.contains("--semantic-provider-mode must be local_only"));
}

#[test]
fn setup_rejects_provider_backed_semantic_modes() {
    let args = vec![
        "--semantic-provider-mode".to_string(),
        "opportunistic".to_string(),
    ];
    let error = SetupOptions::parse(&args).unwrap_err();

    assert!(error.contains("--semantic-provider-mode must be local_only"));
}

#[test]
fn materialize_empty_project_from_native_request() {
    let root = unique_temp_dir("codebase-graph-native-cli");
    fs::create_dir_all(&root).unwrap();
    let request_path = root.join("request.json");
    let manifest_path = root.join("manifest.json");
    let db_path = root.join("graph.lbug");
    let staging_dir = root.join("staging");
    fs::write(
        &request_path,
        format!(
            r#"{{
  "source_root": "{root}",
  "repository_label": "empty",
  "mode": "full",
  "parser_version": "native-test",
  "manifest_schema_version": 1,
  "ontology": "code_ontology_v1",
  "previous_manifest": null,
  "profiles": [],
  "excluded_parts": [],
  "db_path": "{db}",
  "include_fts": false,
  "semantic_enrichment": false,
  "semantic_provider_mode": "local_only",
  "schema_statements": [],
  "staging_dir": "{staging}",
  "atomic_rebuild": true,
  "strict": true
}}"#,
            root = json_path(&root),
            db = json_path(&db_path),
            staging = json_path(&staging_dir),
        ),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "materialize",
            "--native-request",
            request_path.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["skipped"], true);
    assert!(manifest_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn materialize_python_source_root_without_python_request() {
    let root = unique_temp_dir("codebase-graph-rust-source-root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    let db_path = root.join(".codebaseGraph").join("graph.ldb");
    let manifest_path = root.join(".codebaseGraph").join("manifest.json");

    let mut output = Vec::new();
    run(
        [
            "materialize",
            "--source-root",
            root.to_str().unwrap(),
            "--db",
            db_path.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--mode",
            "full",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["database_written"], true);
    assert_eq!(value["skipped"], false);
    assert!(db_path.exists());
    assert!(manifest_path.exists());
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert!(manifest["files"].get("service.py").is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn plan_lists_rebuild_delete_skip_and_ignore_paths() {
    let root = unique_temp_dir("codebase-graph-rust-plan");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    fs::write(root.join("old.py"), "def old():\n    return 1\n").unwrap();
    fs::write(root.join("notes.txt"), "not source\n").unwrap();
    fs::write(root.join("ignored.py"), "def ignored():\n    return 1\n").unwrap();
    fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
    setup_fixture_repo(&root);

    fs::write(root.join("service.py"), "def helper():\n    return 2\n").unwrap();
    fs::write(root.join("new.py"), "def new():\n    return 3\n").unwrap();
    fs::remove_file(root.join("old.py")).unwrap();

    let mut output = Vec::new();
    run(
        [
            "plan",
            "--source-root",
            root.to_str().unwrap(),
            "--no-git",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_json_array_contains(&value["would_rebuild"], "new.py");
    assert_json_array_contains(&value["would_rebuild"], "service.py");
    assert_json_array_contains(&value["would_delete"], "old.py");
    assert_json_array_contains(&value["would_skip"], "notes.txt");
    assert_json_array_contains(&value["ignored_paths"], "ignored.py");
    assert_eq!(value["database_written"], false);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn materialize_honors_config_excludes() {
    let root = unique_temp_dir("codebase-graph-rust-config-excludes");
    fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
    fs::write(root.join("keep.py"), "def keep():\n    return 1\n").unwrap();
    fs::write(root.join("skip.py"), "def skip():\n    return 1\n").unwrap();
    fs::write(
        root.join(".codebaseGraph").join("config.json"),
        r#"{"materialization":{"exclude":["skip.py"]}}"#,
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "plan",
            "--source-root",
            root.to_str().unwrap(),
            "--no-git",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_json_array_contains(&value["would_rebuild"], "keep.py");
    assert_json_array_contains(&value["ignored_paths"], "skip.py");
    assert!(!json_array_contains(&value["would_rebuild"], "skip.py"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn git_diff_plan_scopes_to_changed_paths() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let root = unique_temp_dir("codebase-graph-rust-git-diff");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.py"), "def a():\n    return 1\n").unwrap();
    fs::write(root.join("b.py"), "def b():\n    return 1\n").unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "initial",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    setup_fixture_repo(&root);
    run(
        [
            "materialize",
            "--source-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();

    fs::write(root.join("a.py"), "def a():\n    return 2\n").unwrap();
    let mut output = Vec::new();
    run(
        [
            "plan",
            "--source-root",
            root.to_str().unwrap(),
            "--git-diff",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_json_array_contains(&value["would_rebuild"], "a.py");
    assert!(!json_array_contains(&value["would_rebuild"], "b.py"));
    assert!(!json_array_contains(&value["would_delete"], "b.py"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parallel_materialize_reports_progress_events() {
    let root = unique_temp_dir("codebase-graph-rust-progress");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.py"), "def a():\n    return 1\n").unwrap();
    fs::write(root.join("b.py"), "def b():\n    return 1\n").unwrap();

    let mut output = Vec::new();
    run(
        [
            "materialize",
            "--source-root",
            root.to_str().unwrap(),
            "--no-git",
            "--parallel",
            "--progress",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["database_written"], true);
    assert!(value["progress_events"].as_array().unwrap().len() >= 2);
    assert_eq!(value["diff"]["added"][0], "a.py");
    assert_eq!(value["diff"]["added"][1], "b.py");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn setup_materializes_graph_and_writes_config() {
    let root = unique_temp_dir("codebase-graph-rust-setup");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    let mut output = Vec::new();
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["database_written"], true);
    assert!(root.join(".codebaseGraph").join("config.json").exists());
    assert!(root.join(".codebaseGraph").join("manifest.json").exists());
    assert!(PathBuf::from(value["database_path"].as_str().unwrap()).exists());

    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".codebaseGraph").join("config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["schema_version"], 1);
    assert_eq!(config["mcp"]["server_name"], "codebase_graph");
    let _ = fs::remove_dir_all(root);
}
