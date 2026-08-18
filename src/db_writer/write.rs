use super::extensions::preseed_ladybug_extensions;
use super::phase::{
    phase_worker_available, run_isolated_phase, LadybugWritePhase, LadybugWritePhaseRequest,
};
use super::request::{LadybugWriteMetrics, LadybugWriteRequest};
use super::rss::release_unused_allocator_pages;
use super::schema::statement_phases;
use super::{
    connect_ladybug_database, open_ladybug_database_with_limits, retry_transient_database,
    WRITE_RETRY_POLICY,
};
use crate::error::NativeError;
use lbug::Connection;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const INITIAL_DATABASE_BUFFER_POOL_BYTES: u64 = 256 * 1024 * 1024;
const DATABASE_BUFFER_POOL_STEP_BYTES: u64 = 64 * 1024 * 1024;

pub fn write_database(request: LadybugWriteRequest) -> Result<(), NativeError> {
    write_database_with_metrics(request).map(|_| ())
}

pub fn write_database_with_metrics(
    request: LadybugWriteRequest,
) -> Result<LadybugWriteMetrics, NativeError> {
    if phase_worker_available() {
        write_database_isolated(&request)
    } else {
        retry_transient_database(WRITE_RETRY_POLICY, || write_database_once(&request))?;
        Ok(LadybugWriteMetrics::default())
    }
}

fn write_database_once(request: &LadybugWriteRequest) -> Result<(), NativeError> {
    prepare_candidate(request)?;
    let database = open_ladybug_database_with_limits(
        Path::new(&request.db_path),
        false,
        request.buffer_pool_bytes,
        request.max_num_threads,
    )?;
    let connection = connect_ladybug_database(&database)?;
    if request.defer_hash_indexes {
        connection
            .query("CALL enable_default_hash_index=false")
            .map_err(|error| {
                NativeError::Database(format!(
                    "failed to disable eager primary-key indexes for bulk load: {error}"
                ))
            })?;
    }
    let phases = statement_phases(request.include_fts, &request.schema_statements);
    for statement in phases.pre_copy {
        query_ignoring_existing(&connection, statement)?;
    }
    if request.defer_hash_indexes {
        execute_deferred_index_copy(&connection, request)?;
    } else {
        for (index, statement) in request.copy_statements.iter().enumerate() {
            execute_copy(&connection, request, index, statement)?;
        }
    }
    for statement in phases.post_copy {
        query_ignoring_existing(&connection, statement)?;
    }
    Ok(())
}

fn write_database_isolated(
    request: &LadybugWriteRequest,
) -> Result<LadybugWriteMetrics, NativeError> {
    prepare_candidate(request)?;
    let phases = statement_phases(request.include_fts, &request.schema_statements);
    let pre_copy = phases
        .pre_copy
        .iter()
        .map(|statement| (*statement).to_string())
        .collect::<Vec<_>>();
    let runtime_loads = pre_copy
        .iter()
        .filter(|statement| {
            statement
                .trim_start()
                .get(.."LOAD ".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("LOAD "))
        })
        .cloned()
        .collect::<Vec<_>>();
    // Parsing, external sorting, and sidecar construction can leave free pages
    // resident in the parent allocator. Return them before the first Ladybug
    // child so the combined parent+child RSS supervisor measures live work.
    release_unused_allocator_pages();
    let mut high_water_bytes = run_phase(
        request,
        LadybugWritePhase::Schema {
            defer_hash_indexes: request.defer_hash_indexes,
            statements: pre_copy,
        },
    )?;

    let connector_start = if request.defer_hash_indexes {
        request
            .copy_statements
            .iter()
            .position(|statement| node_table_name(statement).is_none())
            .unwrap_or(request.copy_statements.len())
    } else {
        request.copy_statements.len()
    };
    let early_index_tables =
        tables_requiring_early_index(&request.copy_statements[..connector_start])?;
    let mut node_tables = BTreeSet::new();
    let mut indexed_tables = BTreeSet::new();
    for (index, statement) in request.copy_statements[..connector_start]
        .iter()
        .enumerate()
    {
        high_water_bytes =
            high_water_bytes.max(run_copy_phase(request, index, statement, &runtime_loads)?);
        if request.defer_hash_indexes {
            let table = node_table_name(statement).ok_or_else(|| {
                NativeError::InvalidInput(format!(
                    "node COPY statement has an unsupported target: {}",
                    copy_target(statement)
                ))
            })?;
            if early_index_tables.contains(table) {
                if indexed_tables.insert(table.to_string()) {
                    high_water_bytes = high_water_bytes.max(run_phase(
                        request,
                        LadybugWritePhase::Index {
                            table: table.to_string(),
                        },
                    )?);
                }
            } else {
                node_tables.insert(table.to_string());
            }
        }
    }
    for table in node_tables {
        high_water_bytes =
            high_water_bytes.max(run_phase(request, LadybugWritePhase::Index { table })?);
    }
    for (offset, statement) in request.copy_statements[connector_start..]
        .iter()
        .enumerate()
    {
        high_water_bytes = high_water_bytes.max(run_copy_phase(
            request,
            connector_start + offset,
            statement,
            &[],
        )?);
    }
    for statement in phases.post_copy {
        high_water_bytes = high_water_bytes.max(run_phase(
            request,
            LadybugWritePhase::PostCopy {
                statement: statement.to_string(),
                runtime_loads: runtime_loads.clone(),
            },
        )?);
    }
    Ok(LadybugWriteMetrics { high_water_bytes })
}

