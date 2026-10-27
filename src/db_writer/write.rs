use super::extensions::preseed_ladybug_extensions;
use super::phase::{
    phase_worker_available, run_isolated_phase, LadybugWritePhase, LadybugWritePhaseRequest,
};
use super::request::{LadybugWriteMetrics, LadybugWriteRequest};
use super::rss::release_unused_allocator_pages;
use super::schema::statement_phases;
use super::{connect_ladybug_database, open_ladybug_database_with_threads};
use crate::error::NativeError;
use lbug::Connection;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub fn write_database(request: LadybugWriteRequest) -> Result<(), NativeError> {
    write_database_with_metrics(request).map(|_| ())
}

pub fn write_database_with_metrics(
    request: LadybugWriteRequest,
) -> Result<LadybugWriteMetrics, NativeError> {
    let _database_use = crate::storage::maintenance_ownership::DatabaseUseLease::acquire_shared(
        Path::new(&request.db_path),
    )?;
    if phase_worker_available() {
        write_database_isolated(&request)
    } else {
        write_database_once(&request)?;
        Ok(LadybugWriteMetrics::default())
    }
}

fn write_database_once(request: &LadybugWriteRequest) -> Result<(), NativeError> {
    prepare_candidate(request)?;
    let database = open_ladybug_database_with_threads(
        Path::new(&request.db_path),
        false,
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
        run_schema_query(&connection, statement)?;
    }
    if request.defer_hash_indexes {
        execute_deferred_index_copy(&connection, request)?;
    } else {
        for (index, statement) in request.copy_statements.iter().enumerate() {
            execute_copy(&connection, request, index, statement)?;
        }
    }
    for statement in phases.post_copy {
        run_schema_query(&connection, statement)?;
    }
    Ok(())
}

fn write_database_isolated(
    request: &LadybugWriteRequest,
) -> Result<LadybugWriteMetrics, NativeError> {
    write_database_isolated_with(request, run_phase)
}

fn write_database_isolated_with(
    request: &LadybugWriteRequest,
    mut execute: impl FnMut(&LadybugWriteRequest, LadybugWritePhase) -> Result<u64, NativeError>,
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
    let mut high_water_bytes = execute(
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
        high_water_bytes = high_water_bytes.max(execute(
            request,
            copy_phase(request, index, statement, &runtime_loads),
        )?);
        if request.defer_hash_indexes {
            let table = node_table_name(statement).ok_or_else(|| {
                NativeError::InvalidInput(format!(
                    "node COPY statement has an unsupported target: {}",
                    copy_target(statement)
                ))
            })?;
            if early_index_tables.contains(table) {
                if indexed_tables.insert(table.to_string()) {
                    high_water_bytes = high_water_bytes.max(execute(
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
            high_water_bytes.max(execute(request, LadybugWritePhase::Index { table })?);
    }
    for (offset, statement) in request.copy_statements[connector_start..]
        .iter()
        .enumerate()
    {
        high_water_bytes = high_water_bytes.max(execute(
            request,
            copy_phase(request, connector_start + offset, statement, &[]),
        )?);
    }
    for statement in phases.post_copy {
        high_water_bytes = high_water_bytes.max(execute(
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
    phase: LadybugWritePhase,
) -> LadybugWritePhaseRequest {
    LadybugWritePhaseRequest::new(&request.db_path, request.max_num_threads, phase)
}

fn copy_phase(
    request: &LadybugWriteRequest,
    index: usize,
    statement: &str,
    runtime_loads: &[String],
) -> LadybugWritePhase {
    LadybugWritePhase::Copy {
        index,
        total: request.copy_statements.len(),
        statement: statement.to_string(),
        runtime_loads: runtime_loads.to_vec(),
    }
}

fn run_phase(request: &LadybugWriteRequest, phase: LadybugWritePhase) -> Result<u64, NativeError> {
    run_isolated_phase(&phase_request(request, phase))
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

fn run_schema_query(connection: &Connection<'_>, statement: &str) -> Result<(), NativeError> {
    connection
        .query(statement)
        .map(|_| ())
        .map_err(|error| NativeError::Database(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    fn request(copy_count: usize) -> (PathBuf, LadybugWriteRequest) {
        let root = std::env::temp_dir().join(format!(
            "writer-phase-order-{}-{}-{}",
            std::process::id(),
            copy_count,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut statements = (0..copy_count)
            .map(|i| format!("COPY `Thing` FROM 'rows{i}.csv'"))
            .collect::<Vec<_>>();
        statements.push("COPY `FROM_Links` FROM 'links.csv'".into());
        let request = LadybugWriteRequest {
            db_path: root.join("candidate.lbug").to_string_lossy().to_string(),
            max_num_threads: 3,
            defer_hash_indexes: true,
            include_fts: true,
            schema_statements: vec![
                "CREATE NODE TABLE IF NOT EXISTS `Thing`(`id` STRING PRIMARY KEY)".into(),
                "CALL CREATE_FTS_INDEX('Thing', 'thing', ['id'])".into(),
            ],
            copy_statements: statements,
        };
        (root, request)
    }

    #[test]
    fn isolated_phase_schedule_stops_after_first_error_without_replay() {
        let successful_schedule = ["schema", "copy0", "index", "copy1", "copy2", "postcopy"];
        for failure_index in 0..successful_schedule.len() {
            let (root, request) = request(2);
            fs::create_dir_all(&root).unwrap();
            let calls = RefCell::new(Vec::new());
            let result = write_database_isolated_with(&request, |request, phase| {
                let ordinal = calls.borrow().len();
                let phase_request = phase_request(request, phase.clone());
                let encoded = serde_json::to_value(&phase_request).unwrap();
                assert!(encoded.get("buffer_pool_bytes").is_none());
                assert!(encoded.get("worker_memory_bytes").is_none());
                calls.borrow_mut().push(phase);
                if ordinal == failure_index {
                    Err(NativeError::Database(
                        "first failure: buffer pool is full".into(),
                    ))
                } else {
                    Ok(1)
                }
            });
            assert_eq!(
                result.unwrap_err().to_string(),
                "first failure: buffer pool is full"
            );
            let phases = calls.into_inner();
            assert_eq!(phases.len(), failure_index + 1);
            for (ordinal, phase) in phases.iter().enumerate() {
                match (successful_schedule[ordinal], phase) {
                    ("schema", LadybugWritePhase::Schema { .. }) => {}
                    ("copy0", LadybugWritePhase::Copy { index: 0, .. }) => {}
                    ("copy1", LadybugWritePhase::Copy { index: 1, .. }) => {}
                    ("copy2", LadybugWritePhase::Copy { index: 2, .. }) => {}
                    ("index", LadybugWritePhase::Index { .. }) => {}
                    ("postcopy", LadybugWritePhase::PostCopy { .. }) => {}
                    other => panic!("unexpected phase schedule row: {other:?}"),
                }
            }
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn query_errors_are_not_suppressed_by_duplicate_ddl_wording() {
        let root = std::env::temp_dir().join(format!("writer-query-test-{}", std::process::id()));
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
}
