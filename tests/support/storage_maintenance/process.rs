//! Fixture-owned interruption driver over the real native phase barrier.
//! No numeric PID is used to terminate a process or authorize storage cleanup.
//! Failed/unresolved fixtures retain their directories and real recovery proof.

use super::{observe_small_tree, workload, FixtureRoot, ObservedEntry};
use codebase_graph::protocol::NativeSyntaxMaterializationResponse;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct FaultFixture {
    root: Option<FixtureRoot>,
    control: Option<FixtureRoot>,
    external_before: BTreeMap<PathBuf, ObservedEntry>,
    recovered: bool,
}

impl FaultFixture {
    pub fn new() -> Result<Self, String> {
        let root = FixtureRoot::new("phase-fault").map_err(|error| error.to_string())?;
        root.write_revision(0).map_err(|error| error.to_string())?;
        let control = FixtureRoot::new("phase-control").map_err(|error| error.to_string())?;
        let external = control.path().join("external");
        fs::create_dir(&external).map_err(|error| error.to_string())?;
        fs::write(
            external.join("sentinel.bin"),
            b"external fixture data must survive\0\xff",
        )
        .map_err(|error| error.to_string())?;
        let external_before = observe_small_tree(&external).map_err(|error| error.to_string())?;
        Ok(Self {
            root: Some(root),
            control: Some(control),
            external_before,
            recovered: false,
        })
    }

    pub fn root(&self) -> &FixtureRoot {
        self.root.as_ref().expect("owned fixture root")
    }

