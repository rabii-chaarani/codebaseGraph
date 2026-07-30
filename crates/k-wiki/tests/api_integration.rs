use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use okf_wiki::{
    adapters::{http, mcp},
    api::mcp_operation_descriptor,
    authoring::{
        AuthoringConfig, AuthoringService, BundleRoot, NoopRefreshNotifier, NoopValidator,
        RepositoryRoot,
    },
    service::LocalWikiService,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn mcp_authoring_round_trip_creates_populates_reads_and_searches_a_concept() {
    let temp = TestDir::new("okf-wiki-mcp-round-trip");
    let repository = temp.path().join("repository");
    let bundle = repository.join("docs");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
    )
    .expect("write root index");

    let authoring = AuthoringService::new(
        AuthoringConfig {
            repositories: vec![RepositoryRoot {
                id: "repo".into(),
                root_path: repository,
            }],
            bundles: vec![BundleRoot {
                id: "docs".into(),
                repository_id: "repo".into(),
                root_path: bundle.clone(),
            }],
        },
        NoopValidator,
        NoopRefreshNotifier,
    )
    .expect("configure authoring");
    let api = LocalWikiService::new(vec![bundle])
        .with_authoring(authoring)
        .into_api();
    let mut session = mcp_session();

    let created = call_tool(
        &api,
        &mut session,
        1,
        "wiki_create_page",
        json!({
            "bundle_id": "docs",
            "page_path": "guides/getting-started",
            "type": "guide",
            "title": "Getting Started",
            "tags": ["onboarding"],
            "body_markdown": "Initial body.",
            "include_structured_content": true
        }),
    );
    assert_eq!(created["result"]["isError"], false);
    let content_hash = created["result"]["structuredContent"]["result"]["content_hash"]
        .as_str()
        .expect("created content hash");

    let populated = call_tool(
        &api,
        &mut session,
        2,
        "wiki_populate_page",
        json!({
            "bundle_id": "docs",
            "page_path": "guides/getting-started",
            "frontmatter": {
                "type": "guide",
                "title": "Getting Started",
                "tags": ["onboarding"],
                "extensions": {"owner": "platform"}
            },
            "body_markdown": "# Getting Started\n\nFollow the onboarding path.",
            "expected_content_hash": content_hash,
            "include_structured_content": true
        }),
    );
    assert_eq!(populated["result"]["isError"], false);

    let concept = call_tool(
        &api,
        &mut session,
        3,
        "wiki_get_concept",
        json!({
            "bundle_id": "docs",
            "concept_id": "guides/getting-started",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        concept["result"]["structuredContent"]["result"]["id"],
        "guides/getting-started"
    );

    let search = call_tool(
        &api,
        &mut session,
        4,
        "wiki_search_concepts",
        json!({
            "text": "Getting Started",
            "bundle_id": "docs",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        search["result"]["structuredContent"]["result"][0]["concept_id"],
        "guides/getting-started"
    );
}

#[test]
fn cli_validate_and_build_use_the_integrated_public_api() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("tests/fixtures/comprehensive");
    let temp = TestDir::new("okf-wiki-cli");
    let output = temp.path().join("site");
    let binary = env!("CARGO_BIN_EXE_okf-wiki");

    let validation = Command::new(binary)
        .args([
            "validate",
            fixture.to_str().expect("fixture path"),
            "--profile",
            "consume",
            "--json",
        ])
        .output()
        .expect("run validation");
    assert!(
        validation.status.success(),
        "{}",
        String::from_utf8_lossy(&validation.stderr)
    );
    let validation_payload: Value =
        serde_json::from_slice(&validation.stdout).expect("validation should emit JSON");
    assert_eq!(validation_payload["kind"], "validation");
    assert!(validation_payload["result"]["accepted"].is_boolean());

    let build = Command::new(binary)
        .args([
            "build",
            fixture.to_str().expect("fixture path"),
            "--out",
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run build");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(output.join("index.html").is_file());
}

#[test]
fn mcp_stdio_binary_advertises_the_packaged_knowledge_wiki_schema() {
    let temp = TestDir::new("okf-wiki-mcp-binary");
    let bundle = temp.path().join("docs");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
    )
    .expect("write bundle index");

    let mut child = Command::new(env!("CARGO_BIN_EXE_okf-wiki"))
        .arg("mcp")
        .arg(&bundle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start MCP binary");
    {
        let mut stdin = child.stdin.take().expect("MCP stdin");
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"}
            })
        )
        .expect("write initialize");
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        )
        .expect("write tools list");
    }

    let output = child.wait_with_output().expect("wait for MCP binary");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .expect("UTF-8 MCP output")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC response"))
        .collect::<Vec<_>>();
    assert_eq!(
        responses[0]["result"]["serverInfo"]["name"],
        "Knowledge Wiki"
    );
    let tools = responses[1]["result"]["tools"]
        .as_array()
        .expect("tool list");
    assert_eq!(tools.len(), 11);
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "wiki_populate_page"
            && tool["annotations"]["wikiAccess"] == "write"));
}

