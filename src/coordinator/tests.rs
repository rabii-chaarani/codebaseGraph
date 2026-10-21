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
                    supports_deadlines: true,
                }),
                owner: None,
            }),
            monitor_stop: Arc::new(AtomicBool::new(false)),
            monitor: JoinedThread::new(None),
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

#[test]
fn ping_stays_responsive_and_graph_requests_are_rejected_while_busy() {
    let (entered_tx, entered_rx, release_state) = execution_gate();
    let execute_release_state = Arc::clone(&release_state);
    let root = temp_dir("busy-dispatch");
    let client = test_client(&root, move |operation_id, invocation, _context| {
        if operation_id == "blocked" {
            let _ = entered_tx.send(());
            wait_for_release(&execute_release_state);
        }
        Ok(test_response(operation_id, invocation))
    });
    let release_guard = ReleaseGuard(release_state);

    let request_client = client.clone();
    let request =
        thread::spawn(move || request_client.execute_invocation("blocked", &test_invocation()));
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("blocked request should enter the execution worker");
    thread::sleep(Duration::from_millis(100));
    assert!(
        !request.is_finished(),
        "unbounded graph call must remain in flight"
    );
    assert_eq!(client.active_operation(), Some(true));
    client.ping().expect("Ping must bypass the graph executor");

    let busy = client
        .execute_invocation("later", &test_invocation())
        .unwrap_err();
    assert_eq!(busy.code, "graph_busy");
    assert!(busy.retryable);

    release_guard.release();
    assert_eq!(request.join().unwrap().unwrap().operation, "blocked");
    let idle_deadline = Instant::now() + Duration::from_secs(1);
    while client.active_operation() == Some(true) && Instant::now() < idle_deadline {
        thread::sleep(COORDINATOR_POLL_INTERVAL);
    }
    assert_eq!(client.active_operation(), Some(false));
    assert_eq!(
        client
            .execute_invocation("later", &test_invocation())
            .unwrap()
            .operation,
        "later"
    );
}

