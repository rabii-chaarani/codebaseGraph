use serde_json::json;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")))
}

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
    // Keep the periodic reconciliation fallback inside the generation-change
    // deadline when auto backend selection falls back to polling.
    write_config_with_policy_and_interval(&repo, &storage, "leader", 768, 384, 32, Some(1_000));

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
    // Startup reconciliation must complete before changing a source file, and
    // the service must report the monitoring backend it will use afterward.
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
            "refresh startup readiness and metrics were not published: {health}"
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
    // Preserve the exact preexisting incarnations from deliberate owner death;
    // that process could not publish a terminal event after being killed.
    let mut preexisting = std::collections::BTreeMap::new();
    for category in ["runs", "workers"] {
        for entry in fs::read_dir(storage.join(category)).unwrap() {
            let path = entry.unwrap().path();
            if let Ok(identity) = physical_directory_identity(&path) {
                preexisting.insert(path, identity);
            }
        }
    }
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
    // Auto refresh can retry after failure. Observe a real quiescent owner,
    // then verify retained registered roots are terminal rather than live;
    // their directory count is no longer synchronous disposal evidence.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let health = clients.clients[20].call(
            20_001,
            "tools/call",
            json!({
                "name": "graph_health", "arguments": {"include_structured_content": true}
            }),
        );
        match registered_work_is_terminal_at_quiescence(&storage, &preexisting) {
            Ok(retained_owner_loss) => {
                if !retained_owner_loss.is_empty() {
                    assert_eq!(
                        health["result"]["structuredContent"]["maintenance"]["pending"], true,
                        "retained owner-loss evidence was reported idle"
                    );
                    for path in retained_owner_loss {
                        assert_eq!(
                            physical_directory_identity(&path).unwrap(),
                            preexisting[&path]
                        );
                    }
                }
                break;
            }
            Err(reason) => assert!(
                Instant::now() < deadline,
                "failed refresh did not reach closed owner state: {reason}; health={health}"
            ),
        }
    }
    assert_eq!(active_generation_id(&active_path), active_before);
}

#[test]
fn http_server_plus_coordinator_stays_below_idle_rss_gate() {
    let repo = temp_repo();
    let state = repo.join(".codebaseGraph");
    let storage = state.join("storage");
    fs::create_dir_all(&state).unwrap();
    write_config_with_policy(&repo, &storage, "off", 768, 384, 32);
    let mut child = Command::new(binary())
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
        let mut child = Command::new(binary())
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

fn physical_directory_identity(path: &Path) -> Result<(u64, u64), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("workspace is not a real directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        };
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|error| error.to_string())?;
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut information) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok((
            u64::from(information.dwVolumeSerialNumber),
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("physical workspace identity unavailable".into())
    }
}

