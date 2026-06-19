use super::*;

#[test]
fn mcp_install_writes_generic_client_config() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-install");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
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