    pub fn start_paused_build(&self) -> Result<OwnedBuild<'_>, String> {
        let active = workload::health(self.root())?;
        let active_root = fs::canonicalize(
            Path::new(
                active["database_path"]
                    .as_str()
                    .ok_or("active graph has no database path")?,
            )
            .parent()
            .ok_or("active database has no parent")?,
        )
        .map_err(|error| error.to_string())?;
        let control = self.control.as_ref().expect("owned fixture controls");
        let barrier = control.path().join("barrier");
        fs::create_dir(&barrier).map_err(|error| error.to_string())?;
        let stdout = control.path().join("owner.stdout");
        let stderr = control.path().join("owner.stderr");
        let binary = std::env::var_os("CARGO_BIN_EXE_codebase-graph")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")));
        let child = Command::new(binary)
            .current_dir(self.root().path())
            .args([
                "build",
                "--mode",
                "full",
                "--no-git",
                "--single-thread",
                "--max-parallelism",
                "1",
                "--json",
                "--repo-root",
            ])
            .arg(self.root().path())
            .env("CODEBASE_GRAPH_TEST_PHASE_BARRIER_DIR", &barrier)
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                fs::File::create(&stdout).map_err(|error| error.to_string())?,
            ))
            .stderr(Stdio::from(
                fs::File::create(&stderr).map_err(|error| error.to_string())?,
            ))
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(OwnedBuild {
            child: Some(child),
            root: self.root(),
            active_root,
            barrier,
            stdout,
            stderr,
        })
    }

    /// Only read/runtime recovery entries are repeated here. The subsequent
    /// materialization is issued once, after actual containment is proven empty.
    /// Optional request disposal may wait for that next authorized writer.
    pub fn wait_for_recovery(&self, phase: &PhaseReady, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last = String::new();
        while Instant::now() < deadline {
            match workload::health(self.root()) {
                Ok(health) => {
                    if health["graph_readable"] == true
                        && health["pending_runs"] == 0
                        && phase.observation() == ProcessObservation::Gone
                        && phase.closed_empty_containment()
                    {
                        return Ok(());
                    }
                    last = format!(
                        "readable={} pending_runs={} phase={:?} request_present={} closed_empty={}",
                        health["graph_readable"],
                        health["pending_runs"],
                        phase.observation(),
                        phase.request_path.exists(),
                        phase.closed_empty_containment()
                    );
                }
                Err(error) => last = error,
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(format!("fixture phase recovery did not finish: {last}"))
    }

    pub fn assert_external_unchanged(&self) {
        let external = self.control.as_ref().unwrap().path().join("external");
        assert_eq!(observe_small_tree(&external).unwrap(), self.external_before);
    }

    pub fn finish_recovery(
        &mut self,
        phase: &PhaseReady,
        response: &NativeSyntaxMaterializationResponse,
    ) -> Result<(), String> {
        if !response.database_written
            || response.pending_runs != 0
            || phase.observation() != ProcessObservation::Gone
            || phase.request_path.exists()
        {
            return Err("fixture lacks a completed publication and quiescent failed phase".into());
        }
        self.assert_external_unchanged();
        self.recovered = true;
        Ok(())
    }
}

impl Drop for FaultFixture {
    fn drop(&mut self) {
        if !self.recovered {
            // Retain owned evidence on failure rather than delete a possibly
            // live candidate. No fixture destructor guesses process authority.
            for root in [&mut self.root, &mut self.control] {
                if let Some(root) = root.take() {
                    eprintln!(
                        "retained unresolved qualification fixture: {}",
                        root.path().display()
                    );
                    std::mem::forget(root);
                }
            }
        }
    }
}

pub struct OwnedBuild<'a> {
    child: Option<Child>,
    root: &'a FixtureRoot,
    active_root: PathBuf,
    barrier: PathBuf,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl OwnedBuild<'_> {
    pub fn wait_for_native_phase(&mut self, timeout: Duration) -> Result<PhaseReady, String> {
        let deadline = Instant::now() + timeout;
        let root = fs::canonicalize(self.root.path()).map_err(|error| error.to_string())?;
        while Instant::now() < deadline {
            if let Some(status) = self
                .child
                .as_mut()
                .expect("owned owner")
                .try_wait()
                .map_err(|error| error.to_string())?
            {
                return Err(format!(
                    "build ended before interruption barrier ({status}): {}",
                    self.diagnostics()
                ));
            }
            for entry in fs::read_dir(&self.barrier).map_err(|error| error.to_string())? {
                let path = entry.map_err(|error| error.to_string())?.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err("invalid phase readiness entry".into());
                }
                let ready: Value =
                    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
                        .map_err(|error| error.to_string())?;
                let pid = ready["pid"]
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|pid| *pid > 0)
                    .ok_or("phase readiness has invalid PID")?;
                #[cfg(unix)]
                if pid > i32::MAX as u32 {
                    return Err("phase readiness PID is outside native range".into());
                }
                let request_path = fs::canonicalize(
                    ready["request_path"]
                        .as_str()
                        .ok_or("phase readiness has no request path")?,
                )
                .map_err(|error| error.to_string())?;
                if !request_path.starts_with(&root) {
                    return Err("phase request escaped fixture".into());
                }
                let request_bytes = fs::read(&request_path).map_err(|error| error.to_string())?;
                let request: Value =
                    serde_json::from_slice(&request_bytes).map_err(|error| error.to_string())?;
                let candidate = Path::new(
                    request["db_path"]
                        .as_str()
                        .ok_or("phase request has no database path")?,
                )
                .parent()
                .ok_or("phase database has no parent")?;
                let candidate = fs::canonicalize(candidate).map_err(|error| error.to_string())?;
                if !candidate.starts_with(&root) {
                    return Err("phase candidate escaped fixture".into());
                }
                if candidate == self.active_root {
                    return Err("phase barrier targets the committed generation instead of an independent candidate".into());
                }
                let phase = PhaseReady {
                    pid,
                    request_path,
                    request_bytes,
                    candidate,
                };
                if phase.observation() != ProcessObservation::Running {
                    return Err("phase did not remain at its real barrier".into());
                }
                return Ok(phase);
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(format!(
            "native phase did not reach its real barrier: {}",
            self.diagnostics()
        ))
    }

    pub fn interrupt_owner(&mut self) -> Result<ExitStatus, String> {
        let child = self
            .child
            .as_mut()
            .ok_or("owned owner was already reaped")?;
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!(
                "owner ended before requested interruption: {status}"
            ));
        }
        // Keep the real Child handle in self until wait succeeds, so every
        // error path still runs the fixture's kill-and-reap destructor.
        child.kill().map_err(|error| error.to_string())?;
        let status = child.wait().map_err(|error| error.to_string())?;
        self.child.take();
        Ok(status)
    }

    fn diagnostics(&self) -> String {
        fn bounded(path: &Path) -> String {
            use std::io::Read;
            let mut output = String::new();
            if let Ok(file) = fs::File::open(path) {
                let _ = file.take(64 * 1024).read_to_string(&mut output);
            }
            output
        }
        format!(
            "stdout={} stderr={}",
            bounded(&self.stdout),
            bounded(&self.stderr)
        )
    }
}

