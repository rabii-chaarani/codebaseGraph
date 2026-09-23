use super::{
    http::{
        deadline_tool_response, graph_busy_response, handle_mcp_http_request_with_context,
        prepare_deferred_tool_call, HttpRequest, HttpResponse, MAX_HTTP_BODY_BYTES,
        MAX_HTTP_HEADER_BYTES,
    },
    options::McpHttpOptions,
    state::McpHttpState,
};
use crate::api::{ExecutionContext, MAX_CONNECTIONS, POLL_INTERVAL};
use serde_json::json;
use std::{
    io,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const HEADER_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_READ_PER_TICK: usize = 8 * 1024;
const MAX_WRITE_PER_TICK: usize = 16 * 1024;
const MAX_ACCEPTS_PER_TICK: usize = MAX_CONNECTIONS;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DispatcherSnapshot {
    pub(crate) admitted_connections: usize,
    pub(crate) active_operation: Option<bool>,
    pub(crate) overload_rejections: u64,
    pub(crate) deadline_expirations: u64,
}

pub(crate) struct ControlResponse {
    pub(crate) response: HttpResponse,
    pub(crate) shutdown_after_response: bool,
}

struct Connection {
    id: u64,
    stream: TcpStream,
    accepted_at: Instant,
    request_bytes: Vec<u8>,
    header_end: Option<usize>,
    content_length: Option<usize>,
    request: Option<HttpRequest>,
    waiting_job: Option<WaitingJob>,
    response_bytes: Option<Vec<u8>>,
    response_offset: usize,
    response_ready_at: Option<Instant>,
    close: bool,
}

#[derive(Clone)]
struct WaitingJob {
    deadline: Option<Instant>,
    request: HttpRequest,
}

impl Connection {
    fn new(id: u64, stream: TcpStream) -> Self {
        Self {
            id,
            stream,
            accepted_at: Instant::now(),
            request_bytes: Vec::new(),
            header_end: None,
            content_length: None,
            request: None,
            waiting_job: None,
            response_bytes: None,
            response_offset: 0,
            response_ready_at: None,
            close: false,
        }
    }

    fn set_response(&mut self, response: HttpResponse, now: Instant) {
        match encode_http_response(response) {
            Ok(bytes) => {
                self.response_bytes = Some(bytes);
                self.response_offset = 0;
                self.response_ready_at = Some(now);
            }
            Err(_) => self.close = true,
        }
    }
}

struct ExecutionJob {
    connection_id: u64,
    request: HttpRequest,
    state: McpHttpState,
    message: serde_json::Value,
    context: ExecutionContext,
}

enum WorkerMessage {
    Execute(ExecutionJob),
    Stop,
}

struct WorkerResult {
    connection_id: u64,
    response: HttpResponse,
}

struct ExecutionWorker {
    sender: SyncSender<WorkerMessage>,
    receiver: Receiver<WorkerResult>,
    thread: Option<JoinHandle<()>>,
}

impl ExecutionWorker {
    fn start(options: McpHttpOptions) -> Result<Self, String> {
        let (sender, jobs) = mpsc::sync_channel(1);
        let (results, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("mcp-http-executor".to_string())
            .spawn(move || execution_worker_loop(options, jobs, results))
            .map_err(|error| format!("failed to start MCP HTTP execution worker: {error}"))?;
        Ok(Self {
            sender,
            receiver,
            thread: Some(thread),
        })
    }

    fn stop_and_join(&mut self) {
        let _ = self.sender.send(WorkerMessage::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ExecutionWorker {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn execution_worker_loop(
    options: McpHttpOptions,
    jobs: Receiver<WorkerMessage>,
    results: Sender<WorkerResult>,
) {
    while let Ok(message) = jobs.recv() {
        match message {
            WorkerMessage::Stop => break,
            WorkerMessage::Execute(job) => {
                let response = catch_unwind(AssertUnwindSafe(|| {
                    let mut state = job.state;
                    handle_mcp_http_request_with_context(
                        &options,
                        &mut state,
                        job.request,
                        job.message,
                        job.context,
                    )
                }))
                .unwrap_or_else(|_| {
                    HttpResponse::json(500, json!({"error":"MCP execution worker failed"}))
                });
                let _ = results.send(WorkerResult {
                    connection_id: job.connection_id,
                    response,
                });
            }
        }
    }
}

pub(crate) fn serve_http_dispatcher<F>(
    listener: TcpListener,
    options: &McpHttpOptions,
    max_requests: Option<usize>,
    mut control: F,
) -> Result<(), String>
where
    F: FnMut(&HttpRequest, DispatcherSnapshot) -> Option<ControlResponse>,
{
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to make MCP HTTP listener nonblocking: {error}"))?;
    let mut worker = ExecutionWorker::start(options.clone())?;
    let mut sessions = McpHttpState::default();
    let mut connections = Vec::<Connection>::new();
    let mut next_connection_id = 1_u64;
    let mut handled_requests = 0_usize;
    let mut active_job = None::<u64>;
    let mut overload_rejections = 0_u64;
    let mut deadline_expirations = 0_u64;
    let mut shutting_down = false;

    let result = 'serve: loop {
        let mut made_progress = false;
        let now = Instant::now();
        deadline_expirations = deadline_expirations.saturating_add(drain_worker_results(
            &worker.receiver,
            &mut connections,
            &mut active_job,
            now,
        ));

        if !shutting_down && max_requests.is_none_or(|limit| handled_requests < limit) {
            for _ in 0..MAX_ACCEPTS_PER_TICK {
                match listener.accept() {
                    Ok((stream, _)) => {
                        made_progress = true;
                        if connections.len() >= MAX_CONNECTIONS {
                            overload_rejections = overload_rejections.saturating_add(1);
                            drop(stream);
                            continue;
                        }
                        if stream.set_nonblocking(true).is_err() {
                            drop(stream);
                            continue;
                        }
                        connections.push(Connection::new(next_connection_id, stream));
                        next_connection_id = next_connection_id.wrapping_add(1).max(1);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        break 'serve Err(format!("failed to accept MCP HTTP request: {error}"))
                    }
                }
            }
        }

        let mut request_ids = Vec::new();
        for connection in &mut connections {
            if connection.close {
                continue;
            }
            if connection.response_bytes.is_none() && connection.waiting_job.is_none() {
                let previous_len = connection.request_bytes.len();
                match read_request_tick(connection, now) {
                    ReadProgress::Pending => {}
                    ReadProgress::Ready(request) => {
                        connection.request = Some(request);
                        request_ids.push(connection.id);
                    }
                    ReadProgress::Respond(response) => {
                        if response.status == 408 {
                            deadline_expirations = deadline_expirations.saturating_add(1);
                        }
                        connection.set_response(response, now);
                    }
                    ReadProgress::Closed => connection.close = true,
                }
                made_progress |= connection.request_bytes.len() != previous_len;
            }
        }

        for connection_id in request_ids {
            let Some(index) = connections
                .iter()
                .position(|connection| connection.id == connection_id)
            else {
                continue;
            };
            let snapshot = snapshot(
                connections.len(),
                if active_job.is_some() {
                    Some(true)
                } else {
                    coordinator_active(options)
                },
                overload_rejections,
                deadline_expirations,
            );
            let request = connections[index]
                .request
                .as_ref()
                .expect("ready request remains owned by its connection");
            if let Some(control_result) = control(request, snapshot) {
                connections[index].request = None;
                connections[index].set_response(control_result.response, Instant::now());
                handled_requests = handled_requests.saturating_add(1);
                if control_result.shutdown_after_response {
                    shutting_down = true;
                    connections.retain(|connection| {
                        connection.id == connection_id && connection.response_bytes.is_some()
                    });
                }
                continue;
            }

            match prepare_deferred_tool_call(
                options,
                &sessions,
                request,
                connections[index].accepted_at,
            ) {
                Err(response) => {
                    connections[index].request = None;
                    connections[index].set_response(response, Instant::now());
                    handled_requests = handled_requests.saturating_add(1);
                }
                Ok(None) => {
                    let request = connections[index]
                        .request
                        .take()
                        .expect("request was present during dispatch");
                    let response =
                        super::http::handle_mcp_http_request(options, &mut sessions, request);
                    connections[index].set_response(response, Instant::now());
                    handled_requests = handled_requests.saturating_add(1);
                }
                Ok(Some((message, context))) => {
                    let state = request
                        .header("mcp-session-id")
                        .and_then(|session_id| sessions.snapshot_session(session_id))
                        .expect("deferred request session was validated");
                    let request = connections[index]
                        .request
                        .take()
                        .expect("request was present during dispatch");
                    if context.remaining().is_err() {
                        connections[index]
                            .set_response(deadline_tool_response(&request), Instant::now());
                        deadline_expirations = deadline_expirations.saturating_add(1);
                        handled_requests = handled_requests.saturating_add(1);
                    } else if active_job.is_some() {
                        connections[index]
                            .set_response(graph_busy_response(&request), Instant::now());
                        handled_requests = handled_requests.saturating_add(1);
                    } else {
                        let connection_id = connections[index].id;
                        let deadline = context.deadline;
                        let job = ExecutionJob {
                            connection_id,
                            request: request.clone(),
                            state,
                            message,
                            context,
                        };
                        match worker.sender.try_send(WorkerMessage::Execute(job)) {
                            Ok(()) => {
                                active_job = Some(connection_id);
                                connections[index].waiting_job =
                                    Some(WaitingJob { deadline, request });
                                handled_requests = handled_requests.saturating_add(1);
                            }
                            Err(TrySendError::Full(WorkerMessage::Execute(job))) => {
                                connections[index].set_response(
                                    graph_busy_response(&job.request),
                                    Instant::now(),
                                );
                                handled_requests = handled_requests.saturating_add(1);
                            }
                            Err(TrySendError::Disconnected(WorkerMessage::Execute(_job))) => {
                                connections[index].set_response(
                                    HttpResponse::json(
                                        503,
                                        json!({"error":"execution worker stopped"}),
                                    ),
                                    Instant::now(),
                                );
                            }
                            Err(TrySendError::Full(WorkerMessage::Stop))
                            | Err(TrySendError::Disconnected(WorkerMessage::Stop)) => {
                                connections[index].close = true;
                            }
                        }
                    }
                }
            }
        }

        let now = Instant::now();
        for connection in &mut connections {
            let expired_request = connection
                .waiting_job
                .as_ref()
                .filter(|waiting| waiting.deadline.is_some_and(|deadline| now >= deadline))
                .map(|waiting| waiting.request.clone());
            if connection.response_bytes.is_none() {
                if let Some(request) = expired_request {
                    connection.waiting_job = None;
                    connection.set_response(deadline_tool_response(&request), now);
                    deadline_expirations = deadline_expirations.saturating_add(1);
                }
            }
            if connection.response_bytes.is_none() {
                continue;
            }
            if response_deadline_expired(connection, now) {
                connection.close = true;
                deadline_expirations = deadline_expirations.saturating_add(1);
                continue;
            }
            let Some(bytes) = connection.response_bytes.as_ref() else {
                continue;
            };
            let end = (connection.response_offset + MAX_WRITE_PER_TICK).min(bytes.len());
            match connection
                .stream
                .write(&bytes[connection.response_offset..end])
            {
                Ok(0) => connection.close = true,
                Ok(written) => {
                    made_progress = true;
                    connection.response_offset += written;
                    if connection.response_offset == bytes.len() {
                        connection.close = true;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => connection.close = true,
            }
        }

        connections.retain(|connection| !connection.close);
        deadline_expirations = deadline_expirations.saturating_add(drain_worker_results(
            &worker.receiver,
            &mut connections,
            &mut active_job,
            Instant::now(),
        ));

        let requests_done = max_requests.is_some_and(|limit| handled_requests >= limit);
        if shutting_down && active_job.is_none() && connections.is_empty() {
            break Ok(());
        }
        if requests_done && connections.is_empty() && active_job.is_none() {
            break Ok(());
        }
        if !made_progress {
            thread::sleep(POLL_INTERVAL);
        }
    };

    worker.stop_and_join();
    result
}

fn snapshot(
    admitted_connections: usize,
    active_operation: Option<bool>,
    overload_rejections: u64,
    deadline_expirations: u64,
) -> DispatcherSnapshot {
    DispatcherSnapshot {
        admitted_connections,
        active_operation,
        overload_rejections,
        deadline_expirations,
    }
}

fn coordinator_active(options: &McpHttpOptions) -> Option<bool> {
    options
        .serve
        .api
        .as_ref()
        .map_or(Some(false), |api| api.active_operation())
}

fn drain_worker_results(
    receiver: &Receiver<WorkerResult>,
    connections: &mut [Connection],
    active_job: &mut Option<u64>,
    now: Instant,
) -> u64 {
    let mut expired = 0_u64;
    while let Ok(result) = receiver.try_recv() {
        if *active_job == Some(result.connection_id) {
            *active_job = None;
        }
        if let Some(connection) = connections
            .iter_mut()
            .find(|connection| connection.id == result.connection_id)
        {
            if connection.response_bytes.is_none() {
                let expired_wait = connection
                    .waiting_job
                    .as_ref()
                    .and_then(|waiting| waiting.deadline)
                    .is_some_and(|deadline| now >= deadline);
                if expired_wait {
                    let request = connection
                        .waiting_job
                        .as_ref()
                        .map(|waiting| waiting.request.clone());
                    connection.waiting_job = None;
                    if let Some(request) = request {
                        connection.set_response(deadline_tool_response(&request), now);
                    }
                    expired = expired.saturating_add(1);
                } else {
                    connection.waiting_job = None;
                    connection.set_response(result.response, now);
                }
            }
        }
    }
    expired
}

fn response_deadline_expired(connection: &Connection, now: Instant) -> bool {
    connection
        .response_ready_at
        .is_some_and(|ready| now.saturating_duration_since(ready) >= RESPONSE_TIMEOUT)
}

enum ReadProgress {
    Pending,
    Ready(HttpRequest),
    Respond(HttpResponse),
    Closed,
}

fn read_request_tick(connection: &mut Connection, now: Instant) -> ReadProgress {
    if connection.header_end.is_none()
        && now.saturating_duration_since(connection.accepted_at) >= HEADER_TIMEOUT
    {
        return ReadProgress::Respond(request_error(408, "HTTP request headers timed out"));
    }
    if now.saturating_duration_since(connection.accepted_at) >= REQUEST_TIMEOUT {
        return ReadProgress::Respond(request_error(408, "HTTP request timed out"));
    }

    if connection.request_bytes.len() < MAX_HTTP_HEADER_BYTES.saturating_add(MAX_HTTP_BODY_BYTES) {
        let remaining = MAX_READ_PER_TICK.min(
            MAX_HTTP_HEADER_BYTES
                .saturating_add(MAX_HTTP_BODY_BYTES)
                .saturating_sub(connection.request_bytes.len()),
        );
        let mut chunk = [0_u8; MAX_READ_PER_TICK];
        match connection.stream.read(&mut chunk[..remaining.max(1)]) {
            Ok(0) => return ReadProgress::Closed,
            Ok(read) => connection.request_bytes.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return ReadProgress::Closed,
        }
    }

    if connection.header_end.is_none() {
        if let Some(header_end) = find_header_end(&connection.request_bytes) {
            if header_end + 4 > MAX_HTTP_HEADER_BYTES {
                return ReadProgress::Respond(request_error(
                    431,
                    "HTTP request headers are too large",
                ));
            }
            let parsed = match parse_http_head(&connection.request_bytes[..header_end]) {
                Ok(parsed) => parsed,
                Err(message) => return ReadProgress::Respond(request_error(400, &message)),
            };
            connection.header_end = Some(header_end + 4);
            connection.content_length = Some(parsed.content_length);
            if parsed.content_length > MAX_HTTP_BODY_BYTES {
                return ReadProgress::Ready(HttpRequest {
                    method: parsed.method,
                    path: parsed.path,
                    headers: parsed.headers,
                    body: Vec::new(),
                    body_too_large: true,
                });
            }
        } else if connection.request_bytes.len() > MAX_HTTP_HEADER_BYTES {
            return ReadProgress::Respond(request_error(431, "HTTP request headers are too large"));
        }
    }

    let (Some(header_end), Some(content_length)) =
        (connection.header_end, connection.content_length)
    else {
        return ReadProgress::Pending;
    };
    if connection.request_bytes.len() < header_end + content_length {
        return ReadProgress::Pending;
    }
    let parsed = match parse_http_head(&connection.request_bytes[..header_end - 4]) {
        Ok(parsed) => parsed,
        Err(message) => return ReadProgress::Respond(request_error(400, &message)),
    };
    let body = connection.request_bytes[header_end..header_end + content_length].to_vec();
    ReadProgress::Ready(HttpRequest {
        method: parsed.method,
        path: parsed.path,
        headers: parsed.headers,
        body,
        body_too_large: false,
    })
}

#[derive(Debug)]
struct ParsedHead {
    method: String,
    path: String,
    headers: std::collections::BTreeMap<String, String>,
    content_length: usize,
}

fn parse_http_head(bytes: &[u8]) -> Result<ParsedHead, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "HTTP request is missing a request line".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or(raw_path).to_string();
    let mut headers = std::collections::BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = match headers.get("content-length") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "Content-Length must be an integer".to_string())?,
        None => 0,
    };
    Ok(ParsedHead {
        method,
        path,
        headers,
        content_length,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn request_error(status: u16, message: &str) -> HttpResponse {
    HttpResponse::json(status, json!({"error": message}))
}

fn encode_http_response(response: HttpResponse) -> Result<Vec<u8>, String> {
    let body = if response.status == 202 || response.status == 405 {
        Vec::new()
    } else {
        serde_json::to_vec(&response.payload).map_err(|error| error.to_string())?
    };
    let mut bytes = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        super::http::http_reason(response.status),
        body.len()
    )
    .into_bytes();
    for (name, value) in response.headers {
        bytes.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    bytes.extend_from_slice(b"\r\n");
    bytes.extend_from_slice(&body);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn test_options() -> McpHttpOptions {
        McpHttpOptions {
            serve: super::super::options::McpServeOptions::parse(&[], "").unwrap(),
            host: "127.0.0.1".into(),
            port: 0,
            endpoint_path: "/mcp".into(),
            allow_remote: false,
            auth_token: None,
        }
    }

    #[test]
    fn fast_reader_receives_large_response_within_write_deadline() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            serve_http_dispatcher(listener, &test_options(), Some(1), |_, _| {
                Some(ControlResponse {
                    response: HttpResponse::json(
                        200,
                        json!({"padding": "x".repeat(4 * 1024 * 1024)}),
                    ),
                    shutdown_after_response: false,
                })
            })
            .unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.write_all(b"GET /large HTTP/1.1\r\n\r\n").unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        server.join().unwrap();
        let split = find_header_end(&response).unwrap() + 4;
        let body: serde_json::Value = serde_json::from_slice(&response[split..]).unwrap();
        assert_eq!(body["padding"].as_str().unwrap().len(), 4 * 1024 * 1024);
    }

    #[test]
    fn slow_reader_does_not_block_other_responses_and_is_released() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let result = serve_http_dispatcher(listener, &test_options(), Some(2), |request, _| {
                let payload = if request.path == "/large" {
                    ready_tx.send(()).unwrap();
                    json!({"padding": "x".repeat(16 * 1024 * 1024)})
                } else {
                    json!({"ok":true})
                };
                Some(ControlResponse {
                    response: HttpResponse::json(200, payload),
                    shutdown_after_response: false,
                })
            });
            done_tx.send(result).unwrap();
        });
        let mut slow = TcpStream::connect(address).unwrap();
        slow.write_all(b"GET /large HTTP/1.1\r\n\r\n").unwrap();
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut fast = TcpStream::connect(address).unwrap();
        fast.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        fast.write_all(b"GET /ping HTTP/1.1\r\n\r\n").unwrap();
        let mut response = String::new();
        fast.read_to_string(&mut response).unwrap();
        assert!(response.contains("\"ok\":true"));
        done_rx
            .recv_timeout(Duration::from_secs(4))
            .unwrap()
            .unwrap();
        server.join().unwrap();
    }

    fn stream_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
        let client = TcpStream::connect(listener.local_addr().expect("listener address"))
            .expect("connect to ephemeral port");
        let (server, _) = listener.accept().expect("accept client");
        server
            .set_nonblocking(true)
            .expect("configure nonblocking server stream");
        (server, client)
    }

    fn graph_search_request() -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            headers: BTreeMap::new(),
            body: serde_json::to_vec(&json!({
                "jsonrpc":"2.0",
                "id":77,
                "method":"tools/call",
                "params":{"name":"graph_search","arguments":{"output_format":"json","include_structured_content":true}}
            }))
            .expect("serialize graph request"),
            body_too_large: false,
        }
    }

    #[test]
    fn header_parser_preserves_absolute_content_length_without_body_buffering() {
        let head =
            parse_http_head(b"POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Length: 42\r\n\r\n")
                .expect("valid HTTP head");
        assert_eq!(head.method, "POST");
        assert_eq!(head.path, "/mcp");
        assert_eq!(head.content_length, 42);
    }

    #[test]
    fn header_parser_rejects_malformed_content_length() {
        let error = parse_http_head(b"POST /mcp HTTP/1.1\r\nContent-Length: later\r\n\r\n")
            .expect_err("invalid content length must fail");
        assert_eq!(error, "Content-Length must be an integer");
    }

    #[test]
    fn partial_headers_and_bodies_expire_from_connection_acceptance() {
        let (server, _client) = stream_pair();
        let mut partial_header = Connection::new(1, server);
        partial_header
            .request_bytes
            .extend_from_slice(b"POST /mcp HTTP/1.1\r\n");
        partial_header.accepted_at = Instant::now() - HEADER_TIMEOUT - Duration::from_millis(1);
        assert!(matches!(
            read_request_tick(&mut partial_header, Instant::now()),
            ReadProgress::Respond(HttpResponse { status: 408, .. })
        ));

        let (server, _client) = stream_pair();
        let mut partial_body = Connection::new(2, server);
        partial_body.request_bytes = b"POST /mcp HTTP/1.1\r\nContent-Length: 4\r\n\r\nabc".to_vec();
        partial_body.header_end = Some(partial_body.request_bytes.len() - 3);
        partial_body.content_length = Some(4);
        partial_body.accepted_at = Instant::now() - REQUEST_TIMEOUT - Duration::from_millis(1);
        assert!(matches!(
            read_request_tick(&mut partial_body, Instant::now()),
            ReadProgress::Respond(HttpResponse { status: 408, .. })
        ));
    }

    #[test]
    fn response_deadline_is_absolute_even_when_a_peer_never_reads() {
        let (server, _client) = stream_pair();
        let mut connection = Connection::new(3, server);
        connection.response_bytes = Some(vec![b'x'; MAX_WRITE_PER_TICK * 4]);
        connection.response_ready_at =
            Some(Instant::now() - RESPONSE_TIMEOUT - Duration::from_millis(1));
        assert!(response_deadline_expired(&connection, Instant::now()));
    }

    #[test]
    fn late_execution_result_is_discarded_after_its_deadline() {
        let (server, _client) = stream_pair();
        let request = graph_search_request();
        let mut connection = Connection::new(4, server);
        connection.waiting_job = Some(WaitingJob {
            deadline: Some(Instant::now() - Duration::from_millis(1)),
            request,
        });
        let (sender, receiver) = mpsc::channel();
        sender
            .send(WorkerResult {
                connection_id: 4,
                response: HttpResponse::json(200, json!({"result":"late success"})),
            })
            .expect("queue completed worker result");
        let mut active = Some(4);
        let expired = drain_worker_results(
            &receiver,
            std::slice::from_mut(&mut connection),
            &mut active,
            Instant::now(),
        );
        assert_eq!(expired, 1);
        assert_eq!(active, None);
        let response = String::from_utf8_lossy(
            connection
                .response_bytes
                .as_ref()
                .expect("deadline response"),
        );
        assert!(response.contains("deadline_exceeded"));
        assert!(!response.contains("late success"));
    }
}
