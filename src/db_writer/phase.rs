use super::rss::sample_process_rss;
use super::{connect_ladybug_database, open_ladybug_database_with_limits};
use crate::error::{MemoryBudgetExceeded, NativeError};
use crate::process_lifecycle::{self, OutputMode, PreparedChild, SpawnSpec};
use lbug::Connection;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const PHASE_PROTOCOL_VERSION: u64 = 2;
const MAX_CHILD_ERROR_BYTES: usize = 64 * 1024;
const MEMORY_HEADROOM_BYTES: u64 = 16 * 1024 * 1024;
const RSS_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
static PHASE_WORKER_EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
static PHASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static PHASE_WORKSPACE_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

// Synchronous API execution owns this scope. Keep it local to the calling
// thread, restore nested scopes, and restore on unwind as well as normal exit.
struct PhaseWorkspaceScope(Option<PathBuf>);

impl Drop for PhaseWorkspaceScope {
    fn drop(&mut self) {
        PHASE_WORKSPACE_ROOT.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

pub(crate) fn with_phase_workspace_root<T>(root: PathBuf, execute: impl FnOnce() -> T) -> T {
    let previous = PHASE_WORKSPACE_ROOT.with(|slot| slot.replace(Some(root)));
    let _scope = PhaseWorkspaceScope(previous);
    execute()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LadybugWritePhaseRequest {
    version: u64,
    db_path: PathBuf,
    worker_memory_bytes: u64,
    buffer_pool_bytes: u64,
    max_num_threads: u64,
    phase: LadybugWritePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum LadybugWritePhase {
    Schema {
        defer_hash_indexes: bool,
        statements: Vec<String>,
    },
    Copy {
        index: usize,
        total: usize,
        statement: String,
        runtime_loads: Vec<String>,
    },
    Index {
        table: String,
    },
    PostCopy {
        statement: String,
        runtime_loads: Vec<String>,
    },
}

impl LadybugWritePhaseRequest {
    pub(super) fn new(
        db_path: impl Into<PathBuf>,
        worker_memory_bytes: u64,
        buffer_pool_bytes: u64,
        max_num_threads: u64,
        phase: LadybugWritePhase,
    ) -> Self {
        Self {
            version: PHASE_PROTOCOL_VERSION,
            db_path: db_path.into(),
            worker_memory_bytes,
            buffer_pool_bytes,
            max_num_threads,
            phase,
        }
    }
}

pub(crate) fn register_phase_worker_executable(path: PathBuf) {
    let _ = PHASE_WORKER_EXECUTABLE.set(path);
}

pub(super) fn phase_worker_available() -> bool {
    PHASE_WORKER_EXECUTABLE.get().is_some()
}

pub(super) fn run_isolated_phase(request: &LadybugWritePhaseRequest) -> Result<u64, NativeError> {
    let _database_use =
        crate::storage::maintenance_ownership::DatabaseUseLease::acquire_shared(&request.db_path)?;
    let executable = PHASE_WORKER_EXECUTABLE.get().ok_or_else(|| {
        NativeError::InvalidInput("Ladybug phase worker executable is not registered".to_string())
    })?;
    // The private guardian may launch from its own workspace. Preserve the
    // caller's database target and request identity across that boundary.
    let mut absolute_request = request.clone();
    if absolute_request.db_path.is_relative() {
        absolute_request.db_path = std::env::current_dir()?.join(&absolute_request.db_path);
    }
    let request = &absolute_request;
    let inherited = process_lifecycle::inherited_containment().map_err(NativeError::Database)?;
    let registry_root = if inherited {
        None
    } else {
        PHASE_WORKSPACE_ROOT
            .with(|slot| slot.borrow().clone())
            .map(|root| {
                if root.is_absolute() {
                    Ok(root)
                } else {
                    std::env::current_dir().map(|cwd| cwd.join(root))
                }
            })
            .transpose()?
    };
    let request_path = write_phase_request(request, registry_root.as_deref())?;
    let mut command = traced_phase_command(executable, request, &request_path)?;
    command
        .arg("__codebase_graph_internal")
        .arg("ladybug-write-phase-v2")
        .arg(&request_path);
    let stderr_path = request_path.with_extension("stderr");
    if !inherited {
        let workspace = if registry_root.is_some() {
            request_path
                .parent()
                .expect("phase request has a parent")
                .to_path_buf()
        } else {
            // Lower-level pipeline callers without storage ownership retain
            // their local fallback; API callers always supply a stable root.
            request_path.with_extension("containment")
        };
        return run_guarded_phase(
            request,
            executable,
            command,
            &request_path,
            &stderr_path,
            workspace,
        );
    }
    // Preserve the root guardian's proof writer through both the native child
    // and the optional /usr/bin/time wrapper.
    process_lifecycle::configure_inherited_command(&mut command).map_err(NativeError::Database)?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)?;
    command.stdout(Stdio::null()).stderr(Stdio::from(stderr));
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            cleanup_phase_files(&request_path, &stderr_path);
            return Err(NativeError::Database(format!(
                "failed to start Ladybug write phase: {error}"
            )));
        }
    };
    let mut high_water_bytes = 0_u64;
    let kill_threshold = request
        .worker_memory_bytes
        .saturating_sub(MEMORY_HEADROOM_BYTES);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                // The wrapper may have a surviving native child. The outer
                // build must return this error so its guardian cleans the tree.
                return Err(NativeError::Database(format!(
                    "failed to supervise Ladybug phase: {error}; retain {} until build containment is empty",
                    request_path.display()
                )));
            }
        }
        let child_rss = sample_process_rss(child.id()).unwrap_or(0);
        let parent_rss = sample_process_rss(std::process::id()).unwrap_or(0);
        let observed = child_rss.saturating_add(parent_rss);
        high_water_bytes = high_water_bytes.max(observed);
        if observed > kill_threshold {
            let _ = child.kill();
            let _ = child.wait();
            // Retain artifacts until the root guardian proves the whole build
            // empty; killing a timing wrapper alone cannot prove that.
            return Err(NativeError::MemoryBudgetExceeded(
                MemoryBudgetExceeded::new(
                    phase_memory_label(request),
                    request.worker_memory_bytes,
                    observed,
                    observed,
                )
                .with_process_rss(parent_rss, child_rss),
            ));
        }
        std::thread::sleep(RSS_SAMPLE_INTERVAL);
    };
    let stderr = fs::read(&stderr_path).unwrap_or_default();
    if !status.success() {
        let stderr = bounded_child_error(&stderr);
        return Err(NativeError::Database(format!(
            "isolated Ladybug write phase exited with {}: {stderr}",
            status
        )));
    }
    cleanup_phase_files(&request_path, &stderr_path);
    Ok(high_water_bytes)
}