impl Drop for OwnedBuild<'_> {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

pub struct PhaseReady {
    pid: u32,
    request_path: PathBuf,
    request_bytes: Vec<u8>,
    candidate: PathBuf,
}

impl PhaseReady {
    pub fn assert_candidate_present(&self) {
        assert!(self.candidate.is_dir());
        assert!(self.request_path.is_file());
    }

    fn closed_empty_containment(&self) -> bool {
        use fs2::FileExt;
        if !self.request_path.exists() {
            return true;
        }
        if !fs::read(&self.request_path).is_ok_and(|bytes| bytes == self.request_bytes) {
            return false;
        }
        let Some(root) = self.request_path.parent() else {
            return false;
        };
        let Ok(entries) = fs::read_dir(root) else {
            return false;
        };
        let mut leases = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else { return false };
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".lifecycle-")
            {
                continue;
            }
            let Ok(metadata) = entry.file_type() else {
                return false;
            };
            if !metadata.is_dir() || metadata.is_symlink() {
                return false;
            }
            let path = entry.path();
            let Ok(lease) = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path.join("lease"))
            else {
                return false;
            };
            if lease.try_lock_exclusive().is_err() {
                return false;
            }
            let retired =
                fs::read(path.join("retired")).is_ok_and(|bytes| bytes == b"retired-v2\n");
            let receipt = fs::read(path.join("receipt.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|receipt| receipt["containment_empty"] == true);
            if !retired && !receipt {
                return false;
            }
            leases.push(lease);
        }
        !leases.is_empty()
    }

    fn observation(&self) -> ProcessObservation {
        observe_process(self.pid)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ProcessObservation {
    Running,
    Gone,
    Unknown,
}

#[cfg(unix)]
fn observe_process(pid: u32) -> ProcessObservation {
    // Signal zero is an observation only. It is never a cleanup permit.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return ProcessObservation::Running;
    }
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        ProcessObservation::Gone
    } else {
        ProcessObservation::Unknown
    }
}

#[cfg(windows)]
fn observe_process(pid: u32) -> ProcessObservation {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            ProcessObservation::Gone
        } else {
            ProcessObservation::Unknown
        };
    }
    let status = unsafe { WaitForSingleObject(handle, 0) };
    unsafe {
        CloseHandle(handle);
    }
    match status {
        WAIT_OBJECT_0 => ProcessObservation::Gone,
        WAIT_TIMEOUT => ProcessObservation::Running,
        _ => ProcessObservation::Unknown,
    }
}

#[cfg(not(any(unix, windows)))]
fn observe_process(_pid: u32) -> ProcessObservation {
    ProcessObservation::Unknown
}

pub struct ReapedMcp {
    status: ExitStatus,
}

pub struct OwnedMcp<'a> {
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    responses: std::sync::mpsc::Receiver<Result<Value, String>>,
    reader: Option<std::thread::JoinHandle<()>>,
    next_id: u64,
    stderr: PathBuf,
    _root: &'a FixtureRoot,
}

impl FaultFixture {
    /// Public setup supplies the actual configuration. Resetting ONLY its closed
    /// synthetic managed tree makes the next actual worker create a new root;
    /// no FreshRoot/registration/receipt token is assembled by the fixture.
    pub fn public_health(&self) -> Result<Value, String> {
        workload::health(self.root())
    }

