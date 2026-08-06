use k_wiki::adapters::{
    cli::{self, CliAction, CliRequest, ValidationProfile},
    http,
    install::McpInstallRequest,
    mcp::{self, McpSession},
    TransportError, TransportPayload,
};
use serde_json::json;
use std::{cell::Cell, io::Cursor, path::PathBuf, rc::Rc};

#[test]
fn cli_parses_phase8_commands() {
    assert_eq!(
        cli::parse_args(&[
            "install".to_string(),
            "--repo-root".to_string(),
            "repository".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::Install {
            repo_root: Some(PathBuf::from("repository")),
        })
    );

    assert_eq!(
        cli::parse_args(&[
            "mcp".to_string(),
            "install".to_string(),
            "--client".to_string(),
            "generic".to_string(),
            "--repo-root".to_string(),
            "repository".to_string(),
            "--scope".to_string(),
            "project".to_string(),
            "--name".to_string(),
            "repository_wiki".to_string(),
            "--client-config-path".to_string(),
            "client.json".to_string(),
            "--dry-run".to_string(),
            "--verify".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::InstallMcp {
            request: McpInstallRequest {
                client: "generic".to_string(),
                scope: "project".to_string(),
                name: Some("repository_wiki".to_string()),
                client_config_path: Some(PathBuf::from("client.json")),
                repo_root: Some(PathBuf::from("repository")),
                dry_run: true,
                verify: true,
            },
        })
    );

    assert!(cli::parse_args(&["mcp".to_string(), "install".to_string()])
        .unwrap_err()
        .contains("--client is required"));
    assert_eq!(
        cli::parse_args(&["--version".to_string()]).unwrap(),
        CliAction::Version
    );
    assert!(cli::parse_args(&[
        "mcp".to_string(),
        "install".to_string(),
        "--client".to_string(),
        "unknown".to_string(),
    ])
    .unwrap_err()
    .contains("unsupported MCP client"));
    assert!(cli::parse_args(&[
        "mcp".to_string(),
        "install".to_string(),
        "--client".to_string(),
        "generic".to_string(),
        "--scope".to_string(),
        "invalid".to_string(),
    ])
    .unwrap_err()
    .contains("MCP install scope must be local, user, or project"));

    assert_eq!(
        cli::parse_args(&[
            "validate".to_string(),
            "fixtures/comprehensive".to_string(),
            "--profile".to_string(),
            "conformant".to_string(),
            "--json".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::Validate {
            bundle: PathBuf::from("fixtures/comprehensive"),
            profile: ValidationProfile::Conformant,
            json: true,
        })
    );

    assert_eq!(
        cli::parse_args(&[
            "build".to_string(),
            "fixtures/minimal".to_string(),
            "--out".to_string(),
            "dist".to_string(),
            "--base-url".to_string(),
            "/wiki".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::Build {
            bundle: PathBuf::from("fixtures/minimal"),
            out: PathBuf::from("dist"),
            base_url: Some("/wiki".to_string()),
        })
    );

    assert_eq!(
        cli::parse_args(&[
            "serve".to_string(),
            "fixtures/minimal".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            "4444".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::Serve {
            bundle: PathBuf::from("fixtures/minimal"),
            options: http::HttpServeOptions {
                host: "127.0.0.1".to_string(),
                port: 4444,
            },
        })
    );

    assert_eq!(
        cli::parse_args(&[
            "inspect".to_string(),
            "fixtures/minimal".to_string(),
            "--concept".to_string(),
            "decisions/adr-001".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::Inspect {
            bundle: PathBuf::from("fixtures/minimal"),
            concept_id: "decisions/adr-001".to_string(),
        })
    );

    assert_eq!(
        cli::parse_args(&[
            "check-links".to_string(),
            "fixtures/minimal".to_string(),
            "--include-external".to_string(),
        ])
        .unwrap(),
        CliAction::Request(CliRequest::CheckLinks {
            bundle: PathBuf::from("fixtures/minimal"),
            include_external: true,
        })
    );
}

#[test]
fn cli_run_dispatches_exactly_once() {
    let calls = Rc::new(Cell::new(0));
    let mut stdout = Cursor::new(Vec::new());
    let mut stderr = Cursor::new(Vec::new());
    let tracker = Rc::clone(&calls);

    let exit = cli::run(
        &[
            "inspect".to_string(),
            "fixtures/minimal".to_string(),
            "--concept".to_string(),
            "decisions/adr-001".to_string(),
        ],
        &mut stdout,
        &mut stderr,
        move |_request| {
            tracker.set(tracker.get() + 1);
            Ok(TransportPayload::text("ok"))
        },
    )
    .unwrap();

    assert_eq!(exit, 0);
    assert_eq!(calls.get(), 1);
}

#[test]
fn cli_run_emits_structured_validation_output_when_json_is_requested() {
    let mut stdout = Cursor::new(Vec::new());
    let mut stderr = Cursor::new(Vec::new());
    let exit = cli::run(
        &[
            "validate".to_string(),
            "fixtures/minimal".to_string(),
            "--json".to_string(),
        ],
        &mut stdout,
        &mut stderr,
        |_request| {
            Ok(TransportPayload::structured(
                "human summary",
                json!({"accepted": true, "diagnostics": []}),
            ))
        },
    )
    .unwrap();

    assert_eq!(exit, 0);
    assert!(stderr.into_inner().is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stdout.into_inner()).unwrap(),
        json!({"accepted": true, "diagnostics": []})
    );
}

#[test]
fn cli_run_reports_the_packaged_version_without_dispatching() {
    let mut stdout = Cursor::new(Vec::new());
    let mut stderr = Cursor::new(Vec::new());
    let exit = cli::run(
        &["--version".to_string()],
        &mut stdout,
        &mut stderr,
        |_request| panic!("version output must not dispatch a request"),
    )
    .unwrap();

    assert_eq!(exit, 0);
    assert!(stderr.into_inner().is_empty());
    assert_eq!(
        String::from_utf8(stdout.into_inner()).unwrap(),
        format!("k-wiki {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn serve_rejects_remote_binding_without_security_change() {
    let error =
        http::HttpServeOptions::parse(&["--host".to_string(), "0.0.0.0".to_string()]).unwrap_err();
    assert!(error.contains("localhost"));
}

#[test]
fn http_security_headers_and_safe_errors_follow_phase8_contract() {
    let headers = http::security_headers();
    assert_eq!(headers.len(), 5);
    assert_eq!(headers[1].1, "nosniff");
    assert_eq!(headers[2].1, "no-referrer");
    assert_eq!(headers[3].1, "DENY");
    assert_eq!(headers[4].1, "no-store");

    let error = TransportError::new(
        "invalid_request",
        "bundle at /Users/rabii/private/repo must stay inside /Users/rabii/allowed",
    )
    .with_details(json!({
        "bundle": "/Users/rabii/private/repo",
        "reason": "traversal",
        "url": "https://example.com/docs"
    }));
    let (status, body) = http::safe_error_response(&error);
    let payload = body.0;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(payload["error"]["code"], "invalid_request");
    assert_eq!(
        payload["error"]["message"],
        "bundle at [redacted-path] must stay inside [redacted-path]"
    );
    assert_eq!(payload["error"]["details"]["bundle"], "[redacted-path]");
    assert_eq!(
        payload["error"]["details"]["url"],
        "https://example.com/docs"
    );
}

#[test]
fn mcp_initialize_uses_exact_server_display_name() {
    let mut session = McpSession::default();
    let response = mcp::handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-11-25"}
        }),
        &mut session,
        &mut |_tool| json!({"type": "object", "additionalProperties": false}),
        &mut |_tool, _arguments| Ok(TransportPayload::text("unused")),
    )
    .unwrap();

    assert_eq!(response["result"]["serverInfo"]["name"], "Knowledge Wiki");
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn mcp_tools_list_advertises_declared_metadata() {
    let mut session = McpSession {
        protocol_version: Some("2025-11-25".to_string()),
        initialized: true,
    };
    let response = mcp::handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
        &mut session,
        &mut |tool| {
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {"tool": {"const": tool}}
            })
        },
        &mut |_tool, _arguments| Ok(TransportPayload::text("unused")),
    )
    .unwrap();

    let tools = response["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "wiki_list_bundles",
            "wiki_list_directory",
            "wiki_get_concept",
            "wiki_search_concepts",
            "wiki_get_backlinks",
            "wiki_get_neighborhood",
            "wiki_get_diagnostics",
            "wiki_get_recent_changes",
            "wiki_create_bundle",
            "wiki_create_page",
            "wiki_populate_page",
            "wiki_validate",
            "wiki_check_links",
            "wiki_build",
        ]
    );
    assert_eq!(tools[0]["title"], "List Bundles");
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[8]["title"], "Create Bundle");
    assert_eq!(tools[8]["annotations"]["readOnlyHint"], false);
    assert_eq!(
        tools[8]["inputSchema"]["properties"]["tool"]["const"],
        "wiki_create_bundle"
    );
    assert_eq!(tools[11]["title"], "Validate Bundle");
    assert_eq!(tools[11]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[12]["title"], "Check Links");
    assert_eq!(tools[13]["title"], "Build Site");
    assert_eq!(tools[13]["annotations"]["readOnlyHint"], false);
}

