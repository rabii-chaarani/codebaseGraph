use crate::api::catalog::schema_statements_from_copy_statements;
use crate::db_writer::{
    incoming_row_delete_statements, partition_delete_statements, write_database,
    LadybugWriteRequest,
};
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};

pub(crate) fn write_graph_rows(
    request: &NativeSyntaxMaterializationRequest,
    response: &NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    let schema_statements = if request.schema_statements.is_empty() {
        schema_statements_from_copy_statements(request.include_fts, &response.copy_statements)
    } else {
        request.schema_statements.clone()
    };
    let mut delete_statements =
        partition_delete_statements(request.previous_manifest.as_ref(), &response.diff);
    delete_statements.extend(incoming_row_delete_statements(
        request.previous_manifest.as_ref(),
        &response.diff,
        &response.rebuilt_entries,
    ));
    write_database(LadybugWriteRequest {
        db_path: request.db_path.clone(),
        include_fts: request.include_fts,
        schema_statements,
        replace_database: response.diff.force_rebuild,
        delete_statements,
        copy_statements: response.copy_statements.clone(),
    })
    .map_err(|error| error.to_string())
}