fn phase_memory_label(request: &LadybugWritePhaseRequest) -> String {
    format!(
        "ladybug_{}_pool_{}_mib",
        request.phase.trace_label(),
        request.buffer_pool_bytes / (1024 * 1024)
    )
}

fn run_guarded_phase(
    request: &LadybugWritePhaseRequest,
    executable: &Path,
    command: Command,
    request_path: &Path,
    stderr_path: &Path,
    workspace: PathBuf,
) -> Result<u64, NativeError> {
    let spec = SpawnSpec {
        guardian_executable: Some(executable.to_path_buf()),
        program: PathBuf::from(command.get_program()),
        args: command.get_args().map(|arg| arg.to_os_string()).collect(),
        workspace: workspace.clone(),
        stderr_path: stderr_path.to_path_buf(),
        stdout: OutputMode::Null,
        memory_limit_bytes: Some(request.worker_memory_bytes),
        memory_phase: Some(phase_memory_label(request)),
        parent_rss_pid: Some(std::process::id()),
    };
    let mut child = PreparedChild::prepare(spec).map_err(NativeError::Database)?;
    if let Err(error) = child.start() {
        let cleanup = child.terminate("phase-start-failed");
        if cleanup
            .as_ref()
            .is_ok_and(|outcome| outcome.containment_empty)
        {
            cleanup_phase_files(request_path, stderr_path);
            let _ = fs::remove_dir_all(&workspace);
        }
        return Err(NativeError::Database(format!(
            "failed to start guarded Ladybug phase: {error}; cleanup: {cleanup:?}"
        )));
    }
    let outcome = match child.wait() {
        Ok(outcome) => outcome,
        Err(error) => {
            // A supervision failure does not establish emptiness. Explicitly
            // request teardown, and retain all proof on an unknown result.
            let cleanup = child.terminate("phase-supervision-failed");
            if cleanup
                .as_ref()
                .is_ok_and(|outcome| outcome.containment_empty)
            {
                cleanup_phase_files(request_path, stderr_path);
                let _ = fs::remove_dir_all(&workspace);
            }
            return Err(NativeError::Database(format!(
                "failed to supervise guarded Ladybug phase: {error}; cleanup: {cleanup:?}"
            )));
        }
    };
    let stderr = fs::read(stderr_path).unwrap_or_default();
    if !outcome.containment_empty {
        return Err(NativeError::Database(format!(
            "{outcome}; retain {} until containment is proven empty",
            workspace.display()
        )));
    }
    cleanup_phase_files(request_path, stderr_path);
    let _ = fs::remove_dir_all(&workspace);
    if let Some(reason) = &outcome.budget_reason {
        let failure: MemoryBudgetExceeded = serde_json::from_str(reason).map_err(|error| {
            NativeError::Database(format!(
                "invalid guardian memory failure: {error}: {reason}"
            ))
        })?;
        return Err(NativeError::MemoryBudgetExceeded(failure));
    }
    if !outcome.success {
        return Err(NativeError::Database(format!(
            "isolated Ladybug write phase exited with {outcome}: {}",
            bounded_child_error(&stderr)
        )));
    }
    Ok(outcome.high_water_bytes)
}

