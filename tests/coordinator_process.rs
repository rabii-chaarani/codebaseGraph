use serde_json::json;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn twenty_mcp_clients_share_one_coordinator_worker_and_take_over() {
    let repo = temp_repo();
    let state = repo.join(".codebaseGraph");
    let storage = state.join("storage");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn coordinated_graph() -> bool { true }\n",
    )
    .unwrap();
    fs::create_dir_all(&state).unwrap();
    write_config(&repo, &storage, 768, 384, 32);

    let mut clients = ClientGroup::default();
    for index in 0..20 {
        clients.push(McpClient::start(&repo, index));
    }

    let coordinator_path = storage.join("coordinator.json");
    let first_state = wait_for_coordinator(&coordinator_path, None, Duration::from_secs(10));
    let first_pid = first_state["pid"].as_u64().unwrap() as u32;
    assert_eq!(
        clients
            .clients
            .iter()
            .filter(|client| client.pid() == first_pid)
            .count(),
        1
    );

    let workers_root = storage.join("workers");
    let active_path = storage.join("active.json");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut maximum_workers = 0_usize;
    let mut owner_min_rss = u64::MAX;
    let mut owner_max_rss = 0_u64;
    let mut worker_peak_rss = 0_u64;
    while !active_path.exists() && Instant::now() < deadline {
        maximum_workers = maximum_workers.max(directory_count(&workers_root));
        if let Ok(rss) = sample_process_rss(first_pid) {
            owner_min_rss = owner_min_rss.min(rss);
            owner_max_rss = owner_max_rss.max(rss);
        }
        if let Some(pid) = active_worker_pid(&storage.join("worker.json")) {
            if let Ok(rss) = sample_process_rss(pid) {
                worker_peak_rss = worker_peak_rss.max(rss);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    maximum_workers = maximum_workers.max(directory_count(&workers_root));
    assert!(
        active_path.is_file(),
        "startup refresh did not publish a graph"
    );
    assert_eq!(maximum_workers, 1, "did not observe exactly one worker");
    assert!(worker_peak_rss > 0, "did not sample the isolated worker");
    assert!(
        worker_peak_rss <= 768 * 1024 * 1024,
        "worker RSS exceeded 768 MiB: {worker_peak_rss}"
    );
    assert!(
        owner_max_rss.saturating_sub(owner_min_rss) <= 16 * 1024 * 1024,
        "coordinator RSS grew by more than 16 MiB while the worker built: min={owner_min_rss} max={owner_max_rss}"
    );

    let follower_rss = clients
        .clients
        .iter()
        .filter(|client| client.pid() != first_pid)
        .filter_map(|client| sample_process_rss(client.pid()).ok())
        .collect::<Vec<_>>();
    assert_eq!(follower_rss.len(), 19);
    let follower_rss_p95 = percentile_95(follower_rss);
    let owner_idle_rss = sample_process_rss(first_pid).unwrap();
    let owner_build_growth = owner_max_rss.saturating_sub(owner_min_rss);
    eprintln!(
        "resource_gate stdio_follower_p95={follower_rss_p95} coordinator_idle={owner_idle_rss} coordinator_build_growth={owner_build_growth} worker_peak={worker_peak_rss}"
    );
    assert!(
        follower_rss_p95 <= 40 * 1024 * 1024,
        "idle stdio follower RSS p95 exceeded 40 MiB"
    );
    assert!(
        owner_idle_rss <= 64 * 1024 * 1024,
        "stdio owner plus coordinator RSS exceeded 64 MiB"
    );

    let follower = clients
        .clients
        .iter_mut()
        .find(|client| client.pid() != first_pid)
        .expect("a follower MCP client should exist");
    // Startup worker metrics are published before the refresh leader installs
    // and probes its watcher. Wait for both signals before changing a source
    // file so the test cannot write into that unobserved readiness gap.
    let health_deadline = Instant::now() + Duration::from_secs(10);
    let mut health_id = 10_000;
    let health = loop {
        let health = follower.call(
            health_id,
            "tools/call",
            json!({
                "name": "graph_health",
                "arguments": {"include_structured_content": true}
            }),
        );
        let refresh = &health["result"]["structuredContent"]["refresh"];
        let metrics_published = refresh["phase_high_water_marks"]["materialization_worker_rss"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0);
        let watcher_ready = matches!(refresh["backend"].as_str(), Some("native") | Some("poll"));
        if metrics_published && watcher_ready {
            break health;
        }
        assert!(
            Instant::now() < health_deadline,
            "refresh resource metrics were not published: {health}"
        );
        health_id += 1;
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(
        health["result"]["structuredContent"]["coordinator"]["pid"],
        json!(first_pid)
    );
    let initial_generation = active_generation_id(&active_path);
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn coordinated_graph_v2() -> bool { true }\n",
    )
    .unwrap();
    let updated_generation = wait_for_active_generation_change(
        &active_path,
        &initial_generation,
        Duration::from_secs(30),
    );
    assert_ne!(updated_generation, initial_generation);
    let search_deadline = Instant::now() + Duration::from_secs(10);
    let mut search_id = 10_001;
    loop {
        let search = follower.call(
            search_id,
            "tools/call",
            json!({
                "name": "graph_search",
                "arguments": {
                    "query": "coordinated_graph_v2",
                    "include_structured_content": true
                }
            }),
        );
        if search["result"]["structuredContent"]["results"]
            .as_array()
            .is_some_and(|results| !results.is_empty())
        {
            break;
        }
        assert!(
            Instant::now() < search_deadline,
            "published refresh did not become searchable"
        );
        search_id += 1;
        std::thread::sleep(Duration::from_millis(25));
    }

    clients.kill_pid(first_pid);
    let replacement =
        wait_for_coordinator(&coordinator_path, Some(first_pid), Duration::from_secs(5));
    let replacement_pid = replacement["pid"].as_u64().unwrap() as u32;
    assert_ne!(replacement_pid, first_pid);
    assert!(clients
        .clients
        .iter()
        .any(|client| client.pid() == replacement_pid));

    clients.kill_all();
    let active_before = active_generation_id(&active_path);
    write_config(&repo, &storage, 129, 1, 1);
    fs::write(
        repo.join("src/lib.rs"),
        format!(
            "/*{}*/\npub fn coordinated_graph() -> bool {{ false }}\n",
            "x".repeat(2 * 1024 * 1024)
        ),
    )
    .unwrap();
    clients.push(McpClient::start(&repo, 20));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_error = String::new();
    while Instant::now() < deadline {
        let health = clients.clients[20].call(
            20_000,
            "tools/call",
            json!({
                "name": "graph_health",
                "arguments": {"include_structured_content": true}
            }),
        );
        last_error = health["result"]["structuredContent"]["refresh"]["last_error"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if last_error.contains("memory_budget_exceeded") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        last_error.contains("memory_budget_exceeded"),
        "expected a structured memory failure, got {last_error:?}"
    );
    assert_eq!(active_generation_id(&active_path), active_before);
    assert_eq!(directory_count(&storage.join("runs")), 0);
    assert_eq!(directory_count(&workers_root), 0);
}

#[test]
fn http_server_plus_coordinator_stays_below_idle_rss_gate() {
    let repo = temp_repo();
    let state = repo.join(".codebaseGraph");
    let storage = state.join("storage");
    fs::create_dir_all(&state).unwrap();
    write_config_with_policy(&repo, &storage, "off", 768, 384, 32);
    let mut child = Command::new(env!("CARGO_BIN_EXE_codebase-graph"))
        .arg("mcp")
        .arg("http")
        .arg("--repo-root")
        .arg(&repo)
        .arg("--port")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let coordinator_path = storage.join("coordinator.json");
    let coordinator = wait_for_coordinator(&coordinator_path, None, Duration::from_secs(10));
    assert_eq!(coordinator["pid"], json!(child.id()));
    let mut samples = Vec::new();
    for _ in 0..20 {
        samples.push(sample_process_rss(child.id()).unwrap());
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
    let rss_p95 = percentile_95(samples);
    eprintln!("resource_gate http_coordinator_p95={rss_p95}");
    assert!(
        rss_p95 <= 64 * 1024 * 1024,
        "HTTP server plus coordinator RSS p95 exceeded 64 MiB"
    );
}

#[derive(Default)]
struct ClientGroup {
    clients: Vec<McpClient>,
}

impl ClientGroup {
    fn push(&mut self, client: McpClient) {
        self.clients.push(client);
    }

    fn kill_pid(&mut self, pid: u32) {
        let client = self
            .clients
            .iter_mut()
            .find(|client| client.pid() == pid)
            .expect("coordinator owner should be an MCP client");
        client.kill();
    }

    fn kill_all(&mut self) {
        for client in &mut self.clients {
            client.kill();
        }
    }
}

impl Drop for ClientGroup {
    fn drop(&mut self) {
        for client in &mut self.clients {
            client.kill();
        }
    }
}

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpClient {
    fn start(repo: &Path, index: usize) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codebase-graph"))
            .arg("mcp")
            .arg("start")
            .arg("--repo-root")
            .arg(repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("failed to start MCP client {index}: {error}"));
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": index + 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "coordinator-test", "version": "1"}
                }
            })
        )
        .unwrap();
        stdin.flush().unwrap();
        let mut response = String::new();
        stdout.read_line(&mut response).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response)
            .unwrap_or_else(|error| panic!("invalid MCP response for client {index}: {error}"));
        assert_eq!(response["id"], json!(index + 1));
        assert!(response.get("result").is_some());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn call(&mut self, id: usize, method: &str, params: serde_json::Value) -> serde_json::Value {
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .unwrap();
        self.stdin.flush().unwrap();
        let mut response = String::new();
        self.stdout.read_line(&mut response).unwrap();
        serde_json::from_str(&response).unwrap()
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn kill(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn wait_for_coordinator(
    path: &Path,
    previous_pid: Option<u32>,
    timeout: Duration,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(state) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                let pid = state["pid"].as_u64().map(|pid| pid as u32);
                if pid.is_some() && pid != previous_pid {
                    return state;
                }
            }
        }
        assert!(Instant::now() < deadline, "coordinator takeover timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn directory_count(path: &Path) -> usize {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .count()
        })
        .unwrap_or(0)
}

