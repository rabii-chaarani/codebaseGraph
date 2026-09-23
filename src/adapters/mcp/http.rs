use super::{
    options::McpHttpOptions,
    protocol::{
        handle_mcp_message_with_context, is_supported_protocol_version, parse_mcp_payload,
        rpc_error,
    },
    refresh::start_configured_api,
    state::{McpHttpState, SessionIdGenerationError},
};
use crate::api::{ApiError, ExecutionContext, HOOK_TIMEOUT_HEADER, MAX_HOOK_TIMEOUT};
use serde_json::json;
use std::{
    collections::BTreeMap,
    net::TcpListener,
    time::{Duration, Instant},
};

pub(crate) const MAX_HTTP_HEADER_BYTES: usize = 32 * 1024;
pub(crate) const MAX_HTTP_BODY_BYTES: usize = 1_000_000;

pub(crate) fn serve_mcp_http(options: &McpHttpOptions) -> Result<(), String> {
    let listener = options.bind_listener()?;
    let mut options = options.clone();
    options.serve.api = Some(start_configured_api(&options.serve)?);
    serve_mcp_http_listener(&options, listener, None)
}

pub(in crate::adapters) fn serve_mcp_http_listener(
    options: &McpHttpOptions,
    listener: TcpListener,
    max_requests: Option<usize>,
) -> Result<(), String> {
    super::dispatcher::serve_http_dispatcher(listener, options, max_requests, |_, _| None)
}

pub(in crate::adapters) fn handle_mcp_http_request(
    options: &McpHttpOptions,
    state: &mut McpHttpState,
    request: HttpRequest,
) -> HttpResponse {
    let started = Instant::now();
    let message = match validate_mcp_http_request(options, &request) {
        Ok(message) => message,
        Err(response) => return response,
    };
    let execution_context = match request_execution_context(&request, &message, started) {
        Ok(context) => context,
        Err(response) => return response,
    };
    handle_mcp_http_request_with_context(options, state, request, message, execution_context)
}

