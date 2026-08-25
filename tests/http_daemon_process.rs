use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")))
}

fn temp_repo() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "codebase-graph-http-daemon-{}-{unique}",
        std::process::id()
    ))
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: std::collections::BTreeMap<String, String>,
    body: serde_json::Value,
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&serde_json::Value>,
) -> HttpResponse {
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .unwrap()
        .unwrap_or_default();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .unwrap();
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").unwrap();
    }
    write!(stream, "\r\n").unwrap();
    stream.write_all(&body).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap();
    let head = String::from_utf8_lossy(&bytes[..split]);
    let status = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    let body = serde_json::from_slice(&bytes[split + 4..]).unwrap_or_else(|_| json!({}));
    HttpResponse {
        status,
        headers,
        body,
    }
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {}", path.display());
}

#[test]
fn one_http_daemon_serves_multiple_sessions_and_rejects_duplicate_owner() {
    let root = temp_repo();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    let install = Command::new(binary())
        .args([
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
        ])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );
    let config = root.join(".codebaseGraph/config.json");
    let installed_config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    let configured_endpoint = installed_config["mcp"]["http"]["url"].as_str().unwrap();
    let configured_port = configured_endpoint
        .strip_prefix("http://127.0.0.1:")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let occupied = TcpListener::bind(("127.0.0.1", configured_port)).unwrap();
    let failed = Command::new(binary())
        .args([
            "mcp",
            "daemon",
            "serve",
            "--config",
            config.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("failed to bind"));
    let failure_path = root.join(".codebaseGraph/mcp-daemon-failure.json");
    let failure: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&failure_path).unwrap()).unwrap();
    assert_eq!(failure["schema_version"], 1);
    assert_eq!(failure["phase"], "listener_bind");
    assert!(failure["message"].as_str().unwrap().len() <= 4 * 1024);

    let failed_status = Command::new(binary())
        .args([
            "mcp",
            "daemon",
            "status",
            "--config",
            config.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(failed_status.status.success());
    let failed_status: serde_json::Value = serde_json::from_slice(&failed_status.stdout).unwrap();
    assert_eq!(failed_status["running"], false);
    assert_eq!(failed_status["latest_failure"]["phase"], "listener_bind");
    assert_eq!(failed_status["recommended_action"]["code"], "start_daemon");
    drop(occupied);

    let mut daemon = ChildGuard(
        Command::new(binary())
            .args([
                "mcp",
                "daemon",
                "serve",
                "--config",
                config.to_str().unwrap(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let state_path = root.join(".codebaseGraph/mcp-daemon.json");
    wait_for_file(&state_path);
    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    let pid = state["pid"].as_u64().unwrap();
    let endpoint = state["endpoint"].as_str().unwrap();
    let port = endpoint
        .strip_prefix("http://127.0.0.1:")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();

    let recovered_status = Command::new(binary())
        .args([
            "mcp",
            "daemon",
            "status",
            "--config",
            config.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(recovered_status.status.success());
    let recovered_status: serde_json::Value =
        serde_json::from_slice(&recovered_status.stdout).unwrap();
    assert_eq!(recovered_status["running"], true);
    assert_eq!(
        recovered_status["runtime_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(recovered_status["latest_failure"]["phase"], "listener_bind");
    assert_eq!(recovered_status["recovered"], true);

    let duplicate = Command::new(binary())
        .args([
            "mcp",
            "daemon",
            "serve",
            "--config",
            config.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already running"));

    let health = request(port, "GET", "/_codebasegraph/health", &[], None);
    assert_eq!(health.status, 200);
    assert_eq!(health.body["pid"], pid);
    assert_eq!(health.body["endpoint"], endpoint);

    let initialize = |id| {
        request(
            port,
            "POST",
            "/mcp",
            &[],
            Some(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "initialize",
                "params": {"protocolVersion": MCP_PROTOCOL_VERSION}
            })),
        )
    };
    let first = initialize(1);
    let second = initialize(2);
    assert_eq!(first.status, 200);
    assert_eq!(second.status, 200);
    assert_ne!(
        first.headers.get("mcp-session-id"),
        second.headers.get("mcp-session-id")
    );
    for (id, initialized) in [(3, &first), (4, &second)] {
        let session = initialized.headers.get("mcp-session-id").unwrap();
        let tools = request(
            port,
            "POST",
            "/mcp",
            &[
                ("mcp-session-id", session),
                ("mcp-protocol-version", MCP_PROTOCOL_VERSION),
            ],
            Some(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/list",
                "params": {}
            })),
        );
        assert_eq!(tools.status, 200);
        assert!(tools.body["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "graph_health"));
    }

    let context = request(
        port,
        "POST",
        "/mcp",
        &[
            (
                "mcp-session-id",
                first.headers.get("mcp-session-id").unwrap(),
            ),
            ("mcp-protocol-version", MCP_PROTOCOL_VERSION),
        ],
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "graph_context",
                "arguments": {
                    "query": "helper",
                    "layer": "semantic",
                    "profile": "definitions",
                    "detail": "slim",
                    "context_limit": 1
                }
            }
        })),
    );
    assert_eq!(context.status, 200);
    assert_eq!(context.body["result"]["isError"], false);
    assert!(context.body["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("helper"));
    assert_eq!(
        request(port, "GET", "/_codebasegraph/health", &[], None).body["pid"],
        pid
    );

    let unauthorized = request(
        port,
        "POST",
        "/_codebasegraph/shutdown",
        &[],
        Some(&json!({})),
    );
    assert_eq!(unauthorized.status, 401);
    assert_eq!(
        request(port, "GET", "/_codebasegraph/health", &[], None).body["pid"],
        pid
    );

    let shutdown = request(
        port,
        "POST",
        "/_codebasegraph/shutdown",
        &[(
            "x-codebasegraph-control-token",
            state["control_token"].as_str().unwrap(),
        )],
        Some(&json!({})),
    );
    assert_eq!(shutdown.status, 200);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if daemon.0.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let status = daemon.0.try_wait().unwrap();
    assert!(
        status.is_some(),
        "daemon did not exit after authenticated shutdown"
    );
    assert!(
        !state_path.exists(),
        "daemon state should be removed on clean exit"
    );
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    let _ = fs::remove_dir_all(root);
}
