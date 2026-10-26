use super::rss::sample_process_rss;
use super::{connect_ladybug_database, open_ladybug_database_with_threads};
use crate::error::NativeError;
use crate::process_lifecycle::{self, OutputMode, PreparedChild, SpawnSpec};
use lbug::Connection;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const PHASE_PROTOCOL_VERSION: u64 = 2;
const MAX_CHILD_ERROR_BYTES: usize = 64 * 1024;
static PHASE_WORKER_EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
static PHASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static PHASE_WORKSPACE_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static PHASE_PARENT_RUN: RefCell<Option<crate::storage::run_workspace::ParentRunScope>> = const { RefCell::new(None) };
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

pub(crate) fn phase_has_parent_workspace() -> bool {
    PHASE_PARENT_RUN.with(|slot| slot.borrow().is_some())
}

struct PhaseParentScope(Option<crate::storage::run_workspace::ParentRunScope>);
impl Drop for PhaseParentScope {
    fn drop(&mut self) {
        PHASE_PARENT_RUN.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}
pub(crate) fn with_phase_workspace_scope<T>(
    root: PathBuf,
    parent: Option<crate::storage::run_workspace::ParentRunScope>,
    execute: impl FnOnce() -> T,
) -> T {
    let previous = PHASE_PARENT_RUN.with(|slot| slot.replace(parent));
    let _parent = PhaseParentScope(previous);
    with_phase_workspace_root(root, execute)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LadybugWritePhaseRequest {
    version: u64,
    db_path: PathBuf,
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
        max_num_threads: u64,
        phase: LadybugWritePhase,
    ) -> Self {
        Self {
            version: PHASE_PROTOCOL_VERSION,
            db_path: db_path.into(),
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
    let (request_path, phase_owner) = write_phase_request_owned(request, registry_root.as_deref())?;
    let mut command = traced_phase_command(executable, request, &request_path)?;
    command
        .arg("__codebase_graph_internal")
        .arg("ladybug-write-phase-v2")
        .arg(&request_path);
    let stderr_path = request_path.with_extension("stderr");
    if phase_owner.is_some() {
        if let Ok(Some(output)) =
            crate::storage::maintenance_controls::WorkspaceOutputFile::create_new(&stderr_path)
        {
            // The guardian will append accepted OS bytes to this exact inode.
            // Bound creation provenance owns incomplete output; it does not
            // certify loader usability or pretend to hash child-owned writes.
            let _ = crate::storage::maintenance_origin::record_created_workspace_output(&output);
        }
    }
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
        let result = run_guarded_phase(
            request,
            executable,
            command,
            &request_path,
            &stderr_path,
            workspace,
        );
        if let Some(owner) = phase_owner {
            owner.finish_if_empty();
        }
        return result;
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
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(NativeError::Database(format!(
                    "failed to supervise Ladybug phase: {error}; retain {} until build containment is empty",
                    request_path.display()
                )));
            }
        }
        let observed = sample_process_rss(child.id())
            .unwrap_or(0)
            .saturating_add(sample_process_rss(std::process::id()).unwrap_or(0));
        high_water_bytes = high_water_bytes.max(observed);
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    if !status.success() {
        let stderr = read_error_tail(&stderr_path).unwrap_or_default();
        return Err(NativeError::Database(isolated_phase_error(
            request,
            &status.to_string(),
            &stderr,
        )));
    }
    cleanup_phase_files(&request_path, &stderr_path);
    Ok(high_water_bytes)
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
            cleanup_phase_workspace(&workspace);
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
                cleanup_phase_workspace(&workspace);
            }
            return Err(NativeError::Database(format!(
                "failed to supervise guarded Ladybug phase: {error}; cleanup: {cleanup:?}"
            )));
        }
    };
    if !outcome.containment_empty {
        return Err(NativeError::Database(format!(
            "{outcome}; retain {} until containment is proven empty",
            workspace.display()
        )));
    }
    if !outcome.success {
        let stderr = read_error_tail(stderr_path).unwrap_or_default();
        cleanup_phase_files(request_path, stderr_path);
        cleanup_phase_workspace(&workspace);
        return Err(NativeError::Database(isolated_phase_error(
            request,
            &outcome.to_string(),
            &stderr,
        )));
    }
    cleanup_phase_files(request_path, stderr_path);
    cleanup_phase_workspace(&workspace);
    Ok(outcome.high_water_bytes)
}

fn cleanup_phase_workspace(workspace: &Path) {
    // A registered root is disposed only through the bounded local lane.
    if !workspace.join(".workspace.binding").exists() {
        let _ = fs::remove_dir_all(workspace);
    }
}