    pub fn reset_closed_managed_storage(&self) -> Result<(), String> {
        workload::install(self.root())?;
        let health = workload::health(self.root())?;
        let storage = fs::canonicalize(
            health["storage_root"]
                .as_str()
                .ok_or("fixture has no managed storage root")?,
        )
        .map_err(|error| error.to_string())?;
        let root = fs::canonicalize(self.root().path()).map_err(|error| error.to_string())?;
        if !storage.starts_with(&root) || storage == root {
            return Err("synthetic storage reset escaped fixture".into());
        }
        fs::remove_dir_all(&storage).map_err(|error| error.to_string())?;
        if storage.exists() {
            return Err(
                "synthetic managed storage remains before isolated-worker first creation".into(),
            );
        }
        Ok(())
    }

    pub fn start_mcp(&self) -> Result<OwnedMcp<'_>, String> {
        use std::io::{BufRead, BufReader};
        let stderr = self
            .control
            .as_ref()
            .unwrap()
            .path()
            .join("mcp-owner.stderr");
        let binary = std::env::var_os("CARGO_BIN_EXE_codebase-graph")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")));
        let mut child = Command::new(binary)
            .args(["mcp", "start", "--repo-root"])
            .arg(self.root().path())
            .args(["--max-parallelism", "1", "--spill-chunk-mib", "1"])
            .current_dir(self.root().path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(
                fs::File::create(&stderr).map_err(|error| error.to_string())?,
            ))
            .spawn()
            .map_err(|error| error.to_string())?;
        let stdin = child.stdin.take().ok_or("MCP stdin missing")?;
        let stdout = child.stdout.take().ok_or("MCP stdout missing")?;
        let (sender, responses) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let response = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|error| {
                        format!("invalid actual MCP JSON response: {error}: {line}")
                    })
                });
                if sender.send(response).is_err() {
                    break;
                }
            }
        });
        let mut mcp = OwnedMcp {
            child: Some(child),
            stdin: Some(stdin),
            responses,
            reader: Some(reader),
            next_id: 1,
            stderr,
            _root: self.root(),
        };
        let response = mcp.request("initialize", serde_json::json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"dc-s5-isolated-worker","version":"1"}}))?;
        if !response["result"].is_object() {
            return Err(format!("MCP initialization failed: {response}"));
        }
        mcp.notify("notifications/initialized", serde_json::json!({}))?;
        Ok(mcp)
    }

    pub fn finish_worker_fixture(
        &mut self,
        reaped: ReapedMcp,
        response: &NativeSyntaxMaterializationResponse,
    ) -> Result<(), String> {
        if !reaped.status.success() || !response.database_written {
            return Err("isolated fixture lacks a reaped successful owner and successful later real publication".into());
        }
        self.assert_external_unchanged();
        self.recovered = true;
        Ok(())
    }
}

