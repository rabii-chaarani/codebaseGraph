use crate::api::catalog::schema_statements_from_copy_statements;
use crate::db_writer::{write_database_with_metrics, LadybugWriteMetrics, LadybugWriteRequest};
use crate::error::NativeError;
use crate::protocol::NativeSyntaxMaterializationRequest;

pub(crate) fn write_graph_rows(
    request: &NativeSyntaxMaterializationRequest,
    copy_statements: &[String],
    has_external_search_backend: bool,
) -> Result<LadybugWriteMetrics, NativeError> {
    let use_ladybug_fts = request.include_fts && !has_external_search_backend;
    let schema_statements = if request.schema_statements.is_empty() {
        schema_statements_from_copy_statements(use_ladybug_fts, copy_statements)
    } else {
        request.schema_statements.clone()
    };
    let buffer_pool_bytes = request.database_buffer_pool_bytes()?;
    // Ladybug COPY uses additional native working memory per execution thread.
    // Materialization parallelism is bounded separately; keep the database phase
    // single-threaded so its 256 MiB buffer pool remains the dominant allocation.
    let max_num_threads = 1;
    write_database_with_metrics(LadybugWriteRequest {
        db_path: request.db_path.clone(),
        worker_memory_bytes: request.worker_memory_mib.saturating_mul(1024 * 1024),
        buffer_pool_bytes,
        max_num_threads,
        defer_hash_indexes: true,
        include_fts: use_ladybug_fts,
        schema_statements,
        copy_statements: copy_statements.to_vec(),
    })
}