fn prepare_candidate(request: &LadybugWriteRequest) -> Result<(), NativeError> {
    preseed_ladybug_extensions(request.include_fts)?;
    if Path::new(&request.db_path).exists() {
        return Err(NativeError::InvalidInput(format!(
            "candidate database path already exists: {}",
            request.db_path
        )));
    }
    if let Some(parent) = Path::new(&request.db_path).parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn phase_request(
    request: &LadybugWriteRequest,
    buffer_pool_bytes: u64,
    phase: LadybugWritePhase,
) -> LadybugWritePhaseRequest {
    LadybugWritePhaseRequest::new(
        &request.db_path,
        request.worker_memory_bytes,
        buffer_pool_bytes,
        request.max_num_threads,
        phase,
    )
}

fn run_copy_phase(
    request: &LadybugWriteRequest,
    index: usize,
    statement: &str,
    runtime_loads: &[String],
) -> Result<u64, NativeError> {
    run_phase(
        request,
        LadybugWritePhase::Copy {
            index,
            total: request.copy_statements.len(),
            statement: statement.to_string(),
            runtime_loads: runtime_loads.to_vec(),
        },
    )
}

fn run_phase(request: &LadybugWriteRequest, phase: LadybugWritePhase) -> Result<u64, NativeError> {
    let mut buffer_pool_bytes = request
        .buffer_pool_bytes
        .min(INITIAL_DATABASE_BUFFER_POOL_BYTES);
    loop {
        match run_isolated_phase(&phase_request(request, buffer_pool_bytes, phase.clone())) {
            Ok(high_water_bytes) => return Ok(high_water_bytes),
            Err(error)
                if is_retryable_buffer_pool_exhaustion(&error)
                    && buffer_pool_bytes < request.buffer_pool_bytes =>
            {
                buffer_pool_bytes =
                    next_buffer_pool_bytes(buffer_pool_bytes, request.buffer_pool_bytes);
            }
            Err(error) => return Err(error),
        }
    }
}

fn next_buffer_pool_bytes(current: u64, maximum: u64) -> u64 {
    current
        .saturating_add(DATABASE_BUFFER_POOL_STEP_BYTES)
        .min(maximum)
}

fn is_retryable_buffer_pool_exhaustion(error: &NativeError) -> bool {
    let NativeError::Database(message) = error else {
        return false;
    };
    let message = message.to_ascii_lowercase();
    message.contains("buffer pool is full") && !message.contains("checkpoint after")
}

fn execute_deferred_index_copy(
    connection: &Connection<'_>,
    request: &LadybugWriteRequest,
) -> Result<(), NativeError> {
    let connector_start = request
        .copy_statements
        .iter()
        .position(|statement| node_table_name(statement).is_none())
        .unwrap_or(request.copy_statements.len());
    let early_index_tables =
        tables_requiring_early_index(&request.copy_statements[..connector_start])?;
    let mut node_tables = BTreeSet::new();
    let mut indexed_tables = BTreeSet::new();
    for (index, statement) in request.copy_statements[..connector_start]
        .iter()
        .enumerate()
    {
        execute_copy(connection, request, index, statement)?;
        let table = node_table_name(statement).ok_or_else(|| {
            NativeError::InvalidInput(format!(
                "node COPY statement has an unsupported target: {}",
                copy_target(statement)
            ))
        })?;
        if early_index_tables.contains(table) {
            if indexed_tables.insert(table.to_string()) {
                create_hash_index(connection, table)?;
            }
        } else {
            node_tables.insert(table.to_string());
        }
    }
    for table in node_tables {
        create_hash_index(connection, &table)?;
    }
    for (index, statement) in request.copy_statements[connector_start..]
        .iter()
        .enumerate()
    {
        execute_copy(connection, request, connector_start + index, statement)?;
    }
    Ok(())
}

fn tables_requiring_early_index(
    node_copy_statements: &[String],
) -> Result<BTreeSet<String>, NativeError> {
    let mut seen = BTreeSet::new();
    let mut repeated = BTreeSet::new();
    for statement in node_copy_statements {
        let table = node_table_name(statement).ok_or_else(|| {
            NativeError::InvalidInput(format!(
                "node COPY statement has an unsupported target: {}",
                copy_target(statement)
            ))
        })?;
        if !seen.insert(table.to_string()) {
            repeated.insert(table.to_string());
        }
    }
    Ok(repeated)
}

fn create_hash_index(connection: &Connection<'_>, table: &str) -> Result<(), NativeError> {
    let statement = format!(
        "CREATE HASH INDEX `pk_{table}_id` IF NOT EXISTS FOR (node:`{table}`) ON (node.id)"
    );
    connection.query(&statement).map_err(|error| {
        NativeError::Database(format!(
            "failed to build primary-key index for node table {table}: {error}"
        ))
    })?;
    checkpoint(connection, &format!("primary-key index for {table}"))
}

fn execute_copy(
    connection: &Connection<'_>,
    request: &LadybugWriteRequest,
    index: usize,
    statement: &str,
) -> Result<(), NativeError> {
    connection.query(statement).map_err(|error| {
        NativeError::Database(format!(
            "COPY statement {}/{} ({}) failed: {error}",
            index + 1,
            request.copy_statements.len(),
            copy_target(statement)
        ))
    })?;
    checkpoint(
        connection,
        &format!(
            "COPY statement {}/{} ({})",
            index + 1,
            request.copy_statements.len(),
            copy_target(statement)
        ),
    )
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

fn node_table_name(statement: &str) -> Option<&str> {
    let table = copy_target(statement)
        .strip_prefix('`')?
        .strip_suffix('`')?;
    if table.starts_with("FROM_") || table.starts_with("TO_") {
        return None;
    }
    table
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        .then_some(table)
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

#[cfg(test)]
mod tests {
    use super::{
        is_retryable_buffer_pool_exhaustion, next_buffer_pool_bytes,
        DATABASE_BUFFER_POOL_STEP_BYTES, INITIAL_DATABASE_BUFFER_POOL_BYTES,
    };
    use crate::error::{MemoryBudgetExceeded, NativeError};

    #[test]
    fn database_pool_escalation_is_bounded_and_incremental() {
        let maximum = 384 * 1024 * 1024;
        let second = next_buffer_pool_bytes(INITIAL_DATABASE_BUFFER_POOL_BYTES, maximum);
        let third = next_buffer_pool_bytes(second, maximum);

        assert_eq!(second, 320 * 1024 * 1024);
        assert_eq!(third, maximum);
        assert_eq!(next_buffer_pool_bytes(third, maximum), maximum);
        assert_eq!(DATABASE_BUFFER_POOL_STEP_BYTES, 64 * 1024 * 1024);
    }

    #[test]
    fn only_failed_database_operations_retry_pool_exhaustion() {
        let copy_failure = NativeError::Database(
            "COPY failed: Buffer manager exception: the buffer pool is full".to_string(),
        );
        let checkpoint_failure = NativeError::Database(
            "checkpoint after COPY failed: the buffer pool is full".to_string(),
        );
        let budget_failure = NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new(
            "ladybug_copy",
            768 * 1024 * 1024,
            760 * 1024 * 1024,
            760 * 1024 * 1024,
        ));

        assert!(is_retryable_buffer_pool_exhaustion(&copy_failure));
        assert!(!is_retryable_buffer_pool_exhaustion(&checkpoint_failure));
        assert!(!is_retryable_buffer_pool_exhaustion(&budget_failure));
    }
}