struct PhaseWorkspaceOwner {
    registration: crate::storage::maintenance_controls::WorkspaceRegistration,
    owner: crate::storage::locks::LockedFile,
    mutation: crate::storage::maintenance_ownership::ReferenceMutation<'static>,
    _parent: crate::storage::run_workspace::ParentRunScope,
}
impl PhaseWorkspaceOwner {
    fn finish_if_empty(self) {
        let _ = self.registration.terminal(
            &self.owner,
            &self.mutation,
            crate::storage::maintenance_controls::WorkspaceDisposition::AbortedNotPublished,
        );
    }
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
    execute_phase(&request).map_err(|error| error.to_string())?;
    #[cfg(debug_assertions)]
    record_test_phase_result(&request)?;
    Ok(())
}

#[cfg(debug_assertions)]
fn copy_source_metadata(statement: &str) -> Option<(String, u64)> {
    let source = statement.split_once(" FROM ")?.1.trim_start();
    let quote = source.chars().next()?;
    if quote != '\"' && quote != '\'' {
        return None;
    }
    let mut value = String::new();
    let mut escaped = false;
    for character in source[quote.len_utf8()..].chars() {
        if escaped {
            value.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            let bytes = fs::metadata(&value).ok()?.len();
            return Some((value, bytes));
        } else {
            value.push(character);
        }
    }
    None
}

#[cfg(debug_assertions)]
fn record_test_phase_result(request: &LadybugWritePhaseRequest) -> Result<(), String> {
    let Some(directory) = std::env::var_os("CODEBASE_GRAPH_TEST_COPY_RESULT_DIR") else {
        return Ok(());
    };
    let directory = PathBuf::from(directory);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create test phase result directory: {error}"))?;
    let (phase, copy_index, copy_total, target) = match &request.phase {
        LadybugWritePhase::Schema { .. } => ("schema", None, None, None),
        LadybugWritePhase::Copy {
            index,
            total,
            statement,
            ..
        } => (
            "copy",
            Some(*index),
            Some(*total),
            Some(copy_target(statement).to_string()),
        ),
        LadybugWritePhase::Index { table } => ("index", None, None, Some(table.clone())),
        LadybugWritePhase::PostCopy { .. } => ("postcopy", None, None, None),
    };
    let mut trace = serde_json::to_value(request)
        .map_err(|error| format!("failed to encode test phase request: {error}"))?;
    let object = trace
        .as_object_mut()
        .ok_or_else(|| "test phase request did not encode as an object".to_string())?;
    object.insert("phase_kind".into(), serde_json::json!(phase));
    object.insert("copy_index".into(), serde_json::json!(copy_index));
    object.insert("copy_total".into(), serde_json::json!(copy_total));
    object.insert("target".into(), serde_json::json!(target));
    let staged = match &request.phase {
        LadybugWritePhase::Copy { statement, .. } => copy_source_metadata(statement),
        _ => None,
    };
    object.insert(
        "staged_file_path".into(),
        serde_json::json!(staged.as_ref().map(|(path, _)| path)),
    );
    object.insert(
        "staged_file_bytes".into(),
        serde_json::json!(staged.as_ref().map(|(_, bytes)| bytes)),
    );
    object.insert("pid".into(), serde_json::json!(std::process::id()));
    object.insert("success".into(), serde_json::json!(true));
    let trace_path = directory.join("phases.jsonl");
    let mut trace_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_path)
        .map_err(|error| format!("failed to open test phase results: {error}"))?;
    writeln!(trace_file, "{trace}")
        .map_err(|error| format!("failed to write test phase result: {error}"))?;
    if matches!(&request.phase, LadybugWritePhase::Copy { .. })
        && directory.join("FAIL_AFTER_COPY").exists()
    {
        return Err(
            "COPY result acknowledgement failed: Buffer manager exception: the buffer pool is full"
                .to_string(),
        );
    }
    Ok(())
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
        std::thread::sleep(std::time::Duration::from_millis(25));
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
    let database =
        open_ladybug_database_with_threads(&request.db_path, false, request.max_num_threads)?;
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
                run_schema_query(&connection, statement)?;
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
            connection
                .query(statement)
                .map_err(|error| copy_query_failure(*index, *total, statement, error))?;
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
            run_schema_query(&connection, statement)?;
            checkpoint(&connection, "post-COPY schema creation")
        }
    }
}