#[test]
fn caller_deadline_does_not_release_the_real_execution_permit() {
    let (entered_tx, entered_rx, release_state) = execution_gate();
    let execute_release_state = Arc::clone(&release_state);
    let calls = Arc::new(AtomicU64::new(0));
    let execute_calls = Arc::clone(&calls);
    let root = temp_dir("deadline-permit");
    let client = test_client(&root, move |operation_id, invocation, _context| {
        execute_calls.fetch_add(1, Ordering::Relaxed);
        if operation_id == "health" {
            let _ = entered_tx.send(());
            wait_for_release(&execute_release_state);
        }
        Ok(test_response(operation_id, invocation))
    });
    let release_guard = ReleaseGuard(release_state);

    let request_client = client.clone();
    let deadline = Instant::now() + Duration::from_millis(120);
    let request = thread::spawn(move || {
        request_client.execute_invocation_with_context(
            "health",
            &test_invocation(),
            ExecutionContext {
                deadline: Some(deadline),
            },
        )
    });
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("deadline request should enter the execution worker");
    let error = request.join().unwrap().unwrap_err();
    assert_eq!(error.code, "deadline_exceeded");
    assert_eq!(client.active_operation(), Some(true));
    let busy = client
        .execute_invocation("after-timeout", &test_invocation())
        .unwrap_err();
    assert_eq!(busy.code, "graph_busy");
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "busy work must not dispatch"
    );

    release_guard.release();
    let idle_deadline = Instant::now() + Duration::from_secs(1);
    while client.active_operation() == Some(true) && Instant::now() < idle_deadline {
        thread::sleep(COORDINATOR_POLL_INTERVAL);
    }
    assert_eq!(client.active_operation(), Some(false));
    assert_eq!(
        client
            .execute_invocation("after-timeout", &test_invocation())
            .unwrap()
            .operation,
        "after-timeout"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn drain_waits_for_active_execution_after_the_caller_times_out() {
    let (entered_tx, entered_rx, release_state) = execution_gate();
    let execute_release_state = Arc::clone(&release_state);
    let root = temp_dir("drain-active");
    let client = test_client(&root, move |operation_id, invocation, _context| {
        if operation_id == "health" {
            let _ = entered_tx.send(());
            wait_for_release(&execute_release_state);
        }
        Ok(test_response(operation_id, invocation))
    });
    let release_guard = ReleaseGuard(release_state);
    let request_client = client.clone();
    let request = thread::spawn(move || {
        request_client.execute_invocation_with_context(
            "health",
            &test_invocation(),
            ExecutionContext::with_timeout(Instant::now(), Duration::from_millis(100)),
        )
    });
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("deadline request should enter the execution worker");
    assert_eq!(
        request.join().unwrap().unwrap_err().code,
        "deadline_exceeded"
    );

    let drain_client = client.clone();
    let drain = thread::spawn(move || drain_client.drain_owned_operation());
    thread::sleep(Duration::from_millis(100));
    assert!(
        !drain.is_finished(),
        "drain must wait for native execution to finish"
    );
    release_guard.release();
    drain.join().unwrap();
    assert_eq!(client.active_operation(), Some(false));
}

#[test]
fn elapsed_receive_deadline_is_not_extended_by_trickled_frame_bytes() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_frame_until::<serde_json::Value>(
            &mut stream,
            Instant::now() + Duration::from_millis(150),
        )
    });
    let mut client = TcpStream::connect(endpoint).unwrap();
    let writer = thread::spawn(move || {
        for byte in br#"{"request":"slow"}"# {
            if client.write_all(&[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = client.write_all(b"\n");
    });
    let error = server.join().unwrap().unwrap_err();
    writer.join().unwrap();
    assert!(
        error.contains("timed out"),
        "unexpected read failure: {error}"
    );
}

#[test]
fn expired_request_received_as_trickled_bytes_never_dispatches() {
    let calls = Arc::new(AtomicU64::new(0));
    let execute_calls = Arc::clone(&calls);
    let root = temp_dir("expired-before-dispatch");
    let client = test_client(&root, move |operation_id, invocation, _context| {
        execute_calls.fetch_add(1, Ordering::Relaxed);
        Ok(test_response(operation_id, invocation))
    });
    let state = client.endpoint().unwrap();
    let route_state = client.inner.route.lock().unwrap().state.clone().unwrap();
    let request = CoordinatorRequest {
        version: COORDINATOR_PROTOCOL_VERSION,
        token: route_state.token,
        timeout_ms: Some(50),
        command: CoordinatorCommand::Execute {
            operation_id: "health".to_string(),
            invocation: test_invocation(),
        },
    };
    let mut payload = serde_json::to_vec(&request).unwrap();
    payload.push(b'\n');
    let mut stream = TcpStream::connect(state).unwrap();
    let split = payload.len() / 2;
    stream.write_all(&payload[..split]).unwrap();
    thread::sleep(Duration::from_millis(80));
    stream.write_all(&payload[split..]).unwrap();
    let reply: CoordinatorReply = read_frame(&mut stream).unwrap();
    let CoordinatorReply::Failure(error) = reply else {
        panic!("expired request unexpectedly returned a graph result");
    };
    assert_eq!(error.code, "deadline_exceeded");
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn deadline_calls_fail_promptly_when_owner_state_is_legacy() {
    let root = temp_dir("legacy-deadline");
    let mut state = test_state(
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap(),
    );
    state.supports_deadlines = false;
    let serialized = serde_json::to_value(&state).unwrap();
    let mut legacy = serialized.as_object().unwrap().clone();
    legacy.remove("supports_deadlines");
    let legacy_state: CoordinatorState = serde_json::from_value(legacy.into()).unwrap();
    assert!(!legacy_state.supports_deadlines);
    let client = test_client_with_state(&root, legacy_state);
    let error = client
        .execute_invocation_with_context(
            "health",
            &test_invocation(),
            ExecutionContext::with_timeout(Instant::now(), Duration::from_millis(500)),
        )
        .unwrap_err();
    assert_eq!(error.code, "coordinator_deadline_unsupported");
}

#[test]
fn ambiguous_operation_disconnect_is_not_replayed_or_retryable() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = listener.local_addr().unwrap();
    let received = Arc::new(AtomicU64::new(0));
    let server_received = Arc::clone(&received);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request: CoordinatorRequest = read_frame(&mut stream).unwrap();
        assert!(matches!(
            request.command,
            CoordinatorCommand::Execute { .. }
        ));
        server_received.fetch_add(1, Ordering::Relaxed);
        drop(stream);
    });
    let root = temp_dir("ambiguous-no-replay");
    let client = test_client_with_state(&root, test_state(endpoint));
    let error = client
        .execute_invocation("mutation", &test_invocation())
        .unwrap_err();
    assert_eq!(error.code, "coordinator_transport_error");
    assert!(!error.retryable);
    server.join().unwrap();
    assert_eq!(received.load(Ordering::Relaxed), 1);
}

