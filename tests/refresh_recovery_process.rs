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

#[cfg(debug_assertions)]
#[path = "support/lifecycle_faults.rs"]
mod lifecycle_faults;

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
    let path = std::env::temp_dir().join(format!("cgr-{}-{sequence}-{now}", std::process::id()));
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

struct ManualBuild {
    child: ChildGuard,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl ManualBuild {
    fn start(root: &Path) -> Self {
        let stdout = root.join("manual-build.stdout");
        let stderr = root.join("manual-build.stderr");
        let child = ChildGuard(
            Command::new(binary())
                .current_dir(root)
                .args([
                    "build",
                    "--repo-root",
                    root.to_str().unwrap(),
                    "--mode",
                    "full",
                    "--json",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::from(fs::File::create(&stdout).unwrap()))
                .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
                .spawn()
                .unwrap(),
        );
        Self {
            child,
            stdout,
            stderr,
        }
    }

    fn poll(&mut self) -> Option<std::process::ExitStatus> {
        self.child.0.try_wait().unwrap()
    }

    fn diagnostics(&self) -> String {
        fn bounded_log(path: &Path) -> String {
            let mut text = String::new();
            if let Ok(file) = fs::File::open(path) {
                let _ = file.take(64 * 1024).read_to_string(&mut text);
            }
            text
        }
        format!(
            "stdout:\n{}\nstderr:\n{}",
            bounded_log(&self.stdout),
            bounded_log(&self.stderr)
        )
    }

    fn assert_success(&self, status: std::process::ExitStatus) {
        assert!(
            status.success(),
            "manual build failed ({status}): {}",
            self.diagnostics()
        );
    }

    fn wait_for_success(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline {
            if let Some(status) = self.poll() {
                self.assert_success(status);
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!(
            "manual build did not finish within 90 seconds: {}",
            self.diagnostics()
        );
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
    start_http_server_with_env(config, current_dir, overrides, &[])
}

fn start_http_server_with_env(
    config: &Path,
    current_dir: &Path,
    overrides: &[&str],
    environment: &[(&str, &Path)],
) -> (ChildGuard, u16) {
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
    let mut command = Command::new(binary());
    command
        .current_dir(current_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (key, value) in environment {
        command.env(key, value);
    }
    let mut daemon = ChildGuard(command.spawn().unwrap());
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

fn reconciliation_is_ready(health: &Value) -> bool {
    // An expensive native query can outlast this fixture's 100 ms interval.
    // Overdue describes reconciliation age; publication is checked by search.
    matches!(health["freshness"].as_str(), Some("current" | "overdue"))
        && health["graph_readable"] == true
        && health["refresh_readiness"] == "ready"
        && health["refresh"]["pending"] == false
}

#[test]
fn recovery_wait_requires_ready_reconciliation_not_a_clock_window() {
    let ready = json!({
        "freshness": "overdue", "graph_readable": true,
        "refresh_readiness": "ready", "refresh": {"pending": false}
    });
    assert!(reconciliation_is_ready(&ready));
    let mut current = ready.clone();
    current["freshness"] = json!("current");
    assert!(reconciliation_is_ready(&current));
    for status in ["pending", "unknown"] {
        let mut blocked = ready.clone();
        blocked["freshness"] = json!(status);
        assert!(!reconciliation_is_ready(&blocked));
    }
    let mut pending = ready.clone();
    pending["refresh"]["pending"] = json!(true);
    assert!(!reconciliation_is_ready(&pending));
    let mut blocked = ready.clone();
    blocked["refresh_readiness"] = json!("blocked");
    assert!(!reconciliation_is_ready(&blocked));
    let mut unreadable = ready;
    unreadable["graph_readable"] = json!(false);
    assert!(!reconciliation_is_ready(&unreadable));
    assert!(!reconciliation_is_ready(&Value::Null));
}

fn wait_for_current_symbol(session: &mut McpSession, symbol: &str, path: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = session.health();
        if reconciliation_is_ready(&last)
            && session
                .search_paths(symbol)
                .iter()
                .any(|entry| entry == path)
        {
            return last;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("automatic refresh did not publish {symbol}: {last}");
}

fn assert_recovered_refresh(health: &Value) {
    assert_eq!(health["refresh"]["state"], "running", "{health}");
    assert_eq!(health["refresh"]["task_alive"], true, "{health}");
    assert_eq!(health["refresh"]["pending"], false, "{health}");
    assert_eq!(health["refresh"]["last_error"], Value::Null, "{health}");
    assert_eq!(
        health["refresh"]["dirty_epoch"], health["refresh"]["reconciled_epoch"],
        "{health}"
    );
}

// Run in a separate test process so a mistaken stale-PID signal is observable
// without terminating the process which reports this regression's assertions.
#[test]
#[ignore = "subprocess fixture for legacy worker recovery"]
fn unrelated_worker_pid_fixture() {
    let Some(ready) = std::env::var_os("CODEBASE_GRAPH_UNRELATED_READY") else {
        return;
    };
    fs::write(ready, std::process::id().to_string()).unwrap();
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).unwrap();
}

#[test]
fn legacy_worker_pid_recovers_without_signalling_and_respects_writer_ownership() {
    legacy_worker_pid_recovery(false);
}

#[test]
fn manual_publication_does_not_prevent_later_legacy_pid_recovery() {
    legacy_worker_pid_recovery(true);
}

fn legacy_worker_pid_recovery(manual_publication: bool) {
    use fs2::FileExt;

    let root = temp_repo();
    fs::write(
        root.join("initial.py"),
        "def initial_symbol():\n    return 1\n",
    )
    .unwrap();
    let config = install(&root);
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
    let ready = root.join("unrelated.ready");
    let mut unrelated = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "unrelated_worker_pid_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("CODEBASE_GRAPH_UNRELATED_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_file(&ready);
    let storage = root.join(".codebaseGraph/storage");
    let state_path = storage.join("worker.json");
    let legacy_state = serde_json::to_vec(&json!({
        "version": 1,
        "build_id": "abandoned-worker",
        "worker_pid": unrelated.0.id()
    }))
    .unwrap();
    fs::write(&state_path, &legacy_state).unwrap();
    fs::write(
        root.join("latest.py"),
        "def latest_symbol():\n    return 2\n",
    )
    .unwrap();
    let (pending_symbol, pending_path) = if manual_publication {
        let before: Value =
            serde_json::from_slice(&fs::read(storage.join("active.json")).unwrap()).unwrap();
        ManualBuild::start(&root).wait_for_success();
        let published: Value =
            serde_json::from_slice(&fs::read(storage.join("active.json")).unwrap()).unwrap();
        assert_ne!(published["generation_id"], before["generation_id"]);
        assert!(unrelated.0.try_wait().unwrap().is_none());
        // Reinstate the fault after publication so daemon recovery starts with
        // legacy state even if a manual build has already retired the record.
        fs::write(&state_path, &legacy_state).unwrap();
        // This edit precedes monitoring. Recovery must publish it without a
        // second edit or manual deletion of ownership metadata.
        fs::write(
            root.join("after_manual.py"),
            "def after_manual_symbol():\n    return 4\n",
        )
        .unwrap();
        ("after_manual_symbol", "after_manual.py")
    } else {
        ("latest_symbol", "latest.py")
    };
    let previous: Value =
        serde_json::from_slice(&fs::read(storage.join("active.json")).unwrap()).unwrap();
    let writer = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(storage.join("writer.lock"))
        .unwrap();
    writer.lock_exclusive().unwrap();
    assert!(
        fs::read(&state_path).unwrap() == legacy_state,
        "legacy PID-only state must remain present before daemon startup"
    );
    let (daemon, port, _) = start_daemon(&root, &config, &root, &[]);
    let mut session = McpSession::start(port);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut passed_legacy_recovery = false;
    let mut last_ownership = Value::Null;
    let mut last_recovery_error = None;
    let mut next_health_poll = Instant::now();
    while Instant::now() < deadline {
        let state: Value = fs::read(&state_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        // Ownership diagnostics deliberately exclude private v2 capabilities.
        last_ownership = json!({
            "version": state["version"],
            "build_id": state["build_id"],
            "worker_pid": state["worker_pid"]
        });
        if state["version"] == 2 {
            passed_legacy_recovery = true;
            break;
        }
        assert!(
            unrelated.0.try_wait().unwrap().is_none(),
            "recovery signalled the unrelated process"
        );
        if Instant::now() >= next_health_poll {
            if let Some(error) = session.health()["refresh"]["last_error"].as_str() {
                last_recovery_error = Some(error.to_owned());
            }
            next_health_poll = Instant::now() + Duration::from_millis(250);
        }
        thread::sleep(Duration::from_millis(50));
    }
    if !passed_legacy_recovery {
        let health = session.health();
        panic!("a live unrelated PID blocked legacy metadata recovery; ownership={last_ownership}; last_recovery_error={last_recovery_error:?}; health={health}");
    }
    let held: Value =
        serde_json::from_slice(&fs::read(storage.join("active.json")).unwrap()).unwrap();
    assert_eq!(
        held["generation_id"], previous["generation_id"],
        "refresh bypassed writer ownership"
    );
    assert!(unrelated.0.try_wait().unwrap().is_none());
    assert!(session
        .search_paths("initial_symbol")
        .iter()
        .any(|path| path == "initial.py"));
    if manual_publication {
        assert!(session
            .search_paths("latest_symbol")
            .iter()
            .any(|path| path == "latest.py"));
    }
    FileExt::unlock(&writer).unwrap();
    drop(writer);

    let recovered = wait_for_current_symbol(&mut session, pending_symbol, pending_path);
    assert_recovered_refresh(&recovered);
    assert_ne!(recovered["active_generation"], previous["generation_id"]);
    assert!(unrelated.0.try_wait().unwrap().is_none());
    fs::write(
        root.join("follow_up.py"),
        "def follow_up_symbol():\n    return 3\n",
    )
    .unwrap();
    let followed = wait_for_current_symbol(&mut session, "follow_up_symbol", "follow_up.py");
    assert_recovered_refresh(&followed);
    assert!(
        unrelated.0.try_wait().unwrap().is_none(),
        "a later refresh signalled the unrelated process"
    );

    drop(session);
    drop(daemon);
    drop(unrelated);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn retired_internal_roles_require_restart_before_reading_requests() {
    let root = temp_repo();
    let nonexistent_request = root.join("must-not-read.json");
    let nonexistent_result = root.join("must-not-write.json");
    for role in ["materialization-worker-v1", "ladybug-write-phase-v1"] {
        let mut command = Command::new(binary());
        command
            .current_dir(&root)
            .args(["__codebase_graph_internal", role])
            .arg(&nonexistent_request);
        if role == "materialization-worker-v1" {
            command.arg(&nonexistent_result);
        }
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "retired role {role} unexpectedly ran"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.to_ascii_lowercase().contains("restart"),
            "{role} did not report the restart requirement: {stderr}"
        );
        assert!(!nonexistent_request.exists());
        assert!(!nonexistent_result.exists());
    }
    let _ = fs::remove_dir_all(root);
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
    // Native process suites run concurrently in CI. Keep the recovery bound
    // independent of the short retry cadence and allow the large source to
    // finish loading while other fixtures are building.
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut recovered = None;
    let mut last_health = Value::Null;
    while Instant::now() < deadline {
        let health = session.health();
        last_health = health.clone();
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
    let (recovered, latest_paths) = recovered.unwrap_or_else(|| {
        panic!("refresh should recover without another source edit: {last_health}")
    });
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
