use super::*;

#[test]
fn install_skips_materialization_when_graph_state_already_exists() {
    let root = unique_temp_dir("codebase-graph-rust-install-idempotent");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();

    fs::write(root.join("service.py"), "def helper():\n    return 2\n").unwrap();
    let mut output = Vec::new();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["database_written"], false);
    assert_eq!(value["materialization"]["skipped"], true);
    assert_eq!(
        value["materialization"]["skip_reason"],
        "existing_graph_state"
    );
    assert_eq!(value["storage_format"], "managed_v2");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_writes_managed_v2_config_without_static_database_or_manifest_paths() {
    let root = unique_temp_dir("codebase-graph-rust-install-managed-v2");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    let mut output = Vec::new();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".codebaseGraph").join("config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["schema_version"], 2);
    assert!(config.get("database_path").is_none());
    assert!(config.get("manifest_path").is_none());
    let expected_storage_root =
        fs::canonicalize(root.join(".codebaseGraph").join("storage")).unwrap();
    let configured_storage_root = fs::canonicalize(
        config["storage_root"]
            .as_str()
            .expect("config should contain storage_root"),
    )
    .unwrap();
    assert_eq!(configured_storage_root, expected_storage_root);
    assert_eq!(value["storage_format"], "managed_v2");
    assert_eq!(value["writable"], true);
    let response_storage_root = fs::canonicalize(
        value["storage_root"]
            .as_str()
            .expect("response should contain storage_root"),
    )
    .unwrap();
    assert_eq!(response_storage_root, expected_storage_root);
    assert!(value["db_path"]
        .as_str()
        .unwrap()
        .contains("/.codebaseGraph/storage/generations/gen-"));
    assert!(value["manifest_path"]
        .as_str()
        .unwrap()
        .contains("/.codebaseGraph/storage/generations/gen-"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_rejects_legacy_v1_state_until_reinstall() {
    let root = unique_temp_dir("codebase-graph-rust-install-legacy-v1");
    let state = root.join(".codebaseGraph");
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("config.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "repo_root": root,
            "database_path": state.join("legacy.ldb"),
            "manifest_path": state.join("legacy-manifest.json"),
        }))
        .unwrap(),
    )
    .unwrap();

    let error = run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap_err();

    assert!(error.contains("legacy installed graph storage requires reinstall before writes"));
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
    assert!(error.contains(&format!(
        "codebase-graph reinstall --repo-root {}",
        canonical_root.display()
    )));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reinstall_recreates_graph_state_and_materializes_again() {
    let root = unique_temp_dir("codebase-graph-rust-reinstall");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();

    fs::write(root.join("service.py"), "def helper():\n    return 2\n").unwrap();
    let mut output = Vec::new();
    run(
        [
            "reinstall",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["state"]["action"], "backed_up");
    assert_eq!(value["install"]["database_written"], true);
    assert_eq!(value["install"]["storage_format"], "managed_v2");
    assert!(root.join(".codebaseGraph").join("config.json").exists());
    let backup_path = PathBuf::from(value["state"]["backup_path"].as_str().unwrap());
    assert_eq!(
        backup_path
            .parent()
            .and_then(|path| path.canonicalize().ok())
            .or_else(|| backup_path.parent().map(PathBuf::from)),
        root.parent()
            .and_then(|path| path.canonicalize().ok())
            .or_else(|| root.parent().map(PathBuf::from))
    );
    assert!(!backup_path.starts_with(&root));
    assert!(!backup_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reinstall_dry_run_leaves_existing_graph_state() {
    let root = unique_temp_dir("codebase-graph-rust-reinstall-dry-run");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "reinstall",
            "--repo-root",
            root.to_str().unwrap(),
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--dry-run",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["state"]["action"], "dry_run");
    assert_eq!(value["install"]["database_written"], false);
    assert!(root.join(".codebaseGraph").exists());
    let backup_path = PathBuf::from(value["state"]["backup_path"].as_str().unwrap());
    assert!(!backup_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reinstall_preserves_unrelated_mcp_entries() {
    let root = unique_temp_dir("codebase-graph-rust-reinstall-mcp");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
    let client_config = root.join("client").join("mcp.json");
    fs::create_dir_all(client_config.parent().unwrap()).unwrap();
    fs::write(
        &client_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "codebase_graph": {"command": "old", "args": []},
                "other_server": {"command": "other", "args": ["keep"]}
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "reinstall",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "generic",
            "--mcp-config-path",
            client_config.to_str().unwrap(),
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["install"]["mcp_config"]["action"], "updated");
    let client_payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
    assert_eq!(
        client_payload["mcpServers"]["other_server"]["args"][0],
        "keep"
    );
    assert_eq!(
        client_payload["mcpServers"]["codebase_graph"]["args"][0],
        "mcp"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_install_writes_generic_client_config() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-install");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
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
        &mut Vec::new(),
    )
    .unwrap();
    let client_config = root.join("client").join("mcp.json");
    let config_path = root.join(".codebaseGraph").join("config.json");
    let mut output = Vec::new();
    run(
        [
            "mcp",
            "install",
            "--client",
            "generic",
            "--config-path",
            config_path.to_str().unwrap(),
            "--client-config-path",
            client_config.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["action"], "created");
    assert_eq!(value["method"], "file_adapter");
    let server_name = value["server_name"].as_str().unwrap();
    assert!(server_name.starts_with("codebase_graph_codebase-graph-rust-mcp-install"));
    assert!(client_config.exists());
    let client_payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
    assert_eq!(client_payload["mcpServers"][server_name]["args"][0], "mcp");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_install_reports_copilot_studio_metadata() {
    let root = unique_temp_dir("codebase-graph-rust-copilot-install");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
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
        &mut Vec::new(),
    )
    .unwrap();
    let config_path = root.join(".codebaseGraph").join("config.json");
    let mut output = Vec::new();
    run(
        [
            "mcp",
            "install",
            "--client",
            "copilot-studio",
            "--config-path",
            config_path.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["action"], "reported");
    assert_eq!(value["method"], "manual_metadata");
    assert_eq!(value["payload"]["http"]["url"], "http://127.0.0.1:8765/mcp");
    assert_eq!(value["payload"]["stdio"]["type"], "stdio");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn uninstall_removes_repo_state_instruction_blocks_and_matching_mcp_entry() {
    let root = unique_temp_dir("codebase-graph-rust-uninstall");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
    let instruction_text =
        "before\n\n<!-- codebaseGraph:start -->\nmanaged\n<!-- codebaseGraph:end -->\n\nafter\n";
    fs::write(root.join("AGENTS.md"), instruction_text).unwrap();
    fs::write(root.join("CLAUDE.md"), instruction_text).unwrap();
    let client_config = root.join("client").join("mcp.json");
    fs::create_dir_all(client_config.parent().unwrap()).unwrap();
    fs::write(
        &client_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "codebase_graph": {"command": "codebase-graph", "args": ["mcp", "start"]},
                "other_server": {"command": "other", "args": []}
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "uninstall",
            "--repo-root",
            root.to_str().unwrap(),
            "--mcp-client",
            "generic",
            "--client-config-path",
            client_config.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["server_name"], "codebase_graph");
    assert_eq!(value["state"]["action"], "removed");
    assert!(!root.join(".codebaseGraph").exists());
    for file_name in ["AGENTS.md", "CLAUDE.md"] {
        let text = fs::read_to_string(root.join(file_name)).unwrap();
        assert!(!text.contains("codebaseGraph:start"));
        assert!(text.contains("before"));
        assert!(text.contains("after"));
    }
    let client_payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
    assert!(client_payload["mcpServers"].get("codebase_graph").is_none());
    assert!(client_payload["mcpServers"].get("other_server").is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn uninstall_dry_run_reports_without_removing_files() {
    let root = unique_temp_dir("codebase-graph-rust-uninstall-dry-run");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
    fs::write(
        root.join("AGENTS.md"),
        "<!-- codebaseGraph:start -->\nmanaged\n<!-- codebaseGraph:end -->\n",
    )
    .unwrap();
    let client_config = root.join("client").join("mcp.json");
    fs::create_dir_all(client_config.parent().unwrap()).unwrap();
    fs::write(
        &client_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "codebase_graph": {"command": "codebase-graph", "args": ["mcp", "start"]}
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "uninstall",
            "--repo-root",
            root.to_str().unwrap(),
            "--mcp-client",
            "generic",
            "--client-config-path",
            client_config.to_str().unwrap(),
            "--dry-run",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["state"]["action"], "dry_run");
    assert_eq!(value["mcp_clients"][0]["action"], "dry_run");
    assert!(root.join(".codebaseGraph").exists());
    assert!(fs::read_to_string(root.join("AGENTS.md"))
        .unwrap()
        .contains("codebaseGraph:start"));
    let client_payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
    assert!(client_payload["mcpServers"].get("codebase_graph").is_some());
    let _ = fs::remove_dir_all(root);
}