fn test_client<F>(root: &Path, execute: F) -> CoordinatorClient
where
    F: Fn(&str, &OperationInvocation, ExecutionContext) -> Result<OperationResponse, ApiError>
        + Send
        + Sync
        + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let state = test_state(listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    let server_state = state.clone();
    let server_stop = Arc::clone(&stop);
    let server_dispatcher = Arc::clone(&dispatcher);
    let server = thread::spawn(move || {
        serve_owner_with_executor(
            listener,
            &server_state,
            &server_stop,
            &server_dispatcher,
            Ok,
            execute,
        );
    });
    test_client_with_owner(root, state, stop, dispatcher, server)
}

type ExecutionGateState = Arc<(Mutex<bool>, Condvar)>;
type ExecutionGate = (
    std::sync::mpsc::SyncSender<()>,
    std::sync::mpsc::Receiver<()>,
    ExecutionGateState,
);

fn execution_gate() -> ExecutionGate {
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    (
        entered_tx,
        entered_rx,
        Arc::new((Mutex::new(false), Condvar::new())),
    )
}

fn wait_for_release(state: &ExecutionGateState) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let (released, ready) = &**state;
    let mut released = released.lock().unwrap();
    while !*released {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let (next, timeout) = ready.wait_timeout(released, remaining).unwrap();
        released = next;
        if timeout.timed_out() {
            break;
        }
    }
}

struct ReleaseGuard(ExecutionGateState);

impl ReleaseGuard {
    fn release(&self) {
        let (released, ready) = &*self.0;
        *released.lock().unwrap() = true;
        ready.notify_all();
    }
}

impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        self.release();
    }
}

fn test_client_with_state(root: &Path, state: CoordinatorState) -> CoordinatorClient {
    CoordinatorClient {
        inner: Arc::new(ClientInner {
            control: CoordinatorControlPaths {
                lock: root.join("coordinator.lock"),
                state: root.join("coordinator.json"),
            },
            api_config: direct_config(root),
            route: Mutex::new(CoordinatorRoute {
                state: Some(state),
                owner: None,
            }),
            monitor_stop: Arc::new(AtomicBool::new(true)),
            monitor: JoinedThread::new(None),
        }),
    }
}

fn test_client_with_owner(
    root: &Path,
    state: CoordinatorState,
    stop: Arc<AtomicBool>,
    dispatcher: Arc<ExecutionDispatcher>,
    server: JoinHandle<()>,
) -> CoordinatorClient {
    let mut client = test_client_with_state(root, state.clone());
    let owner = Arc::new(CoordinatorOwner {
        stop,
        thread: JoinedThread::new(Some(server)),
        dispatcher,
        refresh_cancel: RefreshCancellation::default(),
    });
    Arc::get_mut(&mut client.inner)
        .expect("test client has a unique inner")
        .route
        .get_mut()
        .unwrap()
        .owner = Some(owner);
    client
}

fn test_state(endpoint: SocketAddr) -> CoordinatorState {
    CoordinatorState {
        version: COORDINATOR_PROTOCOL_VERSION,
        endpoint,
        token: coordinator_token(endpoint),
        pid: std::process::id(),
        supports_deadlines: true,
    }
}

fn test_invocation() -> OperationInvocation {
    OperationInvocation {
        repo: RepoSelector::default(),
        arguments: serde_json::json!({}),
        output_format: crate::api::OutputFormat::Typed,
    }
}

