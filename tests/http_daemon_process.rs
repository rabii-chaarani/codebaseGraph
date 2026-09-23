use fs2::FileExt;
use serde_json::json;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
#[cfg(not(any(unix, windows)))]
use std::net::Shutdown;
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
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
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

fn mcp_call(
    port: u16,
    session: &str,
    id: u64,
    name: &str,
    arguments: serde_json::Value,
) -> HttpResponse {
    request(
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
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        })),
    )
}

fn responsive_health(port: u16, pid: u64) -> HttpResponse {
    let started = Instant::now();
    let response = request(port, "GET", "/_codebasegraph/health", &[], None);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "transport health blocked: {response:?}"
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.body["pid"], pid);
    assert!(
        response.body["transport"]["admitted_connections"]
            .as_u64()
            .unwrap()
            <= 32
    );
    assert_eq!(response.body["transport"]["queued_operations"], 0);
    response
}

fn wait_for_graph_health(port: u16, session: &str, id: u64) -> HttpResponse {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let response = mcp_call(
            port,
            session,
            id,
            "graph_health",
            json!({"include_structured_content": true}),
        );
        if response.body["result"]["isError"] == false {
            return response;
        }
        assert_eq!(
            response.body["result"]["structuredContent"]["error"]["retryable"], true,
            "unexpected graph failure: {response:?}"
        );
        assert!(
            Instant::now() < deadline,
            "graph did not become available: {response:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn assert_stalled_clients_are_isolated(port: u16, pid: u64, session: &str) {
    let mut idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
    idle.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    let mut partial_header = TcpStream::connect(("127.0.0.1", port)).unwrap();
    partial_header
        .write_all(b"POST /mcp HTTP/1.1\r\nHost: localhost\r\n")
        .unwrap();
    let mut partial_body = TcpStream::connect(("127.0.0.1", port)).unwrap();
    partial_body
        .write_all(b"POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n{")
        .unwrap();
    responsive_health(port, pid);
    let started = Instant::now();
    let ping = request(
        port,
        "POST",
        "/mcp",
        &[("mcp-session-id", session)],
        Some(&json!({"jsonrpc":"2.0","id":100,"method":"ping"})),
    );
    assert_eq!(ping.status, 200);
    assert_eq!(ping.body["result"], json!({}));
    assert!(started.elapsed() < Duration::from_secs(1));
    let mut bytes = Vec::new();
    idle.read_to_end(&mut bytes).unwrap();
    assert!(String::from_utf8_lossy(&bytes).starts_with("HTTP/1.1 408"));
    responsive_health(port, pid);

    let mut oversized = TcpStream::connect(("127.0.0.1", port)).unwrap();
    oversized
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let headers = format!(
        "GET /_codebasegraph/health HTTP/1.1\r\nX-Large: {}\r\n\r\n",
        "x".repeat(33 * 1024)
    );
    let _ = oversized.write_all(headers.as_bytes());
    let mut response = Vec::new();
    // A reset after the rejection is also connection-local; retain bytes read.
    let _ = oversized.read_to_end(&mut response);
    assert!(String::from_utf8_lossy(&response).starts_with("HTTP/1.1 431"));
    responsive_health(port, pid);
}

/// Close a request socket with an abortive reset instead of a graceful FIN.
///
/// A client-side hook timeout usually tears down its HTTP connection while the
/// daemon is still producing the response.  Setting zero linger makes that
/// condition deterministic in this process regression, while keeping the test
/// independent of production-only hooks.
#[cfg(unix)]
fn abort_connection(stream: TcpStream) {
    use std::os::fd::AsRawFd;

    let linger = libc::linger {
        l_onoff: 1,
        l_linger: 0,
    };
    let result = unsafe {
        libc::setsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_LINGER,
            (&linger as *const libc::linger).cast(),
            std::mem::size_of::<libc::linger>() as libc::socklen_t,
        )
    };
    assert_eq!(result, 0, "failed to configure abortive test connection");
    drop(stream);
}

#[cfg(windows)]
fn abort_connection(stream: TcpStream) {
    use std::os::windows::io::AsRawSocket;

    #[repr(C)]
    struct Linger {
        onoff: u16,
        linger: u16,
    }

    #[link(name = "Ws2_32")]
    extern "system" {
        fn setsockopt(
            socket: usize,
            level: i32,
            option_name: i32,
            option_value: *const i8,
            option_length: i32,
        ) -> i32;
    }

    const SOL_SOCKET: i32 = 0xffff;
    const SO_LINGER: i32 = 0x0080;
    let linger = Linger {
        onoff: 1,
        linger: 0,
    };
    let result = unsafe {
        setsockopt(
            usize::try_from(stream.as_raw_socket())
                .expect("Windows socket handle should fit in usize"),
            SOL_SOCKET,
            SO_LINGER,
            (&linger as *const Linger).cast(),
            std::mem::size_of::<Linger>() as i32,
        )
    };
    assert_eq!(result, 0, "failed to configure abortive test connection");
    drop(stream);
}

