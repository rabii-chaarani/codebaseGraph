use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_ADMITTED_CONNECTIONS: usize = 32;

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")))
}

struct TempRepo(PathBuf);

impl TempRepo {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "codebase-graph-http-admission-{}-{unique}",
            std::process::id()
        )))
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
    headers: BTreeMap<String, String>,
    body: Value,
}

fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

fn wait_for_state(path: &std::path::Path, child: &mut Child) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("HTTP daemon exited during startup: {status}");
        }
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str(&text) {
                return state;
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!(
        "timed out waiting for HTTP daemon state at {}",
        path.display()
    );
}

fn read_response(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(bytes),
            Ok(read) => bytes.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {
                return Ok(bytes);
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn decode_response(bytes: &[u8]) -> Result<HttpResponse, String> {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "HTTP response had no header terminator".to_string())?;
    let head = String::from_utf8_lossy(&bytes[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| "HTTP response had an invalid status line".to_string())?;
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    let body = serde_json::from_slice(&bytes[split + 4..]).unwrap_or_else(|_| json!({}));
    Ok(HttpResponse {
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
    timeout: Duration,
) -> Result<HttpResponse, String> {
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|error| format!("connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&body))
        .map_err(|error| format!("request write failed: {error}"))?;
    decode_response(&read_response(&mut stream)?)
}

fn health(port: u16) -> Result<HttpResponse, String> {
    request(
        port,
        "GET",
        "/_codebasegraph/health",
        &[],
        None,
        Duration::from_secs(1),
    )
}

fn initialize(port: u16) -> HttpResponse {
    request(
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
        Duration::from_secs(1),
    )
    .unwrap()
}

fn ping(port: u16, session_id: &str) -> HttpResponse {
    request(
        port,
        "POST",
        "/mcp",
        &[
            ("mcp-session-id", session_id),
            ("mcp-protocol-version", MCP_PROTOCOL_VERSION),
        ],
        Some(&json!({"jsonrpc":"2.0", "id":2, "method":"ping"})),
        Duration::from_secs(1),
    )
    .unwrap()
}

fn assert_process_alive(child: &mut Child, pid: u64) {
    assert!(
        child.try_wait().unwrap().is_none(),
        "HTTP daemon process {pid} exited after client saturation"
    );
}

#[test]
fn http_admission_bounds_idle_clients_and_expires_stalled_requests() {
    let root = TempRepo::new();
    fs::create_dir_all(&root.0).unwrap();
    fs::write(root.0.join("service.py"), "def helper():\n    return 1\n").unwrap();

    let install = Command::new(binary())
        .current_dir(&root.0)
        .args([
            "install",
            "--repo-root",
            root.0.to_str().unwrap(),
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
    let config_path = root.0.join(".codebaseGraph/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["refresh"]["policy"] = json!("off");
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    let port = free_port();
    let mut daemon = ChildGuard(
        Command::new(binary())
            .args([
                "mcp",
                "daemon",
                "serve",
                "--config",
                config_path.to_str().unwrap(),
                "--port",
                &port.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let state = wait_for_state(
        &root.0.join(".codebaseGraph/mcp-daemon.json"),
        &mut daemon.0,
    );
    let pid = state["pid"].as_u64().unwrap();
    assert_eq!(state["endpoint"], format!("http://127.0.0.1:{port}/mcp"));

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match health(port) {
            Ok(response) if response.status == 200 && response.body["pid"] == pid => break,
            _ if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            response => panic!("daemon HTTP health did not become ready: {response:?}"),
        }
    }

    let mut idle = Vec::with_capacity(MAX_ADMITTED_CONNECTIONS);
    for _ in 0..MAX_ADMITTED_CONNECTIONS - 1 {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        idle.push(stream);
    }
    // The health request occupies the final slot and confirms the other 31
    // sockets were admitted. Per-connection sleeps can consume the two-second
    // header deadline on a loaded runner and accidentally test a free slot.
    let saturated = health(port).unwrap();
    assert_eq!(
        saturated.body["transport"]["admitted_connections"], MAX_ADMITTED_CONNECTIONS,
        "failed to establish the admission fixture: {saturated:?}"
    );
    assert_eq!(saturated.body["transport"]["deadline_expirations"], 0);
    // Replace the now-closed health connection, then probe the excess slot.
    let final_idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
    final_idle
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    idle.push(final_idle);
    let mut excess = TcpStream::connect(("127.0.0.1", port)).unwrap();
    excess
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let started = Instant::now();
    let mut byte = [0_u8; 1];
    let excess_result = excess.read(&mut byte);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "excess connection was not rejected promptly: {excess_result:?}"
    );
    assert!(
        matches!(excess_result, Ok(0))
            || matches!(
                excess_result,
                Err(ref error)
                    if matches!(error.kind(), std::io::ErrorKind::ConnectionReset)
            ),
        "excess connection was not closed: {excess_result:?}"
    );
    assert_process_alive(&mut daemon.0, pid);

    let idle_deadline = Instant::now() + Duration::from_secs(4);
    for mut stream in idle {
        let response = read_response(&mut stream)
            .unwrap_or_else(|error| panic!("idle client response failed: {error}"));
        assert_eq!(
            decode_response(&response).unwrap().status,
            408,
            "idle client did not receive a header timeout"
        );
        assert!(
            Instant::now() < idle_deadline,
            "idle client expiry took too long"
        );
    }

    let recovered = health(port).unwrap();
    assert_eq!(recovered.status, 200);
    assert_eq!(recovered.body["pid"], pid);
    assert!(
        recovered.body["transport"]["admitted_connections"]
            .as_u64()
            .unwrap()
            <= MAX_ADMITTED_CONNECTIONS as u64
    );
    assert_eq!(recovered.body["transport"]["queued_operations"], 0);
    assert!(
        recovered.body["transport"]["overload_rejections"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        recovered.body["transport"]["deadline_expirations"]
            .as_u64()
            .unwrap()
            >= MAX_ADMITTED_CONNECTIONS as u64
    );

    let initialized = initialize(port);
    assert_eq!(initialized.status, 200);
    let session = initialized.headers["mcp-session-id"].clone();
    let initial_ping = ping(port, &session);
    assert_eq!(initial_ping.status, 200);
    assert_eq!(initial_ping.body["result"], json!({}));

    // A late header fragment must not renew the two-second deadline. An idle
    // timeout restarted by this fragment would expire after 3.1 seconds.
    let mut trickled = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let trickle_started = Instant::now();
    trickled
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    trickled
        .write_all(b"GET /_codebasegraph/health HTTP/1.1\r\nHost: localhost\r\nX-Trickle: ")
        .unwrap();
    thread::sleep(Duration::from_millis(1100));
    assert!(
        trickle_started.elapsed() < Duration::from_millis(1800),
        "runner did not schedule the late-fragment fixture before header expiry"
    );
    trickled.write_all(b"still incomplete").unwrap();
    let trickle_response = read_response(&mut trickled).unwrap();
    let trickle_elapsed = trickle_started.elapsed();
    assert_eq!(decode_response(&trickle_response).unwrap().status, 408);
    assert!(
        trickle_elapsed < Duration::from_millis(2800),
        "trickled header extended its absolute deadline: {trickle_elapsed:?}"
    );
    assert_process_alive(&mut daemon.0, pid);

    // A completed header with an incomplete non-empty body uses the separate
    // five-second request deadline. Control health and the existing session
    // remain usable while that body is unfinished.
    let mut partial_body = TcpStream::connect(("127.0.0.1", port)).unwrap();
    partial_body
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    let body_started = Instant::now();
    partial_body
        .write_all(
            b"POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{",
        )
        .unwrap();
    thread::sleep(Duration::from_millis(40));
    let control_started = Instant::now();
    let control_health = health(port).unwrap();
    assert_eq!(control_health.status, 200);
    assert_eq!(control_health.body["pid"], pid);
    let control_ping = ping(port, &session);
    assert_eq!(control_ping.status, 200);
    assert_eq!(control_ping.body["result"], json!({}));
    assert!(
        control_started.elapsed() < Duration::from_secs(1),
        "health or ping blocked behind an incomplete request body"
    );
    let body_response = decode_response(&read_response(&mut partial_body).unwrap()).unwrap();
    let body_elapsed = body_started.elapsed();
    assert_eq!(body_response.status, 408);
    assert!(
        (Duration::from_secs(4)..Duration::from_secs(7)).contains(&body_elapsed),
        "incomplete request body did not use the absolute five-second deadline: {body_elapsed:?}"
    );
    let final_health = health(port).unwrap();
    assert_eq!(final_health.body["pid"], pid);
    assert_process_alive(&mut daemon.0, pid);
}