fn test_response(operation_id: &str, invocation: &OperationInvocation) -> OperationResponse {
    OperationResponse::from_payload(
        operation_id,
        invocation.output_format,
        serde_json::json!({"ok": true}),
    )
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

fn maintenance_test_job() -> ExecutionJob {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (_server, _) = listener.accept().unwrap();
    let registry = Arc::new(ConnectionRegistry::default());
    let connection = registry.register(&stream).unwrap();
    ExecutionJob {
        operation_id: "health".into(),
        invocation: test_invocation(),
        context: ExecutionContext::with_timeout(Instant::now(), Duration::from_secs(60)),
        stream,
        _connection: connection,
    }
}

#[test]
fn maintenance_permit_blocks_real_execution_until_actual_drop() {
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    let permit = dispatcher.try_acquire_maintenance().unwrap();
    assert!(dispatcher.active());
    assert!(dispatcher.try_acquire_maintenance().is_none());
    let rejected = dispatcher.try_submit(maintenance_test_job()).unwrap_err();
    assert!(dispatcher.active());
    drop(permit);
    assert!(!dispatcher.active());
    dispatcher.try_submit(*rejected).unwrap();
    // Taking the queued job does not release actual operation ownership.
    let job = dispatcher.take().unwrap();
    assert!(dispatcher.try_acquire_maintenance().is_none());
    drop(job);
    dispatcher.finish_one();
    assert!(dispatcher.try_acquire_maintenance().is_some());
}

#[test]
fn maintenance_cannot_bypass_queued_or_active_real_execution() {
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    dispatcher.try_submit(maintenance_test_job()).unwrap();
    assert!(dispatcher.try_acquire_maintenance().is_none());
    let job = dispatcher.take().unwrap();
    assert!(dispatcher.try_acquire_maintenance().is_none());
    drop(job);
    dispatcher.finish_one();
    let permit = dispatcher.try_acquire_maintenance().unwrap();
    dispatcher.stop_admission();
    assert!(dispatcher.active());
    drop(permit);
    assert!(!dispatcher.active());
    assert!(dispatcher.try_acquire_maintenance().is_none());
    assert!(dispatcher.try_submit(maintenance_test_job()).is_err());
}

#[test]
fn maintenance_permit_is_exclusive_during_simultaneous_grant_requests() {
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    let start = Arc::new(std::sync::Barrier::new(9));
    let acquired = Arc::new(std::sync::Barrier::new(9));
    let release = Arc::new(std::sync::Barrier::new(9));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let dispatcher = Arc::clone(&dispatcher);
        let start = Arc::clone(&start);
        let acquired = Arc::clone(&acquired);
        let release = Arc::clone(&release);
        workers.push(thread::spawn(move || {
            start.wait();
            let permit = dispatcher.try_acquire_maintenance();
            let won = permit.is_some();
            acquired.wait();
            release.wait();
            drop(permit);
            won
        }));
    }
    start.wait();
    acquired.wait();
    assert!(dispatcher.active());
    release.wait();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
    assert!(!dispatcher.active());
}

#[test]
fn maintenance_panic_releases_slot_without_resurrecting_stopped_admission() {
    let dispatcher = Arc::new(ExecutionDispatcher::default());
    let result = std::panic::catch_unwind({
        let dispatcher = Arc::clone(&dispatcher);
        move || {
            let _permit = dispatcher.try_acquire_maintenance().unwrap();
            dispatcher.stop_admission();
            panic!("fixture maintenance panic");
        }
    });
    assert!(result.is_err());
    assert!(!dispatcher.active());
    assert!(dispatcher.try_acquire_maintenance().is_none());
}

// Uses the actual ExecutionDispatcher, not a parallel scheduler occupancy flag.
mod joined_maintenance_driver {
    use super::*;
    use crate::storage::maintenance_budget::{CleanupMemory, MutationGrant, ReservedBytes};
    use crate::storage::maintenance_scheduler::{
        AttemptOutcome, Cancellation, DriverError, DriverLiveness, MaintenanceDriver,
        MaintenanceRuntime, RequestDisposition, TargetBinding, Trigger,
    };
    use std::rc::Rc;
    use std::sync::mpsc::{self, Receiver, Sender};

    struct Work {
        dispose_entered: Sender<()>,
        dispose_release: Receiver<()>,
        _bytes: ReservedBytes,
        _not_send: Rc<()>,
    }
    impl Drop for Work {
        fn drop(&mut self) {
            self.dispose_entered.send(()).unwrap();
            self.dispose_release.recv().unwrap();
        }
    }
    struct Driver {
        prepare_entered: Sender<()>,
        prepare_release: Receiver<()>,
        disposal: Option<(Sender<()>, Receiver<()>)>,
        confirmations: Sender<bool>,
        mode: u8,
        _not_send: Rc<()>,
    }
    impl MaintenanceDriver for Driver {
        type Work = Work;
        fn prepare(
            &mut self,
            _: TargetBinding,
            _: &Cancellation<'_>,
            memory: &CleanupMemory,
        ) -> Result<Option<Work>, DriverError> {
            self.prepare_entered.send(()).unwrap();
            self.prepare_release.recv().unwrap();
            let (dispose_entered, dispose_release) = self.disposal.take().unwrap();
            Ok(Some(Work {
                dispose_entered,
                dispose_release,
                _bytes: memory.reserve(32).unwrap().bytes(32).unwrap(),
                _not_send: Rc::new(()),
            }))
        }
        fn mutate(
            &mut self,
            _: &Work,
            grant: &mut MutationGrant,
            confirmation: bool,
        ) -> Result<AttemptOutcome, DriverError> {
            grant.take_entry().unwrap();
            self.confirmations.send(confirmation).unwrap();
            match self.mode {
                0 => Ok(AttemptOutcome::Complete),
                1 => Err(DriverError::Mutation),
                _ => panic!("fixture mutation panic"),
            }
        }
    }

