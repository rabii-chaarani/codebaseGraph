use super::*;

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