#[cfg(test)]
fn write_phase_request(
    request: &LadybugWritePhaseRequest,
    registry_root: Option<&Path>,
) -> Result<PathBuf, NativeError> {
    write_phase_request_owned(request, registry_root).map(|(path, _)| path)
}
fn write_phase_request_owned(
    request: &LadybugWritePhaseRequest,
    registry_root: Option<&Path>,
) -> Result<(PathBuf, Option<PhaseWorkspaceOwner>), NativeError> {
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
        let mut phase_owner = None;
        let path = if registry_root.is_some() {
            let workspace = parent.join(format!("phase-{}-{sequence}", std::process::id()));
            let parent_scope =
                PHASE_PARENT_RUN.with(|slot| slot.borrow().as_ref().map(|parent| parent.retain()));
            let mutation = parent_scope
                .as_ref()
                .map(|parent| {
                    crate::storage::maintenance_ownership::ReferenceMutation::begin_in_domain(
                        parent.domain(),
                    )
                })
                .transpose()?;
            let _startup = parent_scope
                .as_ref()
                .map(|parent| {
                    crate::storage::locks::open_shared_coordination(
                        &parent.domain().workspace_startup_path(),
                    )
                })
                .transpose()?;
            let Some(fresh) = crate::storage::layout::create_fresh_reference_root(&workspace)?
            else {
                continue;
            };
            if let (Some(parent), Some(mutation)) = (parent_scope, mutation) {
                let directory =
                    crate::storage::locks::RootedDirectory::open(&workspace, fresh.identity())?;
                let owner = crate::storage::locks::create_new_coordination_in(
                    &directory,
                    b"lease.lock",
                    &workspace.join("lease.lock"),
                )?;
                if let Some(registration) =
                    crate::storage::maintenance_controls::WorkspaceRegistration::create_with_parent(
                        &fresh,
                        parent.domain(),
                        &mutation,
                        &owner,
                        3,
                        Some(&parent),
                    )
                    .ok()
                    .flatten()
                {
                    phase_owner = Some(PhaseWorkspaceOwner {
                        registration,
                        owner,
                        mutation,
                        _parent: parent,
                    });
                }
            }
            workspace.join("request.json")
        } else {
            parent.join(format!(
                ".ladybug-phase-{}-{sequence}.json",
                std::process::id()
            ))
        };
        if phase_owner.is_some() {
            match crate::storage::maintenance_controls::WorkspaceOutputFile::create_new(&path) {
                Ok(Some(output)) => {
                    let _ = crate::storage::maintenance_origin::record_created_workspace_output(
                        &output,
                    );
                    let mut writer = BufWriter::new(output);
                    serde_json::to_writer(&mut writer, request)?;
                    writer.flush()?;
                    let output = writer
                        .into_inner()
                        .map_err(|error| NativeError::Io(error.into_error()))?;
                    if let Ok(completed) = output.finish() {
                        let _ =
                            crate::storage::maintenance_origin::record_completed_workspace_output(
                                &completed,
                            );
                    }
                    return Ok((path, phase_owner));
                }
                Ok(None) => {
                    // Optional activation may have created an unattested empty
                    // inode. Primary request construction remains available.
                    let file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)?;
                    let mut writer = BufWriter::new(file);
                    serde_json::to_writer(&mut writer, request)?;
                    writer.flush()?;
                    return Ok((path, phase_owner));
                }
                Err(NativeError::Io(error))
                    if error.kind() == std::io::ErrorKind::AlreadyExists =>
                {
                    continue
                }
                Err(error) => return Err(error),
            }
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                let mut writer = BufWriter::new(file);
                serde_json::to_writer(&mut writer, request)?;
                writer.flush()?;
                return Ok((path, phase_owner));
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
        run_schema_query(connection, statement)?;
    }
    Ok(())
}

fn run_schema_query(connection: &Connection<'_>, statement: &str) -> Result<(), NativeError> {
    connection
        .query(statement)
        .map(|_| ())
        .map_err(|error| NativeError::Database(error.to_string()))
}

fn copy_query_failure(
    index: usize,
    total: usize,
    statement: &str,
    error: impl std::fmt::Display,
) -> NativeError {
    NativeError::Database(format!(
        "COPY statement {}/{} ({}) failed: {error}; operation=COPY-query",
        index.saturating_add(1),
        total,
        copy_target(statement)
    ))
}

fn checkpoint_failure(after: &str, error: impl std::fmt::Display) -> NativeError {
    NativeError::Database(format!(
        "checkpoint after {after} failed: {error}; operation=explicit-CHECKPOINT"
    ))
}