#[cfg(not(any(unix, windows)))]
fn abort_connection(stream: TcpStream) {
    let _ = stream.shutdown(Shutdown::Both);
    drop(stream);
}

fn disconnect_before_response(
    port: u16,
    session: &str,
    id: u64,
    name: &str,
    arguments: serde_json::Value,
) {
    let body = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    }))
    .unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nMcp-Session-Id: {session}\r\nMcp-Protocol-Version: {MCP_PROTOCOL_VERSION}\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
    // The graph operation takes a shared state lock. The test owns that lock
    // while sending the request, so give the daemon/coordinator time to reach
    // the blocked acquisition before resetting the client socket.
    thread::sleep(Duration::from_millis(250));
    // Do not read any response bytes: this models a hook deadline expiring
    // while the daemon is still handling the request.
    abort_connection(stream);
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

    // Hook-style clients can time out after sending a request and before
    // reading its response.  Their abortive disconnect must not terminate the
    // daemon or discard the initialized sessions.
    let first_session = first.headers.get("mcp-session-id").unwrap();
    let second_session = second.headers.get("mcp-session-id").unwrap();
    assert_stalled_clients_are_isolated(port, pid, first_session);
    let state_lock_path = root.join(".codebaseGraph/storage/state.lock");
    {
        let state_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&state_lock_path)
            .unwrap();
        state_lock.lock_exclusive().unwrap();
        let started = Instant::now();
        let expired = request(
            port,
            "POST",
            "/mcp",
            &[
                ("mcp-session-id", first_session),
                ("x-codebasegraph-timeout-ms", "500"),
            ],
            Some(
                &json!({"jsonrpc":"2.0", "id":104, "method":"tools/call", "params":{"name":"graph_health", "arguments":{"include_structured_content":true}}}),
            ),
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "hook read exceeded its budget: {expired:?}"
        );
        assert_eq!(
            expired.body["result"]["structuredContent"]["error"]["code"],
            "deadline_exceeded"
        );
        responsive_health(port, pid);
        for id in 105..108 {
            let busy = mcp_call(
                port,
                second_session,
                id,
                "graph_health",
                json!({"include_structured_content":true}),
            );
            assert_eq!(
                busy.body["result"]["structuredContent"]["error"]["code"],
                "graph_busy"
            );
        }
        drop(state_lock);
        wait_for_graph_health(port, first_session, 108);
    }
    for id in 6..=10 {
        let state_lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&state_lock_path)
            .unwrap();
        state_lock.lock_exclusive().unwrap();
        disconnect_before_response(
            port,
            first_session,
            id,
            "graph_health",
            json!({"include_structured_content": true}),
        );
        drop(state_lock);
        let health = request(port, "GET", "/_codebasegraph/health", &[], None);
        assert_eq!(
            health.status, 200,
            "daemon health after abort {id}: {health:?}"
        );
        assert_eq!(
            health.body["pid"], pid,
            "daemon was replaced after abort {id}"
        );
    }

    let reused = wait_for_graph_health(port, first_session, 11);
    assert_eq!(reused.status, 200);
    assert_eq!(reused.body["result"]["isError"], false);

    let second_reused = mcp_call(
        port,
        second_session,
        12,
        "graph_health",
        json!({"include_structured_content": true}),
    );
    assert_eq!(second_reused.status, 200);
    assert_eq!(second_reused.body["result"]["isError"], false);

    let unknown = mcp_call(
        port,
        "unknown-session",
        13,
        "graph_health",
        json!({"include_structured_content": true}),
    );
    assert_eq!(unknown.status, 400);
    assert_eq!(unknown.body["error"]["code"], -32002);

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

    // Hold a real storage lock rather than relying on a production delay switch.
    // Transport and session operations must continue while graph execution waits.
    let state_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&state_lock_path)
        .unwrap();
    state_lock.lock_exclusive().unwrap();
    let mut blocked = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = json!({"jsonrpc":"2.0", "id":101, "method":"tools/call",
        "params":{"name":"graph_health", "arguments":{"include_structured_content":true}}})
    .to_string();
    write!(blocked, "POST /mcp HTTP/1.1\r\nHost: localhost\r\nMcp-Session-Id: {first_session}\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let wait_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let health = responsive_health(port, pid);
        if health.body["transport"]["active_operation"] == true {
            break;
        }
        assert!(
            Instant::now() < wait_deadline,
            "graph call never entered execution"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let started = Instant::now();
    let busy = mcp_call(
        port,
        second_session,
        102,
        "graph_health",
        json!({"include_structured_content": true}),
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        busy.body["result"]["structuredContent"]["error"]["code"],
        "graph_busy"
    );
    let started = Instant::now();
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
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown acknowledgement blocked on execution"
    );
    thread::sleep(Duration::from_millis(100));
    assert!(
        daemon.0.try_wait().unwrap().is_none(),
        "shutdown abandoned the active operation"
    );
    assert!(
        state_path.exists(),
        "daemon released its state before draining execution"
    );
    drop(state_lock);
    drop(blocked);
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

#[test]
fn config_only_daemon_from_unrelated_cwd_tracks_source_changes() {
    let root = temp_repo();
    let unrelated = temp_repo();
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(
        root.join("initial.py"),
        "def initial_symbol():\n    return 1\n",
    )
    .unwrap();
    let install = Command::new(binary())
        .current_dir(&root)
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
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let config_path = root.join(".codebaseGraph/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["repo_root"] = json!("..");
    config["refresh"]["reconcile_interval_ms"] = json!(100);
    config["refresh"]["backend"] = json!("poll");
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    let mut daemon = ChildGuard(
        Command::new(binary())
            .current_dir(&unrelated)
            .args([
                "mcp",
                "daemon",
                "serve",
                "--config",
                config_path.to_str().unwrap(),
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
    let port = state["endpoint"]
        .as_str()
        .unwrap()
        .strip_prefix("http://127.0.0.1:")
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();

    let initialized = request(
        port,
        "POST",
        "/mcp",
        &[],
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": MCP_PROTOCOL_VERSION}
        })),
    );
    assert_eq!(initialized.status, 200);
    let session = initialized.headers.get("mcp-session-id").unwrap();

    let health_deadline = Instant::now() + Duration::from_secs(20);
    let mut health_id = 2;
    let health = loop {
        assert!(
            Instant::now() < health_deadline,
            "graph_health startup readiness timed out before request id {health_id}"
        );
        let response = mcp_call(
            port,
            session,
            health_id,
            "graph_health",
            json!({"include_structured_content": true}),
        );
        assert_eq!(
            response.status, 200,
            "graph_health HTTP response: {response:?}"
        );
        assert!(
            Instant::now() < health_deadline,
            "graph_health startup readiness timed out after response: {response:?}"
        );
        if response.body["result"]["isError"] == false {
            break response;
        }
        assert_eq!(
            response.body["result"]["structuredContent"]["error"]["retryable"], true,
            "graph_health MCP response was not retryable: {response:?}"
        );
        health_id += 1;
        thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        health.body["result"]["structuredContent"]["repo_root"],
        root.canonicalize().unwrap().to_string_lossy().to_string()
    );

    let search = |id, query| {
        mcp_call(
            port,
            session,
            id,
            "graph_search",
            json!({
                "query": query,
                "layer": "semantic",
                "limit": 10,
                "context_limit": 0,
                "budget": 0,
                "include_structured_content": true
            }),
        )
    };
    let result_paths = |response: &HttpResponse| {
        assert_eq!(
            response.status, 200,
            "graph_search HTTP response: {response:?}"
        );
        assert_eq!(
            response.body["result"]["isError"], false,
            "graph_search MCP response: {response:?}"
        );
        response.body["result"]["structuredContent"]["results"]
            .as_array()
            .expect("graph_search structured results should be an array")
            .iter()
            .filter_map(|result| result["path"].as_str())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let wait_for = |id: &mut u64, predicate: &dyn Fn(&[String]) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let response = search(*id, "tracked_symbol");
            let paths = result_paths(&response);
            if predicate(&paths) {
                return;
            }
            if Instant::now() >= deadline {
                let health_id = id.saturating_add(1);
                let health = mcp_call(
                    port,
                    session,
                    health_id,
                    "graph_health",
                    json!({"include_structured_content": true}),
                );
                panic!("graph did not converge: search={response:?}; health={health:?}");
            }
            *id += 1;
            thread::sleep(Duration::from_millis(250));
        }
    };

    fs::write(
        root.join("created.py"),
        "def tracked_symbol():\n    return 2\n",
    )
    .unwrap();
    let mut search_id = health_id + 1;
    wait_for(&mut search_id, &|paths| {
        paths.iter().any(|path| path == "created.py")
    });

    fs::rename(root.join("created.py"), root.join("renamed.py")).unwrap();
    wait_for(&mut search_id, &|paths| {
        paths.iter().any(|path| path == "renamed.py")
            && !paths.iter().any(|path| path == "created.py")
    });

    fs::remove_file(root.join("renamed.py")).unwrap();
    wait_for(&mut search_id, &|paths| paths.is_empty());

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
    while Instant::now() < deadline && daemon.0.try_wait().unwrap().is_none() {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(daemon.0.try_wait().unwrap().is_some());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(unrelated);
}
