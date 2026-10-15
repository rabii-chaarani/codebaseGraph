use super::*;

fn wait_until(message: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("{message}");
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let running = unsafe { WaitForSingleObject(handle, 0) == WAIT_TIMEOUT };
    unsafe {
        CloseHandle(handle);
    }
    running
}

// These PIDs are captured from the current fixture's ready/start handshake,
// never from legacy recovery metadata or a production daemon.
#[cfg(unix)]
fn pause(pid: u32) {
    assert_eq!(
        unsafe { libc::kill(pid as i32, libc::SIGSTOP) },
        0,
        "pause owned process {pid}"
    );
    // SIGSTOP delivery is asynchronous. Observe the stopped state before
    // fault injection so a still-running watchdog cannot win that race.
    wait_until(
        "owned process did not reach the stopped-state barrier",
        || {
            Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .is_ok_and(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout)
                            .trim_start()
                            .starts_with('T')
                })
        },
    );
}

#[cfg(unix)]
fn resume(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGCONT);
    }
}

#[cfg(unix)]
fn terminate(pid: u32) {
    assert_eq!(
        unsafe { libc::kill(pid as i32, libc::SIGKILL) },
        0,
        "terminate owned process {pid}"
    );
}

#[cfg(windows)]
fn for_each_thread(pid: u32, action: impl Fn(windows_sys::Win32::Foundation::HANDLE)) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            Threading::{OpenThread, THREAD_SUSPEND_RESUME},
        },
    };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    assert_ne!(snapshot, INVALID_HANDLE_VALUE);
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut found = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while found {
        if entry.th32OwnerProcessID == pid {
            let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if !handle.is_null() {
                action(handle);
                unsafe {
                    CloseHandle(handle);
                }
            }
        }
        found = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
}

#[cfg(windows)]
fn pause(pid: u32) {
    for_each_thread(pid, |handle| {
        assert_ne!(
            unsafe { windows_sys::Win32::System::Threading::SuspendThread(handle) },
            u32::MAX
        );
    });
}

#[cfg(windows)]
fn resume(pid: u32) {
    for_each_thread(pid, |handle| unsafe {
        windows_sys::Win32::System::Threading::ResumeThread(handle);
    });
}

#[cfg(windows)]
fn terminate(pid: u32) {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
    };
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    assert!(!handle.is_null());
    assert_ne!(unsafe { TerminateProcess(handle, 125) }, 0);
    unsafe {
        CloseHandle(handle);
    }
}