#[test]
fn mcp_tool_calls_dispatch_exactly_once_and_preserve_error_codes() {
    let mut session = McpSession {
        protocol_version: Some("2025-11-25".to_string()),
        initialized: true,
    };
    let calls = Rc::new(Cell::new(0));
    let tracker = Rc::clone(&calls);
    let success = mcp::handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "wiki_search_concepts",
                "arguments": {"include_structured_content": true}
            }
        }),
        &mut session,
        &mut |_tool| json!({"type": "object", "additionalProperties": false}),
        &mut move |_tool, _arguments| {
            tracker.set(tracker.get() + 1);
            Ok(TransportPayload::structured(
                "search ok",
                json!({"results": [{"id": "guides/overview"}]}),
            ))
        },
    )
    .unwrap();

    assert_eq!(calls.get(), 1);
    assert_eq!(success["result"]["isError"], false);
    assert_eq!(
        success["result"]["structuredContent"]["results"][0]["id"],
        "guides/overview"
    );

    let unknown = mcp::handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "wiki_missing_tool",
                "arguments": {}
            }
        }),
        &mut session,
        &mut |_tool| json!({"type": "object", "additionalProperties": false}),
        &mut |_tool, _arguments| Ok(TransportPayload::text("unused")),
    )
    .unwrap();
    assert_eq!(unknown["error"]["code"], -32602);
    assert!(unknown["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown Knowledge Wiki MCP tool"));
}
