use crate::api::context::resolve_runtime;
use crate::api::{
    ApiError, CodebaseGraphApi, OperationInvocation, OperationResponse, RefreshServiceConfig,
    RepoSelector,
};
use crate::storage::atomic::write_json_atomically;
use crate::storage::layout::{DirectLayout, ManagedLayout};
use crate::storage::locks::{try_open_locked, CoordinatorLease, LockMode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const COORDINATOR_PROTOCOL_VERSION: u64 = 1;
const COORDINATOR_AUTHENTICATION_FAILED: &str = "coordinator_authentication_failed";
const COORDINATOR_REQUEST_RECEIVE_FAILED: &str = "coordinator_request_receive_failed";
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
const ELECTION_RETRY_INTERVAL: Duration = Duration::from_millis(50);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const STREAM_IO_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
const MONITOR_INTERVAL: Duration = Duration::from_secs(1);
static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct CoordinatorApiConfig {
    selector: RepoSelector,
    refresh: Option<RefreshServiceConfig>,
}

impl CoordinatorApiConfig {
    pub(crate) fn new(selector: RepoSelector, refresh: Option<RefreshServiceConfig>) -> Self {
        Self { selector, refresh }
    }

    fn build_api(&self) -> CodebaseGraphApi {
        CodebaseGraphApi::for_coordinator_owner(self.selector.clone(), self.refresh)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CoordinatorClient {
    inner: Arc<ClientInner>,
}

#[derive(Debug)]
struct ClientInner {
    control: CoordinatorControlPaths,
    api_config: CoordinatorApiConfig,
    route: Mutex<CoordinatorRoute>,
    monitor_stop: AtomicBool,
    monitor: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for ClientInner {
    fn drop(&mut self) {
        self.monitor_stop.store(true, Ordering::Release);
        if let Ok(mut monitor) = self.monitor.lock() {
            // The last strong reference can be released by the monitor itself.
            // Dropping the handle detaches safely and the weak loop then exits.
            monitor.take();
        }
    }
}

#[derive(Debug, Default)]
struct CoordinatorRoute {
    state: Option<CoordinatorState>,
    owner: Option<Arc<CoordinatorOwner>>,
}

#[derive(Debug)]
struct CoordinatorOwner {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for CoordinatorOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut thread) = self.thread.lock() {
            if let Some(thread) = thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl CoordinatorOwner {
    fn is_running(&self) -> bool {
        self.thread
            .lock()
            .map(|thread| thread.as_ref().is_some_and(|thread| !thread.is_finished()))
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug)]
struct CoordinatorControlPaths {
    lock: PathBuf,
    state: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CoordinatorState {
    version: u64,
    endpoint: SocketAddr,
    token: String,
    pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoordinatorRequest {
    version: u64,
    token: String,
    command: CoordinatorCommand,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum CoordinatorCommand {
    Ping,
    Execute {
        operation_id: String,
        invocation: OperationInvocation,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
enum CoordinatorReply {
    Pong,
    Success(OperationResponse),
    Failure(ApiError),
}

impl CoordinatorClient {
    pub(crate) fn connect_or_start(config: CoordinatorApiConfig) -> Result<Self, String> {
        let control = coordinator_control_paths(&config.selector)?;
        let client = Self {
            inner: Arc::new(ClientInner {
                control,
                api_config: config,
                route: Mutex::new(CoordinatorRoute::default()),
                monitor_stop: AtomicBool::new(false),
                monitor: Mutex::new(None),
            }),
        };
        client.refresh_route()?;
        client.start_monitor()?;
        Ok(client)
    }

    fn start_monitor(&self) -> Result<(), String> {
        let weak = Arc::downgrade(&self.inner);
        let monitor = thread::Builder::new()
            .name("codebase-graph-coordinator-monitor".to_string())
            .spawn(move || monitor_route(weak))
            .map_err(|error| format!("failed to start coordinator monitor: {error}"))?;
        *self
            .inner
            .monitor
            .lock()
            .map_err(|_| "coordinator monitor lock is poisoned".to_string())? = Some(monitor);
        Ok(())
    }

    pub(crate) fn execute_invocation(
        &self,
        operation_id: &str,
        invocation: &OperationInvocation,
    ) -> Result<OperationResponse, ApiError> {
        let command = CoordinatorCommand::Execute {
            operation_id: operation_id.to_string(),
            invocation: invocation.clone(),
        };
        let reply = self
            .send_command_with_recovery(&command)
            .map_err(|error| ApiError::new("coordinator_unavailable", error).retryable(true))?;
        match reply {
            CoordinatorReply::Success(response) => Ok(response),
            CoordinatorReply::Failure(error) => Err(error),
            CoordinatorReply::Pong => Err(coordinator_protocol_error(
                "coordinator returned pong for an operation request",
            )),
        }
    }

    fn refresh_route(&self) -> Result<(), String> {
        let mut route = self
            .inner
            .route
            .lock()
            .map_err(|_| "coordinator route lock is poisoned".to_string())?;
        route.state = None;
        route.owner = None;
        let deadline = Instant::now() + ELECTION_TIMEOUT;
        loop {
            if let Some(lease) = try_open_locked(&self.inner.control.lock, LockMode::Exclusive)
                .map_err(|error| error.to_string())?
            {
                let (state, owner) = start_owner(
                    self.inner.control.clone(),
                    lease,
                    self.inner.api_config.clone(),
                )?;
                route.state = Some(state);
                route.owner = Some(Arc::new(owner));
                return Ok(());
            }
            if let Ok(state) = read_coordinator_state(&self.inner.control.state) {
                if ping_state(&state).is_ok() {
                    route.state = Some(state);
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "timed out waiting for repository coordinator at {}",
                    self.inner.control.state.display()
                ));
            }
            thread::sleep(ELECTION_RETRY_INTERVAL);
        }
    }

    fn send_command(&self, command: &CoordinatorCommand) -> Result<CoordinatorReply, String> {
        let state = self
            .inner
            .route
            .lock()
            .map_err(|_| "coordinator route lock is poisoned".to_string())?
            .state
            .clone()
            .ok_or_else(|| "repository coordinator route is unavailable".to_string())?;
        send_to_state(&state, command)
    }

    fn send_command_with_recovery(
        &self,
        command: &CoordinatorCommand,
    ) -> Result<CoordinatorReply, String> {
        let deadline = Instant::now() + COMMAND_RETRY_TIMEOUT;
        let mut retried_ambiguous_failure = false;
        loop {
            match self.send_command(command) {
                Ok(reply) if reply_is_safe_to_retry(&reply) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "repository coordinator request kept failing before dispatch: {reply:?}"
                        ));
                    }
                    thread::sleep(ELECTION_RETRY_INTERVAL);
                }
                Ok(reply) if reply_requires_route_refresh(&reply) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "repository coordinator route stayed stale: {reply:?}"
                        ));
                    }
                    self.refresh_route()?;
                }
                Ok(reply) => return Ok(reply),
                Err(error) => {
                    if retried_ambiguous_failure || Instant::now() >= deadline {
                        return Err(error);
                    }
                    retried_ambiguous_failure = true;
                    self.refresh_route()?;
                }
            }
        }
    }

    #[cfg(test)]
    fn is_owner(&self) -> bool {
        self.inner
            .route
            .lock()
            .map(|route| route.owner.is_some())
            .unwrap_or(false)
    }

    #[cfg(test)]
    fn endpoint(&self) -> Option<SocketAddr> {
        self.inner
            .route
            .lock()
            .ok()
            .and_then(|route| route.state.as_ref().map(|state| state.endpoint))
    }

    #[cfg(test)]
    fn ping(&self) -> Result<(), String> {
        match self.send_command_with_recovery(&CoordinatorCommand::Ping)? {
            CoordinatorReply::Pong => Ok(()),
            reply => Err(format!(
                "repository coordinator returned a non-pong reply: {reply:?}"
            )),
        }
    }
}

fn monitor_route(inner: Weak<ClientInner>) {
    loop {
        thread::sleep(MONITOR_INTERVAL);
        let Some(inner) = inner.upgrade() else {
            return;
        };
        if inner.monitor_stop.load(Ordering::Acquire) {
            return;
        }
        let (state, owner) = inner
            .route
            .lock()
            .ok()
            .map(|route| (route.state.clone(), route.owner.clone()))
            .unwrap_or((None, None));
        let route_is_reachable = match owner {
            Some(owner) => owner.is_running(),
            None => state
                .as_ref()
                .is_some_and(|state| ping_state(state).is_ok()),
        };
        if !route_is_reachable {
            let client = CoordinatorClient { inner };
            let _ = client.refresh_route();
        }
    }
}

fn start_owner(
    control: CoordinatorControlPaths,
    lease: CoordinatorLease,
    config: CoordinatorApiConfig,
) -> Result<(CoordinatorState, CoordinatorOwner), String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to bind repository coordinator: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure repository coordinator: {error}"))?;
    let endpoint = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect repository coordinator address: {error}"))?;
    let state = CoordinatorState {
        version: COORDINATOR_PROTOCOL_VERSION,
        endpoint,
        token: coordinator_token(endpoint),
        pid: std::process::id(),
    };
    write_json_atomically(&control.state, &state).map_err(|error| error.to_string())?;
    restrict_state_permissions(&control.state)?;

    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_state = state.clone();
    let state_path = control.state.clone();
    let thread = thread::Builder::new()
        .name("codebase-graph-coordinator".to_string())
        .spawn(move || {
            let api = config.build_api();
            serve_owner(
                listener,
                &api,
                &config.selector,
                &server_state,
                &server_stop,
            );
            remove_owned_state(&state_path, &server_state.token);
            drop(lease);
        })
        .map_err(|error| format!("failed to start repository coordinator: {error}"))?;
    Ok((
        state,
        CoordinatorOwner {
            stop,
            thread: Mutex::new(Some(thread)),
        },
    ))
}