pub(crate) fn handle_mcp_http_request_with_context(
    options: &McpHttpOptions,
    state: &mut McpHttpState,
    request: HttpRequest,
    message: serde_json::Value,
    execution_context: ExecutionContext,
) -> HttpResponse {
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
    let session_request =
        match resolve_session_request(state, &method, session_id, request_id.clone()) {
            Ok(session_request) => session_request,
            Err(response) => return response,
        };
    let (resolved_session_id, session) = match session_request {
        SessionRequest::Existing(session_id) => {
            let session = state.sessions.entry(session_id.clone()).or_default();
            (session_id, session)
        }
        SessionRequest::Create => {
            match create_session_with(state, request_id.clone(), |bytes| {
                getrandom::fill(bytes).map_err(|_| ())
            }) {
                Ok(session) => session,
                Err(response) => return response,
            }
        }
    };
    match handle_mcp_message_with_context(message, session, &options.serve, execution_context) {
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

pub(crate) fn prepare_deferred_tool_call(
    options: &McpHttpOptions,
    state: &McpHttpState,
    request: &HttpRequest,
    accepted_at: Instant,
) -> Result<Option<(serde_json::Value, ExecutionContext)>, HttpResponse> {
    let message = validate_mcp_http_request(options, request)?;
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let context = request_execution_context(request, &message, accepted_at)?;
    if method != "tools/call" {
        return Ok(None);
    }
    let request_id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let session_request = resolve_session_request(
        state,
        method,
        request.header("mcp-session-id"),
        request_id.clone(),
    )?;
    let SessionRequest::Existing(session_id) = session_request else {
        return Err(session_header_error(400, request_id));
    };
    let session_is_ready = state
        .sessions
        .get(&session_id)
        .is_some_and(|session| session.protocol_version.is_some());
    if !session_is_ready {
        return Err(HttpResponse::json(
            400,
            rpc_error(request_id, -32002, "MCP session is not initialized"),
        ));
    }
    Ok(Some((message, context)))
}

enum SessionRequest {
    Create,
    Existing(String),
}

fn resolve_session_request(
    state: &McpHttpState,
    method: &str,
    session_id: Option<&str>,
    request_id: serde_json::Value,
) -> Result<SessionRequest, HttpResponse> {
    match session_id {
        Some(session_id) if state.sessions.contains_key(session_id) => {
            Ok(SessionRequest::Existing(session_id.to_string()))
        }
        Some(_) => Err(session_header_error(404, request_id)),
        None if method == "initialize" => Ok(SessionRequest::Create),
        None => Err(session_header_error(400, request_id)),
    }
}

fn session_header_error(status: u16, request_id: serde_json::Value) -> HttpResponse {
    HttpResponse::json(
        status,
        rpc_error(request_id, -32002, "MCP session is not initialized"),
    )
}

fn map_session_id_generation(
    result: Result<String, SessionIdGenerationError>,
    request_id: serde_json::Value,
) -> Result<String, HttpResponse> {
    result.map_err(|_| {
        HttpResponse::json(
            500,
            rpc_error(request_id, -32603, "Failed to generate MCP session ID"),
        )
    })
}

fn create_session_with<F, E>(
    state: &mut McpHttpState,
    request_id: serde_json::Value,
    fill_random: F,
) -> Result<(String, &mut super::McpSession), HttpResponse>
where
    F: FnOnce(&mut [u8]) -> Result<(), E>,
{
    let session_id =
        map_session_id_generation(state.next_session_id_with(fill_random), request_id)?;
    let session = state.sessions.entry(session_id.clone()).or_default();
    Ok((session_id, session))
}

pub(crate) fn graph_busy_response(request: &HttpRequest) -> HttpResponse {
    tool_error_response(
        request,
        ApiError::new("graph_busy", "graph executor is busy; retry the request")
            .with_details(json!({"retryable": true}))
            .retryable(true),
    )
}

pub(crate) fn deadline_tool_response(request: &HttpRequest) -> HttpResponse {
    tool_error_response(request, ExecutionContext::expired_error())
}

fn tool_error_response(request: &HttpRequest, error: ApiError) -> HttpResponse {
    let parsed = parse_mcp_payload(&request.body).ok();
    let request_id = parsed
        .as_ref()
        .and_then(|message| message.get("id"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let params = parsed.as_ref().and_then(|message| message.get("params"));
    let tool_name = params
        .and_then(|params| params.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = super::tools::mcp_tool_error_result(tool_name, &arguments, &error).unwrap_or_else(
        |message| json!({"content":[{"type":"text","text":message}],"isError":true}),
    );
    HttpResponse::json(
        200,
        json!({"jsonrpc":"2.0","id":request_id,"result":result}),
    )
}

fn validate_mcp_http_request(
    options: &McpHttpOptions,
    request: &HttpRequest,
) -> Result<serde_json::Value, HttpResponse> {
    if request.path != options.endpoint_path {
        return Err(HttpResponse::json(
            404,
            rpc_error(serde_json::Value::Null, -32601, "MCP endpoint not found"),
        ));
    }
    if request.method != "POST" {
        return Err(HttpResponse {
            status: 405,
            payload: json!({}),
            headers: vec![("Allow".to_string(), "POST".to_string())],
        });
    }
    if !valid_http_origin(request.header("origin")) {
        return Err(HttpResponse::json(
            403,
            rpc_error(serde_json::Value::Null, -32000, "Forbidden origin"),
        ));
    }
    if let Some(auth_token) = options.auth_token.as_deref() {
        let authorization = request.header("authorization").unwrap_or("");
        if authorization.strip_prefix("Bearer ") != Some(auth_token) {
            return Err(HttpResponse {
                status: 401,
                payload: rpc_error(serde_json::Value::Null, -32000, "Unauthorized"),
                headers: vec![("WWW-Authenticate".to_string(), "Bearer".to_string())],
            });
        }
    }
    if let Some(protocol) = request.header("mcp-protocol-version") {
        if !is_supported_protocol_version(protocol) {
            return Err(HttpResponse::json(
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
            ));
        }
    }
    if request.body_too_large {
        return Err(HttpResponse::json(
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
        ));
    }
    parse_mcp_payload(&request.body).map_err(|error| {
        HttpResponse::json(
            400,
            rpc_error(
                serde_json::Value::Null,
                -32700,
                &format!("Invalid JSON-RPC payload: {error}"),
            ),
        )
    })
}

fn request_execution_context(
    request: &HttpRequest,
    message: &serde_json::Value,
    accepted_at: Instant,
) -> Result<ExecutionContext, HttpResponse> {
    let Some(raw_timeout) = request.header(HOOK_TIMEOUT_HEADER) else {
        return Ok(ExecutionContext::default());
    };
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let tool_name = message
        .get("params")
        .and_then(|params| params.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if method != "tools/call" || !matches!(tool_name, "graph_health" | "graph_search") {
        return Err(HttpResponse::json(
            400,
            rpc_error(
                message
                    .get("id")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                -32602,
                "timeout header is supported only for graph_health and graph_search",
            ),
        ));
    }
    let timeout_ms = raw_timeout.parse::<u64>().ok().filter(|value| *value > 0);
    let Some(timeout_ms) = timeout_ms else {
        return Err(HttpResponse::json(
            400,
            rpc_error(
                message
                    .get("id")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                -32602,
                "timeout header must be a positive integer in milliseconds",
            ),
        ));
    };
    let timeout = Duration::from_millis(timeout_ms).min(MAX_HOOK_TIMEOUT);
    Ok(ExecutionContext {
        deadline: Some(accepted_at.checked_add(timeout).unwrap_or(accepted_at)),
    })
}

pub(in crate::adapters) fn valid_http_origin(origin: Option<&str>) -> bool {
    match origin.and_then(http_origin_host) {
        None => true,
        Some(host) => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
    }
}

pub(in crate::adapters) fn http_origin_host(origin: &str) -> Option<String> {
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

pub(in crate::adapters) fn http_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    }
}

#[derive(Debug, Clone)]
pub(in crate::adapters) struct HttpRequest {
    pub(in crate::adapters) method: String,
    pub(in crate::adapters) path: String,
    pub(in crate::adapters) headers: BTreeMap<String, String>,
    pub(in crate::adapters) body: Vec<u8>,
    pub(in crate::adapters) body_too_large: bool,
}

impl HttpRequest {
    pub(in crate::adapters) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug)]
pub(in crate::adapters) struct HttpResponse {
    pub(in crate::adapters) status: u16,
    pub(in crate::adapters) payload: serde_json::Value,
    pub(in crate::adapters) headers: Vec<(String, String)>,
}

impl HttpResponse {
    pub(in crate::adapters) fn json(status: u16, payload: serde_json::Value) -> Self {
        Self {
            status,
            payload,
            headers: Vec::new(),
        }
    }
}

pub(in crate::adapters) fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request_with_timeout(timeout: &str, tool_name: &str) -> HttpRequest {
        let mut request = HttpRequest {
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            headers: BTreeMap::from([(HOOK_TIMEOUT_HEADER.to_string(), timeout.to_string())]),
            body: Vec::new(),
            body_too_large: false,
        };
        request.body = serde_json::to_vec(&json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":tool_name,"arguments":{"query":"term"}}
        }))
        .expect("JSON value serializes");
        request
    }

    fn session_request(headers: &[(&str, &str)], payload: serde_json::Value) -> HttpRequest {
        let headers = headers
            .iter()
            .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
            .collect();
        HttpRequest {
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            headers,
            body: serde_json::to_vec(&payload).expect("JSON value serializes"),
            body_too_large: false,
        }
    }

    #[test]
    fn hook_deadline_is_clamped_and_includes_time_spent_receiving() {
        let request = request_with_timeout("5000", "graph_search");
        let message = json!({"id":1,"method":"tools/call","params":{"name":"graph_search"}});
        let accepted_at = Instant::now();
        let context = request_execution_context(&request, &message, accepted_at)
            .expect("supported hook deadline");
        let remaining = context
            .remaining()
            .expect("fresh budget")
            .expect("budget is set");
        assert!(remaining <= MAX_HOOK_TIMEOUT);

        let expired_at = Instant::now() - MAX_HOOK_TIMEOUT - Duration::from_millis(1);
        let expired = request_execution_context(&request, &message, expired_at)
            .expect("supported hook deadline");
        assert_eq!(expired.remaining().unwrap_err().code, "deadline_exceeded");
    }

    #[test]
    fn hook_deadline_rejects_malformed_or_unbounded_tools() {
        let message = json!({"id":1,"method":"tools/call","params":{"name":"graph_search"}});
        let malformed = request_with_timeout("soon", "graph_search");
        assert_eq!(
            request_execution_context(&malformed, &message, Instant::now())
                .unwrap_err()
                .status,
            400
        );

        let unsupported = request_with_timeout("100", "graph_context");
        let message = json!({"id":1,"method":"tools/call","params":{"name":"graph_context"}});
        assert_eq!(
            request_execution_context(&unsupported, &message, Instant::now())
                .unwrap_err()
                .status,
            400
        );
    }

    #[test]
    fn session_id_entropy_failure_returns_internal_error_without_inserting_session() {
        let mut state = McpHttpState::default();
        let existing = super::super::McpSession {
            protocol_version: Some("2025-11-25".to_string()),
            initialized: true,
        };
        state
            .sessions
            .insert("existing-session".to_string(), existing);

        let response = create_session_with(&mut state, json!(71), |_| Err::<(), _>(()))
            .expect_err("entropy failure must reject allocation");

        assert_eq!(response.status, 500);
        assert_eq!(response.payload["id"], json!(71));
        assert_eq!(response.payload["error"]["code"], -32603);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(
            state.sessions["existing-session"]
                .protocol_version
                .as_deref(),
            Some("2025-11-25")
        );
        assert!(state.sessions["existing-session"].initialized);
    }

    #[test]
    fn session_id_collision_returns_internal_error_without_overwriting_session() {
        let mut state = McpHttpState::default();
        let collision_id = format!("native-http-session-{}", "ab".repeat(32));
        let existing = super::super::McpSession {
            protocol_version: Some("2025-11-25".to_string()),
            initialized: true,
        };
        state.sessions.insert(collision_id.clone(), existing);

        let response = create_session_with(&mut state, json!(72), |bytes| {
            bytes.fill(0xab);
            Ok::<(), ()>(())
        })
        .expect_err("ID collision must reject allocation");

        assert_eq!(response.status, 500);
        assert_eq!(response.payload["id"], json!(72));
        assert_eq!(response.payload["error"]["code"], -32603);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(
            state.sessions[&collision_id].protocol_version.as_deref(),
            Some("2025-11-25")
        );
        assert!(state.sessions[&collision_id].initialized);
    }

    #[test]
    fn deferred_tool_admission_classifies_missing_unknown_valid_and_removed_sessions() {
        let options = McpHttpOptions::parse(&[], "").expect("default HTTP options");
        let mut state = McpHttpState::default();
        let initialize = handle_mcp_http_request(
            &options,
            &mut state,
            session_request(
                &[("mcp-protocol-version", "2025-11-25")],
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {"protocolVersion": "2025-11-25"},
                }),
            ),
        );
        assert_eq!(initialize.status, 200);
        let session_id = initialize
            .headers
            .iter()
            .find(|(name, _)| name == "Mcp-Session-Id")
            .map(|(_, value)| value.clone())
            .expect("initialize returns a session ID");

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "graph_health", "arguments": {}},
        });
        let missing_request = session_request(&[], payload.clone());
        let missing =
            prepare_deferred_tool_call(&options, &state, &missing_request, Instant::now())
                .expect_err("missing session is rejected before admission");
        assert_eq!(missing.status, 400);
        assert_eq!(missing.payload["error"]["code"], -32002);
        assert_eq!(missing.payload["id"], 2);

        let unknown_request = session_request(
            &[("mcp-session-id", "native-http-session-unknown")],
            payload.clone(),
        );
        let unknown =
            prepare_deferred_tool_call(&options, &state, &unknown_request, Instant::now())
                .expect_err("unknown session is rejected before admission");
        assert_eq!(unknown.status, 404);
        assert_eq!(unknown.payload["error"]["code"], -32002);
        assert_eq!(unknown.payload["id"], 2);

        let valid_request = session_request(&[("mcp-session-id", session_id.as_str())], payload);
        assert!(
            prepare_deferred_tool_call(&options, &state, &valid_request, Instant::now())
                .expect("valid session passes admission checks")
                .is_some()
        );

        state.sessions.remove(&session_id);
        let removed = prepare_deferred_tool_call(&options, &state, &valid_request, Instant::now())
            .expect_err("removed session is rejected before admission");
        assert_eq!(removed.status, 404);
        assert_eq!(removed.payload["error"]["code"], -32002);
        assert_eq!(removed.payload["id"], 2);
    }
}
