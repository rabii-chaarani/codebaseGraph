//! Small, bounded HTTP/MCP client used by local integrations.
//!
//! This module deliberately only accepts the repository daemon's IPv4 loopback
//! endpoint.  It does not start a process, open a graph database, or refresh a
//! repository; callers are expected to use the managed daemon selected by the
//! install config.

use crate::api::{HOOK_TIMEOUT_HEADER, MAX_HOOK_TIMEOUT};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct McpHttpResponse {
    pub(crate) status: u16,
    pub(crate) payload: Value,
    pub(crate) headers: BTreeMap<String, String>,
}

/// A short-lived Streamable HTTP MCP session. Hook invocations use one
/// session for health and search so a prompt does not pay two handshakes.
pub(crate) struct McpLoopbackSession {
    endpoint: String,
    session_id: String,
}

pub(crate) fn endpoint_port(endpoint: &str) -> Result<u16, String> {
    let authority = endpoint
        .strip_prefix("http://127.0.0.1:")
        .ok_or_else(|| "managed MCP endpoint must use http://127.0.0.1:<port>".to_string())?;
    let (port, path) = authority
        .split_once('/')
        .ok_or_else(|| "managed MCP endpoint must include the /mcp path".to_string())?;
    let port = port
        .parse::<u16>()
        .map_err(|_| "managed MCP endpoint port is invalid".to_string())?;
    if port == 0 || path != "mcp" {
        return Err("managed MCP endpoint must use a non-zero /mcp port".to_string());
    }
    Ok(port)
}

pub(crate) fn loopback_http_json_request(
    endpoint: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
    timeout: Duration,
) -> Result<McpHttpResponse, String> {
    loopback_http_json_request_with_hook_deadline(
        endpoint, method, path, headers, body, timeout, None,
    )
}

fn loopback_http_json_request_with_hook_deadline(
    endpoint: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
    timeout: Duration,
    hook_deadline: Option<Instant>,
) -> Result<McpHttpResponse, String> {
    let started = Instant::now();
    let request_deadline = started
        .checked_add(timeout)
        .ok_or_else(|| "managed MCP request timeout is out of range".to_string())?;
    let io_deadline = hook_deadline
        .map(|deadline| deadline.min(request_deadline))
        .unwrap_or(request_deadline);
    let port = endpoint_port(endpoint)?;
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();

    let remaining = remaining_until(io_deadline, "managed MCP request")?;
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = TcpStream::connect_timeout(&address, remaining)
        .map_err(|error| format_io_error("failed to connect to managed MCP daemon", error))?;

    // Compute hook metadata only after connect and directly before writing the
    // request. This keeps the header aligned with the budget that remains when
    // the server receives the call, instead of the budget at hook dispatch.
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    if let Some(timeout_ms) = hook_deadline
        .map(|deadline| hook_timeout_header_value(deadline, io_deadline))
        .transpose()?
    {
        request.push_str(HOOK_TIMEOUT_HEADER);
        request.push_str(": ");
        request.push_str(&timeout_ms);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    write_all_until(
        &mut stream,
        request.as_bytes(),
        io_deadline,
        "failed to write managed MCP request",
    )?;
    write_all_until(
        &mut stream,
        &body,
        io_deadline,
        "failed to write managed MCP request body",
    )?;

    let response = read_http_response(&mut stream, io_deadline)?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "managed MCP daemon returned an invalid HTTP response".to_string())?;
    let head = String::from_utf8_lossy(&response[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(500);
    let payload =
        serde_json::from_slice::<Value>(&response[split + 4..]).unwrap_or_else(|_| json!({}));
    let response_headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    Ok(McpHttpResponse {
        status,
        payload,
        headers: response_headers,
    })
}

fn remaining_until(deadline: Instant, operation: &str) -> Result<Duration, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(format!("{operation} exceeded its deadline"));
    }
    Ok(remaining)
}

fn hook_timeout_header_value(
    hook_deadline: Instant,
    io_deadline: Instant,
) -> Result<String, String> {
    let hook_remaining = hook_deadline.saturating_duration_since(Instant::now());
    let io_remaining = io_deadline.saturating_duration_since(Instant::now());
    let remaining = hook_remaining.min(io_remaining).min(MAX_HOOK_TIMEOUT);
    let millis = remaining.as_millis();
    if millis == 0 {
        return Err("managed MCP tool call exceeded its deadline".to_string());
    }
    Ok(millis.to_string())
}

