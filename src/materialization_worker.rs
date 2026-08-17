use crate::api::context::{resolve_runtime, RepoRuntime, DEFAULT_WORKER_MEMORY_MIB};
use crate::api::{
    execute_candidate_materialization, execute_materialization, read_manifest, MaterializeOptions,
    RepoSelector,
};
use crate::db_writer::sample_process_rss;
use crate::error::{MemoryBudgetExceeded, NativeError};
use crate::hash::sha256_file;
use crate::protocol::{
    GraphSummary, ManifestDiff, NativeSyntaxMaterializationResponse, ProgressEvent,
};
use crate::storage::atomic::write_json_atomically;
use crate::storage::layout::{managed_generation_id, DirectLayout, ManagedLayout};
use crate::storage::locks::{try_open_locked, LockMode, WorkerLease};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

const WORKER_PROTOCOL_VERSION: u64 = 1;
const RSS_SAMPLE_INTERVAL: Duration = Duration::from_millis(25);
const MEMORY_HEADROOM_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PROGRESS_FRAME_BYTES: usize = 64 * 1024;
const MAX_CHILD_ERROR_BYTES: usize = 64 * 1024;
const SUPERVISOR_CHECK_INTERVAL: Duration = Duration::from_millis(50);
const ORPHAN_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const MIB: u64 = 1024 * 1024;
static WORKER_EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
static WORKER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize)]
struct MaterializationWorkerRequest {
    version: u64,
    build_id: String,
    options: MaterializeOptions,
    candidate_paths: Option<Vec<String>>,
    supervisor_pid: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct MaterializationWorkerResult {
    version: u64,
    build_id: String,
    outcome: Result<NativeSyntaxMaterializationResponse, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WorkerProgressFrame {
    version: u64,
    build_id: String,
    phase: String,
    current: usize,
    total: usize,
    path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct WorkerState {
    version: u64,
    build_id: String,
    worker_pid: u32,
}

struct SupervisorWatchdog {
    finished: Arc<AtomicBool>,
}

impl Drop for SupervisorWatchdog {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::Release);
    }
}

pub(crate) fn register_worker_executable(path: PathBuf) {
    let _ = WORKER_EXECUTABLE.set(path);
}

pub(crate) fn execute_refresh_worker(
    options: &MaterializeOptions,
    candidate_paths: Vec<String>,
    worker_pid: impl FnMut(Option<u32>),
) -> Result<NativeSyntaxMaterializationResponse, String> {
    execute_worker(options, Some(candidate_paths), worker_pid)
}

pub(crate) fn execute_explicit_worker(
    options: &MaterializeOptions,
    worker_pid: impl FnMut(Option<u32>),
) -> Result<NativeSyntaxMaterializationResponse, String> {
    execute_worker(options, None, worker_pid)
}

fn execute_worker(
    options: &MaterializeOptions,
    candidate_paths: Option<Vec<String>>,
    mut worker_pid: impl FnMut(Option<u32>),
) -> Result<NativeSyntaxMaterializationResponse, String> {
    let Some(executable) = WORKER_EXECUTABLE.get() else {
        return match candidate_paths {
            Some(candidate_paths) => execute_candidate_materialization(options, candidate_paths),
            None => execute_materialization(options),
        }
        .map(|(_, response)| response);
    };
    let _lease = acquire_worker_lease(options)?;
    recover_orphan_worker(options)?;
    let build_id = managed_generation_id();
    let workspace = create_worker_workspace(options, &build_id)?;
    let request_path = workspace.join("request.json");
    let result_path = workspace.join("result.json");
    let stderr_path = workspace.join("stderr.log");
    let start_path = workspace.join("START");
    let state_path = worker_state_path(options)?;
    let before_publication = publication_marker(options)?;
    if let Err(error) = write_worker_request(
        &request_path,
        &MaterializationWorkerRequest {
            version: WORKER_PROTOCOL_VERSION,
            build_id: build_id.clone(),
            options: options.clone(),
            candidate_paths,
            supervisor_pid: std::process::id(),
        },
    ) {
        cleanup_worker_workspace(&workspace);
        return Err(error);
    }

    let stderr = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)
    {
        Ok(stderr) => stderr,
        Err(error) => {
            cleanup_worker_workspace(&workspace);
            return Err(format!(
                "failed to create materialization worker stderr: {error}"
            ));
        }
    };
    let mut command = Command::new(executable);
    command
        .arg("__codebase_graph_internal")
        .arg("materialization-worker-v1")
        .arg(&request_path)
        .arg(&result_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr));
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            cleanup_worker_workspace(&workspace);
            return Err(format!("failed to start materialization worker: {error}"));
        }
    };
    let pid = child.id();
    if let Err(error) = write_json_atomically(
        &state_path,
        &WorkerState {
            version: WORKER_PROTOCOL_VERSION,
            build_id: build_id.clone(),
            worker_pid: pid,
        },
    ) {
        let _ = child.kill();
        let _ = child.wait();
        remove_worker_state(&state_path, &build_id);
        cleanup_worker_workspace(&workspace);
        return Err(format!(
            "failed to record materialization worker state: {error}"
        ));
    }
    if let Err(error) = fs::write(&start_path, b"ready\n") {
        let _ = child.kill();
        let _ = child.wait();
        remove_worker_state(&state_path, &build_id);
        cleanup_worker_workspace(&workspace);
        return Err(format!("failed to release materialization worker: {error}"));
    }
    worker_pid(Some(pid));
    let progress = child
        .stdout
        .take()
        .map(|stdout| thread::spawn(move || drain_progress(stdout)));
    let memory_limit = match options
        .worker_memory_mib
        .unwrap_or(DEFAULT_WORKER_MEMORY_MIB)
        .checked_mul(MIB)
    {
        Some(memory_limit) => memory_limit,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            worker_pid(None);
            remove_worker_state(&state_path, &build_id);
            cleanup_worker_workspace(&workspace);
            return Err("materialization worker memory limit overflowed".to_string());
        }
    };
    let kill_threshold = memory_limit.saturating_sub(MEMORY_HEADROOM_BYTES);
    let mut high_water_bytes = 0_u64;
    let mut budget_failure = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                worker_pid(None);
                remove_worker_state(&state_path, &build_id);
                cleanup_worker_workspace(&workspace);
                return Err(format!(
                    "failed to supervise materialization worker: {error}"
                ));
            }
        }
        if let Ok(rss) = sample_process_rss(pid) {
            high_water_bytes = high_water_bytes.max(rss);
            if rss > kill_threshold {
                let _ = child.kill();
                budget_failure = Some(
                    NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
                        "materialization_worker",
                        memory_limit,
                        rss,
                        rss,
                    ))
                    .to_string(),
                );
                match child.wait() {
                    Ok(status) => break status,
                    Err(error) => {
                        worker_pid(None);
                        remove_worker_state(&state_path, &build_id);
                        cleanup_worker_workspace(&workspace);
                        return Err(format!(
                            "failed to reap materialization worker after budget kill: {error}"
                        ));
                    }
                }
            }
        }
        thread::sleep(RSS_SAMPLE_INTERVAL);
    };
    worker_pid(None);
    if let Some(progress) = progress {
        let _ = progress.join();
    }

    let outcome = if result_path.exists() {
        read_worker_result(&result_path, &build_id).and_then(|result| result.outcome)
    } else if let Some(error) = budget_failure {
        reconcile_completed_publication(options, before_publication, &build_id).or(Err(error))
    } else if status.success() {
        reconcile_completed_publication(options, before_publication, &build_id)
    } else {
        let error = bounded_child_error(&fs::read(&stderr_path).unwrap_or_default());
        Err(format!(
            "materialization worker exited with {status}: {error}"
        ))
    };
    let outcome = outcome.map(|mut response| {
        response
            .phase_high_water_marks
            .insert("materialization_worker_rss".to_string(), high_water_bytes);
        response
    });
    remove_worker_state(&state_path, &build_id);
    cleanup_worker_workspace(&workspace);
    outcome
}