impl OwnedMcp<'_> {
    fn write(&mut self, value: &Value) -> Result<(), String> {
        use std::io::Write;
        let input = self.stdin.as_mut().ok_or("MCP input closed")?;
        serde_json::to_writer(&mut *input, value).map_err(|error| error.to_string())?;
        input
            .write_all(b"\n")
            .and_then(|_| input.flush())
            .map_err(|error| error.to_string())
    }
    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.write(&serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            let response = self
                .responses
                .recv_timeout(Duration::from_secs(30))
                .map_err(|error| {
                    format!(
                        "actual MCP response unavailable: {error}; {}",
                        fs::read_to_string(&self.stderr).unwrap_or_default()
                    )
                })??;
            if response.get("id").is_none() {
                continue;
            }
            if response["id"] != id {
                return Err(format!("MCP response identity mismatch: {response}"));
            }
            return Ok(response);
        }
    }
    pub fn wait_for_revision(
        &mut self,
        revision: usize,
        timeout: Duration,
    ) -> Result<Value, String> {
        let deadline = Instant::now() + timeout;
        let mut last = Value::Null;
        while Instant::now() < deadline {
            let health = self.request("tools/call", serde_json::json!({"name":"graph_health","arguments":{"include_structured_content":true}}))?;
            if health["result"]["isError"] == true {
                if health["result"]["structuredContent"]["error"]["retryable"] == true {
                    last = health;
                    thread::sleep(Duration::from_millis(25));
                    continue;
                }
                return Err(format!("nonretryable MCP health failure: {health}"));
            }
            if health["result"]["structuredContent"]["graph_readable"] == true {
                let query = self.request("tools/call", serde_json::json!({"name":"graph_query","arguments":{"statement":"MATCH (n:Function) RETURN n.label ORDER BY n.label","limit":100,"include_structured_content":true}}))?;
                if query["result"]["isError"] == true {
                    if query["result"]["structuredContent"]["error"]["retryable"] != true {
                        return Err(format!("nonretryable MCP graph query failure: {query}"));
                    }
                } else {
                    let data = &query["result"]["structuredContent"];
                    let mut expected =
                        vec![format!("revision_{revision}"), "stable_helper".to_owned()];
                    expected.sort();
                    let rows = data["rows"]
                        .as_array()
                        .ok_or("MCP graph query rows missing")?;
                    let actual = rows
                        .iter()
                        .map(|row| {
                            row[0]
                                .as_str()
                                .map(str::to_owned)
                                .ok_or("MCP function label missing")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if data["truncated"] == false && actual == expected {
                        let worker = health["result"]["structuredContent"]["refresh"]
                            ["phase_high_water_marks"]["materialization_worker_rss"]
                            .as_u64();
                        if worker.is_some_and(|bytes| bytes > 0) {
                            let fresh = self.request("tools/call", serde_json::json!({"name":"graph_health","arguments":{"include_structured_content":true}}))?;
                            if fresh["result"]["isError"] == false
                                && fresh["result"]["structuredContent"]["graph_readable"] == true
                                && fresh["result"]["structuredContent"]["active_generation"]
                                    == health["result"]["structuredContent"]["active_generation"]
                            {
                                return Ok(fresh["result"]["structuredContent"].clone());
                            }
                        }
                    }
                }
                last = query;
            } else {
                last = health;
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(format!("actual MCP isolated refresh never reached revision{revision} with worker telemetry: {last}"))
    }
    pub fn close(mut self) -> Result<ReapedMcp, String> {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let child = self.child.as_mut().unwrap();
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                self.child.take();
                let reader_deadline = Instant::now() + Duration::from_secs(2);
                while self
                    .reader
                    .as_ref()
                    .is_some_and(|reader| !reader.is_finished())
                {
                    if Instant::now() >= reader_deadline {
                        return Err(
                            "reaped MCP owner left its stdout reader live; fixture retained".into(),
                        );
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                if let Some(reader) = self.reader.take() {
                    reader.join().map_err(|_| "MCP stdout reader panicked")?;
                }
                if !status.success() {
                    return Err(format!(
                        "MCP owner failed during graceful EOF: {status}; {}",
                        fs::read_to_string(&self.stderr).unwrap_or_default()
                    ));
                }
                return Ok(ReapedMcp { status });
            }
            if Instant::now() >= deadline {
                return Err("MCP owner did not join after graceful EOF".into());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for OwnedMcp<'_> {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if let Some(child) = self.child.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            if reader.is_finished() {
                let _ = reader.join();
            } else {
                eprintln!("unresolved MCP stdout reader detached; fixture roots remain retained");
            }
        }
        // FaultFixture preserves roots unless a normal EOF/reap token and later
        // actual successful publication mark recovery. No descendant PID kill.
    }
}

pub struct CoveredEpochObservation {
    storage: PathBuf,
    epoch: Value,
    use_header: Vec<u8>,
    publication_binding: Vec<u8>,
}
impl CoveredEpochObservation {
    pub fn observe(root: &FixtureRoot, health: &Value) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        let storage = fs::canonicalize(
            health["storage_root"]
                .as_str()
                .ok_or("MCP health storage root missing")?,
        )
        .map_err(|error| error.to_string())?;
        if !storage.starts_with(fs::canonicalize(root.path()).map_err(|error| error.to_string())?) {
            return Err("MCP storage escaped fixture".into());
        }
        let artifacts = storage.join("artifacts");
        let serial = fs::File::open(artifacts.join(".reference-epoch.lock"))
            .map_err(|error| error.to_string())?;
        let lock_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match fs2::FileExt::try_lock_shared(&serial) {
                Ok(()) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < lock_deadline =>
                {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => {
                    return Err(format!(
                        "cannot observe actual serialized reference epoch: {error}"
                    ))
                }
            }
        }
        let epoch_bytes =
            fs::read(artifacts.join(".reference-epoch")).map_err(|error| error.to_string())?;
        if epoch_bytes.len() != 2048
            || &epoch_bytes[..8] != b"CGRFE001"
            || epoch_bytes[2016..] != Sha256::digest(&epoch_bytes[..2016])[..]
        {
            return Err("actual epoch envelope invalid".into());
        }
        let length = u32::from_le_bytes(epoch_bytes[8..12].try_into().unwrap()) as usize;
        if length > 2004 {
            return Err("actual epoch payload exceeds frame".into());
        }
        let epoch: Value = serde_json::from_slice(&epoch_bytes[12..12 + length])
            .map_err(|error| error.to_string())?;
        if epoch["version"] != 2 || epoch["covered"] != true {
            return Err(format!(
                "actual worker did not retain covered reference epoch: {epoch}"
            ));
        }
        let use_header =
            fs::read(artifacts.join(".use.lock")).map_err(|error| error.to_string())?;
        if use_header.len() != 72
            || &use_header[..8] != b"CGCOOR02"
            || use_header[40..] != Sha256::digest(&use_header[..40])[..]
        {
            return Err("actual coordination creation header invalid".into());
        }
        if epoch["use_incarnation"] != serde_json::json!(use_header[8..40]) {
            return Err("covered epoch uses another actual creation nonce".into());
        }
        let publication_binding =
            fs::read(artifacts.join(".publication.binding")).map_err(|error| error.to_string())?;
        if publication_binding.is_empty() {
            return Err("actual root publication binding empty".into());
        }
        let manifest: codebase_graph::protocol::NativeManifest = serde_json::from_slice(
            &fs::read(
                health["manifest_path"]
                    .as_str()
                    .ok_or("actual manifest path missing")?,
            )
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        for entry in manifest.files.values() {
            let key = entry
                .artifact_key
                .as_deref()
                .ok_or("actual producer entry lacks artifact key")?;
            if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("producer key malformed".into());
            }
            let directory = artifacts.join(&key[..2]).join(key);
            let origin =
                fs::read(directory.join(".entry-origin")).map_err(|error| error.to_string())?;
            if origin.len() != 2048 {
                return Err(format!(
                    "actual isolated producer lacks full Published receipt at {}",
                    directory.display()
                ));
            }
            for (phase, frame) in origin.chunks_exact(512).enumerate() {
                if &frame[..8] != b"CGORIG01"
                    || frame[24] != phase as u8
                    || frame[480..] != Sha256::digest(&frame[..480])[..]
                {
                    return Err("actual producer receipt incomplete or corrupt".into());
                }
            }
            let last = &origin[1536..];
            let actual_key = last[64..96]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let payload =
                fs::read(directory.join("partition.json")).map_err(|error| error.to_string())?;
            if actual_key != key
                || payload.len() as u64 != u64::from_le_bytes(last[256..264].try_into().unwrap())
                || last[264..296] != Sha256::digest(&payload)[..]
            {
                return Err("actual producer receipt does not bind its accepted payload".into());
            }
        }
        Ok(Self {
            storage,
            epoch,
            use_header,
            publication_binding,
        })
    }
    pub fn assert_same_birth_and_advanced_epoch(&self, previous: &Self) {
        assert_eq!(self.storage, previous.storage);
        assert_eq!(
            self.use_header, previous.use_header,
            "followup reenrolled existing root"
        );
        assert_eq!(self.publication_binding, previous.publication_binding);
        assert_eq!(self.epoch["root"], previous.epoch["root"]);
        assert_eq!(self.epoch["incarnation"], previous.epoch["incarnation"]);
        assert!(
            self.epoch["counter"].as_u64().unwrap() > previous.epoch["counter"].as_u64().unwrap(),
            "followup failed to advance covered reference E"
        );
    }
}