fn control(descriptor: &Value, command: &str, nonce: &str) -> String {
    let address = descriptor["endpoint"].as_str().unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    serde_json::to_writer(&mut stream, &json!({"nonce":nonce,"command":command})).unwrap();
    stream.write_all(b"\n").unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

fn control_after_connect_delay(descriptor: &Value) {
    let mut stream = TcpStream::connect(descriptor["endpoint"].as_str().unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    thread::sleep(Duration::from_millis(50));
    serde_json::to_writer(
        &mut stream,
        &json!({"nonce": descriptor["nonce"], "command":"PING"}),
    )
    .unwrap();
    stream.write_all(b"\n").unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert_eq!(
        reply.trim(),
        "OK",
        "delayed authenticated control frame was rejected"
    );

    let mut slow = TcpStream::connect(descriptor["endpoint"].as_str().unwrap()).unwrap();
    slow.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    slow.write_all(b"{\"nonce\":").unwrap();
    let mut trickle = slow.try_clone().unwrap();
    let writer = thread::spawn(move || {
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(40));
            if trickle.write_all(b" ").is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut reply = String::new();
    slow.read_to_string(&mut reply).unwrap();
    assert_eq!(reply.trim(), "ERR");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "trickled control input stalled supervision"
    );
    writer.join().unwrap();
}

fn read_json_when_present(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn find_json(root: &Path, name: &str) -> Option<(PathBuf, Value)> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = entry.file_type().ok()?;
        if kind.is_dir() {
            if let Some(found) = find_json(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file == name) {
            if let Some(value) = read_json_when_present(&path) {
                return Some((path, value));
            }
        }
    }
    None
}

fn configured_repo() -> (PathBuf, PathBuf, Value) {
    let root = temp_repo();
    fs::write(
        root.join("initial.py"),
        "def initial_symbol():\n    return 1\n",
    )
    .unwrap();
    let config = install(&root);
    set_config(
        &config,
        ConfigSettings {
            policy: "leader",
            backend: "poll",
            interval_ms: 100,
            worker_memory_mib: 512,
            rust_memory_mib: 384,
            spill_chunk_mib: 32,
            max_parallelism: 1,
            exclude: &[],
        },
    );
    let previous =
        read_json_when_present(&root.join(".codebaseGraph/storage/active.json")).unwrap();
    fs::write(
        root.join("pending.py"),
        "def pending_symbol():\n    return 2\n",
    )
    .unwrap();
    (root, config, previous)
}

fn assert_generation(root: &Path, previous: &Value) {
    let active = read_json_when_present(&root.join(".codebaseGraph/storage/active.json")).unwrap();
    assert_eq!(active["generation_id"], previous["generation_id"]);
}

#[test]
fn verified_abandoned_guardian_is_terminated_before_automatic_refresh() {
    let (root, config, previous) = configured_repo();
    // On macOS this can use /var while runtime discovery uses /private/var.
    // Equivalent private directory aliases must not strand empty containment.
    let workspace = root.join(".codebaseGraph/storage/workers/worker-orphan");
    let lifecycle = workspace.join(".lifecycle-owned");
    fs::create_dir_all(&lifecycle).unwrap();
    fs::write(lifecycle.join("lease"), b"").unwrap();
    let random_hex = || {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).unwrap();
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let descriptor = json!({
        "version":2,"build_id":random_hex(),"lifecycle_dir":lifecycle,
        "endpoint":"","nonce":random_hex(),"worker_pid":0,"guardian_pid":0,
        "proof":{"kind":"pending"}
    });
    let launch_path = lifecycle.join("launch.json");
    fs::write(
        &launch_path,
        serde_json::to_vec(&json!({"descriptor":descriptor,"spec":{
        "program":binary(),"args":vec![std::ffi::OsString::from("__codebase_graph_internal"),std::ffi::OsString::from("not-started")],
            "workspace":workspace,"stderr_path":workspace.join("stderr.log"),"stdout":"Null",
            "memory_limit_bytes":null,"parent_rss_pid":null
        }}))
        .unwrap(),
    )
    .unwrap();
    let mut command = Command::new(binary());
    command
        .args(["__codebase_graph_internal", "build-guardian-v2"])
        .arg(&launch_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    let mut guardian = ChildGuard(command.spawn().unwrap());
    // Keep owner authority alive deliberately: only authenticated recovery by
    // the new exclusive worker-lock holder may end this abandoned startup.
    let owner_pipe = guardian.0.stdin.take().unwrap();
    let mut ready = Value::Null;
    wait_until("abandoned guardian never established containment", || {
        if let Some(value) = read_json_when_present(&lifecycle.join("ready.json")) {
            ready = value;
            true
        } else {
            false
        }
    });
    let worker = ready["worker_pid"].as_u64().unwrap() as u32;
    assert!(alive(worker));
    fs::write(
        root.join(".codebaseGraph/storage/worker.json"),
        serde_json::to_vec(&ready).unwrap(),
    )
    .unwrap();
    assert_generation(&root, &previous);
    let (daemon, port) = start_http_server(&config, &root, &[]);
    let mut session = McpSession::start(port);
    wait_until("verified abandoned worker survived recovery", || {
        !alive(worker)
    });
    wait_for_current_symbol(&mut session, "pending_symbol", "pending.py");
    assert!(guardian.0.wait().unwrap().success());
    drop(owner_pipe);
    fs::write(
        root.join("follow_up.py"),
        "def following_symbol():\n    return 3\n",
    )
    .unwrap();
    wait_for_current_symbol(&mut session, "following_symbol", "follow_up.py");
    drop(daemon);
    fs::remove_dir_all(root).unwrap();
}

fn owner_exit_at_start_boundary(environment: &str, marker_name: &str, has_state: bool) {
    let (root, config, previous) = configured_repo();
    let barrier = root.join("start-barrier");
    fs::create_dir(&barrier).unwrap();
    let (daemon, _) =
        start_http_server_with_env(&config, &root, &[], &[(environment, barrier.as_path())]);
    let mut marker = Value::Null;
    wait_until(
        "build did not reach its deterministic startup boundary",
        || {
            if let Some(value) = read_json_when_present(&barrier.join(marker_name)) {
                marker = value;
                true
            } else {
                false
            }
        },
    );
    let state_path = root.join(".codebaseGraph/storage/worker.json");
    assert_eq!(state_path.exists(), has_state);
    #[cfg(unix)]
    if has_state {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&state_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let descriptor = if has_state {
        read_json_when_present(&state_path).unwrap()
    } else {
        marker
    };
    let lifecycle = PathBuf::from(descriptor["lifecycle_dir"].as_str().unwrap());
    let worker = descriptor["worker_pid"].as_u64().unwrap() as u32;
    assert!(alive(worker));
    assert!(
        lifecycle.exists(),
        "live startup lease was deleted before owner exit"
    );
    assert_generation(&root, &previous);
    drop(daemon);
    wait_until("gated worker escaped after owner termination", || {
        !alive(worker)
    });
    let (daemon, port) = start_http_server(&config, &root, &[]);
    let mut session = McpSession::start(port);
    wait_for_current_symbol(&mut session, "pending_symbol", "pending.py");
    drop(daemon);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn owner_exit_before_guardian_readiness_cleans_gated_tree() {
    owner_exit_at_start_boundary(
        "CODEBASE_GRAPH_TEST_GUARDIAN_READY_BARRIER_DIR",
        "before-ready.json",
        false,
    );
}

#[test]
fn owner_exit_after_state_before_start_cleans_gated_tree() {
    owner_exit_at_start_boundary(
        "CODEBASE_GRAPH_TEST_WORKER_START_BARRIER_DIR",
        "state-persisted.json",
        true,
    );
}

#[test]
fn standalone_owner_loss_preserves_candidate_while_guardian_is_unavailable() {
    let (root, config, previous) = configured_repo();
    let barrier = root.join("standalone-barrier");
    fs::create_dir(&barrier).unwrap();
    let error_path = root.join("standalone.stderr");
    let mut build = ChildGuard(
        Command::new(binary())
            .current_dir(&root)
            .args([
                "build",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--json",
            ])
            .env("CODEBASE_GRAPH_TEST_PHASE_BARRIER_DIR", &barrier)
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&error_path).unwrap()))
            .spawn()
            .unwrap(),
    );
    let mut phase = Value::Null;
    wait_until("standalone loader did not reach its ready barrier", || {
        for entry in fs::read_dir(&barrier).unwrap().flatten() {
            if let Some(value) = read_json_when_present(&entry.path()) {
                if value["request_path"].is_string() {
                    phase = value;
                    return true;
                }
            }
        }
        false
    });
    let request_path = PathBuf::from(phase["request_path"].as_str().unwrap());
    let request = read_json_when_present(&request_path).unwrap();
    let candidate_parent = Path::new(request["db_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();
    let phase_pid = phase["pid"].as_u64().unwrap() as u32;
    struct ResumeOwned(Vec<u32>);
    impl Drop for ResumeOwned {
        fn drop(&mut self) {
            for pid in &self.0 {
                resume(*pid);
            }
        }
    }
    let (_, descriptor) =
        find_json(&root.join(".codebaseGraph/storage/phases"), "ready.json").unwrap();
    let lifecycle = PathBuf::from(descriptor["lifecycle_dir"].as_str().unwrap());
    let guardian_pid = descriptor["guardian_pid"].as_u64().unwrap() as u32;
    let mut paused = ResumeOwned(Vec::new());
    pause(phase_pid);
    paused.0.push(phase_pid);
    pause(guardian_pid);
    paused.0.push(guardian_pid);
    terminate(build.0.id());
    let mut result = None;
    wait_until(
        "standalone owner did not exit at the injected failure",
        || {
            result = build.0.try_wait().unwrap();
            result.is_some()
        },
    );
    assert!(!result.unwrap().success());
    assert!(alive(phase_pid));
    assert!(
        candidate_parent.is_dir(),
        "abort deleted a possibly live candidate"
    );
    assert!(request_path.is_file(), "abort deleted the loader request");
    assert!(lifecycle.is_dir(), "abort deleted containment evidence");
    assert_generation(&root, &previous);

    let retry = Command::new(binary())
        .current_dir(&root)
        .args(["build", "--repo-root", root.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        !retry.status.success(),
        "a new build overlapped unknown standalone containment"
    );
    assert!(String::from_utf8_lossy(&retry.stderr).contains("containment_unknown"));
    assert!(candidate_parent.is_dir());
    assert!(request_path.is_file());

    let (daemon, port) = start_http_server(&config, &root, &[]);
    let mut session = McpSession::start(port);
    let mut blocked = Value::Null;
    wait_until("automatic refresh did not retain its blocked work", || {
        blocked = session.health();
        blocked["refresh"]["pending"] == true
            && blocked["refresh"]["last_error"]
                .as_str()
                .is_some_and(|error| error.contains("containment_unknown"))
    });
    assert!(session
        .search_paths("initial_symbol")
        .iter()
        .any(|path| path == "initial.py"));
    assert_generation(&root, &previous);
    fs::write(barrier.join("CONTINUE"), b"continue\n").unwrap();
    drop(paused);
    wait_until(
        "resumed guardian did not clean the abandoned standalone loader",
        || !alive(phase_pid),
    );
    wait_for_current_symbol(&mut session, "pending_symbol", "pending.py");
    drop(daemon);
    fs::remove_dir_all(root).unwrap();
}

struct Fixture {
    root: PathBuf,
    config: PathBuf,
    barrier: PathBuf,
    previous_generation: Value,
    daemon: Option<ChildGuard>,
    session: McpSession,
    descriptor: Value,
    phase_pid: u32,
    paused: Vec<u32>,
}

impl Fixture {
    fn new(timing: bool) -> Self {
        let root = temp_repo();
        fs::write(
            root.join("initial.py"),
            "def initial_symbol():\n    return 1\n",
        )
        .unwrap();
        let config = install(&root);
        set_config(
            &config,
            ConfigSettings {
                policy: "leader",
                backend: "poll",
                interval_ms: 100,
                worker_memory_mib: 512,
                rust_memory_mib: 384,
                spill_chunk_mib: 32,
                max_parallelism: 1,
                exclude: &[],
            },
        );
        let previous_generation: Value = serde_json::from_slice(
            &fs::read(root.join(".codebaseGraph/storage/active.json")).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("pending.py"),
            "def pending_symbol():\n    return 2\n",
        )
        .unwrap();
        let barrier = root.join("phase-barrier");
        fs::create_dir(&barrier).unwrap();
        let timing_dir = root.join("phase-timing");
        let mut environment = vec![("CODEBASE_GRAPH_TEST_PHASE_BARRIER_DIR", barrier.as_path())];
        if timing {
            environment.push(("CODEBASE_GRAPH_PHASE_TIMING_DIR", timing_dir.as_path()));
        }
        let (daemon, port) = start_http_server_with_env(&config, &root, &[], &environment);
        let session = McpSession::start(port);
        let mut phase_pid = 0;
        wait_until(
            "native database phase did not reach its ready barrier",
            || {
                for entry in fs::read_dir(&barrier).unwrap() {
                    let path = entry.unwrap().path();
                    if path
                        .extension()
                        .is_some_and(|extension| extension == "json")
                        && !path.to_string_lossy().ends_with(".memory.json")
                    {
                        let value: Value =
                            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
                        phase_pid = value["pid"].as_u64().unwrap() as u32;
                        let request_path =
                            fs::canonicalize(value["request_path"].as_str().unwrap()).unwrap();
                        assert!(request_path.starts_with(fs::canonicalize(&root).unwrap()));
                        return true;
                    }
                }
                false
            },
        );
        let descriptor: Value = serde_json::from_slice(
            &fs::read(root.join(".codebaseGraph/storage/worker.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(descriptor["version"], 2);
        assert!(alive(phase_pid));
        Self {
            root,
            config,
            barrier,
            previous_generation,
            daemon: Some(daemon),
            session,
            descriptor,
            phase_pid,
            paused: Vec::new(),
        }
    }
    fn worker(&self) -> u32 {
        self.descriptor["worker_pid"].as_u64().unwrap() as u32
    }
    fn guardian(&self) -> u32 {
        self.descriptor["guardian_pid"].as_u64().unwrap() as u32
    }
    fn pause(&mut self, pid: u32) {
        pause(pid);
        self.paused.push(pid);
    }
    fn unchanged(&self) {
        let active: Value = serde_json::from_slice(
            &fs::read(self.root.join(".codebaseGraph/storage/active.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            active["generation_id"],
            self.previous_generation["generation_id"]
        );
    }
    fn release(&self) {
        fs::write(self.barrier.join("CONTINUE"), b"continue\n").unwrap();
    }
    fn verify_recovery(&mut self) {
        self.release();
        wait_for_current_symbol(&mut self.session, "pending_symbol", "pending.py");
        fs::write(
            self.root.join("follow_up.py"),
            "def following_symbol():\n    return 3\n",
        )
        .unwrap();
        wait_for_current_symbol(&mut self.session, "following_symbol", "follow_up.py");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Resuming a paused owned worker permits its parent-lifetime watchdog
        // to handle the deliberately lost-guardian fixture during unwinding.
        for pid in self.paused.drain(..) {
            resume(pid);
        }
        self.daemon.take();
        let deadline = Instant::now() + Duration::from_secs(6);
        while alive(self.phase_pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        if !alive(self.phase_pid) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[test]
fn paused_database_loader_exits_after_outer_worker_failure_and_refresh_recovers() {
    let mut fixture = Fixture::new(false);
    fixture.pause(fixture.phase_pid);
    terminate(fixture.worker());
    wait_until("paused loader survived termination of its worker", || {
        !alive(fixture.phase_pid)
    });
    fixture.paused.clear();
    fixture.unchanged();
    fixture.verify_recovery();
}

#[test]
fn authenticated_cancellation_cleans_loader_and_retries_pending_edit() {
    let mut fixture = Fixture::new(false);
    fixture.pause(fixture.phase_pid);
    control_after_connect_delay(&fixture.descriptor);
    let rejected = control(&fixture.descriptor, "CANCEL test", "incorrect-capability");
    assert_eq!(rejected.trim(), "ERR");
    assert!(alive(fixture.phase_pid));
    let reply = control(
        &fixture.descriptor,
        "CANCEL regression",
        fixture.descriptor["nonce"].as_str().unwrap(),
    );
    assert_eq!(reply.trim(), "OK");
    wait_until("cancelled loader remained alive", || {
        !alive(fixture.phase_pid)
    });
    fixture.paused.clear();
    fixture.unchanged();
    fixture.verify_recovery();
}

#[test]
fn real_loader_memory_failure_cleans_containment_and_retries_pending_edit() {
    let mut fixture = Fixture::new(false);
    // The native loader owns the allocation. Its ordinary RSS sampler must
    // detect this while the loader remains inside its ready barrier.
    fs::write(
        fixture.barrier.join("ALLOCATE_BYTES"),
        (512_u64 * 1024 * 1024).to_string(),
    )
    .unwrap();
    let mut last_health = Value::Null;
    wait_until(
        "native loader allocation did not report a memory budget failure",
        || {
            last_health = fixture.session.health();
            last_health["refresh"]["pending"] == true
                && last_health["refresh"]["last_error"]
                    .as_str()
                    .is_some_and(|error| error.contains("memory_budget_exceeded"))
        },
    );
    wait_until("memory failure left the loader alive", || {
        !alive(fixture.phase_pid)
    });
    fixture.unchanged();
    fs::remove_file(fixture.barrier.join("ALLOCATE_BYTES")).unwrap();
    fixture.verify_recovery();
}

#[test]
fn owner_exit_cleans_paused_worker_and_loader_before_recovery() {
    let mut fixture = Fixture::new(false);
    fixture.pause(fixture.phase_pid);
    fixture.pause(fixture.worker());
    let worker = fixture.worker();
    fixture.daemon.take();
    wait_until("owner exit did not clean its stopped build tree", || {
        !alive(worker) && !alive(fixture.phase_pid)
    });
    fixture.paused.clear();
    fixture.unchanged();
    fixture.release();
    let (daemon, port) = start_http_server_with_env(
        &fixture.config,
        &fixture.root,
        &[],
        &[(
            "CODEBASE_GRAPH_TEST_PHASE_BARRIER_DIR",
            fixture.barrier.as_path(),
        )],
    );
    fixture.daemon = Some(daemon);
    fixture.session = McpSession::start(port);
    fixture.verify_recovery();
}

#[test]
fn unavailable_guardian_preserves_paused_tree_evidence_until_emptiness_is_proven() {
    let mut fixture = Fixture::new(false);
    fixture.pause(fixture.phase_pid);
    fixture.pause(fixture.worker());
    fixture.pause(fixture.guardian());
    fixture.daemon.take();
    let (daemon, port) = start_http_server_with_env(
        &fixture.config,
        &fixture.root,
        &[],
        &[(
            "CODEBASE_GRAPH_TEST_PHASE_BARRIER_DIR",
            fixture.barrier.as_path(),
        )],
    );
    fixture.daemon = Some(daemon);
    fixture.session = McpSession::start(port);
    let mut blocked = Value::Null;
    wait_until("lost guardian did not expose unknown containment", || {
        blocked = fixture.session.health();
        blocked["refresh"]["pending"] == true
            && blocked["refresh"]["last_error"]
                .as_str()
                .is_some_and(|error| error.contains("containment_unknown"))
    });
    let retained: Value = serde_json::from_slice(
        &fs::read(fixture.root.join(".codebaseGraph/storage/worker.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(retained["build_id"], fixture.descriptor["build_id"]);
    assert!(Path::new(retained["lifecycle_dir"].as_str().unwrap()).exists());
    assert!(alive(fixture.phase_pid));
    fixture.unchanged();
    fixture.release();
    resume(fixture.guardian());
    wait_until("resumed guardian did not close its abandoned tree", || {
        !alive(fixture.phase_pid)
    });
    fixture.paused.clear();
    fixture.verify_recovery();
}

#[test]
fn missing_guardian_closes_the_build_tree_and_automatic_refresh_retries() {
    let mut fixture = Fixture::new(false);
    fixture.pause(fixture.phase_pid);
    fixture.pause(fixture.worker());
    let worker = fixture.worker();
    terminate(fixture.guardian());
    wait_until(
        "guardian loss left contained native processes alive",
        || !alive(worker) && !alive(fixture.phase_pid),
    );
    fixture.paused.clear();
    fixture.unchanged();
    fixture.verify_recovery();
}

#[cfg(target_os = "macos")]
#[test]
fn timing_wrapper_keeps_loader_in_build_containment() {
    let mut fixture = Fixture::new(true);
    fixture.pause(fixture.phase_pid);
    terminate(fixture.worker());
    wait_until("timing-wrapped loader escaped containment", || {
        !alive(fixture.phase_pid)
    });
    fixture.paused.clear();
    fixture.unchanged();
    fixture.verify_recovery();
    assert!(fs::read_dir(fixture.root.join("phase-timing"))
        .unwrap()
        .next()
        .is_some());
}