fn serve_owner(
    listener: TcpListener,
    api: &CodebaseGraphApi,
    selector: &RepoSelector,
    state: &CoordinatorState,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(STREAM_IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(STREAM_IO_TIMEOUT));
                let reply = handle_connection(&mut stream, api, selector, state);
                let _ = write_frame(&mut stream, &reply);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ELECTION_RETRY_INTERVAL);
            }
            Err(_) => thread::sleep(ELECTION_RETRY_INTERVAL),
        }
    }
}

fn handle_connection(
    stream: &mut TcpStream,
    api: &CodebaseGraphApi,
    selector: &RepoSelector,
    state: &CoordinatorState,
) -> CoordinatorReply {
    let request = match receive_request(stream) {
        Ok(request) => request,
        Err(reply) => return reply,
    };
    if request.version != COORDINATOR_PROTOCOL_VERSION || request.token != state.token {
        return CoordinatorReply::Failure(ApiError::new(
            COORDINATOR_AUTHENTICATION_FAILED,
            "repository coordinator protocol or token is invalid",
        ));
    }
    match request.command {
        CoordinatorCommand::Ping => CoordinatorReply::Pong,
        CoordinatorCommand::Execute {
            operation_id,
            mut invocation,
        } => {
            invocation.repo = selector.clone();
            match api.execute_invocation(&operation_id, &invocation) {
                Ok(mut response) => {
                    attach_coordinator_status(&mut response, state);
                    CoordinatorReply::Success(response)
                }
                Err(error) => CoordinatorReply::Failure(error),
            }
        }
    }
}

