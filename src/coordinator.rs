use crate::api::context::{bind_repo_selector, resolve_runtime, RepositoryIdentity};
use crate::api::ExecutionContext;
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
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const COORDINATOR_PROTOCOL_VERSION: u64 = 1;
const COORDINATOR_AUTHENTICATION_FAILED: &str = "coordinator_authentication_failed";
const COORDINATOR_REQUEST_RECEIVE_FAILED: &str = "coordinator_request_receive_failed";
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const ELECTION_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const STREAM_IO_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
const MONITOR_INTERVAL: Duration = Duration::from_secs(1);
const COORDINATOR_MAX_CONNECTIONS: usize = 32;
const COORDINATOR_POLL_INTERVAL: Duration = Duration::from_millis(10);
const COORDINATOR_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DEADLINE_MILLIS: u64 = 900;
static TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct CoordinatorApiConfig {
    selector: RepoSelector,
    refresh: Option<RefreshServiceConfig>,
    identity: Option<RepositoryIdentity>,
}

impl CoordinatorApiConfig {
    pub(crate) fn new(selector: RepoSelector, refresh: Option<RefreshServiceConfig>) -> Self {
        Self {
            selector,
            refresh,
            identity: None,
        }
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
    dispatcher: Arc<ExecutionDispatcher>,
}

impl Drop for CoordinatorOwner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.dispatcher.stop_admission();
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

