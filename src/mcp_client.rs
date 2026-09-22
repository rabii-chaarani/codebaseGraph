//! Small, bounded HTTP/MCP client used by local integrations.
//!
//! This module deliberately only accepts the repository daemon's IPv4 loopback
//! endpoint.  It does not start a process, open a graph database, or refresh a
//! repository; callers are expected to use the managed daemon selected by the
//! install config.

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
    let port = endpoint_port(endpoint)?;
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = TcpStream::connect_timeout(&address, timeout)
        .map_err(|error| format!("failed to connect to managed MCP daemon: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .map_err(|error| error.to_string())?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").map_err(|error| error.to_string())?;
    }
    write!(stream, "\r\n").map_err(|error| error.to_string())?;
    stream.write_all(&body).map_err(|error| error.to_string())?;

    let mut response = Vec::new();
    stream
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut response)
        .map_err(|error| format!("failed to read managed MCP daemon response: {error}"))?;
    if response.len() > MAX_RESPONSE_BYTES {
        return Err("managed MCP daemon response exceeded the bounded size".to_string());
    }
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
        let response = loopback_http_json_request(
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

    #[test]
    fn endpoint_validation_rejects_non_loopback_and_wrong_path() {
        assert!(endpoint_port("http://localhost:41000/mcp").is_err());
        assert!(endpoint_port("http://127.0.0.1:41000/other").is_err());
        assert_eq!(endpoint_port("http://127.0.0.1:41000/mcp").unwrap(), 41000);
    }
}