fn receive_request(stream: &mut TcpStream) -> Result<CoordinatorRequest, CoordinatorReply> {
    read_frame(stream).map_err(coordinator_request_receive_failure)
}

fn coordinator_request_receive_failure(error: String) -> CoordinatorReply {
    CoordinatorReply::Failure(
        ApiError::new(COORDINATOR_REQUEST_RECEIVE_FAILED, error).retryable(true),
    )
}

fn attach_coordinator_status(response: &mut OperationResponse, state: &CoordinatorState) {
    if response.operation != "health" {
        return;
    }
    let status = serde_json::json!({
        "role": "owner",
        "pid": state.pid,
        "endpoint": state.endpoint.to_string(),
    });
    if let Some(structured) = response.payload.get_mut("structured") {
        if let Some(object) = structured.as_object_mut() {
            object.insert("coordinator".to_string(), status);
        }
    } else if let Some(object) = response.payload.as_object_mut() {
        object.insert("coordinator".to_string(), status);
    }
}

fn send_to_state(
    state: &CoordinatorState,
    command: &CoordinatorCommand,
) -> Result<CoordinatorReply, String> {
    validate_state(state)?;
    let mut stream = TcpStream::connect_timeout(&state.endpoint, CONNECT_TIMEOUT)
        .map_err(|error| format!("failed to connect to repository coordinator: {error}"))?;
    if matches!(command, CoordinatorCommand::Ping) {
        stream
            .set_read_timeout(Some(STREAM_IO_TIMEOUT))
            .map_err(|error| format!("failed to configure coordinator ping reads: {error}"))?;
    }
    stream
        .set_write_timeout(Some(STREAM_IO_TIMEOUT))
        .map_err(|error| format!("failed to configure coordinator stream writes: {error}"))?;
    write_frame(
        &mut stream,
        &CoordinatorRequest {
            version: COORDINATOR_PROTOCOL_VERSION,
            token: state.token.clone(),
            command: clone_command(command),
        },
    )?;
    read_frame(&mut stream)
}