    fn real_request_progresses(dispatcher: &Arc<ExecutionDispatcher>) {
        dispatcher.try_submit(maintenance_test_job()).unwrap();
        let job = dispatcher.take().unwrap();
        assert!(dispatcher.active());
        drop(job);
        dispatcher.finish_one();
    }

    #[test]
    fn dc_joined_driver_releases_real_permit_before_slow_drop_on_success_error_and_panic() {
        for mode in 0..3 {
            let dispatcher = Arc::new(ExecutionDispatcher::default());
            let (prepare_entered, prepared) = mpsc::channel();
            let (release_prepare, prepare_release) = mpsc::channel();
            let (dispose_entered, disposing) = mpsc::channel();
            let (release_dispose, dispose_release) = mpsc::channel();
            let (confirmations, confirmation) = mpsc::channel();
            let mut runtime = MaintenanceRuntime::start(
                CleanupMemory::new(1024 * 1024).unwrap(),
                TargetBinding([1; 32]),
                dispatcher.clone(),
                move |_| {
                    Ok(Driver {
                        prepare_entered,
                        prepare_release,
                        disposal: Some((dispose_entered, dispose_release)),
                        confirmations,
                        mode,
                        _not_send: Rc::new(()),
                    })
                },
            )
            .unwrap();
            prepared.recv_timeout(Duration::from_secs(5)).unwrap();
            real_request_progresses(&dispatcher);
            assert_eq!(
                runtime
                    .handle()
                    .request(Trigger::Health, Some(TargetBinding([1; 32]))),
                RequestDisposition::Enqueued
            );
            // Duplicate true coalesces; a later ordinary request cannot clear it.
            assert_eq!(
                runtime
                    .handle()
                    .request(Trigger::Health, Some(TargetBinding([1; 32]))),
                RequestDisposition::Enqueued
            );
            assert_eq!(
                runtime.handle().request(Trigger::Health, None),
                RequestDisposition::Enqueued
            );
            release_prepare.send(()).unwrap();
            assert!(confirmation.recv_timeout(Duration::from_secs(5)).unwrap());
            disposing.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(!dispatcher.active());
            real_request_progresses(&dispatcher);
            let handle = runtime.handle();
            // Joining must wait for actual disposal, but cannot hold the mailbox.
            let (joined_tx, joined) = mpsc::channel();
            let joiner = thread::spawn(move || {
                runtime.stop_and_join();
                joined_tx.send(()).unwrap();
            });
            while handle.snapshot().liveness == DriverLiveness::Running {
                thread::yield_now();
            }
            assert_eq!(
                handle.request(Trigger::Health, None),
                RequestDisposition::Stopped
            );
            assert!(joined.try_recv().is_err());
            release_dispose.send(()).unwrap();
            joined.recv_timeout(Duration::from_secs(5)).unwrap();
            joiner.join().unwrap();
            assert_eq!(
                handle.snapshot().liveness,
                if mode == 2 {
                    DriverLiveness::Failed
                } else {
                    DriverLiveness::Stopped
                }
            );
        }
    }