/// Observation only: real worker/writer/owner leases exclude concurrent births
/// and mutations while test assertions inspect committed terminal control facts.
/// These bytes are never used to authorize cleanup or adopt unknown payloads.
fn registered_work_is_terminal_at_quiescence(
    storage: &Path,
    preexisting: &std::collections::BTreeMap<PathBuf, (u64, u64)>,
) -> Result<Vec<PathBuf>, String> {
    use sha2::{Digest, Sha256};
    let lock = |path: &Path| -> Result<fs::File, String> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        fs2::FileExt::try_lock_exclusive(&file)
            .map_err(|error| format!("busy {}: {error}", path.display()))?;
        Ok(file)
    };
    let _worker = lock(&storage.join("worker.lock"))?;
    let _writer = lock(&storage.join("writer.lock"))?;
    if active_worker_pid(&storage.join("worker.json")).is_some() {
        return Err("actual worker execution state remains live".into());
    }
    let number = |bytes: &[u8]| u64::from_le_bytes(bytes.try_into().unwrap());
    let mut retained_owner_loss = Vec::new();
    for category in ["runs", "workers"] {
        let entries = fs::read_dir(storage.join(category)).map_err(|error| error.to_string())?;
        for entry in entries {
            let root = entry.map_err(|error| error.to_string())?.path();
            let _owner = lock(&root.join("lease.lock"))?;
            let read = |name: &str| fs::read(root.join(name)).map_err(|error| error.to_string());
            let scope = read(".workspace.binding")?;
            if scope.len() != 512
                || &scope[..8] != b"CGWSCP01"
                || Sha256::digest(&scope[..480]).as_slice() != &scope[480..]
            {
                return Err(format!("unverified registration {}", root.display()));
            }
            let selector = read(".publication.selector")?;
            if selector.len() != 2392
                || &selector[..8] != b"CGCOMM01"
                || selector[8..40] != scope[128..160]
                || Sha256::digest(&selector[..96]).as_slice() != &selector[96..128]
            {
                return Err(format!("uncommitted control {}", root.display()));
            }
            let intent = &selector[128..];
            if &intent[..8] == b"CGINTN01"
                && intent[8..40] == scope[128..160]
                && Sha256::digest(&intent[..2232]).as_slice() == &intent[2232..]
                && number(&intent[48..56]) > number(&selector[40..48])
            {
                return Err(format!(
                    "control recovery remains pending {}",
                    root.display()
                ));
            }
            let active = number(&selector[48..56]);
            if active > 1 {
                return Err("invalid active control slot".into());
            }
            let slot = read(&format!(".publication.data{active}"))?;
            if slot.len() != 2136
                || &slot[..8] != b"CGSLOT01"
                || slot[8..40] != scope[128..160]
                || slot[40..48] != selector[40..48]
                || number(&slot[48..56]) != 456
                || Sha256::digest(&slot[..2104]).as_slice() != &slot[2104..]
                || slot[2104..] != selector[56..88]
            {
                return Err(format!("unverified terminal payload {}", root.display()));
            }
            let control = &slot[56..512];
            if &control[..8] == b"CGWCTL01"
                && control[8] == 0
                && control[9] == 0
                && category == "workers"
                && preexisting.get(&root) == Some(&physical_directory_identity(&root)?)
            {
                // An earlier deliberately killed owner cannot produce terminal
                // authority. Its actual unlocked lease + empty-tree
                // receipt prove inactivity; preserve its Building evidence.
                let mut leases = Vec::new();
                for entry in fs::read_dir(&root).map_err(|error| error.to_string())? {
                    let path = entry.map_err(|error| error.to_string())?.path();
                    if !path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".lifecycle-")
                    {
                        continue;
                    }
                    let lease = lock(&path.join("lease"))?;
                    let receipt: serde_json::Value = serde_json::from_slice(
                        &fs::read(path.join("receipt.json")).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                    if receipt["containment_empty"] != true {
                        return Err(format!(
                            "preexisting owner-loss containment unproven {}",
                            root.display()
                        ));
                    }
                    leases.push(lease);
                }
                if leases.is_empty() {
                    return Err("owner-loss root lacks actual empty-tree proof".into());
                }
                retained_owner_loss.push(root);
                continue;
            }
            if &control[..8] != b"CGWCTL01"
                || !matches!(control[8], 4 | 5)
                || !matches!(control[9], 1 | 2)
                || control[40..48] != control[48..56]
                || control[16..24] != control[56..64]
            {
                return Err(format!(
                    "nonterminal registered {} phase={}",
                    root.display(),
                    control[8]
                ));
            }
        }
    }
    Ok(retained_owner_loss)
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
    write_config_with_policy_and_interval(repo, storage, policy, worker, rust, spill, None);
}

fn write_config_with_policy_and_interval(
    repo: &Path,
    storage: &Path,
    policy: &str,
    worker: u64,
    rust: u64,
    spill: u64,
    reconcile_interval_ms: Option<u64>,
) {
    let mut refresh = json!({"policy": policy, "backend": "auto"});
    if let Some(interval_ms) = reconcile_interval_ms {
        refresh["reconcile_interval_ms"] = json!(interval_ms);
    }
    fs::write(
        repo.join(".codebaseGraph/config.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 3,
            "repo_root": repo,
            "storage_root": storage,
            "refresh": refresh,
            "materialization": {
                "include_fts": true,
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