fn active_generation_id(path: &Path) -> String {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    value["generation_id"].as_str().unwrap().to_string()
}

fn wait_for_active_generation_change(path: &Path, previous: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        if path.is_file() {
            let generation = active_generation_id(path);
            if generation != previous {
                return generation;
            }
        }
        assert!(
            Instant::now() < deadline,
            "active generation did not change"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn active_worker_pid(path: &Path) -> Option<u32> {
    let bytes = fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value["worker_pid"].as_u64().map(|pid| pid as u32)
}

fn percentile_95(mut values: Vec<u64>) -> u64 {
    assert!(!values.is_empty());
    values.sort_unstable();
    let index = (values.len() * 95).div_ceil(100).saturating_sub(1);
    values[index]
}

#[cfg(target_os = "macos")]
fn sample_process_rss(pid: u32) -> std::io::Result<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: `info` is writable storage for the requested rusage structure.
    let status = unsafe {
        libc::proc_pid_rusage(
            i32::try_from(pid).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "PID exceeds i32")
            })?,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: proc_pid_rusage initialized the structure on success.
    Ok(unsafe { info.assume_init() }.ri_resident_size)
}

#[cfg(target_os = "linux")]
fn sample_process_rss(pid: u32) -> std::io::Result<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "VmRSS is missing"))?;
    kib.checked_mul(1024)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "VmRSS overflowed"))
}

