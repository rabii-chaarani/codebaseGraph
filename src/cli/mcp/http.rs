use super::{
    options::McpHttpOptions,
    protocol::{handle_mcp_message, is_supported_protocol_version, parse_mcp_payload, rpc_error},
};
use crate::cli::{constants::MAX_HTTP_BODY_BYTES, install::McpHttpState};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

pub(in crate::cli) fn serve_mcp_http(options: &McpHttpOptions) -> Result<(), String> {
    let listener = options.bind_listener()?;
    serve_mcp_http_listener(options, listener, None)
}

pub(in crate::cli) fn serve_mcp_http_listener(
    options: &McpHttpOptions,
    listener: TcpListener,
    max_requests: Option<usize>,
) -> Result<(), String> {
    let mut state = McpHttpState::default();
    let mut handled = 0_usize;
    loop {
        if max_requests.is_some_and(|limit| handled >= limit) {
            break;
        }
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("failed to accept MCP HTTP request: {error}"))?;
        if let Err(error) = handle_mcp_http_stream(options, &mut state, &mut stream) {
            let _ = write_http_json(
                &mut stream,
                500,
                &rpc_error(serde_json::Value::Null, -32000, &error),
                &[],
            );
        }
        handled += 1;
    }
    Ok(())
}

pub(in crate::cli) fn handle_mcp_http_stream(
    options: &McpHttpOptions,
    state: &mut McpHttpState,
    stream: &mut TcpStream,
) -> Result<(), String> {
    let request = read_http_request(stream)?;
    let response = handle_mcp_http_request(options, state, request);
    write_http_json(
        stream,
        response.status,
        &response.payload,
        &response.headers,
    )
}

pub(in crate::cli) fn handle_mcp_http_request(
    options: &McpHttpOptions,
    state: &mut McpHttpState,
    request: HttpRequest,
) -> HttpResponse {
    if request.path != options.endpoint_path {
        return HttpResponse::json(
            404,
            rpc_error(serde_json::Value::Null, -32601, "MCP endpoint not found"),
        );
    }
    if request.method != "POST" {
        return HttpResponse {
            status: 405,
            payload: json!({}),
            headers: vec![("Allow".to_string(), "POST".to_string())],
        };
    }
    if !valid_http_origin(request.header("origin")) {
        return HttpResponse::json(
            403,
            rpc_error(serde_json::Value::Null, -32000, "Forbidden origin"),
        );
    }
    if let Some(auth_token) = options.auth_token.as_deref() {
        let authorization = request.header("authorization").unwrap_or("");
        if authorization.strip_prefix("Bearer ") != Some(auth_token) {
            return HttpResponse {
                status: 401,
                payload: rpc_error(serde_json::Value::Null, -32000, "Unauthorized"),
                headers: vec![("WWW-Authenticate".to_string(), "Bearer".to_string())],
            };
        }
    }
    if let Some(protocol) = request.header("mcp-protocol-version") {
        if !is_supported_protocol_version(protocol) {
            return HttpResponse::json(
                400,
                json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32602,
                        "message": "Unsupported MCP protocol version",
                        "data": {
                            "supported": ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"],
                            "requested": protocol,
                        },
                    },
                }),
            );
        }
    }
    if request.body_too_large {
        return HttpResponse::json(
            413,
            json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32000,
                    "message": "MCP request body is too large",
                    "data": {"max_bytes": MAX_HTTP_BODY_BYTES},
                },
            }),
        );
    }
    let message = match parse_mcp_payload(&request.body) {
        Ok(message) => message,
        Err(error) => {
            return HttpResponse::json(
                400,
                rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )
        }
    };
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let request_id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let session_id = request.header("mcp-session-id");
    let (resolved_session_id, session) = if method == "initialize" {
        let id = session_id
            .filter(|id| state.sessions.contains_key(*id))
            .map(str::to_string)
            .unwrap_or_else(|| state.next_session_id());
        let session = state.sessions.entry(id.clone()).or_default();
        (id, session)
    } else {
        match session_id.and_then(|id| {
            state
                .sessions
                .get_mut(id)
                .map(|session| (id.to_string(), session))
        }) {
            Some((id, session)) => (id, session),
            None => {
                return HttpResponse::json(
                    400,
                    rpc_error(request_id, -32002, "MCP session is not initialized"),
                )
            }
        }
    };
    match handle_mcp_message(message, session, &options.serve) {
        Some(payload) => {
            let headers = if method == "initialize" {
                vec![("Mcp-Session-Id".to_string(), resolved_session_id)]
            } else {
                Vec::new()
            };
            HttpResponse {
                status: 200,
                payload,
                headers,
            }
        }
        None => HttpResponse {
            status: 202,
            payload: json!({}),
            headers: Vec::new(),
        },
    }
}