fn checkpoint(connection: &Connection<'_>, after: &str) -> Result<(), NativeError> {
    connection
        .query("CHECKPOINT")
        .map_err(|error| checkpoint_failure(after, error))?;
    Ok(())
}

fn copy_target(statement: &str) -> &str {
    statement
        .trim_start()
        .strip_prefix("COPY ")
        .and_then(|copy| copy.split_once(" FROM ").map(|(target, _)| target))
        .unwrap_or("unknown target")
}

fn read_error_tail(path: &Path) -> Result<String, std::io::Error> {
    read_error_tail_from(&mut fs::File::open(path)?)
}

fn read_error_tail_from(file: &mut (impl Read + Seek)) -> Result<String, std::io::Error> {
    let len = file.seek(SeekFrom::End(0))?;
    let start = len.saturating_sub(MAX_CHILD_ERROR_BYTES as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((len - start) as usize);
    file.take(MAX_CHILD_ERROR_BYTES as u64)
        .read_to_end(&mut bytes)?;
    let decoded = String::from_utf8_lossy(&bytes);
    let marker = "[stderr truncated] ";
    let truncated = start > 0 || decoded.len() > MAX_CHILD_ERROR_BYTES;
    let budget = MAX_CHILD_ERROR_BYTES - if truncated { marker.len() } else { 0 };
    let tail = truncate_utf8_tail(decoded.trim(), budget);
    Ok(if truncated {
        format!("{marker}{tail}")
    } else {
        tail.to_string()
    })
}

fn isolated_phase_error(
    request: &LadybugWritePhaseRequest,
    status: &str,
    child_error: &str,
) -> String {
    let db_path = request.db_path.display().to_string();
    let db_path = truncate_utf8_head(&db_path, 1024);
    let status = truncate_utf8_head(status, 1024);
    let (phase, target) = match &request.phase {
        LadybugWritePhase::Schema { .. } => ("schema", String::new()),
        LadybugWritePhase::Copy {
            index,
            total,
            statement,
            ..
        } => (
            "copy",
            format!(
                " copy={}/{} target={}",
                index.saturating_add(1),
                total,
                truncate_utf8_head(copy_target(statement), 1024)
            ),
        ),
        LadybugWritePhase::Index { table } => (
            "index",
            format!(" target={}", truncate_utf8_head(table, 1024)),
        ),
        LadybugWritePhase::PostCopy { .. } => ("post-copy", String::new()),
    };
    let context = format!(
        "isolated Ladybug write phase exited with {status}: phase={phase} db={db_path} max_num_threads={}{target}; ",
        request.max_num_threads,
    );
    let marker = "[stderr truncated] ";
    let payload = child_error.strip_prefix(marker).unwrap_or(child_error);
    let truncated = child_error.starts_with(marker)
        || payload.len() > MAX_CHILD_ERROR_BYTES.saturating_sub(context.len());
    let budget = MAX_CHILD_ERROR_BYTES
        .saturating_sub(context.len())
        .saturating_sub(if truncated { marker.len() } else { 0 });
    let tail = truncate_utf8_tail(payload, budget);
    if truncated {
        format!("{context}{marker}{tail}")
    } else {
        format!("{context}{tail}")
    }
}

fn truncate_utf8_head(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn truncate_utf8_tail(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
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
    fn phase_query_keeps_successful_if_not_exists_idempotence_but_surfaces_native_error() {
        let root = std::env::temp_dir().join(format!(
            "phase-query-test-{}-{}",
            std::process::id(),
            PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("db.lbug");
        let database = open_ladybug_database_with_threads(&path, false, 1).unwrap();
        let connection = connect_ladybug_database(&database).unwrap();
        run_schema_query(
            &connection,
            "CREATE NODE TABLE IF NOT EXISTS `Thing`(`id` STRING PRIMARY KEY)",
        )
        .unwrap();
        run_schema_query(
            &connection,
            "CREATE NODE TABLE IF NOT EXISTS `Thing`(`id` STRING PRIMARY KEY)",
        )
        .unwrap();
        let error = run_schema_query(
            &connection,
            "CREATE NODE TABLE `Thing`(`id` STRING PRIMARY KEY)",
        )
        .unwrap_err();
        let message = error.to_string().to_lowercase();
        assert!(message.contains("already exists") || message.contains("exists already"));
        drop(connection);
        drop(database);
        fs::remove_dir_all(root).unwrap();
    }

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
    fn error_tail_reads_at_most_the_limit_and_decodes_utf8_lossily() {
        let root = std::env::temp_dir().join(format!(
            "phase-error-tail-{}-{}",
            std::process::id(),
            PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("stderr");
        let mut contents = vec![b'x'; MAX_CHILD_ERROR_BYTES + 4096];
        contents[MAX_CHILD_ERROR_BYTES + 3] = 0xff;
        contents[MAX_CHILD_ERROR_BYTES + 4094] = 0xc3;
        contents[MAX_CHILD_ERROR_BYTES + 4095] = 0xa9;
        fs::write(&path, contents).unwrap();
        let tail = read_error_tail(&path).unwrap();
        assert!(tail.len() <= MAX_CHILD_ERROR_BYTES + "[stderr truncated]\n".len());
        assert!(tail.starts_with("[stderr truncated]"));
        assert!(tail.contains('\u{fffd}'));
        assert!(tail.ends_with('é'));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn error_tail_reader_requests_no_more_than_the_limit() {
        struct CountingCursor {
            cursor: std::io::Cursor<Vec<u8>>,
            requested: usize,
        }
        impl Read for CountingCursor {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.requested = self.requested.saturating_add(buffer.len());
                self.cursor.read(buffer)
            }
        }
        impl Seek for CountingCursor {
            fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
                self.cursor.seek(position)
            }
        }
        let mut reader = CountingCursor {
            cursor: std::io::Cursor::new(vec![b'x'; MAX_CHILD_ERROR_BYTES * 5]),
            requested: 0,
        };
        let tail = read_error_tail_from(&mut reader).unwrap();
        assert_eq!(reader.requested, MAX_CHILD_ERROR_BYTES);
        assert!(tail.starts_with("[stderr truncated]"));
        assert!(tail.len() <= MAX_CHILD_ERROR_BYTES + "[stderr truncated]\n".len());
    }

    #[test]
    fn oversized_native_error_preserves_query_vs_explicit_checkpoint_boundary() {
        let statement = "COPY `Thing` FROM 'rows.json'";
        let request = LadybugWritePhaseRequest::new(
            "candidate/db",
            1,
            LadybugWritePhase::Copy {
                index: 0,
                total: 1,
                statement: statement.into(),
                runtime_loads: vec![],
            },
        );
        let native = "é".repeat(MAX_CHILD_ERROR_BYTES);
        let failures = [
            (
                copy_query_failure(0, 1, statement, &native),
                "operation=COPY-query",
            ),
            (
                checkpoint_failure("COPY statement 1/1", &native),
                "operation=explicit-CHECKPOINT",
            ),
        ];
        for (failure, boundary) in failures {
            let mut stderr =
                std::io::Cursor::new(format!("earlier native output\n{failure}\n").into_bytes());
            let tail = read_error_tail_from(&mut stderr).unwrap();
            let diagnostic = isolated_phase_error(&request, "exit status: 1", &tail);
            assert!(diagnostic.len() <= MAX_CHILD_ERROR_BYTES);
            assert!(diagnostic.contains("[stderr truncated]"));
            assert!(
                diagnostic.ends_with(boundary),
                "operation boundary was lost"
            );
            assert!(diagnostic.contains("copy=1/1 target=`Thing`"));
        }
    }

    #[test]
    fn isolated_error_keeps_context_inside_the_cap() {
        let request = LadybugWritePhaseRequest::new(
            "candidate/db",
            2,
            LadybugWritePhase::Copy {
                index: 7,
                total: 10,
                statement: "COPY `DocumentationSource` FROM 'rows.csv'".into(),
                runtime_loads: vec![],
            },
        );
        let message = isolated_phase_error(&request, "exit status: 1", &"é".repeat(40_000));
        assert!(message.len() <= MAX_CHILD_ERROR_BYTES);
        assert!(message.contains("copy=8/10 target=`DocumentationSource`"));
        assert!(message.contains("max_num_threads=2"));
        assert!(std::str::from_utf8(message.as_bytes()).is_ok());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn copy_trace_metadata_reports_generated_file_bytes() {
        let root = std::env::temp_dir().join(format!(
            "phase-copy-source-{}-{}",
            std::process::id(),
            PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("rows.json");
        fs::write(&source, b"[{}]").unwrap();
        let statement = format!("COPY `Thing` FROM \"{}\"", source.display());
        assert_eq!(
            copy_source_metadata(&statement),
            Some((source.to_string_lossy().to_string(), 4))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn phase_request_v2_preserves_thread_setting() {
        let request = LadybugWritePhaseRequest::new(
            "candidate/db",
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
        assert_eq!(roundtrip.max_num_threads, 3);
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
