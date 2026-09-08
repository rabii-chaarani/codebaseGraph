use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")))
}

fn temp_repo() -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "codebase-graph-refresh-recovery-{}-{sequence}-{now}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
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
    body: Value,
}

fn try_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> Option<HttpResponse> {
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .ok()?
        .unwrap_or_default();
    let address = ("127.0.0.1", port).to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok()?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .ok()?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").ok()?;
    }
    write!(stream, "\r\n").ok()?;
    stream.write_all(&body).ok()?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).ok()?;
    let split = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&bytes[..split]);
    let status = head
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    let body = serde_json::from_slice(&bytes[split + 4..]).unwrap_or_else(|_| json!({}));
    Some(HttpResponse {
        status,
        headers,
        body,
    })
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> HttpResponse {
    try_request(port, method, path, headers, body)
        .unwrap_or_else(|| panic!("HTTP request failed: {method} {path} on port {port}"))
}

struct McpSession {
    port: u16,
    session_id: String,
    next_id: u64,
}

impl McpSession {
    fn start(port: u16) -> Self {
        let response = request(
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
        assert_eq!(
            response.status, 200,
            "MCP initialize response: {response:?}"
        );
        Self {
            port,
            session_id: response
                .headers
                .get("mcp-session-id")
                .cloned()
                .expect("MCP initialize should return a session id"),
            next_id: 2,
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let response = request(
            self.port,
            "POST",
            "/mcp",
            &[
                ("mcp-session-id", self.session_id.as_str()),
                ("mcp-protocol-version", MCP_PROTOCOL_VERSION),
            ],
            Some(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            })),
        );
        assert_eq!(response.status, 200, "{name} HTTP response: {response:?}");
        assert_eq!(
            response.body["result"]["isError"], false,
            "{name} MCP response: {response:?}"
        );
        response.body["result"]["structuredContent"].clone()
    }

    fn health(&mut self) -> Value {
        self.call("graph_health", json!({"include_structured_content": true}))
    }

    fn search_paths(&mut self, query: &str) -> Vec<String> {
        self.call(
            "graph_search",
            json!({
                "query": query,
                "layer": "semantic",
                "limit": 20,
                "context_limit": 0,
                "budget": 0,
                "include_structured_content": true
            }),
        )["results"]
            .as_array()
            .expect("graph_search structured results should be an array")
            .iter()
            .filter_map(|result| result["path"].as_str().map(str::to_string))
            .collect()
    }
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if path.is_file() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {}", path.display());
}