pub(in crate::cli) fn valid_http_origin(origin: Option<&str>) -> bool {
    match origin.and_then(http_origin_host) {
        None => true,
        Some(host) => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
    }
}

pub(in crate::cli) fn http_origin_host(origin: &str) -> Option<String> {
    let after_scheme = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    if authority.starts_with('[') {
        return authority
            .split_once(']')
            .map(|(host, _)| host.trim_start_matches('[').to_string());
    }
    let host = authority.split(':').next().unwrap_or(authority).trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

pub(in crate::cli) fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read HTTP request: {error}"))?;
        if read == 0 {
            return Err("HTTP request ended before headers were complete".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() > MAX_HTTP_BODY_BYTES {
            return Err("HTTP headers exceed maximum MCP request size".to_string());
        }
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = headers.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "HTTP request is missing a request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let raw_path = request_parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or(raw_path).to_string();
    let mut header_map = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            header_map.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let length = match header_map.get("content-length") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "Content-Length must be an integer".to_string())?,
        None => 0,
    };
    if length > MAX_HTTP_BODY_BYTES {
        return Ok(HttpRequest {
            method,
            path,
            headers: header_map,
            body: Vec::new(),
            body_too_large: true,
        });
    }
    let body_start = header_end + 4;
    let mut body = buffer.get(body_start..).unwrap_or(&[]).to_vec();
    while body.len() < length {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read HTTP body: {error}"))?;
        if read == 0 {
            return Err("HTTP request ended before Content-Length bytes were read".to_string());
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);
    Ok(HttpRequest {
        method,
        path,
        headers: header_map,
        body,
        body_too_large: false,
    })
}

pub(in crate::cli) fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

pub(in crate::cli) fn write_http_json(
    stream: &mut TcpStream,
    status: u16,
    payload: &serde_json::Value,
    headers: &[(String, String)],
) -> Result<(), String> {
    let body = if status == 202 || status == 405 {
        Vec::new()
    } else {
        serde_json::to_vec(payload).map_err(|error| error.to_string())?
    };
    let reason = http_reason(status);
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    )
    .map_err(|error| error.to_string())?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").map_err(|error| error.to_string())?;
    }
    write!(stream, "\r\n").map_err(|error| error.to_string())?;
    stream.write_all(&body).map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())
}

pub(in crate::cli) fn http_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Internal Server Error",
    }
}

#[derive(Debug)]
pub(in crate::cli) struct HttpRequest {
    pub(in crate::cli) method: String,
    pub(in crate::cli) path: String,
    pub(in crate::cli) headers: BTreeMap<String, String>,
    pub(in crate::cli) body: Vec<u8>,
    pub(in crate::cli) body_too_large: bool,
}

impl HttpRequest {
    pub(in crate::cli) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug)]
pub(in crate::cli) struct HttpResponse {
    pub(in crate::cli) status: u16,
    pub(in crate::cli) payload: serde_json::Value,
    pub(in crate::cli) headers: Vec<(String, String)>,
}

impl HttpResponse {
    pub(in crate::cli) fn json(status: u16, payload: serde_json::Value) -> Self {
        Self {
            status,
            payload,
            headers: Vec::new(),
        }
    }
}

pub(in crate::cli) fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}