pub(crate) fn execute_worker_file(request_path: &Path, result_path: &Path) -> Result<(), String> {
    let request: MaterializationWorkerRequest = serde_json::from_reader(BufReader::new(
        File::open(request_path)
            .map_err(|error| format!("failed to open materialization worker request: {error}"))?,
    ))
    .map_err(|error| format!("failed to parse materialization worker request: {error}"))?;
    if request.version != WORKER_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported materialization worker protocol {}; expected {WORKER_PROTOCOL_VERSION}",
            request.version
        ));
    }
    let _watchdog = start_supervisor_watchdog(request.supervisor_pid);
    wait_for_start_gate(request_path)?;
    emit_progress(&WorkerProgressFrame {
        version: WORKER_PROTOCOL_VERSION,
        build_id: request.build_id.clone(),
        phase: "started".to_string(),
        current: 0,
        total: 1,
        path: None,
    })?;
    let refresh_result = request.candidate_paths.is_some();
    let mut outcome = match request.candidate_paths {
        Some(candidate_paths) => {
            execute_candidate_materialization(&request.options, candidate_paths)
        }
        None => execute_materialization(&request.options),
    }
    .map(|(_, response)| response);
    if let Ok(response) = &outcome {
        for event in &response.progress_events {
            emit_progress_event(&request.build_id, event)?;
        }
    }
    if refresh_result {
        if let Ok(response) = &mut outcome {
            compact_refresh_worker_result(response);
        }
    }
    write_json_atomically(
        result_path,
        &MaterializationWorkerResult {
            version: WORKER_PROTOCOL_VERSION,
            build_id: request.build_id.clone(),
            outcome,
        },
    )
    .map_err(|error| format!("failed to write materialization worker result: {error}"))?;
    emit_progress(&WorkerProgressFrame {
        version: WORKER_PROTOCOL_VERSION,
        build_id: request.build_id,
        phase: "completed".to_string(),
        current: 1,
        total: 1,
        path: None,
    })
}

