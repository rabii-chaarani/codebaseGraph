use super::*;

#[test]
fn mcp_graph_query_binds_json_parameters() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-query-params");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    setup_fixture_repo(&root);

    let options = McpServeOptions {
        repo_root: Some(root.clone()),
        config: None,
        db: None,
        manifest: None,
        refresh: None,
    };
    let result = mcp_call_tool_result(
        "graph_query",
        &json!({
            "statement": "MATCH (n) WHERE n.path = $path RETURN n.path LIMIT 1",
            "parameters": {"path": "service.py"},
            "output_format": "json",
            "include_structured_content": true,
        }),
        &options,
    )
    .unwrap();

    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["row_count"], 1);
    assert_eq!(result["structuredContent"]["rows"][0][0], "service.py");
    let text_payload: serde_json::Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text_payload["rows"][0][0], "service.py");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_serves_tools_and_tool_errors() {
    let root = unique_temp_dir("codebase-graph-rust-mcp");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();
    setup_search_fixture_repo(&root);

    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-11-25"},
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "graph_health",
                "arguments": {"include_structured_content": true},
            },
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "graph_search",
                "arguments": {
                    "query": "SampleService",
                    "limit": 2,
                    "output_format": "json",
                },
            },
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "graph_query",
                "arguments": {
                    "statement": "MATCH (n) DELETE n",
                    "include_structured_content": true,
                },
            },
        }),
    ];
    let input = requests
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
        + "\n";
    let options = McpServeOptions {
        repo_root: Some(root.clone()),
        config: None,
        db: None,
        manifest: None,
        refresh: None,
    };
    let mut output = Vec::new();
    serve_mcp_stdio(&options, std::io::Cursor::new(input), &mut output).unwrap();
    let responses: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "graph_health"));
    assert!(tools.iter().any(|tool| tool["name"] == "graph_search"));
    assert!(tools.iter().any(|tool| tool["name"] == "graph_query"));
    assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]
        .get("output_format")
        .is_some()));
    assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]
        .get("include_structured_content")
        .is_some()));

    assert_eq!(responses[2]["result"]["isError"], false);
    assert_eq!(responses[2]["result"]["structuredContent"]["ok"], true);
    assert_eq!(
        responses[2]["result"]["structuredContent"]["refresh"]["enabled"],
        true
    );
    assert!(responses[2]["result"]["structuredContent"]["refresh"]["backend"].is_string());
    assert!(responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("health ok=true"));

    let search_text = responses[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let search_payload: serde_json::Value = serde_json::from_str(search_text).unwrap();
    assert!(search_payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|hit| hit["label"] == "SampleService"));

    assert_eq!(responses[4]["result"]["isError"], true);
    assert_eq!(
        responses[4]["result"]["structuredContent"]["error"]["tool"],
        "graph_query"
    );
    assert!(responses[4]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("error tool=graph_query type=ValueError"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_http_rejects_remote_bind_without_auth_token() {
    let error = McpHttpOptions::parse(&[
        "--host".to_string(),
        "0.0.0.0".to_string(),
        "--allow-remote".to_string(),
    ])
    .unwrap_err();
    assert!(error.contains("auth token"));

    let local_error =
        McpHttpOptions::parse(&["--host".to_string(), "0.0.0.0".to_string()]).unwrap_err();
    assert!(local_error.contains("localhost"));
}

#[test]
fn mcp_http_handles_initialize_list_call_and_protocol_errors() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-http");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();
    setup_search_fixture_repo(&root);

    let options = test_http_options(root.clone(), None);
    let mut state = McpHttpState::default();
    let initialize = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[("mcp-protocol-version", "2025-11-25")],
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(initialize.status, 200);
    assert_eq!(
        initialize.payload["result"]["protocolVersion"],
        "2025-11-25"
    );
    let session_id = initialize
        .headers
        .iter()
        .find(|(name, _)| name == "Mcp-Session-Id")
        .map(|(_, value)| value.clone())
        .unwrap();

    let missing_session = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[("mcp-protocol-version", "2025-11-25")],
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        ),
    );
    assert_eq!(missing_session.status, 400);
    assert_eq!(missing_session.payload["error"]["code"], -32002);

    let listed = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("mcp-protocol-version", "2025-11-25"),
                ("mcp-session-id", session_id.as_str()),
            ],
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}}),
        ),
    );
    assert_eq!(listed.status, 200);
    assert!(listed.payload["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "graph_context"));

    let health = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("mcp-protocol-version", "2025-11-25"),
                ("mcp-session-id", session_id.as_str()),
            ],
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "graph_health", "arguments": {}},
            }),
        ),
    );
    assert_eq!(health.status, 200);
    assert_eq!(health.payload["result"]["isError"], false);
    assert!(health.payload["result"]["structuredContent"].is_null());
    assert!(health.payload["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("health ok=true "));

    let protocol_error = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("mcp-protocol-version", "1900-01-01"),
                ("mcp-session-id", session_id.as_str()),
            ],
            json!({"jsonrpc": "2.0", "id": 5, "method": "ping", "params": {}}),
        ),
    );
    assert_eq!(protocol_error.status, 400);
    assert_eq!(protocol_error.payload["error"]["code"], -32602);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_http_enforces_bearer_token_when_configured() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-http-auth");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    setup_search_fixture_repo(&root);

    let options = test_http_options(root.clone(), Some("secret"));
    let mut state = McpHttpState::default();
    let missing = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[("origin", "http://127.0.0.1:8765")],
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(missing.status, 401);

    let wrong = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("origin", "http://127.0.0.1:8765"),
                ("authorization", "Bearer wrong"),
            ],
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(wrong.status, 401);

    let ok = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("origin", "http://127.0.0.1:8765"),
                ("authorization", "Bearer secret"),
            ],
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(ok.status, 200);
    assert_eq!(ok.payload["result"]["protocolVersion"], "2025-11-25");

    let _ = fs::remove_dir_all(root);
}
