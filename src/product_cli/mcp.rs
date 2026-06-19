use super::*;

pub(super) fn run_mcp_command<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("-h" | "--help") | None => {
            writeln!(stdout, "{}", mcp_help()).map_err(|error| error.to_string())?;
            Ok(())
        }
        Some("install") => run_mcp_install(&args[1..], stdout),
        Some("serve") => Err("mcp serve requires the process stdin/stdout transport; run it through the codebase-graph binary".to_string()),
        Some("http") => Err("mcp http starts a blocking HTTP server; run it through the codebase-graph binary".to_string()),
        Some(command) => Err(format!("unknown mcp command: {command}\n\n{}", mcp_help())),
    }
}
pub(super) fn serve_mcp_stdio<R: BufRead, W: Write>(
    options: &McpServeOptions,
    mut input: R,
    output: &mut W,
) -> Result<(), String> {
    let mut session = McpSession::default();
    while let Some(message) = read_mcp_message(&mut input, output)? {
        if let Some(response) = handle_mcp_message(message, &mut session, options) {
            write_mcp_message(output, &response)?;
        }
    }
    Ok(())
}

pub(super) fn serve_mcp_http(options: &McpHttpOptions) -> Result<(), String> {
    let listener = options.bind_listener()?;
    serve_mcp_http_listener(options, listener, None)
}