fn acquire_worker_lease(options: &MaterializeOptions) -> Result<WorkerLease, String> {
    let lock_path = worker_control_paths(options)?.0;
    try_open_locked(&lock_path, LockMode::Exclusive)
        .map_err(|error| format!("failed to acquire materialization worker lock: {error}"))?
        .ok_or_else(|| "another materialization worker is already active".to_string())
}

fn worker_control_paths(options: &MaterializeOptions) -> Result<(PathBuf, PathBuf), String> {
    if let Some(storage_root) = options.storage_root.as_ref() {
        let layout = ManagedLayout::new(storage_root);
        return Ok((layout.worker_lock_path(), layout.worker_state_path()));
    }
    let db_path = options
        .db
        .as_ref()
        .ok_or_else(|| "materialization worker requires a database path".to_string())?;
    let manifest_path = options
        .manifest
        .as_ref()
        .ok_or_else(|| "materialization worker requires a manifest path".to_string())?;
    let layout = DirectLayout::new(db_path, manifest_path);
    Ok((layout.worker_lock_path(), layout.worker_state_path()))
}

fn worker_state_path(options: &MaterializeOptions) -> Result<PathBuf, String> {
    worker_control_paths(options).map(|(_, state)| state)
}

fn recover_orphan_worker(options: &MaterializeOptions) -> Result<(), String> {
    let state_path = worker_state_path(options)?;
    if state_path.exists() {
        let metadata = fs::symlink_metadata(&state_path)
            .map_err(|error| format!("failed to inspect materialization worker state: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "materialization worker state must be a real file: {}",
                state_path.display()
            ));
        }
        let state: WorkerState = serde_json::from_reader(BufReader::new(
            File::open(&state_path)
                .map_err(|error| format!("failed to open materialization worker state: {error}"))?,
        ))
        .map_err(|error| format!("failed to parse materialization worker state: {error}"))?;
        if state.version != WORKER_PROTOCOL_VERSION {
            return Err("materialization worker state version is unsupported".to_string());
        }
        let deadline = std::time::Instant::now() + ORPHAN_EXIT_TIMEOUT;
        while process_is_alive(state.worker_pid) && std::time::Instant::now() < deadline {
            thread::sleep(SUPERVISOR_CHECK_INTERVAL);
        }
        if process_is_alive(state.worker_pid) {
            return Err(format!(
                "orphaned materialization worker {} did not exit",
                state.worker_pid
            ));
        }
        remove_worker_state(&state_path, &state.build_id);
    }
    cleanup_abandoned_workspaces(options)
}