fn format_io_error(operation: &str, error: std::io::Error) -> String {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        format!("{operation}: managed MCP request exceeded its deadline")
    } else {
        format!("{operation}: {error}")
    }
}

fn write_all_until(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    deadline: Instant,
    operation: &str,
) -> Result<(), String> {
    while !bytes.is_empty() {
        let remaining = remaining_until(deadline, "managed MCP request")?;
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|error| format_io_error(operation, error))?;
        match stream.write(bytes) {
            Ok(0) => return Err(format!("{operation}: connection closed while writing")),
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format_io_error(operation, error)),
        }
    }
    Ok(())
}

fn read_http_response(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];
    let split = loop {
        let read_limit = (MAX_RESPONSE_BYTES + 1 - response.len()).min(buffer.len());
        if read_limit == 0 {
            return Err("managed MCP daemon response exceeded the bounded size".to_string());
        }
        let count = read_until(stream, &mut buffer[..read_limit], deadline)?;
        if count == 0 {
            return Err("managed MCP daemon returned an invalid HTTP response".to_string());
        }
        response.extend_from_slice(&buffer[..count]);
        if let Some(split) = response.windows(4).position(|window| window == b"\r\n\r\n") {
            break split;
        }
    };

    let head = String::from_utf8_lossy(&response[..split]);
    let mut content_length = None;
    let mut chunked = false;
    for line in head.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            } else if name.trim().eq_ignore_ascii_case("transfer-encoding") {
                chunked = value
                    .split(',')
                    .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"));
            }
        }
    }
    let body_start = split + 4;
    if let Some(content_length) = content_length.filter(|_| !chunked) {
        let expected_len = body_start
            .checked_add(content_length)
            .filter(|length| *length <= MAX_RESPONSE_BYTES)
            .ok_or_else(|| "managed MCP daemon response exceeded the bounded size".to_string())?;
        if response.len() > expected_len {
            response.truncate(expected_len);
        }
        while response.len() < expected_len {
            let read_limit = (expected_len - response.len()).min(buffer.len());
            let count = read_until(stream, &mut buffer[..read_limit], deadline)?;
            if count == 0 {
                return Err("managed MCP daemon returned an incomplete HTTP response".to_string());
            }
            response.extend_from_slice(&buffer[..count]);
        }
    } else {
        while response.len() <= MAX_RESPONSE_BYTES {
            let read_limit = (MAX_RESPONSE_BYTES + 1 - response.len()).min(buffer.len());
            let count = read_until(stream, &mut buffer[..read_limit], deadline)?;
            if count == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..count]);
        }
        if response.len() > MAX_RESPONSE_BYTES {
            return Err("managed MCP daemon response exceeded the bounded size".to_string());
        }
    }
    Ok(response)
}

fn read_until(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<usize, String> {
    loop {
        let remaining = remaining_until(deadline, "managed MCP response")?;
        stream.set_read_timeout(Some(remaining)).map_err(|error| {
            format_io_error("failed to read managed MCP daemon response", error)
        })?;
        match stream.read(buffer) {
            Ok(count) => return Ok(count),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(format_io_error(
                    "failed to read managed MCP daemon response",
                    error,
                ));
            }
        }
    }
}

fn verify_health_identity(
    endpoint: &str,
    expected_fingerprint: Option<&str>,
    expected_repo_root: Option<&Path>,
    timeout: Duration,
) -> Result<(), String> {
    let health = loopback_http_json_request(
        endpoint,
        "GET",
        "/_codebasegraph/health",
        &[],
        None,
        timeout,
    )?;
    if health.status / 100 != 2
        || health.payload.get("server").and_then(Value::as_str) != Some("codebase-graph")
    {
        return Err("managed MCP daemon health identity is invalid".to_string());
    }
    if let Some(expected) = expected_fingerprint {
        if health
            .payload
            .get("repository_fingerprint")
            .and_then(Value::as_str)
            != Some(expected)
        {
            return Err(
                "managed MCP daemon repository fingerprint does not match setup config".to_string(),
            );
        }
    }
    // The daemon's transport health response intentionally does not expose the
    // root.  graph_health below carries the structured root and is checked by
    // call_mcp_tool when one is supplied.
    let _ = expected_repo_root;
    Ok(())
}