pub(super) fn serve_mcp_http_listener(
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

pub(super) fn handle_mcp_http_stream(
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

pub(super) fn handle_mcp_http_request(
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

pub(super) fn handle_mcp_message(
    message: serde_json::Value,
    session: &mut McpSession,
    options: &McpServeOptions,
) -> Option<serde_json::Value> {
    let request_id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if method == "notifications/initialized" {
        session.initialized = true;
        return None;
    }
    if method.starts_with("notifications/") {
        return None;
    }
    if matches!(method, "tools/list" | "tools/call") && session.protocol_version.is_none() {
        return Some(rpc_error(
            request_id,
            -32002,
            "MCP session is not initialized",
        ));
    }
    let result = match method {
        "initialize" => {
            let requested = message
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let protocol_version = negotiate_protocol_version(requested);
            session.protocol_version = Some(protocol_version.to_string());
            Ok(json!({
                "protocolVersion": protocol_version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "codebase_graph", "version": env!("CARGO_PKG_VERSION")},
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => metadata_payload(MCP_TOOL_SPECS_JSON),
        "tools/call" => {
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            mcp_call_tool_result(tool_name, &arguments, options)
        }
        _ => {
            return Some(rpc_error(
                request_id,
                -32601,
                &format!("Unsupported MCP method: {method}"),
            ));
        }
    };
    match result {
        Ok(result) => Some(json!({"jsonrpc": "2.0", "id": request_id, "result": result})),
        Err(error) => Some(rpc_error(request_id, -32602, &error)),
    }
}

pub(super) fn mcp_call_tool_result(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    let payload = mcp_tool_payload(tool_name, arguments, options);
    let output_format = arguments
        .get("output_format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("block");
    let include_structured = arguments
        .get("include_structured_content")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match payload {
        Ok(payload) => {
            let text = if output_format == "json" {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            } else {
                mcp_block_text(tool_name, &payload)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": false,
            });
            if include_structured {
                result["structuredContent"] = payload;
            }
            Ok(result)
        }
        Err(error)
            if tool_name.is_empty() || error.starts_with("Unknown codebaseGraph MCP tool") =>
        {
            Err(error)
        }
        Err(error) => {
            let payload = json!({
                "error": {
                    "tool": tool_name,
                    "type": "ValueError",
                    "message": error,
                }
            });
            let text = if output_format == "json" {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            } else {
                serialize_error_block(&payload)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": true,
            });
            if include_structured {
                result["structuredContent"] = payload;
            }
            Ok(result)
        }
    }
}

pub(super) fn mcp_tool_payload(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    match tool_name {
        "graph_health" => graph_health_payload(options),
        "graph_schema" => metadata_payload(GRAPH_SCHEMA_JSON),
        "graph_query_helpers" => metadata_payload(QUERY_HELPERS_JSON),
        "graph_architecture_queries" => {
            let mut payload = metadata_payload(ARCHITECTURE_QUERIES_JSON)?;
            if let Some(group) = arguments.get("group").and_then(serde_json::Value::as_str) {
                filter_architecture_group(&mut payload, group)?;
            }
            Ok(payload)
        }
        "graph_search" => {
            let search = graph_search_options_from_mcp(arguments, options, true)?;
            let runtime = resolve_health_runtime(&options.health_options())?;
            let results = execute_graph_search(&runtime.db_path, &search)?;
            Ok(json!({
                "query": search.query,
                "profile": search.profile,
                "limit": search.limit,
                "budget": search.budget,
                "results": results,
            }))
        }
        "graph_context" => {
            let context = graph_context_options_from_mcp(arguments, options)?;
            let runtime = resolve_health_runtime(&options.health_options())?;
            if let (Some(node_id), Some(node_type)) =
                (context.node_id.as_ref(), context.node_type.as_ref())
            {
                let rows =
                    execute_graph_context(&runtime.db_path, node_id, node_type, &context.search)?;
                Ok(json!({
                    "node_id": node_id,
                    "node_type": node_type,
                    "profile": context.search.profile,
                    "context": rows,
                }))
            } else {
                let results = execute_graph_search(&runtime.db_path, &context.search)?;
                Ok(json!({
                    "query": context.search.query,
                    "profile": context.search.profile,
                    "limit": context.search.limit,
                    "budget": context.search.budget,
                    "results": results,
                }))
            }
        }
        "graph_query" => {
            let statement = arguments
                .get("statement")
                .or_else(|| arguments.get("query"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();
            if statement.is_empty() {
                return Err("graph_query requires a non-empty statement".to_string());
            }
            validate_read_only_statement(statement)?;
            let parameters = arguments
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let parameters = parameters
                .as_object()
                .ok_or_else(|| "graph_query parameters must be a JSON object".to_string())?;
            let limit = arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(100) as usize;
            if limit == 0 || limit > 1000 {
                return Err("graph_query limit must be between 1 and 1000".to_string());
            }
            let runtime = resolve_health_runtime(&options.health_options())?;
            let (rows, truncated) =
                execute_read_only_query(&runtime.db_path, statement, parameters, limit)?;
            Ok(json!({
                "statement": statement,
                "row_count": rows.len(),
                "rows": rows,
                "truncated": truncated,
            }))
        }
        _ => Err(format!("Unknown codebaseGraph MCP tool: {tool_name}")),
    }
}

pub(super) fn graph_health_payload(options: &McpServeOptions) -> Result<serde_json::Value, String> {
    let runtime = resolve_health_runtime(&options.health_options())?;
    let database_exists = runtime.db_path.exists();
    let manifest_exists = runtime.manifest_path.exists();
    let mut graph_readable = false;
    let mut total_nodes = 0_u64;
    let mut error_message = None;
    if database_exists {
        match count_graph_nodes(&runtime.db_path) {
            Ok(count) => {
                graph_readable = true;
                total_nodes = count;
            }
            Err(error) => error_message = Some(error),
        }
    }
    Ok(json!({
        "ok": database_exists && graph_readable,
        "repo_root": runtime.repo_root,
        "database_path": runtime.db_path,
        "manifest_path": runtime.manifest_path,
        "database_exists": database_exists,
        "manifest_exists": manifest_exists,
        "graph_readable": graph_readable,
        "total_nodes": total_nodes,
        "error": error_message,
    }))
}

pub(super) fn graph_search_options_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
    require_query: bool,
) -> Result<GraphSearchOptions, String> {
    let query = arguments
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    if require_query && query.trim().is_empty() {
        return Err("Search query must not be empty".to_string());
    }
    let detail = arguments
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("standard");
    if detail != "standard" && detail != "slim" {
        return Err("--detail must be standard or slim".to_string());
    }
    Ok(GraphSearchOptions {
        query,
        limit: json_usize(arguments, "limit", 3),
        profile: arguments
            .get("profile")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("brief")
            .to_string(),
        budget: json_usize(arguments, "budget", 600),
        context_limit: json_usize(arguments, "context_limit", 3),
        max_depth: arguments
            .get("max_depth")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize),
        detail: detail.to_string(),
        repo_root: options.repo_root.clone(),
        config: options.config.clone(),
        db: options.db.clone(),
        manifest: options.manifest.clone(),
        output: MetadataOutputOptions {
            format: arguments
                .get("output_format")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("block")
                .to_string(),
            pretty: false,
            help: false,
        },
    })
}

pub(super) fn graph_context_options_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<GraphContextOptions, String> {
    let node_id = arguments
        .get("node_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let node_type = arguments
        .get("node_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if node_id.is_some() != node_type.is_some() {
        return Err(
            "graph-context explicit lookup requires both --node-id and --node-type".to_string(),
        );
    }
    let search = graph_search_options_from_mcp(arguments, options, node_id.is_none())?;
    Ok(GraphContextOptions {
        search,
        node_id,
        node_type,
    })
}

pub(super) fn json_usize(arguments: &serde_json::Value, key: &str, default: usize) -> usize {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

pub(super) fn mcp_block_text(tool_name: &str, payload: &serde_json::Value) -> String {
    match tool_name {
        "graph_health" => serialize_health_block(payload),
        "graph_schema" => serialize_schema_block(payload),
        "graph_query_helpers" => serialize_query_helpers_block(payload),
        "graph_architecture_queries" => serialize_architecture_queries_block(payload),
        "graph_search" => serialize_search_block(payload),
        "graph_context" => {
            if payload.get("context").is_some() {
                serialize_context_block(payload)
            } else {
                serialize_search_block(payload)
            }
        }
        "graph_query" => serialize_query_block(payload),
        _ => serde_json::to_string(payload).unwrap_or_default(),
    }
}

pub(super) fn read_mcp_message<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
) -> Result<Option<serde_json::Value>, String> {
    let mut line = String::new();
    let bytes = input
        .read_line(&mut line)
        .map_err(|error| format!("failed to read MCP frame: {error}"))?;
    if bytes == 0 {
        return Ok(None);
    }
    if line.to_ascii_lowercase().starts_with("content-length:") {
        let length = match line
            .split_once(':')
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        {
            Some(length) => length,
            None => {
                write_mcp_message(
                    output,
                    &rpc_error(
                        serde_json::Value::Null,
                        -32700,
                        "Invalid JSON-RPC payload: Content-Length must be an integer",
                    ),
                )?;
                return Ok(None);
            }
        };
        loop {
            line.clear();
            let bytes = input
                .read_line(&mut line)
                .map_err(|error| format!("failed to read MCP headers: {error}"))?;
            if bytes == 0 || line == "\n" || line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0_u8; length];
        input.read_exact(&mut body).map_err(|error| {
            format!("Body ended before Content-Length bytes were read: {error}")
        })?;
        return parse_mcp_payload(&body).map(Some).or_else(|error| {
            log_mcp_stdio_parse_error(&error);
            write_mcp_message(
                output,
                &rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )?;
            Ok(None)
        });
    }
    parse_mcp_payload(line.as_bytes())
        .map(Some)
        .or_else(|error| {
            log_mcp_stdio_parse_error(&error);
            write_mcp_message(
                output,
                &rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )?;
            Ok(None)
        })
}

pub(super) fn log_mcp_stdio_parse_error(error: &str) {
    eprintln!(
        "{}",
        json!({
            "event": "mcp.stdio_parse_error",
            "message": error,
        })
    );
}

pub(super) fn parse_mcp_payload(data: &[u8]) -> Result<serde_json::Value, String> {
    let payload: serde_json::Value =
        serde_json::from_slice(data).map_err(|error| error.to_string())?;
    if !payload.is_object() {
        return Err("JSON-RPC payload must be an object".to_string());
    }
    Ok(payload)
}

pub(super) fn write_mcp_message<W: Write>(
    output: &mut W,
    message: &serde_json::Value,
) -> Result<(), String> {
    let body = serde_json::to_string(message).map_err(|error| error.to_string())?;
    writeln!(output, "{body}").map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

pub(super) fn negotiate_protocol_version(requested: &str) -> String {
    match requested {
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05" => requested.to_string(),
        _ => LATEST_PROTOCOL_VERSION.to_string(),
    }
}

pub(super) fn rpc_error(
    request_id: serde_json::Value,
    code: i64,
    message: &str,
) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

pub(super) fn is_supported_protocol_version(version: &str) -> bool {
    matches!(
        version,
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05"
    )
}

pub(super) fn valid_http_origin(origin: Option<&str>) -> bool {
    match origin.and_then(http_origin_host) {
        None => true,
        Some(host) => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
    }
}

pub(super) fn http_origin_host(origin: &str) -> Option<String> {
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

pub(super) fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
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

pub(super) fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

pub(super) fn write_http_json(
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

pub(super) fn http_reason(status: u16) -> &'static str {
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
#[derive(Debug, Default)]
pub(super) struct McpSession {
    pub(super) protocol_version: Option<String>,
    pub(super) initialized: bool,
}

#[derive(Debug)]
pub(super) struct McpServeOptions {
    pub(super) repo_root: PathBuf,
    pub(super) config: Option<PathBuf>,
    pub(super) db: Option<PathBuf>,
    pub(super) manifest: Option<PathBuf>,
}

impl McpServeOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: PathBuf::from("."),
            config: None,
            db: None,
            manifest: None,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo-root" => {
                    options.repo_root = PathBuf::from(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--config" => {
                    options.config = Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--db" => {
                    options.db = Some(PathBuf::from(required_arg(args, index, "--db")?));
                    index += 2;
                }
                "--manifest" => {
                    options.manifest =
                        Some(PathBuf::from(required_arg(args, index, "--manifest")?));
                    index += 2;
                }
                other => {
                    return Err(format!(
                        "unknown mcp serve option: {other}\n\n{}",
                        mcp_help()
                    ));
                }
            }
        }
        Ok(options)
    }

    pub(super) fn health_options(&self) -> HealthOptions {
        HealthOptions {
            repo_root: self.repo_root.clone(),
            config: self.config.clone(),
            db: self.db.clone(),
            manifest: self.manifest.clone(),
            help: false,
            json: false,
        }
    }
}

#[derive(Debug)]
pub(super) struct McpHttpOptions {
    pub(super) serve: McpServeOptions,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) endpoint_path: String,
    pub(super) allow_remote: bool,
    pub(super) auth_token: Option<String>,
}

impl McpHttpOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            serve: McpServeOptions {
                repo_root: PathBuf::from("."),
                config: None,
                db: None,
                manifest: None,
            },
            host: "127.0.0.1".to_string(),
            port: 8765,
            endpoint_path: "/mcp".to_string(),
            allow_remote: false,
            auth_token: None,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo-root" => {
                    options.serve.repo_root =
                        PathBuf::from(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--config" => {
                    options.serve.config =
                        Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--db" => {
                    options.serve.db = Some(PathBuf::from(required_arg(args, index, "--db")?));
                    index += 2;
                }
                "--manifest" => {
                    options.serve.manifest =
                        Some(PathBuf::from(required_arg(args, index, "--manifest")?));
                    index += 2;
                }
                "--host" => {
                    options.host = required_arg(args, index, "--host")?.to_string();
                    index += 2;
                }
                "--port" => {
                    options.port = required_arg(args, index, "--port")?
                        .parse::<u16>()
                        .map_err(|_| "--port must be between 0 and 65535".to_string())?;
                    index += 2;
                }
                "--path" => {
                    options.endpoint_path = required_arg(args, index, "--path")?.to_string();
                    if !options.endpoint_path.starts_with('/') {
                        return Err("--path must start with /".to_string());
                    }
                    index += 2;
                }
                "--allow-remote" => {
                    options.allow_remote = true;
                    index += 1;
                }
                "--auth-token" => {
                    options.auth_token =
                        Some(required_arg(args, index, "--auth-token")?.to_string());
                    index += 2;
                }
                "--auth-token-env" => {
                    let name = required_arg(args, index, "--auth-token-env")?;
                    let value = env::var(name).map_err(|_| {
                        format!("Environment variable {name:?} must contain the HTTP bearer token")
                    })?;
                    options.auth_token = Some(value);
                    index += 2;
                }
                other => {
                    return Err(format!(
                        "unknown mcp http option: {other}\n\n{}",
                        mcp_help()
                    ));
                }
            }
        }
        options.validate()?;
        Ok(options)
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self
            .auth_token
            .as_deref()
            .is_some_and(|token| token.trim().is_empty())
        {
            return Err("MCP HTTP auth token must not be blank".to_string());
        }
        if self.allow_remote && self.auth_token.is_none() {
            return Err("MCP HTTP remote bind requires an auth token".to_string());
        }
        if !self.allow_remote && !is_local_host(&self.host) {
            return Err(
                "MCP HTTP transport may only bind to localhost unless allow_remote is enabled"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub(super) fn bind_listener(&self) -> Result<TcpListener, String> {
        self.validate()?;
        TcpListener::bind((self.host.as_str(), self.port))
            .map_err(|error| format!("failed to bind MCP HTTP server: {error}"))
    }
}
#[derive(Debug)]
pub(super) struct HttpRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: BTreeMap<String, String>,
    pub(super) body: Vec<u8>,
    pub(super) body_too_large: bool,
}

impl HttpRequest {
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug)]
pub(super) struct HttpResponse {
    pub(super) status: u16,
    pub(super) payload: serde_json::Value,
    pub(super) headers: Vec<(String, String)>,
}

impl HttpResponse {
    pub(super) fn json(status: u16, payload: serde_json::Value) -> Self {
        Self {
            status,
            payload,
            headers: Vec::new(),
        }
    }
}

pub(super) fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}
