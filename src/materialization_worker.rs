use crate::api::context::{resolve_runtime, RepoRuntime, DEFAULT_WORKER_MEMORY_MIB};
use crate::api::{
    execute_candidate_materialization, execute_materialization, read_manifest, MaterializeOptions,
    RepoSelector,
};
use crate::hash::sha256_file;
use crate::process_lifecycle::{
    self, BuildDescriptorV2, ExitOutcome, OutputMode, PreparedChild, RecoveryOutcome, SpawnSpec,
};
use crate::protocol::{
    GraphSummary, ManifestDiff, NativeSyntaxMaterializationResponse, ProgressEvent,
};
use crate::storage::atomic::write_json_atomically;
use crate::storage::layout::{managed_generation_id, DirectLayout, ManagedLayout};
use crate::storage::locks::{try_open_locked, LockMode, WorkerLease};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const WORKER_PROTOCOL_VERSION: u64 = 2;
const WORKER_STATE_VERSION: u64 = 2;
const MAX_PROGRESS_FRAME_BYTES: usize = 64 * 1024;
const MAX_CHILD_ERROR_BYTES: usize = 64 * 1024;
const SUPERVISOR_CHECK_INTERVAL: Duration = Duration::from_millis(50);
const MIB: u64 = 1024 * 1024;
const MATERIALIZATION_CANCELLED_ERROR: &str = r#"{"error":"materialization_cancelled"}"#;
static WORKER_EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
static WORKER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize)]
struct MaterializationWorkerRequest {
    version: u64,
    build_id: String,
    options: MaterializeOptions,
    candidate_paths: Option<Vec<String>>,
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

pub(crate) fn register_worker_executable(path: PathBuf) {
    let _ = WORKER_EXECUTABLE.set(path);
}

pub(crate) fn execute_refresh_worker_cancellable(
    options: &MaterializeOptions,
    candidate_paths: Vec<String>,
    worker_pid: impl FnMut(Option<u32>),
    should_cancel: impl Fn() -> bool,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    execute_worker(
        options,
        Some(candidate_paths),
        worker_pid,
        should_cancel,
        true,
    )
}

pub(crate) fn execute_explicit_worker(
    options: &MaterializeOptions,
    worker_pid: impl FnMut(Option<u32>),
) -> Result<NativeSyntaxMaterializationResponse, String> {
    execute_worker(options, None, worker_pid, || false, false)
}

fn execute_worker(
    options: &MaterializeOptions,
    candidate_paths: Option<Vec<String>>,
    mut worker_pid: impl FnMut(Option<u32>),
    should_cancel: impl Fn() -> bool,
    wait_for_lock: bool,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    if should_cancel() {
        return Err(materialization_cancelled_error());
    }
    let Some(executable) = WORKER_EXECUTABLE.get() else {
        // The in-process fallback has no child supervisor and therefore cannot
        // interrupt synchronous materialization after dispatch. The pre-dispatch
        // cancellation check above is the strongest guarantee available here;
        // production MCP refreshes register an isolated worker executable.
        return match candidate_paths {
            Some(candidate_paths) => execute_candidate_materialization(options, candidate_paths),
            None => execute_materialization(options),
        }
        .map(|(_, response)| response);
    };
    let _lease = if wait_for_lock {
        acquire_worker_lease_cancellable(options, &should_cancel)?
    } else {
        acquire_worker_lease(options)?
    };
    if should_cancel() {
        return Err(materialization_cancelled_error());
    }
    recover_orphan_worker(options, &should_cancel)?;
    if should_cancel() {
        return Err(materialization_cancelled_error());
    }
    let build_id = managed_generation_id();
    let workspace = create_worker_workspace(options, &build_id)?;
    if should_cancel() {
        cleanup_worker_workspace(&workspace);
        return Err(materialization_cancelled_error());
    }
    let request_path = workspace.join("request.json");
    let result_path = workspace.join("result.json");
    let stderr_path = workspace.join("stderr.log");
    let state_path = worker_state_path(options)?;
    let before_publication = publication_marker(options)?;
    if let Err(error) = write_worker_request(
        &request_path,
        &MaterializationWorkerRequest {
            version: WORKER_PROTOCOL_VERSION,
            build_id: build_id.clone(),
            options: options.clone(),
            candidate_paths,
        },
    ) {
        cleanup_worker_workspace(&workspace);
        return Err(error);
    }

    let memory_limit = options
        .worker_memory_mib
        .unwrap_or(DEFAULT_WORKER_MEMORY_MIB)
        .checked_mul(MIB)
        .ok_or_else(|| {
            cleanup_worker_workspace(&workspace);
            "materialization worker memory limit overflowed".to_string()
        })?;
    let mut child = match PreparedChild::prepare(SpawnSpec {
        program: executable.clone(),
        guardian_executable: Some(executable.clone()),
        args: vec![
            "__codebase_graph_internal".into(),
            "materialization-worker-v2".into(),
            request_path.into_os_string(),
            result_path.clone().into_os_string(),
        ],
        workspace: workspace.clone(),
        stderr_path: stderr_path.clone(),
        stdout: OutputMode::Pipe,
        memory_limit_bytes: Some(memory_limit),
        memory_phase: Some("materialization_worker".into()),
        parent_rss_pid: None,
    }) {
        Ok(child) => child,
        Err(error) => {
            // A failed READY may still have a guardian or contained descendants.
            if process_lifecycle::cleanup_if_inactive(&workspace).unwrap_or(false) {
                cleanup_worker_workspace(&workspace);
                return Err(error);
            }
            return Err(containment_unknown_error(error));
        }
    };
    let descriptor = child.descriptor().clone();
    let pid = child.id();
    let mut progress = child.take_stdout().map(|stdout| {
        let progress_build_id = build_id.clone();
        thread::spawn(move || drain_progress(stdout, &progress_build_id))
    });
    // Persist the capability and native proof before allowing any build work.
    if let Err(error) = process_lifecycle::write_private_json(&state_path, &descriptor) {
        match child.terminate("state persistence failed") {
            Ok(outcome) if outcome.containment_empty => {
                join_progress(progress.take());
                cleanup_worker_workspace(&workspace);
                return Err(format!("failed to record materialization worker state: {error}"));
            }
            _ => return Err(containment_unknown_error(format!(
                "failed to record materialization worker state and could not prove teardown: {error}"
            ))),
        }
    }
    let started = debug_worker_start_barrier(&should_cancel).and_then(|()| {
        if should_cancel() {
            child.terminate("cancel")
        } else {
            child.start().and_then(|()| {
                worker_pid(Some(pid));
                supervise_worker_process(&mut child, &should_cancel)
            })
        }
    });
    let supervision = match started {
        Ok(outcome) => outcome,
        Err(error) => {
            // A failed control exchange is ambiguous: retain evidence unless a
            // subsequent authenticated teardown proves the containment empty.
            match child.terminate("supervision failed") {
                Ok(outcome) if outcome.containment_empty => {
                    join_progress(progress.take());
                    clear_worker_execution_state(
                        &state_path,
                        &descriptor.build_id,
                        &workspace,
                        &mut worker_pid,
                    );
                    return Err(format!(
                        "failed to supervise materialization worker: {error}"
                    ));
                }
                _ => {
                    worker_pid(None);
                    return Err(containment_unknown_error(error));
                }
            }
        }
    };
    worker_pid(None);
    if !supervision.containment_empty {
        return Err(containment_unknown_error(supervision.to_string()));
    }
    join_progress(progress);
    if supervision.cancelled {
        clear_worker_execution_state(
            &state_path,
            &descriptor.build_id,
            &workspace,
            &mut worker_pid,
        );
        return Err(materialization_cancelled_error());
    }
    let outcome = if result_path.exists() {
        read_worker_result(&result_path, &build_id).and_then(|result| result.outcome)
    } else if let Some(error) = &supervision.budget_reason {
        reconcile_completed_publication(options, before_publication, &build_id)
            .or(Err(error.clone()))
    } else if supervision.success {
        reconcile_completed_publication(options, before_publication, &build_id)
    } else {
        let error = bounded_child_error(&fs::read(&stderr_path).unwrap_or_default());
        Err(format!(
            "materialization worker exited with {supervision}: {error}"
        ))
    };
    let outcome = outcome.map(|mut response| {
        response.phase_high_water_marks.insert(
            "materialization_worker_rss".to_string(),
            supervision.high_water_bytes,
        );
        response
    });
    remove_worker_state(&state_path, &descriptor.build_id);
    cleanup_worker_workspace(&workspace);
    outcome
}

/// Deterministic fault boundary after durable recovery state and before START.
/// The release binary deliberately ignores all test barrier configuration.
fn debug_worker_start_barrier(should_cancel: &impl Fn() -> bool) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if let Some(directory) = std::env::var_os("CODEBASE_GRAPH_TEST_WORKER_START_BARRIER_DIR") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("failed to create worker START barrier: {error}"))?;
        write_json_atomically(
            &directory.join("state-persisted.json"),
            &serde_json::json!({"owner_pid": std::process::id()}),
        )
        .map_err(|error| format!("failed to publish worker START barrier: {error}"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if should_cancel() || directory.join("CONTINUE").is_file() {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err("materialization worker START barrier timed out".into());
            }
            thread::sleep(SUPERVISOR_CHECK_INTERVAL);
        }
    }
    let _ = should_cancel;
    Ok(())
}

