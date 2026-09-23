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
            monitor_stop: AtomicBool::new(true),
            monitor: Mutex::new(None),
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
        thread: Mutex::new(Some(server)),
        dispatcher,
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