fn ping_state(state: &CoordinatorState) -> Result<(), String> {
    match send_to_state(state, &CoordinatorCommand::Ping)? {
        CoordinatorReply::Pong => Ok(()),
        _ => Err("repository coordinator did not answer ping".to_string()),
    }
}

fn reply_requires_route_refresh(reply: &CoordinatorReply) -> bool {
    matches!(
        reply,
        CoordinatorReply::Failure(error) if error.code == COORDINATOR_AUTHENTICATION_FAILED
    )
}

fn reply_is_safe_to_retry(reply: &CoordinatorReply) -> bool {
    matches!(
        reply,
        CoordinatorReply::Failure(error)
            if error.code == COORDINATOR_REQUEST_RECEIVE_FAILED && error.retryable
    )
}

fn clone_command(command: &CoordinatorCommand) -> CoordinatorCommand {
    match command {
        CoordinatorCommand::Ping => CoordinatorCommand::Ping,
        CoordinatorCommand::Execute {
            operation_id,
            invocation,
        } => CoordinatorCommand::Execute {
            operation_id: operation_id.clone(),
            invocation: invocation.clone(),
        },
    }
}

fn coordinator_control_paths(selector: &RepoSelector) -> Result<CoordinatorControlPaths, String> {
    let runtime = resolve_runtime(selector)?;
    if let Some(storage_root) = runtime.storage_root.as_ref() {
        let layout = ManagedLayout::new(storage_root);
        return Ok(CoordinatorControlPaths {
            lock: layout.coordinator_lock_path(),
            state: layout.coordinator_state_path(),
        });
    }
    let layout = DirectLayout::new(&runtime.db_path, &runtime.manifest_path);
    Ok(CoordinatorControlPaths {
        lock: layout.coordinator_lock_path(),
        state: layout.coordinator_state_path(),
    })
}