    fn stop_and_drain(&self) {
        self.stop.store(true, Ordering::Release);
        self.dispatcher.stop_admission();
        if let Ok(mut thread) = self.thread.lock() {
            if let Some(thread) = thread.take() {
                let _ = thread.join();
            }
        }
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
    #[serde(default)]
    supports_deadlines: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoordinatorRequest {
    version: u64,
    token: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
    command: CoordinatorCommand,
}

#[derive(Debug)]
struct ExecutionJob {
    operation_id: String,
    invocation: OperationInvocation,
    context: ExecutionContext,
    stream: TcpStream,
    _connection: ConnectionPermit,
}

#[derive(Debug, Default)]
struct ExecutionDispatchState {
    job: Option<ExecutionJob>,
    active: bool,
    stopping: bool,
}

#[derive(Debug, Default)]
struct ExecutionDispatcher {
    state: Mutex<ExecutionDispatchState>,
    ready: Condvar,
    active_snapshot: AtomicBool,
}

impl ExecutionDispatcher {
    fn try_submit(&self, job: ExecutionJob) -> Result<(), Box<ExecutionJob>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.stopping || state.active || state.job.is_some() {
            return Err(Box::new(job));
        }
        state.active = true;
        self.active_snapshot.store(true, Ordering::Release);
        state.job = Some(job);
        self.ready.notify_one();
        Ok(())
    }

    fn take(&self) -> Option<ExecutionJob> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(job) = state.job.take() {
                return Some(job);
            }
            if state.stopping {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn finish_one(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active = false;
        self.active_snapshot.store(false, Ordering::Release);
        self.ready.notify_all();
    }

    fn stop_admission(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.stopping = true;
        self.ready.notify_all();
    }

    fn active(&self) -> bool {
        self.active_snapshot.load(Ordering::Acquire)
    }
}

#[derive(Debug, Default)]
struct ConnectionRegistry {
    next_id: AtomicU64,
    streams: Mutex<std::collections::HashMap<u64, TcpStream>>,
    writers: Mutex<Vec<JoinHandle<()>>>,
}

impl ConnectionRegistry {
    fn register(self: &Arc<Self>, stream: &TcpStream) -> std::io::Result<ConnectionPermit> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut streams = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if streams.len() >= COORDINATOR_MAX_CONNECTIONS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "repository coordinator connection limit reached",
            ));
        }
        streams.insert(id, stream.try_clone()?);
        Ok(ConnectionPermit {
            registry: Arc::clone(self),
            id,
        })
    }

    fn close_all(&self) {
        let streams = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for stream in streams.values() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn send_response(
        self: &Arc<Self>,
        mut stream: TcpStream,
        reply: CoordinatorReply,
        connection: ConnectionPermit,
    ) {
        let writer = thread::Builder::new()
            .name("codebase-graph-coordinator-response".to_string())
            .spawn(move || {
                let _ = write_frame_until(
                    &mut stream,
                    &reply,
                    Instant::now() + COORDINATOR_FRAME_TIMEOUT,
                );
                drop(connection);
            });
        if let Ok(writer) = writer {
            let mut writers = self
                .writers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut index = 0;
            while index < writers.len() {
                if writers[index].is_finished() {
                    let finished = writers.swap_remove(index);
                    let _ = finished.join();
                } else {
                    index += 1;
                }
            }
            writers.push(writer);
        }
    }

    fn join_writers(&self) {
        let writers = std::mem::take(
            &mut *self
                .writers
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        for writer in writers {
            let _ = writer.join();
        }
    }
}

#[derive(Debug)]
struct ConnectionPermit {
    registry: Arc<ConnectionRegistry>,
    id: u64,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.registry
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
    }
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
    pub(crate) fn connect_or_start(mut config: CoordinatorApiConfig) -> Result<Self, String> {
        config.selector = bind_repo_selector(&config.selector)?;
        config.identity = Some(RepositoryIdentity::capture(&config.selector)?);
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
        self.execute_invocation_with_context(operation_id, invocation, ExecutionContext::default())
    }

    pub(crate) fn execute_invocation_with_context(
        &self,
        operation_id: &str,
        invocation: &OperationInvocation,
        context: ExecutionContext,
    ) -> Result<OperationResponse, ApiError> {
        let command = CoordinatorCommand::Execute {
            operation_id: operation_id.to_string(),
            invocation: invocation.clone(),
        };
        let reply = self.send_command_with_recovery(&command, context)?;
        match reply {
            CoordinatorReply::Success(response) => Ok(response),
            CoordinatorReply::Failure(error) => Err(error),
            CoordinatorReply::Pong => Err(coordinator_protocol_error(
                "coordinator returned pong for an operation request",
            )),
        }
    }

    fn refresh_route(&self) -> Result<(), String> {
        self.refresh_route_with_context(ExecutionContext::default())
            .map_err(|error| error.message)
    }

    fn refresh_route_with_context(&self, context: ExecutionContext) -> Result<(), ApiError> {
        let election_deadline = context.deadline.map_or_else(
            || Instant::now() + ELECTION_TIMEOUT,
            |deadline| deadline.min(Instant::now() + ELECTION_TIMEOUT),
        );
        loop {
            context.remaining()?;
            if let Some(lease) = try_open_locked(&self.inner.control.lock, LockMode::Exclusive)
                .map_err(|error| {
                    ApiError::new("coordinator_unavailable", error.to_string()).retryable(true)
                })?
            {
                let (state, owner) = start_owner(
                    self.inner.control.clone(),
                    lease,
                    self.inner.api_config.clone(),
                )
                .map_err(|error| ApiError::new("coordinator_unavailable", error).retryable(true))?;
                context.remaining()?;
                let old_owner = {
                    let mut route = self.lock_route(context)?;
                    route.state = Some(state);
                    route.owner.replace(Arc::new(owner))
                };
                drop(old_owner);
                return Ok(());
            }
            if let Ok(state) = read_coordinator_state(&self.inner.control.state) {
                if ping_state_with_context(&state, context).is_ok() {
                    let mut route = self.lock_route(context)?;
                    let same_endpoint = route
                        .state
                        .as_ref()
                        .is_some_and(|current| current.endpoint == state.endpoint);
                    let old_owner = if same_endpoint {
                        None
                    } else {
                        route.owner.take()
                    };
                    route.state = Some(state);
                    drop(route);
                    drop(old_owner);
                    return Ok(());
                }
            }
            if Instant::now() >= election_deadline {
                if context.deadline.is_some() && Instant::now() >= context.deadline.unwrap() {
                    return Err(ExecutionContext::expired_error());
                }
                return Err(ApiError::new(
                    "coordinator_unavailable",
                    format!(
                        "timed out waiting for repository coordinator at {}",
                        self.inner.control.state.display()
                    ),
                )
                .retryable(true));
            }
            thread::sleep(
                COORDINATOR_POLL_INTERVAL
                    .min(election_deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn lock_route(
        &self,
        context: ExecutionContext,
    ) -> Result<MutexGuard<'_, CoordinatorRoute>, ApiError> {
        if context.deadline.is_none() {
            return self.inner.route.lock().map_err(|_| {
                ApiError::new(
                    "coordinator_unavailable",
                    "coordinator route lock is poisoned",
                )
                .retryable(true)
            });
        }
        loop {
            context.remaining()?;
            match self.inner.route.try_lock() {
                Ok(route) => return Ok(route),
                Err(std::sync::TryLockError::Poisoned(error)) => {
                    return Err(ApiError::new(
                        "coordinator_unavailable",
                        format!("coordinator route lock is poisoned: {error}"),
                    )
                    .retryable(true));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(COORDINATOR_POLL_INTERVAL);
                }
            }
        }
    }

    fn send_command(
        &self,
        command: &CoordinatorCommand,
        context: ExecutionContext,
    ) -> Result<CoordinatorReply, ApiError> {
        let state = self.lock_route(context)?.state.clone().ok_or_else(|| {
            ApiError::new(
                "coordinator_unavailable",
                "repository coordinator route is unavailable",
            )
            .retryable(true)
        })?;
        if context.deadline.is_some() && !state.supports_deadlines {
            return Err(ApiError::new(
                "coordinator_deadline_unsupported",
                "the repository coordinator owner does not support bounded graph execution",
            ));
        }
        send_to_state(&state, command, context)
    }

    fn send_command_with_recovery(
        &self,
        command: &CoordinatorCommand,
        context: ExecutionContext,
    ) -> Result<CoordinatorReply, ApiError> {
        let deadline = context
            .deadline
            .unwrap_or_else(|| Instant::now() + COMMAND_RETRY_TIMEOUT);
        loop {
            context.remaining()?;
            match self.send_command(command, context) {
                Ok(reply) if reply_is_safe_to_retry(&reply) => {
                    if Instant::now() >= deadline {
                        return Err(if context.deadline.is_some() {
                            ExecutionContext::expired_error()
                        } else {
                            ApiError::new(
                                "coordinator_unavailable",
                                format!("repository coordinator kept rejecting the request before dispatch: {reply:?}"),
                            )
                            .retryable(true)
                        });
                    }
                    thread::sleep(
                        COORDINATOR_POLL_INTERVAL
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
                Ok(reply) if reply_requires_route_refresh(&reply) => {
                    if Instant::now() >= deadline {
                        return Err(if context.deadline.is_some() {
                            ExecutionContext::expired_error()
                        } else {
                            ApiError::new(
                                "coordinator_unavailable",
                                format!("repository coordinator route stayed stale: {reply:?}"),
                            )
                            .retryable(true)
                        });
                    }
                    if context.deadline.is_some() {
                        return Err(ApiError::new(
                            "coordinator_route_stale",
                            "repository coordinator rejected the request; route recovery continues in the background",
                        )
                        .retryable(true));
                    }
                    self.refresh_route_with_context(context)?;
                }
                Ok(reply) => return Ok(reply),
                // A transport failure after connect or request transmission is
                // ambiguous: the owner may already be executing the request.
                // Never replay an operation without an explicit pre-dispatch reply.
                Err(error) => return Err(error),
            }
        }
    }

    /// Returns the local executor state, or `None` when this process is a follower
    /// or cannot take a nonblocking route snapshot.
    pub(crate) fn active_operation(&self) -> Option<bool> {
        self.inner
            .route
            .try_lock()
            .ok()
            .and_then(|route| route.owner.as_ref().map(|owner| owner.dispatcher.active()))
    }

    pub(crate) fn drain_owned_operation(&self) {
        self.inner.monitor_stop.store(true, Ordering::Release);
        if let Ok(mut monitor) = self.inner.monitor.lock() {
            if let Some(monitor) = monitor.take() {
                if monitor.thread().id() != thread::current().id() {
                    let _ = monitor.join();
                }
            }
        }
        let owner = loop {
            match self.inner.route.try_lock() {
                Ok(route) => break route.owner.clone(),
                Err(std::sync::TryLockError::Poisoned(error)) => {
                    break error.into_inner().owner.clone()
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(COORDINATOR_POLL_INTERVAL)
                }
            }
        };
        if let Some(owner) = owner {
            owner.stop_and_drain();
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
        match self
            .send_command_with_recovery(&CoordinatorCommand::Ping, ExecutionContext::default())
            .map_err(|error| error.message)?
        {
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
        supports_deadlines: true,
    };
    write_json_atomically(&control.state, &state).map_err(|error| error.to_string())?;
    restrict_state_permissions(&control.state)?;

    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_state = state.clone();
    let state_path = control.state.clone();
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    let server_dispatcher = Arc::clone(&dispatcher);
    let thread = thread::Builder::new()
        .name("codebase-graph-coordinator".to_string())
        .spawn(move || {
            let api = config.build_api();
            let identity = config
                .identity
                .as_ref()
                .expect("coordinator identity is captured before owner startup")
                .clone();
            let owner_selector = config.selector.clone();
            serve_owner(
                listener,
                &api,
                &owner_selector,
                &identity,
                &server_state,
                &server_stop,
                &server_dispatcher,
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
            dispatcher,
        },
    ))
}

fn serve_owner(
    listener: TcpListener,
    api: &CodebaseGraphApi,
    selector: &RepoSelector,
    identity: &RepositoryIdentity,
    state: &CoordinatorState,
    stop: &AtomicBool,
    dispatcher: &Arc<ExecutionDispatcher>,
) {
    let api = api.clone();
    let selector = selector.clone();
    let identity = identity.clone();
    let owner_api = api.clone();
    serve_owner_with_executor(
        listener,
        state,
        stop,
        dispatcher,
        move |mut invocation| {
            identity
                .validate()
                .map_err(|error| ApiError::new("repository_identity_changed", error))?;
            invocation.repo = selector.clone();
            Ok(invocation)
        },
        move |operation_id, invocation, context| {
            context.remaining()?;
            owner_api.execute_invocation(operation_id, invocation)
        },
    );
}

fn serve_owner_with_executor<P, E>(
    listener: TcpListener,
    state: &CoordinatorState,
    stop: &AtomicBool,
    dispatcher: &Arc<ExecutionDispatcher>,
    prepare: P,
    execute: E,
) where
    P: Fn(OperationInvocation) -> Result<OperationInvocation, ApiError> + Send + Sync + 'static,
    E: Fn(&str, &OperationInvocation, ExecutionContext) -> Result<OperationResponse, ApiError>
        + Send
        + Sync
        + 'static,
{
    let dispatcher = Arc::clone(dispatcher);
    let worker_dispatcher = Arc::clone(&dispatcher);
    let worker_state = state.clone();
    let prepare = Arc::new(prepare);
    let execute = Arc::new(execute);
    let worker_execute = Arc::clone(&execute);
    let worker = thread::Builder::new()
        .name("codebase-graph-coordinator-executor".to_string())
        .spawn(move || {
            while let Some(job) = worker_dispatcher.take() {
                let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if job.context.remaining().is_err() {
                        None
                    } else {
                        match worker_execute(&job.operation_id, &job.invocation, job.context) {
                            Ok(mut response) if job.context.remaining().is_ok() => {
                                attach_coordinator_status(&mut response, &worker_state);
                                Some(CoordinatorReply::Success(response))
                            }
                            Ok(_) if job.context.remaining().is_err() => None,
                            Ok(_) => None,
                            Err(_) if job.context.remaining().is_err() => None,
                            Err(error) => Some(CoordinatorReply::Failure(error)),
                        }
                    }
                }))
                .unwrap_or(None);
                worker_dispatcher.finish_one();
                if let Some(reply) = reply {
                    let registry = Arc::clone(&job._connection.registry);
                    registry.send_response(job.stream, reply, job._connection);
                } else {
                    let _ = job.stream.shutdown(std::net::Shutdown::Both);
                    drop(job._connection);
                }
            }
        });

    let Ok(worker) = worker else {
        dispatcher.stop_admission();
        return;
    };
    let registry = Arc::new(ConnectionRegistry::default());
    let mut handlers = Vec::new();
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let accepted_at = Instant::now();
                if stream.set_nonblocking(true).is_err() {
                    continue;
                }
                let Ok(connection) = registry.register(&stream) else {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                };
                let server_state = state.clone();
                let server_dispatcher = Arc::clone(&dispatcher);
                let server_prepare = Arc::clone(&prepare);
                if let Ok(handler) = thread::Builder::new()
                    .name("codebase-graph-coordinator-client".to_string())
                    .spawn(move || {
                        handle_connection_dispatch(
                            &mut stream,
                            accepted_at,
                            &server_state,
                            &server_dispatcher,
                            connection,
                            server_prepare,
                        );
                    })
                {
                    handlers.push(handler);
                }
                let mut index = 0;
                while index < handlers.len() {
                    if handlers[index].is_finished() {
                        let handler = handlers.swap_remove(index);
                        let _ = handler.join();
                    } else {
                        index += 1;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(COORDINATOR_POLL_INTERVAL);
            }
            Err(_) => thread::sleep(COORDINATOR_POLL_INTERVAL),
        }
    }
    dispatcher.stop_admission();
    registry.close_all();
    for handler in handlers {
        let _ = handler.join();
    }
    let _ = worker.join();
    registry.close_all();
    registry.join_writers();
}

fn handle_connection_dispatch<P>(
    stream: &mut TcpStream,
    accepted_at: Instant,
    state: &CoordinatorState,
    dispatcher: &ExecutionDispatcher,
    connection: ConnectionPermit,
    prepare: Arc<P>,
) where
    P: Fn(OperationInvocation) -> Result<OperationInvocation, ApiError> + Send + Sync + 'static,
{
    let request_deadline = accepted_at + COORDINATOR_FRAME_TIMEOUT;
    let request: CoordinatorRequest = match read_frame_until(stream, request_deadline) {
        Ok(request) => request,
        Err(error) => {
            let reply = coordinator_request_receive_failure(error);
            let _ = write_frame_until(stream, &reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
            return;
        }
    };
    if request.version != COORDINATOR_PROTOCOL_VERSION || request.token != state.token {
        let reply = CoordinatorReply::Failure(ApiError::new(
            COORDINATOR_AUTHENTICATION_FAILED,
            "repository coordinator protocol or token is invalid",
        ));
        let _ = write_frame_until(stream, &reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
        return;
    }
    match request.command {
        CoordinatorCommand::Ping => {
            let _ = write_frame_until(
                stream,
                &CoordinatorReply::Pong,
                Instant::now() + COORDINATOR_FRAME_TIMEOUT,
            );
        }
        CoordinatorCommand::Execute {
            operation_id,
            invocation,
        } => {
            if request.timeout_ms.is_some() && !matches!(operation_id.as_str(), "health" | "search")
            {
                let reply = CoordinatorReply::Failure(ApiError::new(
                    "invalid_coordinator_deadline",
                    "bounded coordinator execution is only supported for health and search",
                ));
                let _ =
                    write_frame_until(stream, &reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
                return;
            }
            let context = match coordinator_execution_context(request.timeout_ms, accepted_at) {
                Ok(context) => context,
                Err(error) => {
                    let reply = CoordinatorReply::Failure(error);
                    let _ = write_frame_until(
                        stream,
                        &reply,
                        Instant::now() + COORDINATOR_FRAME_TIMEOUT,
                    );
                    return;
                }
            };
            let invocation = match prepare(invocation) {
                Ok(invocation) => invocation,
                Err(error) => {
                    let reply = CoordinatorReply::Failure(error);
                    let _ = write_frame_until(
                        stream,
                        &reply,
                        Instant::now() + COORDINATOR_FRAME_TIMEOUT,
                    );
                    return;
                }
            };
            if context.remaining().is_err() {
                let reply = CoordinatorReply::Failure(ExecutionContext::expired_error());
                let _ =
                    write_frame_until(stream, &reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
                return;
            }
            let Ok(job_stream) = stream.try_clone() else {
                return;
            };
            let job = ExecutionJob {
                operation_id,
                invocation,
                context,
                stream: job_stream,
                _connection: connection,
            };
            if let Err(job) = dispatcher.try_submit(job) {
                let reply = CoordinatorReply::Failure(
                    ApiError::new(
                        "graph_busy",
                        "repository graph executor is already handling another operation",
                    )
                    .retryable(true),
                );
                let _ =
                    write_frame_until(stream, &reply, Instant::now() + COORDINATOR_FRAME_TIMEOUT);
                drop(job);
            }
        }
    }
}

fn coordinator_execution_context(
    timeout_ms: Option<u64>,
    accepted_at: Instant,
) -> Result<ExecutionContext, ApiError> {
    let Some(timeout_ms) = timeout_ms else {
        return Ok(ExecutionContext::default());
    };
    if timeout_ms == 0 {
        return Err(ApiError::new(
            "invalid_coordinator_deadline",
            "coordinator timeout_ms must be greater than zero",
        ));
    }
    let timeout = Duration::from_millis(timeout_ms.min(MAX_DEADLINE_MILLIS));
    Ok(ExecutionContext::with_timeout(accepted_at, timeout))
}

#[cfg(test)]
fn receive_request(stream: &mut TcpStream) -> Result<CoordinatorRequest, CoordinatorReply> {
    read_frame_until(stream, Instant::now() + COORDINATOR_FRAME_TIMEOUT)
        .map_err(coordinator_request_receive_failure)
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
    context: ExecutionContext,
) -> Result<CoordinatorReply, ApiError> {
    send_to_state_with_context(state, command, context)
}

fn send_to_state_with_context(
    state: &CoordinatorState,
    command: &CoordinatorCommand,
    context: ExecutionContext,
) -> Result<CoordinatorReply, ApiError> {
    validate_state(state).map_err(|error| ApiError::new("coordinator_protocol_error", error))?;
    context.remaining()?;
    let ping = matches!(command, CoordinatorCommand::Ping);
    let connect_deadline = context.deadline.unwrap_or_else(|| {
        Instant::now()
            + if ping {
                STREAM_IO_TIMEOUT
            } else {
                CONNECT_TIMEOUT
            }
    });
    let connect_budget =
        CONNECT_TIMEOUT.min(connect_deadline.saturating_duration_since(Instant::now()));
    if connect_budget.is_zero() {
        return Err(ExecutionContext::expired_error());
    }
    let mut stream =
        TcpStream::connect_timeout(&state.endpoint, connect_budget).map_err(|error| {
            if context
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                ExecutionContext::expired_error()
            } else {
                ApiError::new(
                    "coordinator_unavailable",
                    format!("failed to connect to repository coordinator: {error}"),
                )
                .retryable(true)
            }
        })?;
    stream.set_nonblocking(true).map_err(|error| {
        ApiError::new(
            "coordinator_unavailable",
            format!("failed to configure coordinator stream: {error}"),
        )
        .retryable(true)
    })?;
    let timeout_ms = context
        .remaining()?
        .map(|remaining| remaining.as_nanos().saturating_add(999_999) / 1_000_000)
        .map(|millis| millis.clamp(1, u128::from(MAX_DEADLINE_MILLIS)) as u64);
    let coordinator_deadline =
        timeout_ms.map(|timeout| Instant::now() + Duration::from_millis(timeout));
    let execution_deadline = context
        .deadline
        .into_iter()
        .chain(coordinator_deadline)
        .min();
    let write_deadline = execution_deadline
        .unwrap_or_else(|| Instant::now() + COORDINATOR_FRAME_TIMEOUT)
        .min(Instant::now() + COORDINATOR_FRAME_TIMEOUT);
    write_frame_until(
        &mut stream,
        &CoordinatorRequest {
            version: COORDINATOR_PROTOCOL_VERSION,
            token: state.token.clone(),
            timeout_ms,
            command: clone_command(command),
        },
        write_deadline,
    )
    .map_err(|error| {
        if execution_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            ExecutionContext::expired_error()
        } else {
            ApiError::new("coordinator_transport_error", error).retryable(ping)
        }
    })?;
    let read_deadline =
        execution_deadline.or_else(|| ping.then_some(Instant::now() + STREAM_IO_TIMEOUT));
    read_frame_with_policy(&mut stream, read_deadline, read_deadline.is_none()).map_err(|error| {
        if execution_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            ExecutionContext::expired_error()
        } else {
            ApiError::new("coordinator_transport_error", error).retryable(ping)
        }
    })
}

fn ping_state(state: &CoordinatorState) -> Result<(), String> {
    ping_state_with_context(state, ExecutionContext::default())
}

fn ping_state_with_context(
    state: &CoordinatorState,
    context: ExecutionContext,
) -> Result<(), String> {
    match send_to_state_with_context(state, &CoordinatorCommand::Ping, context)
        .map_err(|error| error.message)?
    {
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

#[cfg(test)]
fn write_frame<T: Serialize>(stream: &mut TcpStream, value: &T) -> Result<(), String> {
    write_frame_until(stream, value, Instant::now() + COORDINATOR_FRAME_TIMEOUT)
}

fn write_frame_until<T: Serialize>(
    stream: &mut TcpStream,
    value: &T,
    deadline: Instant,
) -> Result<(), String> {
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
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure coordinator stream: {error}"))?;
    let mut offset = 0;
    while offset < payload.len() {
        if Instant::now() >= deadline {
            return Err("timed out writing repository coordinator frame".to_string());
        }
        match stream.write(&payload[offset..]) {
            Ok(0) => return Err("repository coordinator stream closed while writing".to_string()),
            Ok(written) => offset += written,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(
                    COORDINATOR_POLL_INTERVAL
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(format!(
                    "failed to write repository coordinator frame: {error}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn read_frame<T: DeserializeOwned>(stream: &mut TcpStream) -> Result<T, String> {
    read_frame_until(stream, Instant::now() + COORDINATOR_FRAME_TIMEOUT)
}

fn read_frame_until<T: DeserializeOwned>(
    stream: &mut TcpStream,
    deadline: Instant,
) -> Result<T, String> {
    read_frame_with_policy(stream, Some(deadline), false)
}

fn read_frame_with_policy<T: DeserializeOwned>(
    stream: &mut TcpStream,
    deadline: Option<Instant>,
    start_timeout_after_first_byte: bool,
) -> Result<T, String> {
    stream
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure coordinator stream: {error}"))?;
    let mut payload = Vec::new();
    payload
        .try_reserve(4096)
        .map_err(|_| "repository coordinator frame allocation failed".to_string())?;
    let mut buffer = [0_u8; 8192];
    let mut deadline = deadline;
    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err("timed out reading repository coordinator frame".to_string());
        }
        match stream.read(&mut buffer) {
            Ok(0) if payload.is_empty() => {
                return Err("repository coordinator frame is missing".to_string());
            }
            Ok(0) => return Err("repository coordinator frame ended before newline".to_string()),
            Ok(read) => {
                if start_timeout_after_first_byte && deadline.is_none() {
                    deadline = Some(Instant::now() + COORDINATOR_FRAME_TIMEOUT);
                }
                let frame_end = buffer[..read].iter().position(|byte| *byte == b'\n');
                let bytes_to_append = frame_end.unwrap_or(read);
                if payload.len().saturating_add(bytes_to_append) > MAX_FRAME_BYTES {
                    return Err(format!(
                        "repository coordinator frame exceeds {MAX_FRAME_BYTES} bytes"
                    ));
                }
                payload.extend_from_slice(&buffer[..bytes_to_append]);
                if frame_end.is_some() {
                    return serde_json::from_slice(&payload).map_err(|error| {
                        format!("failed to decode repository coordinator frame: {error}")
                    });
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let pause = deadline
                    .map(|deadline| {
                        COORDINATOR_POLL_INTERVAL
                            .min(deadline.saturating_duration_since(Instant::now()))
                    })
                    .unwrap_or(COORDINATOR_POLL_INTERVAL);
                if !pause.is_zero() {
                    thread::sleep(pause);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(format!(
                    "failed to read repository coordinator frame: {error}"
                ))
            }
        }
    }
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
#[path = "coordinator/tests.rs"]
mod tests;