fn cleanup_phase_files(request_path: &Path, stderr_path: &Path) {
    let _ = fs::remove_file(request_path);
    let _ = fs::remove_file(stderr_path);
}

fn traced_phase_command(
    executable: &Path,
    _request: &LadybugWritePhaseRequest,
    _request_path: &Path,
) -> Result<Command, NativeError> {
    #[cfg(target_os = "macos")]
    if let Some(trace_root) = std::env::var_os("CODEBASE_GRAPH_PHASE_TIMING_DIR") {
        let trace_root = PathBuf::from(trace_root);
        fs::create_dir_all(&trace_root)?;
        let request_name = _request_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("ladybug-phase");
        let trace_path = trace_root.join(format!(
            "{}-{}.time",
            request_name,
            _request.phase.trace_label()
        ));
        let mut command = Command::new("/usr/bin/time");
        command.arg("-l").arg("-o").arg(trace_path).arg(executable);
        return Ok(command);
    }
    Ok(Command::new(executable))
}

pub(crate) fn execute_phase_file(path: &Path) -> Result<(), String> {
    process_lifecycle::validate_inherited_containment()?;
    let file = fs::File::open(path)
        .map_err(|error| format!("failed to open Ladybug phase request: {error}"))?;
    let request: LadybugWritePhaseRequest = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("failed to parse Ladybug phase request: {error}"))?;
    validate_phase_protocol(request.version)?;
    let _database_use =
        crate::storage::maintenance_ownership::DatabaseUseLease::acquire_shared(&request.db_path)
            .map_err(|error| error.to_string())?;
    #[cfg(debug_assertions)]
    wait_for_test_phase_barrier(path)?;
    execute_phase(&request).map_err(|error| error.to_string())
}