fn create_worker_workspace(
    options: &MaterializeOptions,
    build_id: &str,
) -> Result<PathBuf, String> {
    let root = worker_workspace_root(options)?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("failed to create materialization worker root: {error}"))?;
    let metadata = fs::symlink_metadata(&root)
        .map_err(|error| format!("failed to inspect materialization worker root: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "materialization worker root must be a real directory: {}",
            root.display()
        ));
    }
    let sequence = WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let workspace = root.join(format!(
        "worker-{build_id}-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&workspace)
        .map_err(|error| format!("failed to create materialization worker workspace: {error}"))?;
    Ok(workspace)
}

fn worker_workspace_root(options: &MaterializeOptions) -> Result<PathBuf, String> {
    if let Some(storage_root) = options.storage_root.as_ref() {
        return Ok(PathBuf::from(storage_root).join("workers"));
    }
    let db_path = options
        .db
        .as_ref()
        .ok_or_else(|| "materialization worker requires a database path".to_string())?;
    let manifest_path = options
        .manifest
        .as_ref()
        .ok_or_else(|| "materialization worker requires a manifest path".to_string())?;
    Ok(DirectLayout::new(db_path, manifest_path).worker_workspace_root_path())
}

fn cleanup_abandoned_workspaces(options: &MaterializeOptions) -> Result<(), String> {
    let root = worker_workspace_root(options)?;
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to inspect materialization worker workspaces: {error}"
            ))
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("worker-"))
        {
            cleanup_worker_workspace(&path);
        }
    }
    Ok(())
}

fn wait_for_start_gate(request_path: &Path) -> Result<(), String> {
    let start_path = request_path
        .parent()
        .ok_or_else(|| "materialization worker request has no workspace".to_string())?
        .join("START");
    let deadline = std::time::Instant::now() + ORPHAN_EXIT_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if start_path.is_file() {
            return Ok(());
        }
        thread::sleep(SUPERVISOR_CHECK_INTERVAL);
    }
    Err("materialization worker start gate timed out".to_string())
}

fn start_supervisor_watchdog(supervisor_pid: u32) -> SupervisorWatchdog {
    let finished = Arc::new(AtomicBool::new(false));
    let pipe_finished = Arc::clone(&finished);
    thread::spawn(move || {
        let mut byte = [0_u8; 1];
        let read = std::io::stdin().read(&mut byte);
        if !pipe_finished.load(Ordering::Acquire) && !matches!(read, Ok(1)) {
            std::process::exit(125);
        }
    });
    let thread_finished = Arc::clone(&finished);
    thread::spawn(move || {
        while !thread_finished.load(Ordering::Acquire) {
            if !process_is_alive(supervisor_pid) {
                std::process::exit(125);
            }
            thread::sleep(SUPERVISOR_CHECK_INTERVAL);
        }
    });
    SupervisorWatchdog { finished }
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs existence and permission checking without
    // delivering a signal to the target process.
    let status = unsafe { libc::kill(pid, 0) };
    status == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the returned handle is checked and closed before returning.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    // SAFETY: `handle` is valid and has not been closed yet.
    unsafe { CloseHandle(handle) };
    true
}

#[cfg(not(any(unix, windows)))]
fn process_is_alive(_pid: u32) -> bool {
    true
}