pub(crate) fn call_mcp_tool(
    endpoint: &str,
    expected_fingerprint: Option<&str>,
    expected_repo_root: Option<&Path>,
    tool_name: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    let session = McpLoopbackSession::connect(endpoint, expected_fingerprint, timeout)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("managed MCP tool call exceeded its deadline".to_string());
    }
    session.call_tool(tool_name, arguments, expected_repo_root, remaining)
}

impl McpLoopbackSession {
    pub(crate) fn connect(
        endpoint: &str,
        expected_fingerprint: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, String> {
        let deadline = Instant::now() + timeout;
        verify_health_identity(endpoint, expected_fingerprint, None, timeout)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("managed MCP initialize exceeded its deadline".to_string());
        }
        let initialized = loopback_http_json_request(
            endpoint,
            "POST",
            "/mcp",
            &[],
            Some(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": crate::api::CodebaseGraphApi::latest_mcp_protocol_version()}
            })),
            remaining,
        )?;
        if initialized.status / 100 != 2 {
            return Err(format!(
                "managed MCP initialize returned HTTP {}",
                initialized.status
            ));
        }
        let session_id = initialized
            .headers
            .get("mcp-session-id")
            .ok_or_else(|| {
                "managed MCP initialize response did not return a session ID".to_string()
            })?
            .clone();
        Ok(Self {
            endpoint: endpoint.to_string(),
            session_id,
        })
    }

    pub(crate) fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
        expected_repo_root: Option<&Path>,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.call_tool_inner(tool_name, arguments, expected_repo_root, timeout, None)
    }

    pub(crate) fn call_tool_with_deadline(
        &self,
        tool_name: &str,
        arguments: Value,
        expected_repo_root: Option<&Path>,
        timeout: Duration,
        deadline: Instant,
    ) -> Result<Value, String> {
        if !matches!(tool_name, "graph_health" | "graph_search") {
            return Err(
                "hook deadlines are only supported for graph_health and graph_search".to_string(),
            );
        }
        self.call_tool_inner(
            tool_name,
            arguments,
            expected_repo_root,
            timeout,
            Some(deadline),
        )
    }

    fn call_tool_inner(
        &self,
        tool_name: &str,
        arguments: Value,
        expected_repo_root: Option<&Path>,
        timeout: Duration,
        hook_deadline: Option<Instant>,
    ) -> Result<Value, String> {
        let response = loopback_http_json_request_with_hook_deadline(
            &self.endpoint,
            "POST",
            "/mcp",
            &[
                ("mcp-session-id", self.session_id.as_str()),
                (
                    "mcp-protocol-version",
                    crate::api::CodebaseGraphApi::latest_mcp_protocol_version(),
                ),
            ],
            Some(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": tool_name, "arguments": arguments}
            })),
            timeout,
            hook_deadline,
        )?;
        if response.status / 100 != 2 {
            return Err(format!(
                "managed MCP tool call returned HTTP {}",
                response.status
            ));
        }
        let result = response
            .payload
            .get("result")
            .cloned()
            .unwrap_or(response.payload);
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            let message = result
                .pointer("/structuredContent/error/message")
                .and_then(Value::as_str)
                .or_else(|| result.pointer("/content/0/text").and_then(Value::as_str))
                .unwrap_or("managed MCP tool returned an error")
                .to_string();
            let retryable = result
                .pointer("/structuredContent/error/retryable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            return Err(if retryable {
                format!("retryable MCP error: {message}")
            } else {
                message
            });
        }
        if let Some(expected_root) = expected_repo_root {
            if tool_name == "graph_health" {
                let actual = result
                    .pointer("/structuredContent/repo_root")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        "graph_health response did not include repository root".to_string()
                    })?;
                let actual = Path::new(actual).canonicalize().map_err(|error| {
                    format!("graph_health repository root is unreadable: {error}")
                })?;
                let expected = expected_root
                    .canonicalize()
                    .map_err(|error| format!("expected repository root is unreadable: {error}"))?;
                if actual != expected {
                    return Err(format!(
                        "graph_health repository root {} does not match expected {}",
                        actual.display(),
                        expected.display()
                    ));
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn endpoint_validation_rejects_non_loopback_and_wrong_path() {
        assert!(endpoint_port("http://localhost:41000/mcp").is_err());
        assert!(endpoint_port("http://127.0.0.1:41000/other").is_err());
        assert_eq!(endpoint_port("http://127.0.0.1:41000/mcp").unwrap(), 41000);
    }

    fn fake_mcp_server() -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                if stream.read_exact(&mut byte).is_err() {
                    return;
                }
                request.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&request).into_owned();
            let content_length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let body_start = request.len();
            request.resize(body_start + content_length, 0);
            if stream.read_exact(&mut request[body_start..]).is_err() {
                return;
            }
            sender.send(head).unwrap();
            let body = br#"{"jsonrpc":"2.0","id":2,"result":{"structuredContent":{"ok":true}}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        (format!("http://127.0.0.1:{port}/mcp"), receiver, worker)
    }

    fn hook_timeout_header(request: &str) -> Option<u64> {
        request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(HOOK_TIMEOUT_HEADER)
                .then(|| value.trim().parse::<u64>().ok())
                .flatten()
        })
    }

    fn test_session(endpoint: String) -> McpLoopbackSession {
        McpLoopbackSession {
            endpoint,
            session_id: "test-session".to_string(),
        }
    }

    #[test]
    fn hook_tool_calls_send_clamped_remaining_deadline_and_regular_calls_omit_it() {
        let (endpoint, request, worker) = fake_mcp_server();
        test_session(endpoint)
            .call_tool_with_deadline(
                "graph_health",
                json!({}),
                None,
                Duration::from_secs(2),
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        let clamped = hook_timeout_header(&request.recv().unwrap()).unwrap();
        worker.join().unwrap();
        assert!((1..=900).contains(&clamped));
        assert!(clamped >= 800, "expected the 900 ms cap, got {clamped} ms");

        let (endpoint, request, worker) = fake_mcp_server();
        test_session(endpoint)
            .call_tool_with_deadline(
                "graph_search",
                json!({}),
                None,
                Duration::from_secs(1),
                Instant::now() + Duration::from_millis(180),
            )
            .unwrap();
        let remaining = hook_timeout_header(&request.recv().unwrap()).unwrap();
        worker.join().unwrap();
        assert!((1..=180).contains(&remaining));

        let (endpoint, request, worker) = fake_mcp_server();
        test_session(endpoint)
            .call_tool("graph_health", json!({}), None, Duration::from_secs(1))
            .unwrap();
        let regular = request.recv().unwrap();
        worker.join().unwrap();
        assert_eq!(hook_timeout_header(&regular), None);
    }

    #[test]
    fn expired_hook_budget_does_not_connect() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://127.0.0.1:{}/mcp",
            listener.local_addr().unwrap().port()
        );
        let error = test_session(endpoint)
            .call_tool_with_deadline(
                "graph_search",
                json!({}),
                None,
                Duration::from_secs(2),
                Instant::now() - Duration::from_millis(1),
            )
            .unwrap_err();
        assert!(error.contains("deadline"));
        assert!(matches!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn trickled_response_cannot_extend_the_elapsed_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                if stream.read_exact(&mut byte).is_err() {
                    return;
                }
                request.push(byte[0]);
            }
            let body = br#"{"value":"a deliberately slow response"}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            for byte in body {
                thread::sleep(Duration::from_millis(55));
                if stream.write_all(&[*byte]).is_err() {
                    return;
                }
            }
        });
        let started = Instant::now();
        let result = loopback_http_json_request(
            &format!("http://127.0.0.1:{port}/mcp"),
            "GET",
            "/health",
            &[],
            None,
            Duration::from_millis(250),
        );
        let elapsed = started.elapsed();
        assert!(result.is_err(), "slow response unexpectedly completed");
        assert!(
            elapsed < Duration::from_millis(750),
            "deadline took {elapsed:?}"
        );
        worker.join().unwrap();
    }

    #[test]
    fn zero_timeout_does_not_connect() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://127.0.0.1:{}/mcp",
            listener.local_addr().unwrap().port()
        );
        let error =
            loopback_http_json_request(&endpoint, "GET", "/health", &[], None, Duration::ZERO)
                .unwrap_err();
        assert!(error.contains("deadline"));
        assert!(matches!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        ));
    }
}
