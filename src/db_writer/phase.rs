use super::rss::sample_process_rss;
use super::{connect_ladybug_database, open_ladybug_database_with_limits};
use crate::error::{MemoryBudgetExceeded, NativeError};
use lbug::Connection;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const PHASE_PROTOCOL_VERSION: u64 = 1;
const MAX_CHILD_ERROR_BYTES: usize = 64 * 1024;
const MEMORY_HEADROOM_BYTES: u64 = 16 * 1024 * 1024;
const RSS_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
static PHASE_WORKER_EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();
static PHASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    let executable = PHASE_WORKER_EXECUTABLE.get().ok_or_else(|| {
        NativeError::InvalidInput("Ladybug phase worker executable is not registered".to_string())
    })?;
    let request_path = write_phase_request(request)?;
    let mut command = traced_phase_command(executable, request, &request_path)?;
    command
        .arg("__codebase_graph_internal")
        .arg("ladybug-write-phase-v1")
        .arg(&request_path);
    let stderr_path = request_path.with_extension("stderr");
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
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let child_rss = sample_process_rss(child.id()).unwrap_or(0);
        let parent_rss = sample_process_rss(std::process::id()).unwrap_or(0);
        let observed = child_rss.saturating_add(parent_rss);
        high_water_bytes = high_water_bytes.max(observed);
        if observed > kill_threshold {
            let _ = child.kill();
            let _ = child.wait();
            cleanup_phase_files(&request_path, &stderr_path);
            return Err(NativeError::MemoryBudgetExceeded(
                MemoryBudgetExceeded::new(
                    format!("ladybug_{}", request.phase.trace_label()),
                    request.worker_memory_bytes,
                    observed,
                    observed,
                ),
            ));
        }
        std::thread::sleep(RSS_SAMPLE_INTERVAL);
    };
    let stderr = fs::read(&stderr_path).unwrap_or_default();
    cleanup_phase_files(&request_path, &stderr_path);
    if !status.success() {
        let stderr = bounded_child_error(&stderr);
        return Err(NativeError::Database(format!(
            "isolated Ladybug write phase exited with {}: {stderr}",
            status
        )));
    }
    Ok(high_water_bytes)
}

fn cleanup_phase_files(request_path: &Path, stderr_path: &Path) {
    let _ = fs::remove_file(request_path);
    let _ = fs::remove_file(stderr_path);
}

fn traced_phase_command(
    executable: &Path,
    request: &LadybugWritePhaseRequest,
    request_path: &Path,
) -> Result<Command, NativeError> {
    #[cfg(target_os = "macos")]
    if let Some(trace_root) = std::env::var_os("CODEBASE_GRAPH_PHASE_TIMING_DIR") {
        let trace_root = PathBuf::from(trace_root);
        fs::create_dir_all(&trace_root)?;
        let request_name = request_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("ladybug-phase");
        let trace_path = trace_root.join(format!(
            "{}-{}.time",
            request_name,
            request.phase.trace_label()
        ));
        let mut command = Command::new("/usr/bin/time");
        command.arg("-l").arg("-o").arg(trace_path).arg(executable);
        return Ok(command);
    }
    Ok(Command::new(executable))
}

pub(crate) fn execute_phase_file(path: &Path) -> Result<(), String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("failed to open Ladybug phase request: {error}"))?;
    let request: LadybugWritePhaseRequest = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("failed to parse Ladybug phase request: {error}"))?;
    if request.version != PHASE_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported Ladybug phase protocol version {}; expected {PHASE_PROTOCOL_VERSION}",
            request.version
        ));
    }
    execute_phase(&request).map_err(|error| error.to_string())
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

fn write_phase_request(request: &LadybugWritePhaseRequest) -> Result<PathBuf, NativeError> {
    let parent = request.db_path.parent().ok_or_else(|| {
        NativeError::InvalidInput(format!(
            "candidate database path has no parent: {}",
            request.db_path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;
    for _ in 0..16 {
        let sequence = PHASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".ladybug-phase-{}-{sequence}.json",
            std::process::id()
        ));
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