fn read_coordinator_state(path: &Path) -> Result<CoordinatorState, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect repository coordinator state: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "repository coordinator state must be a real file: {}",
            path.display()
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read repository coordinator state: {error}"))?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err("repository coordinator state is too large".to_string());
    }
    let state: CoordinatorState = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse repository coordinator state: {error}"))?;
    validate_state(&state)?;
    Ok(state)
}

fn validate_state(state: &CoordinatorState) -> Result<(), String> {
    if state.version != COORDINATOR_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported repository coordinator protocol version {}",
            state.version
        ));
    }
    if !state.endpoint.ip().is_loopback() || state.token.len() != 64 {
        return Err("repository coordinator state is invalid".to_string());
    }
    Ok(())
}

fn coordinator_token(endpoint: SocketAddr) -> String {
    let sequence = TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut digest = Sha256::new();
    digest.update(std::process::id().to_le_bytes());
    digest.update(timestamp.to_le_bytes());
    digest.update(sequence.to_le_bytes());
    digest.update(endpoint.to_string().as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_frame<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<(), String> {
    let mut payload = Vec::new();
    payload
        .try_reserve(4096)
        .map_err(|_| "repository coordinator frame allocation failed".to_string())?;
    serde_json::to_writer(&mut payload, value)
        .map_err(|error| format!("failed to encode repository coordinator frame: {error}"))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(format!(
            "repository coordinator frame exceeds {MAX_FRAME_BYTES} bytes"
        ));
    }
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .map_err(|error| format!("failed to write repository coordinator frame: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("failed to flush repository coordinator frame: {error}"))
}

fn read_frame<T: DeserializeOwned>(stream: &mut TcpStream) -> Result<T, String> {
    let mut payload = Vec::new();
    payload
        .try_reserve(4096)
        .map_err(|_| "repository coordinator frame allocation failed".to_string())?;
    let mut reader = BufReader::new(stream);
    reader
        .by_ref()
        .take((MAX_FRAME_BYTES + 1) as u64)
        .read_until(b'\n', &mut payload)
        .map_err(|error| format!("failed to read repository coordinator frame: {error}"))?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES || payload.last() != Some(&b'\n') {
        return Err("repository coordinator frame is missing or too large".to_string());
    }
    payload.pop();
    serde_json::from_slice(&payload)
        .map_err(|error| format!("failed to decode repository coordinator frame: {error}"))
}

fn coordinator_protocol_error(message: impl Into<String>) -> ApiError {
    ApiError::new("coordinator_protocol_error", message.into())
}

fn remove_owned_state(path: &Path, token: &str) {
    if read_coordinator_state(path)
        .ok()
        .is_some_and(|state| state.token == token)
    {
        let _ = fs::remove_file(path);
    }
}

#[cfg(unix)]
fn restrict_state_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to restrict repository coordinator state: {error}"))
}

#[cfg(not(unix))]
fn restrict_state_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_receive_failure_is_retryable_before_dispatch() {
        use std::net::Shutdown;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        client.shutdown(Shutdown::Write).unwrap();

        let reply = receive_request(&mut server).unwrap_err();
        assert!(reply_is_safe_to_retry(&reply));
        assert!(!reply_requires_route_refresh(&reply));
        let CoordinatorReply::Failure(error) = reply else {
            panic!("closed request stream should produce a failure reply");
        };
        assert_eq!(error.code, COORDINATOR_REQUEST_RECEIVE_FAILED);
        assert!(error.retryable);
    }

    #[test]
    fn retryable_receive_failure_retries_the_same_owner() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = listener.local_addr().unwrap();
        let token = "a".repeat(64);
        let server_token = token.clone();
        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let request: CoordinatorRequest = read_frame(&mut stream).unwrap();
                assert_eq!(request.token, server_token);
                let reply = if attempt == 0 {
                    coordinator_request_receive_failure("timed out before dispatch".to_string())
                } else {
                    CoordinatorReply::Pong
                };
                write_frame(&mut stream, &reply).unwrap();
            }
        });

        let root = temp_dir("retry-receive");
        let client = CoordinatorClient {
            inner: Arc::new(ClientInner {
                control: CoordinatorControlPaths {
                    lock: root.join("coordinator.lock"),
                    state: root.join("coordinator.json"),
                },
                api_config: direct_config(&root),
                route: Mutex::new(CoordinatorRoute {
                    state: Some(CoordinatorState {
                        version: COORDINATOR_PROTOCOL_VERSION,
                        endpoint,
                        token,
                        pid: std::process::id(),
                    }),
                    owner: None,
                }),
                monitor_stop: AtomicBool::new(false),
                monitor: Mutex::new(None),
            }),
        };

        client.ping().unwrap();
        assert_eq!(client.endpoint(), Some(endpoint));
        server.join().unwrap();
    }

    #[test]
    fn concurrent_clients_share_one_repository_coordinator() {
        let root = temp_dir("shared");
        let config = direct_config(&root);
        let mut clients = Vec::new();
        for _ in 0..20 {
            clients.push(CoordinatorClient::connect_or_start(config.clone()).unwrap());
        }

        let endpoint = clients[0].endpoint().unwrap();
        assert!(clients
            .iter()
            .all(|client| client.endpoint() == Some(endpoint)));
        assert_eq!(clients.iter().filter(|client| client.is_owner()).count(), 1);
        for client in &clients {
            client.ping().unwrap();
        }
        let refreshed_endpoint = clients[0].endpoint().unwrap();
        assert!(clients
            .iter()
            .all(|client| client.endpoint() == Some(refreshed_endpoint)));
        assert_eq!(clients.iter().filter(|client| client.is_owner()).count(), 1);
    }

    #[test]
    fn follower_takes_over_after_owner_release() {
        let root = temp_dir("takeover");
        let config = direct_config(&root);
        let owner = CoordinatorClient::connect_or_start(config.clone()).unwrap();
        let follower = CoordinatorClient::connect_or_start(config).unwrap();
        let previous = owner.endpoint().unwrap();
        assert!(owner.is_owner());
        assert!(!follower.is_owner());

        drop(owner);
        let deadline = Instant::now() + ELECTION_TIMEOUT;
        while !follower.is_owner() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(
            follower.is_owner(),
            "standby did not take over within five seconds"
        );
        assert_ne!(follower.endpoint(), Some(previous));
    }

    #[test]
    fn follower_refreshes_a_stale_authenticated_route() {
        let root = temp_dir("stale-route");
        let config = direct_config(&root);
        let owner = CoordinatorClient::connect_or_start(config.clone()).unwrap();
        let follower = CoordinatorClient::connect_or_start(config).unwrap();

        let invalidate_route = || {
            follower
                .inner
                .route
                .lock()
                .unwrap()
                .state
                .as_mut()
                .unwrap()
                .token = "0".repeat(64);
        };

        invalidate_route();
        let response = follower
            .execute_invocation(
                "syntax",
                &OperationInvocation {
                    repo: RepoSelector::default(),
                    arguments: serde_json::json!({"language": "python"}),
                    output_format: crate::api::OutputFormat::Typed,
                },
            )
            .unwrap();
        assert_eq!(response.operation, "syntax");

        invalidate_route();
        follower.ping().unwrap();
        assert_eq!(follower.endpoint(), owner.endpoint());
        assert!(owner.is_owner());
        assert!(!follower.is_owner());
    }

    fn direct_config(root: &Path) -> CoordinatorApiConfig {
        let selector = RepoSelector {
            repo_root: Some(root.to_path_buf()),
            config_path: None,
            db_path: Some(root.join("graph.ldb")),
            manifest_path: Some(root.join("manifest.json")),
        };
        CoordinatorApiConfig::new(selector, None)
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codebase-graph-coordinator-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