fn validate_phase_protocol(version: u64) -> Result<(), String> {
    if version != PHASE_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported Ladybug phase protocol version {version}; expected {PHASE_PROTOCOL_VERSION}; restart the daemon"
        ));
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn wait_for_test_phase_barrier(request_path: &Path) -> Result<(), String> {
    let Some(directory) = std::env::var_os("CODEBASE_GRAPH_TEST_PHASE_BARRIER_DIR") else {
        return Ok(());
    };
    let directory = PathBuf::from(directory);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create test phase barrier: {error}"))?;
    let pid = std::process::id();
    let readiness = serde_json::json!({ "pid": pid, "request_path": request_path });
    let ready_path = directory.join(format!("phase-{pid}.json"));
    let temporary_path = ready_path.with_extension("tmp");
    fs::write(&temporary_path, readiness.to_string())
        .and_then(|()| fs::rename(&temporary_path, &ready_path))
        .map_err(|error| format!("failed to write test phase barrier readiness: {error}"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut allocation = None;
    while !directory.join("CONTINUE").exists() {
        if allocation.is_none() {
            allocation = allocate_test_phase_memory(&directory, pid)?;
        }
        // Keep touched pages live while the real guardian/phase RSS sampler
        // observes them. The hook never synthesizes a memory-budget result.
        std::hint::black_box(&allocation);
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "test phase barrier timed out waiting for {}",
                directory.join("CONTINUE").display()
            ));
        }
        std::thread::sleep(RSS_SAMPLE_INTERVAL);
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn allocate_test_phase_memory(directory: &Path, pid: u32) -> Result<Option<Vec<u8>>, String> {
    let mut trigger = None;
    // A fixture can target the PID from its ready handshake without faulting
    // a replacement loader that starts before the trigger is removed.
    for path in [
        directory.join(format!("ALLOCATE_BYTES-{pid}")),
        directory.join("ALLOCATE_BYTES"),
    ] {
        match fs::read_to_string(path) {
            Ok(text) => {
                trigger = Some(text);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to read test phase memory trigger: {error}")),
        }
    }
    let Some(text) = trigger else { return Ok(None) };
    let bytes: usize = text
        .trim()
        .parse()
        .map_err(|error| format!("invalid test phase allocation byte count: {error}"))?;
    if bytes == 0 || bytes > 512 * 1024 * 1024 {
        return Err("test phase allocation must be between 1 byte and 512 MiB".into());
    }
    let mut allocation = Vec::new();
    allocation
        .try_reserve_exact(bytes)
        .map_err(|error| format!("failed to reserve test phase memory: {error}"))?;
    allocation.resize(bytes, 1_u8);
    std::hint::black_box(&allocation);
    let ready_path = directory.join(format!("phase-{pid}.memory.json"));
    let temporary_path = ready_path.with_extension("tmp");
    let readiness = serde_json::json!({ "pid": pid, "bytes": bytes });
    fs::write(&temporary_path, readiness.to_string())
        .and_then(|()| fs::rename(&temporary_path, &ready_path))
        .map_err(|error| format!("failed to write test phase memory readiness: {error}"))?;
    Ok(Some(allocation))
}

fn execute_phase(request: &LadybugWritePhaseRequest) -> Result<(), NativeError> {
    let database = open_ladybug_database_with_limits(
        &request.db_path,
        false,
        request.buffer_pool_bytes,
        request.max_num_threads,
    )?;
    let connection = connect_ladybug_database(&database)?;
    match &request.phase {
        LadybugWritePhase::Schema {
            defer_hash_indexes,
            statements,
        } => {
            if *defer_hash_indexes {
                connection
                    .query("CALL enable_default_hash_index=false")
                    .map_err(|error| {
                        NativeError::Database(format!(
                            "failed to disable eager primary-key indexes for bulk load: {error}"
                        ))
                    })?;
            }
            for statement in statements {
                query_ignoring_existing(&connection, statement)?;
            }
            checkpoint(&connection, "schema creation")
        }
        LadybugWritePhase::Copy {
            index,
            total,
            statement,
            runtime_loads,
        } => {
            load_runtime_extensions(&connection, runtime_loads)?;
            connection.query(statement).map_err(|error| {
                NativeError::Database(format!(
                    "COPY statement {}/{} ({}) failed: {error}",
                    index + 1,
                    total,
                    copy_target(statement)
                ))
            })?;
            checkpoint(
                &connection,
                &format!(
                    "COPY statement {}/{} ({})",
                    index + 1,
                    total,
                    copy_target(statement)
                ),
            )
        }
        LadybugWritePhase::Index { table } => {
            let statement = format!(
                "CREATE HASH INDEX `pk_{table}_id` IF NOT EXISTS FOR (node:`{table}`) ON (node.id)"
            );
            connection.query(&statement).map_err(|error| {
                NativeError::Database(format!(
                    "failed to build primary-key index for node table {table}: {error}"
                ))
            })?;
            checkpoint(&connection, &format!("primary-key index for {table}"))
        }
        LadybugWritePhase::PostCopy {
            statement,
            runtime_loads,
        } => {
            load_runtime_extensions(&connection, runtime_loads)?;
            query_ignoring_existing(&connection, statement)?;
            checkpoint(&connection, "post-COPY schema creation")
        }
    }
}

fn write_phase_request(
    request: &LadybugWritePhaseRequest,
    registry_root: Option<&Path>,
) -> Result<PathBuf, NativeError> {
    let parent = match registry_root {
        Some(root) => root,
        None => request.db_path.parent().ok_or_else(|| {
            NativeError::InvalidInput(format!(
                "candidate database path has no parent: {}",
                request.db_path.display()
            ))
        })?,
    };
    fs::create_dir_all(parent)?;
    for _ in 0..16 {
        let sequence = PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = if registry_root.is_some() {
            let workspace = parent.join(format!("phase-{}-{sequence}", std::process::id()));
            match fs::create_dir(&workspace) {
                Ok(()) => workspace.join("request.json"),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(NativeError::Io(error)),
            }
        } else {
            parent.join(format!(
                ".ladybug-phase-{}-{sequence}.json",
                std::process::id()
            ))
        };
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                let mut writer = BufWriter::new(file);
                serde_json::to_writer(&mut writer, request)?;
                writer.flush()?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(NativeError::Io(error)),
        }
    }
    Err(NativeError::InvalidInput(
        "could not allocate a unique Ladybug phase request path".to_string(),
    ))
}

fn load_runtime_extensions(
    connection: &Connection<'_>,
    statements: &[String],
) -> Result<(), NativeError> {
    for statement in statements {
        query_ignoring_existing(connection, statement)?;
    }
    Ok(())
}

fn query_ignoring_existing(
    connection: &Connection<'_>,
    statement: &str,
) -> Result<(), NativeError> {
    match connection.query(statement) {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string().to_lowercase();
            if message.contains("already exists")
                || message.contains("exists already")
                || message.contains("already installed")
            {
                Ok(())
            } else {
                Err(NativeError::Database(error.to_string()))
            }
        }
    }
}