fn write_worker_request(path: &Path, request: &MaterializationWorkerRequest) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create materialization worker request: {error}"))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, request)
        .map_err(|error| format!("failed to encode materialization worker request: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("failed to flush materialization worker request: {error}"))
}

fn remove_worker_state(path: &Path, build_id: &str) {
    let owned = File::open(path)
        .ok()
        .and_then(|file| serde_json::from_reader::<_, WorkerState>(BufReader::new(file)).ok())
        .is_some_and(|state| state.build_id == build_id);
    if owned {
        let _ = fs::remove_file(path);
    }
}

fn read_worker_result(path: &Path, build_id: &str) -> Result<MaterializationWorkerResult, String> {
    let result: MaterializationWorkerResult =
        serde_json::from_reader(BufReader::new(File::open(path).map_err(|error| {
            format!("failed to open materialization worker result: {error}")
        })?))
        .map_err(|error| format!("failed to parse materialization worker result: {error}"))?;
    if result.version != WORKER_PROTOCOL_VERSION || result.build_id != build_id {
        return Err("materialization worker result identity is invalid".to_string());
    }
    Ok(result)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ProgressDrainStats {
    valid_frames: usize,
    oversized_frames: usize,
}

fn drain_progress(stdout: impl std::io::Read) -> ProgressDrainStats {
    let mut reader = BufReader::with_capacity(8 * 1024, stdout);
    let mut frame = Vec::new();
    let _ = frame.try_reserve(4 * 1024);
    let mut oversized = false;
    let mut stats = ProgressDrainStats::default();
    loop {
        let available = match reader.fill_buf() {
            Ok([]) | Err(_) => return stats,
            Ok(available) => available,
        };
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let payload = &available[..newline.unwrap_or(available.len())];
        if !oversized {
            let required = frame.len().saturating_add(payload.len());
            if required > MAX_PROGRESS_FRAME_BYTES
                || frame
                    .try_reserve(required.saturating_sub(frame.len()))
                    .is_err()
            {
                frame.clear();
                oversized = true;
            } else {
                frame.extend_from_slice(payload);
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            if oversized {
                stats.oversized_frames += 1;
            } else if serde_json::from_slice::<WorkerProgressFrame>(&frame).is_ok() {
                stats.valid_frames += 1;
            }
            frame.clear();
            oversized = false;
        }
    }
}

fn compact_refresh_worker_result(response: &mut NativeSyntaxMaterializationResponse) {
    response.snapshots.clear();
    response.rebuilt_entries.clear();
    response.materialized_entries.clear();
    response.copy_statements.clear();
    response.progress_events.clear();
    response.diagnostics.clear();
    response.diff.unchanged.clear();
}

fn emit_progress_event(build_id: &str, event: &ProgressEvent) -> Result<(), String> {
    emit_progress(&WorkerProgressFrame {
        version: WORKER_PROTOCOL_VERSION,
        build_id: build_id.to_string(),
        phase: event.phase.clone(),
        current: event.current,
        total: event.total,
        path: event.path.clone(),
    })
}

fn emit_progress(frame: &WorkerProgressFrame) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, frame)
        .map_err(|error| format!("failed to encode materialization worker progress: {error}"))?;
    stdout
        .write_all(b"\n")
        .and_then(|_| stdout.flush())
        .map_err(|error| format!("failed to write materialization worker progress: {error}"))
}

fn publication_marker(options: &MaterializeOptions) -> Result<Option<String>, String> {
    let path = if let Some(storage_root) = options.storage_root.as_ref() {
        ManagedLayout::new(storage_root).active_pointer_path()
    } else {
        options
            .manifest
            .clone()
            .ok_or_else(|| "materialization worker requires a manifest path".to_string())?
    };
    match sha256_file(&path) {
        Ok(hash) => Ok(Some(hash)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to hash publication marker {}: {error}",
            path.display()
        )),
    }
}

fn reconcile_completed_publication(
    options: &MaterializeOptions,
    before: Option<String>,
    build_id: &str,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    let runtime = reconcile_runtime(options)?;
    let after = publication_marker_from_runtime(&runtime)?;
    if before == after {
        return Err(
            "materialization worker exited successfully without a result or publication".into(),
        );
    }
    let manifest = read_manifest(&runtime.manifest_path)?;
    let graph_summary = GraphSummary {
        node_count: manifest.files.values().map(|entry| entry.node_count).sum(),
        edge_count: manifest.files.values().map(|entry| entry.edge_count).sum(),
    };
    let mut response = NativeSyntaxMaterializationResponse::skipped(
        BTreeMap::new(),
        ManifestDiff {
            added: Vec::new(),
            modified: Vec::new(),
            unchanged: manifest.files.keys().cloned().collect(),
            deleted: Vec::new(),
            force_rebuild: false,
        },
        vec![format!(
            "reconciled completed materialization worker build {build_id} after result loss"
        )],
        Vec::new(),
        BTreeMap::new(),
    );
    response.skipped = false;
    response.database_written = true;
    response.storage_format = if runtime.storage_root.is_some() {
        "managed_v2".to_string()
    } else {
        "direct".to_string()
    };
    response.active_generation = runtime
        .active_read
        .as_ref()
        .map(|read| read.generation_id.clone());
    response.graph_summary = graph_summary;
    response.search_backend = manifest.search_backend;
    Ok(response)
}