fn wait_for_http(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if try_request(port, "GET", "/_codebasegraph/health", &[], None)
            .is_some_and(|response| response.status == 200)
        {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for HTTP daemon on port {port}");
}

fn daemon_port(state: &Value) -> u16 {
    state["endpoint"]
        .as_str()
        .expect("daemon state should contain endpoint")
        .strip_prefix("http://127.0.0.1:")
        .expect("daemon endpoint should be loopback HTTP")
        .split('/')
        .next()
        .expect("daemon endpoint should contain a port")
        .parse()
        .expect("daemon endpoint port should be numeric")
}

fn install(root: &Path) -> PathBuf {
    let output = Command::new(binary())
        .current_dir(root)
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
            "--no-semantic-enrichment",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let build = Command::new(binary())
        .current_dir(root)
        .args([
            "build",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--no-semantic-enrichment",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "initial build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    root.join(".codebaseGraph/config.json")
}

struct ConfigSettings<'a> {
    policy: &'a str,
    backend: &'a str,
    interval_ms: u64,
    worker_memory_mib: u64,
    rust_memory_mib: u64,
    spill_chunk_mib: u64,
    max_parallelism: usize,
    exclude: &'a [&'a str],
}

fn set_config(path: &Path, settings: ConfigSettings<'_>) {
    let mut config: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    config["refresh"] = json!({
        "policy": settings.policy,
        "backend": settings.backend,
        "reconcile_interval_ms": settings.interval_ms
    });
    config["materialization"]["worker_memory_mib"] = json!(settings.worker_memory_mib);
    config["materialization"]["rust_memory_mib"] = json!(settings.rust_memory_mib);
    config["materialization"]["spill_chunk_mib"] = json!(settings.spill_chunk_mib);
    config["materialization"]["max_parallelism"] = json!(settings.max_parallelism);
    config["materialization"]["exclude"] = json!(settings.exclude);
    fs::write(path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
}

fn start_daemon(
    root: &Path,
    config: &Path,
    current_dir: &Path,
    overrides: &[&str],
) -> (ChildGuard, u16, Value) {
    let mut args = vec![
        "mcp".to_string(),
        "daemon".to_string(),
        "serve".to_string(),
        "--config".to_string(),
        config.to_str().unwrap().to_string(),
    ];
    args.extend(overrides.iter().map(|value| (*value).to_string()));
    let daemon = ChildGuard(
        Command::new(binary())
            .current_dir(current_dir)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let state_path = root.join(".codebaseGraph/mcp-daemon.json");
    wait_for_file(&state_path);
    let state: Value = serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    let port = daemon_port(&state);
    wait_for_http(port);
    (daemon, port, state)
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start_http_server(config: &Path, current_dir: &Path, overrides: &[&str]) -> (ChildGuard, u16) {
    let port = free_port();
    let mut args = vec![
        "mcp".to_string(),
        "http".to_string(),
        "--config".to_string(),
        config.to_str().unwrap().to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
    ];
    args.extend(overrides.iter().map(|value| (*value).to_string()));
    let mut daemon = ChildGuard(
        Command::new(binary())
            .current_dir(current_dir)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if try_request(
            port,
            "POST",
            "/mcp",
            &[],
            Some(&json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {"protocolVersion": MCP_PROTOCOL_VERSION}
            })),
        )
        .is_some_and(|response| response.status == 200)
        {
            return (daemon, port);
        }
        if let Some(status) = daemon.0.try_wait().unwrap() {
            let mut stderr = String::new();
            if let Some(mut pipe) = daemon.0.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            panic!("mcp http exited before binding ({status}): {stderr}");
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for HTTP daemon on port {port}");
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn memory_limits(health: &Value) -> &Value {
    &health["refresh"]["memory_limits"]
}

#[test]
fn blocked_refresh_recovers_after_memory_budget_restore_without_source_edit() {
    let root = temp_repo();
    let unrelated = temp_repo();
    fs::write(
        root.join("initial.py"),
        "def initial_symbol():\n    return 1\n",
    )
    .unwrap();
    let config = install(&root);

    // The source edit is made before the daemon starts.  The initial graph is
    // therefore the only valid generation available while the tiny budget is
    // rejecting the pending startup reconciliation.
    fs::write(
        root.join("latest.py"),
        format!(
            "# {}\ndef latest_symbol():\n    return 2\n",
            "x".repeat(4 * 1024 * 1024)
        ),
    )
    .unwrap();
    set_config(
        &config,
        ConfigSettings {
            policy: "leader",
            backend: "poll",
            interval_ms: 100,
            worker_memory_mib: 2,
            rust_memory_mib: 1,
            spill_chunk_mib: 1,
            max_parallelism: 1,
            exclude: &[],
        },
    );

    let (daemon, port, _state) = start_daemon(&root, &config, &unrelated, &[]);
    let mut session = McpSession::start(port);
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut blocked_health = None;
    while Instant::now() < deadline {
        let health = session.health();
        let refresh = &health["refresh"];
        if matches!(
            refresh["state"].as_str(),
            Some("blocked") | Some("retrying")
        ) {
            blocked_health = Some(health);
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let blocked = blocked_health.expect("tiny memory budget should block or retry refresh");
    let blocked_refresh = &blocked["refresh"];
    assert_eq!(blocked["ok"], true, "graph health must remain readable");
    assert_eq!(blocked["graph_readable"], true);
    assert_eq!(blocked_refresh["task_alive"], true);
    assert_eq!(blocked_refresh["pending"], true);
    assert!(
        blocked_refresh["last_error"]
            .as_str()
            .is_some_and(|error| error.contains("memory_budget") || error.contains("memory")),
        "blocked refresh should expose the memory failure: {blocked}"
    );
    if blocked_refresh["state"] == "retrying" {
        let next_retry = blocked_refresh["next_retry_unix_ms"]
            .as_u64()
            .expect("retrying refresh must expose next_retry_unix_ms");
        assert!(
            next_retry + 1_000 >= now_unix_ms(),
            "next_retry_unix_ms must not be in the past while retrying/blocked: {blocked}"
        );
    } else if let Some(next_retry) = blocked_refresh["next_retry_unix_ms"].as_u64() {
        assert!(
            next_retry + 1_000 >= now_unix_ms(),
            "blocked next_retry_unix_ms must not be in the past: {blocked}"
        );
    }
    let old_paths = session.search_paths("initial_symbol");
    assert!(
        old_paths.iter().any(|path| path == "initial.py"),
        "the previous generation must remain queryable during refresh failure: {old_paths:?}"
    );

    set_config(
        &config,
        ConfigSettings {
            policy: "leader",
            backend: "poll",
            interval_ms: 100,
            worker_memory_mib: 768,
            rust_memory_mib: 384,
            spill_chunk_mib: 32,
            max_parallelism: 1,
            exclude: &[],
        },
    );
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut recovered = None;
    while Instant::now() < deadline {
        let health = session.health();
        let refresh = &health["refresh"];
        let latest_paths = session.search_paths("latest_symbol");
        let limits = memory_limits(&health);
        if refresh["state"] == "running"
            && refresh["task_alive"] == true
            && refresh["pending"] == false
            && refresh["dirty_epoch"] == refresh["reconciled_epoch"]
            && refresh["next_retry_unix_ms"].is_null()
            && limits["worker_memory_mib"] == 768
            && limits["rust_memory_mib"] == 384
            && limits["spill_chunk_mib"] == 32
            && limits["max_parallelism"] == 1
            && latest_paths.iter().any(|path| path == "latest.py")
        {
            recovered = Some((health, latest_paths));
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let (recovered, latest_paths) =
        recovered.expect("refresh should recover without another source edit");
    assert_eq!(recovered["refresh"]["reconcile_interval_ms"], 100);
    assert!(latest_paths.iter().any(|path| path == "latest.py"));

    drop(session);
    drop(daemon);
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(unrelated);
}

#[test]
fn config_hot_reload_applies_excludes_while_cli_policy_and_backend_overrides_persist() {
    let root = temp_repo();
    let unrelated = temp_repo();
    fs::write(
        root.join("tracked.py"),
        "def tracked_symbol():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        root.join("ignored.py"),
        "def ignored_symbol():\n    return 2\n",
    )
    .unwrap();
    let config = install(&root);
    set_config(
        &config,
        ConfigSettings {
            policy: "off",
            backend: "native",
            interval_ms: 500,
            worker_memory_mib: 768,
            rust_memory_mib: 384,
            spill_chunk_mib: 32,
            max_parallelism: 1,
            exclude: &[],
        },
    );

    let overrides = [
        "--refresh-policy",
        "leader",
        "--refresh-backend",
        "poll",
        "--worker-memory-mib",
        "768",
        "--rust-memory-mib",
        "384",
        "--spill-chunk-mib",
        "32",
        "--max-parallelism",
        "1",
    ];
    let (daemon, port) = start_http_server(&config, &unrelated, &overrides);
    let mut session = McpSession::start(port);
    let initial_deadline = Instant::now() + Duration::from_secs(40);
    let mut initial = None;
    while Instant::now() < initial_deadline {
        let health = session.health();
        let paths = session.search_paths("ignored_symbol");
        if health["refresh"]["state"] == "running"
            && health["refresh"]["backend"] == "poll"
            && health["refresh"]["enabled"] == true
            && health["refresh"]["reconcile_interval_ms"] == 500
            && paths.iter().any(|path| path == "ignored.py")
        {
            initial = Some(health);
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let initial = initial.expect("CLI policy/backend overrides should run the service");
    assert_eq!(memory_limits(&initial)["worker_memory_mib"], 768);
    assert_eq!(memory_limits(&initial)["rust_memory_mib"], 384);
    assert_eq!(memory_limits(&initial)["spill_chunk_mib"], 32);
    assert_eq!(memory_limits(&initial)["max_parallelism"], 1);

    // The config now requests the opposite policy/backend and excludes an
    // existing source file.  The explicit CLI settings must remain in force,
    // while the source-selection rule must be rebuilt from the new config.
    set_config(
        &config,
        ConfigSettings {
            policy: "off",
            backend: "native",
            interval_ms: 100,
            worker_memory_mib: 2,
            rust_memory_mib: 1,
            spill_chunk_mib: 1,
            max_parallelism: 1,
            exclude: &["ignored.py"],
        },
    );
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut reloaded = None;
    while Instant::now() < deadline {
        let health = session.health();
        let paths = session.search_paths("ignored_symbol");
        let limits = memory_limits(&health);
        if health["refresh"]["state"] == "running"
            && health["refresh"]["backend"] == "poll"
            && health["refresh"]["enabled"] == true
            && health["refresh"]["reconcile_interval_ms"] == 100
            && limits["worker_memory_mib"] == 768
            && limits["rust_memory_mib"] == 384
            && limits["spill_chunk_mib"] == 32
            && limits["max_parallelism"] == 1
            && !paths.iter().any(|path| path == "ignored.py")
        {
            reloaded = Some(health);
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let reloaded = reloaded.expect("config hot reload should apply excludes and interval");
    assert_eq!(reloaded["refresh"]["backend"], "poll");
    assert_eq!(reloaded["refresh"]["reconcile_interval_ms"], 100);
    assert_eq!(reloaded["refresh"]["enabled"], true);
    assert_eq!(memory_limits(&reloaded)["worker_memory_mib"], 768);
    assert_eq!(memory_limits(&reloaded)["rust_memory_mib"], 384);

    drop(session);
    drop(daemon);
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(unrelated);
}
