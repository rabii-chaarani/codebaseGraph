use crate::api::catalog::schema_statements_from_copy_statements;
use crate::db_writer::{write_database, LadybugWriteRequest};
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
    write_database(LadybugWriteRequest {
        db_path: request.db_path.clone(),
        include_fts: request.include_fts,
        schema_statements,
        copy_statements: response.copy_statements.clone(),
    })
    .map_err(|error| error.to_string())
}