    #[test]
    fn dc_joined_driver_rebind_and_stop_during_preparation_prevent_mutation() {
        for stop in [false, true] {
            let dispatcher = Arc::new(ExecutionDispatcher::default());
            let (prepare_entered, prepared) = mpsc::channel();
            let (release_prepare, prepare_release) = mpsc::channel();
            let (dispose_entered, disposing) = mpsc::channel();
            let (release_dispose, dispose_release) = mpsc::channel();
            let (confirmations, confirmation) = mpsc::channel();
            let mut runtime = MaintenanceRuntime::start(
                CleanupMemory::new(1024 * 1024).unwrap(),
                TargetBinding([1; 32]),
                dispatcher.clone(),
                move |_| {
                    Ok(Driver {
                        prepare_entered,
                        prepare_release,
                        disposal: Some((dispose_entered, dispose_release)),
                        confirmations,
                        mode: 0,
                        _not_send: Rc::new(()),
                    })
                },
            )
            .unwrap();
            prepared.recv_timeout(Duration::from_secs(5)).unwrap();
            let handle = runtime.handle();
            assert_eq!(
                handle.request(Trigger::Health, Some(TargetBinding([9; 32]))),
                RequestDisposition::UnboundConfirmation
            );
            assert_eq!(
                handle.request(Trigger::Health, Some(TargetBinding([1; 32]))),
                RequestDisposition::Enqueued
            );
            assert_eq!(
                handle.rebind(TargetBinding([1; 32])),
                RequestDisposition::Enqueued
            );
            assert_eq!(handle.snapshot().generation, 0);
            if stop {
                runtime.request_stop();
            } else {
                handle.rebind(TargetBinding([2; 32]));
            }
            release_prepare.send(()).unwrap();
            disposing.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(confirmation.try_recv().is_err());
            if !stop {
                real_request_progresses(&dispatcher);
            }
            release_dispose.send(()).unwrap();
            runtime.stop_and_join();
            assert!(!dispatcher.active());
            assert!(handle
                .snapshot()
                .last_attempt
                .is_none_or(|record| record.generation == handle.snapshot().generation));
        }
    }

    #[test]
    fn dc_joined_driver_start_limit_and_factory_failure_are_truthful() {
        let dispatcher = Arc::new(ExecutionDispatcher::default());
        assert!(MaintenanceRuntime::start::<Driver, _>(
            CleanupMemory::new(128).unwrap(),
            TargetBinding([1; 32]),
            dispatcher.clone(),
            |_| unreachable!()
        )
        .is_err());
        let mut runtime = MaintenanceRuntime::start::<Driver, _>(
            CleanupMemory::new(1024 * 1024).unwrap(),
            TargetBinding([1; 32]),
            dispatcher,
            |_| Err(DriverError::ResourceLimit),
        )
        .unwrap();
        let handle = runtime.handle();
        let deadline = Instant::now() + Duration::from_secs(5);
        while handle.snapshot().liveness != DriverLiveness::Failed {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert_eq!(handle.snapshot().failure, Some(DriverError::ResourceLimit));
        assert_eq!(
            handle.request(Trigger::Health, None),
            RequestDisposition::Failed
        );
        runtime.stop_and_join();
    }

    struct GateAdmission {
        dispatcher: Arc<ExecutionDispatcher>,
        acquired: Sender<()>,
        release: Mutex<Receiver<()>>,
    }
    impl crate::storage::maintenance_scheduler::MaintenanceAdmission for GateAdmission {
        type Permit = MaintenanceExecutionPermit;
        fn try_acquire(&self) -> Option<Self::Permit> {
            let permit = self.dispatcher.try_acquire_maintenance()?;
            self.acquired.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            Some(permit)
        }
        fn stop_admission(&self) {
            self.dispatcher.stop_admission();
        }
    }

    #[test]
    fn dc_joined_driver_stop_or_rebind_winning_postpermit_gate_has_no_mutation() {
        for stop in [false, true] {
            let dispatcher = Arc::new(ExecutionDispatcher::default());
            let (prepare_entered, prepared) = mpsc::channel();
            let (release_prepare, prepare_release) = mpsc::channel();
            let (dispose_entered, disposing) = mpsc::channel();
            let (release_dispose, dispose_release) = mpsc::channel();
            let (confirmations, confirmation) = mpsc::channel();
            let (acquired, acquired_rx) = mpsc::channel();
            let (release_gate, release) = mpsc::channel();
            let mut runtime = MaintenanceRuntime::start(
                CleanupMemory::new(1024 * 1024).unwrap(),
                TargetBinding([1; 32]),
                GateAdmission {
                    dispatcher: dispatcher.clone(),
                    acquired,
                    release: Mutex::new(release),
                },
                move |_| {
                    Ok(Driver {
                        prepare_entered,
                        prepare_release,
                        disposal: Some((dispose_entered, dispose_release)),
                        confirmations,
                        mode: 0,
                        _not_send: Rc::new(()),
                    })
                },
            )
            .unwrap();
            prepared.recv_timeout(Duration::from_secs(5)).unwrap();
            release_prepare.send(()).unwrap();
            acquired_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(dispatcher.active());
            assert!(dispatcher.try_submit(maintenance_test_job()).is_err());
            if stop {
                runtime.request_stop();
            } else {
                runtime.handle().rebind(TargetBinding([2; 32]));
            }
            release_gate.send(()).unwrap();
            disposing.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(!dispatcher.active());
            assert!(confirmation.try_recv().is_err());
            if !stop {
                real_request_progresses(&dispatcher);
            }
            release_dispose.send(()).unwrap();
            runtime.stop_and_join();
        }
    }
}

mod refresh_owned_join {
    use super::*;
    use std::sync::mpsc;