fn reconcile_runtime(options: &MaterializeOptions) -> Result<RepoRuntime, String> {
    let managed = options.storage_root.is_some();
    resolve_runtime(&RepoSelector {
        repo_root: options.source_root.clone(),
        config_path: options.config.clone(),
        db_path: if managed { None } else { options.db.clone() },
        manifest_path: if managed {
            None
        } else {
            options.manifest.clone()
        },
    })
}

fn publication_marker_from_runtime(runtime: &RepoRuntime) -> Result<Option<String>, String> {
    let path = runtime
        .storage_root
        .as_ref()
        .map(|root| ManagedLayout::new(root).active_pointer_path())
        .unwrap_or_else(|| runtime.manifest_path.clone());
    match sha256_file(&path) {
        Ok(hash) => Ok(Some(hash)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to hash reconciled publication marker {}: {error}",
            path.display()
        )),
    }
}

fn bounded_child_error(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(MAX_CHILD_ERROR_BYTES);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
}

fn cleanup_worker_workspace(path: &Path) {
    let is_owned_workspace = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("worker-"));
    if is_owned_workspace
        && fs::symlink_metadata(path)
            .ok()
            .is_some_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
    {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_request_round_trips_without_semantic_state() {
        let request = MaterializationWorkerRequest {
            version: WORKER_PROTOCOL_VERSION,
            build_id: "build-1".to_string(),
            options: MaterializeOptions::default(),
            candidate_paths: Some(vec!["src/lib.rs".to_string()]),
            supervisor_pid: std::process::id(),
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        let decoded: MaterializationWorkerRequest = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.version, WORKER_PROTOCOL_VERSION);
        assert_eq!(decoded.build_id, "build-1");
        assert_eq!(
            decoded.candidate_paths,
            Some(vec!["src/lib.rs".to_string()])
        );
        assert!(!decoded.options.semantic_enrichment);
    }

    #[test]
    fn explicit_worker_request_has_no_refresh_candidate_override() {
        let request = MaterializationWorkerRequest {
            version: WORKER_PROTOCOL_VERSION,
            build_id: "build-explicit".to_string(),
            options: MaterializeOptions::default(),
            candidate_paths: None,
            supervisor_pid: std::process::id(),
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        let decoded: MaterializationWorkerRequest = serde_json::from_slice(&encoded).unwrap();
        assert!(decoded.candidate_paths.is_none());
    }

    #[test]
    fn worker_lock_allows_only_one_active_owner() {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-worker-lock-{}-{}",
            std::process::id(),
            WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let options = MaterializeOptions {
            db: Some(root.join("graph.ldb")),
            manifest: Some(root.join("manifest.json")),
            ..MaterializeOptions::default()
        };
        let first = acquire_worker_lease(&options).unwrap();
        assert!(acquire_worker_lease(&options).is_err());
        drop(first);
        assert!(acquire_worker_lease(&options).is_ok());
    }

    #[test]
    fn memory_failure_is_structured() {
        let error = NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
            "materialization_worker",
            768 * MIB,
            769 * MIB,
            769 * MIB,
        ));
        let value: serde_json::Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(value["error"], serde_json::json!("memory_budget_exceeded"));
        assert_eq!(value["phase"], serde_json::json!("materialization_worker"));
    }

    #[test]
    fn progress_drain_discards_oversized_frames_and_continues_bounded() {
        let valid = serde_json::to_vec(&WorkerProgressFrame {
            version: WORKER_PROTOCOL_VERSION,
            build_id: "build-progress".to_string(),
            phase: "complete".to_string(),
            current: 1,
            total: 1,
            path: None,
        })
        .unwrap();
        let mut input = vec![b'x'; MAX_PROGRESS_FRAME_BYTES + 1];
        input.push(b'\n');
        input.extend(valid);
        input.push(b'\n');
        assert_eq!(
            drain_progress(std::io::Cursor::new(input)),
            ProgressDrainStats {
                valid_frames: 1,
                oversized_frames: 1,
            }
        );
    }
}