fn materialization_cancelled_error() -> String {
    MATERIALIZATION_CANCELLED_ERROR.to_string()
}

fn containment_unknown_error(reason: impl ToString) -> String {
    serde_json::json!({"error": "containment_unknown", "message": reason.to_string()}).to_string()
}

fn supervise_worker_process(
    child: &mut PreparedChild,
    should_cancel: &impl Fn() -> bool,
) -> Result<ExitOutcome, String> {
    loop {
        if should_cancel() {
            return child.terminate("cancel");
        }
        if let Some(outcome) = child.try_wait()? {
            return Ok(outcome);
        }
        thread::sleep(SUPERVISOR_CHECK_INTERVAL);
    }
}

fn join_progress(progress: Option<thread::JoinHandle<ProgressDrainStats>>) {
    if let Some(progress) = progress {
        let _ = progress.join();
    }
}

fn clear_worker_execution_state(
    state_path: &Path,
    build_id: &str,
    workspace: &Path,
    worker_pid: &mut impl FnMut(Option<u32>),
) {
    worker_pid(None);
    remove_worker_state(state_path, build_id);
    cleanup_worker_workspace(workspace);
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
    process_lifecycle::validate_inherited_containment()?;
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

fn acquire_worker_lease_cancellable(
    options: &MaterializeOptions,
    should_cancel: &impl Fn() -> bool,
) -> Result<WorkerLease, String> {
    let lock_path = worker_control_paths(options)?.0;
    loop {
        if should_cancel() {
            return Err(materialization_cancelled_error());
        }
        match try_open_locked(&lock_path, LockMode::Exclusive)
            .map_err(|error| format!("failed to acquire materialization worker lock: {error}"))?
        {
            Some(lease) => {
                if should_cancel() {
                    drop(lease);
                    return Err(materialization_cancelled_error());
                }
                return Ok(lease);
            }
            None => thread::sleep(SUPERVISOR_CHECK_INTERVAL),
        }
    }
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

fn recover_orphan_worker(
    options: &MaterializeOptions,
    should_cancel: &impl Fn() -> bool,
) -> Result<(), String> {
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
        let value: serde_json::Value = serde_json::from_reader(BufReader::new(
            File::open(&state_path).map_err(containment_unknown_error)?,
        ))
        .map_err(containment_unknown_error)?;
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(1) => {
                // Legacy PIDs have no incarnation proof and convey no authority.
                let state: WorkerState =
                    serde_json::from_value(value).map_err(containment_unknown_error)?;
                remove_worker_state(&state_path, &state.build_id);
            }
            Some(WORKER_STATE_VERSION) => {
                let descriptor: BuildDescriptorV2 =
                    serde_json::from_value(value).map_err(containment_unknown_error)?;
                match process_lifecycle::recover_descriptor(&descriptor)
                    .map_err(containment_unknown_error)?
                {
                    RecoveryOutcome::Empty => {}
                    RecoveryOutcome::Terminated(outcome) if outcome.containment_empty => {}
                    RecoveryOutcome::Terminated(outcome) => {
                        return Err(containment_unknown_error(outcome))
                    }
                    RecoveryOutcome::ContainmentUnknown(reason) => {
                        return Err(containment_unknown_error(reason))
                    }
                }
                remove_worker_state(&state_path, &descriptor.build_id);
            }
            _ => {
                return Err(containment_unknown_error(
                    "unsupported or missing worker state version",
                ))
            }
        }
    }
    if should_cancel() {
        return Err(materialization_cancelled_error());
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
    let mut unknown = None;
    for entry in entries {
        let entry = entry.map_err(containment_unknown_error)?;
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("worker-"))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(containment_unknown_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            unknown.get_or_insert_with(|| {
                format!(
                    "worker workspace must be a real directory: {}",
                    path.display()
                )
            });
            continue;
        }
        match process_lifecycle::cleanup_if_inactive(&path) {
            Ok(true) => cleanup_worker_workspace(&path),
            Ok(false) => {
                unknown.get_or_insert_with(|| {
                    format!(
                        "abandoned worker containment is still active or unproven: {}",
                        path.display()
                    )
                });
            }
            Err(error) => {
                unknown.get_or_insert(error);
            }
        }
    }
    match unknown {
        Some(reason) => Err(containment_unknown_error(reason)),
        None => Ok(()),
    }
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

fn drain_progress(stdout: impl std::io::Read, build_id: &str) -> ProgressDrainStats {
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
            } else if serde_json::from_slice::<WorkerProgressFrame>(&frame).is_ok_and(|frame| {
                frame.version == WORKER_PROTOCOL_VERSION && frame.build_id == build_id
            }) {
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
        && process_lifecycle::cleanup_if_inactive(path).unwrap_or(false)
    {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{MemoryBudgetExceeded, NativeError};

    #[test]
    fn worker_request_ignores_retired_semantic_keys() {
        let request = MaterializationWorkerRequest {
            version: WORKER_PROTOCOL_VERSION,
            build_id: "build-1".to_string(),
            options: MaterializeOptions::default(),
            candidate_paths: Some(vec!["src/lib.rs".to_string()]),
        };
        let mut encoded = serde_json::to_value(&request).unwrap();
        let options = encoded["options"].as_object_mut().unwrap();
        options.insert("semantic_enrichment".to_string(), serde_json::json!(true));
        options.insert(
            "semantic_provider_mode".to_string(),
            serde_json::json!("remote"),
        );
        let decoded: MaterializationWorkerRequest = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.version, WORKER_PROTOCOL_VERSION);
        assert_eq!(decoded.build_id, "build-1");
        assert_eq!(
            decoded.candidate_paths,
            Some(vec!["src/lib.rs".to_string()])
        );
        let encoded = serde_json::to_value(decoded).unwrap();
        assert!(encoded["options"].get("semantic_enrichment").is_none());
        assert!(encoded["options"].get("semantic_provider_mode").is_none());
    }

    #[test]
    fn explicit_worker_request_has_no_refresh_candidate_override() {
        let request = MaterializationWorkerRequest {
            version: WORKER_PROTOCOL_VERSION,
            build_id: "build-explicit".to_string(),
            options: MaterializeOptions::default(),
            candidate_paths: None,
        };
        let encoded = serde_json::to_vec(&request).unwrap();
        let decoded: MaterializationWorkerRequest = serde_json::from_slice(&encoded).unwrap();
        assert!(decoded.candidate_paths.is_none());
        assert!(serde_json::to_value(&decoded)
            .unwrap()
            .get("supervisor_pid")
            .is_none());
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
    fn cancellable_worker_lock_wait_stops_when_cancelled() {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-worker-lock-cancel-{}-{}",
            std::process::id(),
            WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let options = MaterializeOptions {
            db: Some(root.join("graph.ldb")),
            manifest: Some(root.join("manifest.json")),
            ..MaterializeOptions::default()
        };
        let held = acquire_worker_lease(&options).unwrap();
        let polls = AtomicU64::new(0);
        let should_cancel = || polls.fetch_add(1, Ordering::SeqCst) >= 1;
        let started = std::time::Instant::now();
        let result = acquire_worker_lease_cancellable(&options, &should_cancel);
        assert!(matches!(
            result,
            Err(error) if error == materialization_cancelled_error()
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(held);
        let _ = fs::remove_dir_all(root);
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
            drain_progress(std::io::Cursor::new(input), "build-progress"),
            ProgressDrainStats {
                valid_frames: 1,
                oversized_frames: 1,
            }
        );
    }

    #[test]
    fn legacy_pid_state_is_retired_without_waiting_for_process() {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-legacy-{}-{}",
            std::process::id(),
            WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let options = MaterializeOptions {
            storage_root: Some(root.clone()),
            ..MaterializeOptions::default()
        };
        let state_path = worker_state_path(&options).unwrap();
        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        write_json_atomically(
            &state_path,
            &WorkerState {
                version: 1,
                build_id: "legacy".into(),
                worker_pid: std::process::id(),
            },
        )
        .unwrap();
        recover_orphan_worker(&options, &|| false).unwrap();
        assert!(!state_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn abandoned_workspace_without_global_state_retains_held_lifecycle() {
        use fs2::FileExt;
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-held-lifecycle-{}-{}",
            std::process::id(),
            WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let options = MaterializeOptions {
            storage_root: Some(root.clone()),
            ..MaterializeOptions::default()
        };
        let workspace = worker_workspace_root(&options)
            .unwrap()
            .join("worker-pre-ready");
        let lifecycle = workspace.join(".lifecycle-pre-ready");
        fs::create_dir_all(&lifecycle).unwrap();
        let lease = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(lifecycle.join("lease"))
            .unwrap();
        lease.lock_exclusive().unwrap();
        let error = recover_orphan_worker(&options, &|| false).unwrap_err();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&error).unwrap()["error"],
            "containment_unknown"
        );
        assert!(workspace.exists());
        drop(lease);
        recover_orphan_worker(&options, &|| false).unwrap();
        assert!(!workspace.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v2_retirement_matches_descriptor_build_identity() {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-descriptor-state-{}-{}",
            std::process::id(),
            WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let state_path = root.join("worker.json");
        let descriptor = BuildDescriptorV2 {
            version: 2,
            build_id: "guardian-identity".into(),
            lifecycle_dir: root.join(".lifecycle-test"),
            endpoint: "127.0.0.1:1".into(),
            nonce: "test-only".into(),
            worker_pid: 123,
            guardian_pid: 124,
            proof: process_lifecycle::ContainmentProof::Pending,
        };
        write_json_atomically(&state_path, &descriptor).unwrap();
        remove_worker_state(&state_path, "caller-result-identity");
        assert!(state_path.exists());
        remove_worker_state(&state_path, &descriptor.build_id);
        assert!(!state_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_v2_state_is_preserved_as_unknown() {
        let root = std::env::temp_dir().join(format!(
            "codebase-graph-unknown-{}-{}",
            std::process::id(),
            WORKER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let options = MaterializeOptions {
            storage_root: Some(root.clone()),
            ..MaterializeOptions::default()
        };
        let state_path = worker_state_path(&options).unwrap();
        fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        write_json_atomically(
            &state_path,
            &WorkerState {
                version: 2,
                build_id: "unknown".into(),
                worker_pid: std::process::id(),
            },
        )
        .unwrap();
        let error = recover_orphan_worker(&options, &|| false).unwrap_err();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&error).unwrap()["error"],
            "containment_unknown"
        );
        assert!(state_path.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