fn checkpoint(connection: &Connection<'_>, after: &str) -> Result<(), NativeError> {
    connection.query("CHECKPOINT").map_err(|error| {
        NativeError::Database(format!("checkpoint after {after} failed: {error}"))
    })?;
    Ok(())
}

fn copy_target(statement: &str) -> &str {
    statement
        .trim_start()
        .strip_prefix("COPY ")
        .and_then(|copy| copy.split_once(" FROM ").map(|(target, _)| target))
        .unwrap_or("unknown target")
}

fn bounded_child_error(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(MAX_CHILD_ERROR_BYTES);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
}

impl LadybugWritePhase {
    fn trace_label(&self) -> String {
        let label = match self {
            Self::Schema { .. } => "schema",
            Self::Copy { statement, .. } => copy_target(statement),
            Self::Index { table } => table,
            Self::PostCopy { .. } => "post-copy",
        };
        label
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_registry_scope_restores_nested_and_unwound_contexts() {
        assert!(PHASE_WORKSPACE_ROOT.with(|slot| slot.borrow().is_none()));
        with_phase_workspace_root(PathBuf::from("outer"), || {
            assert_eq!(
                PHASE_WORKSPACE_ROOT.with(|slot| slot.borrow().clone()),
                Some("outer".into())
            );
            let result = std::panic::catch_unwind(|| {
                with_phase_workspace_root(PathBuf::from("inner"), || {
                    assert_eq!(
                        PHASE_WORKSPACE_ROOT.with(|slot| slot.borrow().clone()),
                        Some("inner".into())
                    );
                    panic!("test scope restoration");
                });
            });
            assert!(result.is_err());
            assert_eq!(
                PHASE_WORKSPACE_ROOT.with(|slot| slot.borrow().clone()),
                Some("outer".into())
            );
        });
        assert!(PHASE_WORKSPACE_ROOT.with(|slot| slot.borrow().is_none()));
    }

    #[test]
    fn scoped_phase_request_lives_outside_candidate_and_preserves_database_path() {
        let root = std::env::temp_dir().join(format!(
            "phase-registry-test-{}-{}",
            std::process::id(),
            PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let registry = root.join("stable").join("phases");
        let request = LadybugWritePhaseRequest::new(
            root.join("run/candidate/db"),
            1024,
            128,
            2,
            LadybugWritePhase::Index {
                table: "Function".into(),
            },
        );
        let path = write_phase_request(&request, Some(&registry)).unwrap();
        assert!(path.starts_with(&registry));
        assert!(!root.join("run").exists());
        let stored: LadybugWritePhaseRequest =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored.db_path, request.db_path);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(debug_assertions)]
    #[test]
    fn phase_test_allocation_is_scoped_to_the_ready_loader() {
        let root = std::env::temp_dir().join(format!(
            "phase-scoped-memory-test-{}-{}",
            std::process::id(),
            PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("ALLOCATE_BYTES-7"), "4096").unwrap();
        assert!(
            allocate_test_phase_memory(&root, 8).unwrap().is_none(),
            "a replacement loader must not inherit another loader's fault"
        );
        assert_eq!(
            allocate_test_phase_memory(&root, 7).unwrap().unwrap().len(),
            4096
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(debug_assertions)]
    #[test]
    fn phase_test_allocation_requires_trigger_and_rejects_unbounded_memory() {
        let root = std::env::temp_dir().join(format!(
            "phase-memory-test-{}-{}",
            std::process::id(),
            PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        assert!(allocate_test_phase_memory(&root, 1).unwrap().is_none());
        fs::write(root.join("ALLOCATE_BYTES"), "536870913").unwrap();
        assert!(allocate_test_phase_memory(&root, 1)
            .unwrap_err()
            .contains("512 MiB"));
        fs::write(root.join("ALLOCATE_BYTES"), "4096").unwrap();
        let allocation = allocate_test_phase_memory(&root, 1).unwrap().unwrap();
        assert_eq!(allocation, vec![1_u8; 4096]);
        let ready: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("phase-1.memory.json")).unwrap()).unwrap();
        assert_eq!(ready["bytes"], 4096);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn phase_protocol_requires_v2_containment_contract() {
        assert!(validate_phase_protocol(2).is_ok());
        for version in [0, 1, 3] {
            let error = validate_phase_protocol(version).unwrap_err();
            assert!(error.contains(&format!("version {version}; expected 2")));
            assert!(error.contains("restart the daemon"));
        }
    }

    #[test]
    fn phase_request_v2_preserves_native_resource_settings() {
        let request = LadybugWritePhaseRequest::new(
            "candidate/db",
            1024 * 1024 * 1024,
            128 * 1024 * 1024,
            3,
            LadybugWritePhase::Copy {
                index: 2,
                total: 4,
                statement: "COPY `Function` FROM 'functions.csv'".into(),
                runtime_loads: vec!["LOAD extension".into()],
            },
        );
        let roundtrip: LadybugWritePhaseRequest =
            serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
        assert_eq!(roundtrip.version, 2);
        assert_eq!(roundtrip.worker_memory_bytes, request.worker_memory_bytes);
        assert_eq!(roundtrip.buffer_pool_bytes, request.buffer_pool_bytes);
        assert_eq!(roundtrip.max_num_threads, 3);
        assert_eq!(
            phase_memory_label(&roundtrip),
            "ladybug__Function__pool_128_mib"
        );
        assert!(matches!(
            roundtrip.phase,
            LadybugWritePhase::Copy {
                index: 2,
                total: 4,
                ..
            }
        ));
    }
}