#[cfg(windows)]
fn sample_process_rss(pid: u32) -> std::io::Result<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let mut counters = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS>::zeroed();
    let ok = unsafe {
        GetProcessMemoryInfo(
            handle,
            counters.as_mut_ptr(),
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { counters.assume_init() }.WorkingSetSize as u64)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn sample_process_rss(_pid: u32) -> std::io::Result<u64> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "RSS sampling is unsupported",
    ))
}

fn write_config(repo: &Path, storage: &Path, worker: u64, rust: u64, spill: u64) {
    write_config_with_policy(repo, storage, "leader", worker, rust, spill);
}

fn write_config_with_policy(
    repo: &Path,
    storage: &Path,
    policy: &str,
    worker: u64,
    rust: u64,
    spill: u64,
) {
    fs::write(
        repo.join(".codebaseGraph/config.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 3,
            "repo_root": repo,
            "storage_root": storage,
            "refresh": {"policy": policy, "backend": "auto"},
            "materialization": {
                "include_fts": true,
                "semantic_enrichment": false,
                "worker_memory_mib": worker,
                "rust_memory_mib": rust,
                "spill_chunk_mib": spill,
                "max_parallelism": 2
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn temp_repo() -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "codebase_graph_coordinator_process_{}_{}",
        std::process::id(),
        sequence
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