    struct Release(Option<mpsc::Sender<()>>);
    impl Release {
        fn release(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    impl Drop for Release {
        fn drop(&mut self) {
            self.release();
        }
    }

    struct Payload {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        panic_again: bool,
    }
    impl Drop for Payload {
        fn drop(&mut self) {
            let _ = self.entered.send(());
            let _ = self.release.recv();
            if self.panic_again {
                panic!("fixture join payload destructor");
            }
        }
    }
    #[test]
    fn refresh_join_two_drainers_wait_for_payload_drop_and_wake_after_unwind() {
        for panic_again in [false, true] {
            let (entered, disposing) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let target = thread::spawn(move || {
                std::panic::panic_any(Payload {
                    entered,
                    release: released,
                    panic_again,
                })
            });
            let joined = Arc::new(JoinedThread::new(Some(target)));
            let mut release = Release(Some(release));
            let first = joined.clone();
            let (first_tx, first_rx) = mpsc::channel();
            let first = thread::spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| first.join()));
                first_tx.send(result.is_ok()).unwrap();
            });
            disposing.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(
                joined.state.try_lock().is_ok(),
                "mutex held during join/payload drop"
            );
            assert!(matches!(
                *joined.state.lock().unwrap(),
                JoinState::Joining(_)
            ));
            let second = joined.clone();
            let (second_tx, second_rx) = mpsc::channel();
            let second = thread::spawn(move || second_tx.send(second.join()).unwrap());
            assert!(
                second_rx.recv_timeout(Duration::from_millis(50)).is_err(),
                "concurrent drain returned before disposal"
            );
            assert!(first_rx.try_recv().is_err());
            release.release();
            assert_eq!(
                first_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                !panic_again
            );
            assert!(second_rx.recv_timeout(Duration::from_secs(5)).unwrap());
            first.join().unwrap();
            second.join().unwrap();
            assert!(matches!(*joined.state.lock().unwrap(), JoinState::Joined));
        }
    }

    #[test]
    fn refresh_join_self_identity_survives_handle_taken_by_another_drainer() {
        let joined = Arc::new(JoinedThread::new(None));
        let weak = Arc::downgrade(&joined);
        let (release, released) = mpsc::channel();
        let (result, result_rx) = mpsc::channel();
        let target = thread::spawn(move || {
            let _ = released.recv();
            result.send(weak.upgrade().unwrap().join()).unwrap();
        });
        joined.install(target);
        let mut release = Release(Some(release));
        let drainer = joined.clone();
        let drainer = thread::spawn(move || assert!(drainer.join()));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(*joined.state.lock().unwrap(), JoinState::Joining(_)) {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        release.release();
        assert!(!result_rx.recv_timeout(Duration::from_secs(5)).unwrap());
        drainer.join().unwrap();
        assert!(joined.join());
    }

    fn await_join_transition(joined: &JoinedThread) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // Condition synchronization only: the actual unlocked-join probe
            // below executes once, after admission's short guard has gone.
            if let Ok(state) = joined.state.try_lock() {
                if matches!(*state, JoinState::Joining(_)) {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "drainer did not release its joining transition"
            );
            thread::yield_now();
        }
    }

    #[test]
    fn refresh_owner_and_monitor_release_state_and_route_mutexes_before_join() {
        let stop = Arc::new(AtomicBool::new(false));
        let owner = Arc::new(CoordinatorOwner {
            stop: stop.clone(),
            thread: JoinedThread::new(None),
            dispatcher: Arc::new(ExecutionDispatcher::default()),
            refresh_cancel: RefreshCancellation::default(),
        });
        let weak = Arc::downgrade(&owner);
        let (observed, observed_rx) = mpsc::channel();
        let target = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            let owner = weak.upgrade().unwrap();
            await_join_transition(&owner.thread);
            observed
                .send(owner.thread.state.try_lock().is_ok())
                .unwrap();
        });
        owner.thread.install(target);
        owner.stop_and_drain();
        assert!(observed_rx.recv_timeout(Duration::from_secs(5)).unwrap());

        let root = temp_dir("monitor-unlocked-drain");
        let client = test_client_with_state(&root, test_state("127.0.0.1:1".parse().unwrap()));
        client.inner.monitor_stop.store(false, Ordering::Release);
        let weak = Arc::downgrade(&client.inner);
        let stop = client.inner.monitor_stop.clone();
        let (observed, observed_rx) = mpsc::channel();
        let target = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            let inner = weak.upgrade().unwrap();
            await_join_transition(&inner.monitor);
            observed
                .send((
                    inner.monitor.state.try_lock().is_ok(),
                    inner.route.try_lock().is_ok(),
                ))
                .unwrap();
        });
        client.inner.monitor.install(target);
        client.drain_owned_operation();
        assert_eq!(
            observed_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            (true, true)
        );
    }

    #[test]
    fn refresh_monitor_final_self_drop_exits_before_another_wait_or_work() {
        let root = temp_dir("monitor-self-drop-tail");
        let owner_stop = Arc::new(AtomicBool::new(false));
        let server_stop = owner_stop.clone();
        let (server_done, server_drained) = mpsc::channel();
        let server = thread::spawn(move || {
            while !server_stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            server_done.send(()).unwrap();
        });
        let client = test_client_with_owner(
            &root,
            test_state("127.0.0.1:1".parse().unwrap()),
            owner_stop,
            Arc::new(ExecutionDispatcher::default()),
            server,
        );
        client.inner.monitor_stop.store(false, Ordering::Release);
        let stop = client.inner.monitor_stop.clone();
        let weak = Arc::downgrade(&client.inner);
        let probe_weak = weak.clone();
        let (observed, observation) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let waits = Arc::new(AtomicU64::new(0));
        let probe_waits = waits.clone();
        let (done, drained) = mpsc::channel();
        let monitor = thread::spawn(move || {
            monitor_route_with_probe(probe_weak, stop, |phase| match phase {
                MonitorPhase::BeforeWait => {
                    probe_waits.fetch_add(1, Ordering::SeqCst);
                }
                MonitorPhase::Observed => {
                    observed.send(()).unwrap();
                    let _ = released.recv();
                }
            });
            done.send(()).unwrap();
        });
        client.inner.monitor.install(monitor);
        let mut release = Release(Some(release));
        observation.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(client); // monitor's iteration now owns the last strong reference
        release.release();
        drained.recv_timeout(Duration::from_secs(5)).unwrap();
        server_drained.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(weak.upgrade().is_none());
        assert_eq!(waits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn refresh_owner_executor_start_failure_and_stop_before_api_start_join_refresh() {
        for stop_before_api in [false, true] {
            let root = temp_dir("owner-refresh-startup");
            let control = CoordinatorControlPaths {
                lock: root.join("coordinator.lock"),
                state: root.join("coordinator.json"),
            };
            let lease = try_open_locked(&control.lock, LockMode::Exclusive)
                .unwrap()
                .unwrap();
            let mut config = direct_config(&root);
            config.identity = Some(RepositoryIdentity::capture(&config.selector).unwrap());
            config.refresh = Some(RefreshServiceConfig::default());
            let (before, before_rx) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let (started, started_rx) = mpsc::channel();
            let (_, owner) =
                start_owner_with_probe(control.clone(), lease, config, move |stage| match stage {
                    OwnerStartup::BeforeRefresh => {
                        if stop_before_api {
                            before.send(()).unwrap();
                            let _ = released.recv();
                        }
                    }
                    OwnerStartup::RefreshStarted(state) => {
                        INJECT_EXECUTOR_SPAWN_FAILURE.with(|flag| flag.set(true));
                        started.send(state.unwrap()).unwrap();
                    }
                })
                .unwrap();
            let mut release = Release(Some(release));
            if stop_before_api {
                before_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                // This is the exact owner token created before its serving thread.
                owner.stop.store(true, Ordering::Release);
                owner.refresh_cancel.request_stop();
                owner.dispatcher.stop_admission();
                release.release();
            }
            let refresh = started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            owner.stop_and_drain();
            assert!(!refresh.snapshot().task_alive);
            assert!(
                !control.state.exists(),
                "serving state survived completed cleanup"
            );
            assert!(
                try_open_locked(&control.lock, LockMode::Exclusive)
                    .unwrap()
                    .is_some(),
                "coordinator lease escaped drain"
            );
            if stop_before_api {
                assert_eq!(refresh.snapshot().effective_root, None);
            }
        }
    }
}