#[tokio::test]
async fn preview_http_dispatches_health_and_serves_static_content_with_security_headers() {
    let temp = TestDir::new("okf-wiki-http");
    let bundle = temp.path().join("docs");
    let site = temp.path().join("site");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::create_dir_all(&site).expect("create site");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
    )
    .expect("write root index");
    fs::write(site.join("index.html"), "<h1>Knowledge Wiki</h1>").expect("write site");

    let api = LocalWikiService::new(vec![bundle]).into_api();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind preview");
    let address = listener.local_addr().expect("preview address");
    let server = tokio::spawn(async move {
        axum::serve(listener, http::preview_router(api, site))
            .await
            .expect("serve preview");
    });

    let health = http_request(address, "/healthz").await;
    assert!(health.starts_with("HTTP/1.1 200 OK"));
    assert!(health
        .to_ascii_lowercase()
        .contains("content-security-policy:"));
    assert!(health.contains("\"kind\":\"health\""));

    let page = http_request(address, "/").await;
    assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
    assert!(page.contains("<h1>Knowledge Wiki</h1>"));
    assert!(page.to_ascii_lowercase().contains("x-frame-options: deny"));

    server.abort();
}

#[test]
fn wiki_graph_context_and_service_do_not_import_graph_internals() {
    let graph_context = include_str!("../src/graph_context.rs");
    let refresh = include_str!("../src/refresh.rs");
    let service = include_str!("../src/service.rs");
    for source in [graph_context, refresh, service] {
        assert!(!source.contains("api::core"));
        assert!(!source.contains("api::refresh"));
        assert!(!source.contains("crate::storage"));
        assert!(!source.contains("src/adapters"));
    }
    assert!(refresh.contains("OperationRequest::Refresh"));
    assert!(refresh.contains("CodebaseGraphApi::new"));
}

fn call_tool(
    api: &okf_wiki::api::OkfWikiApi<LocalWikiService>,
    session: &mut mcp::McpSession,
    id: u64,
    tool: &str,
    arguments: Value,
) -> Value {
    mcp::handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}
        }),
        session,
        &mut |tool_name| {
            mcp_operation_descriptor(tool_name)
                .map(|descriptor| (descriptor.request_schema)())
                .unwrap_or_else(|| json!({"type": "object"}))
        },
        &mut |tool_name, arguments| mcp::dispatch_api(api, tool_name, arguments),
    )
    .expect("MCP response")
}

fn mcp_session() -> mcp::McpSession {
    mcp::McpSession {
        protocol_version: Some(mcp::protocol_version().to_string()),
        initialized: true,
    }
}

async fn http_request(address: std::net::SocketAddr, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect preview");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .expect("write request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("read response");
    String::from_utf8(response).expect("UTF-8 HTTP response")
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
