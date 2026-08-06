use super::extensions::preseed_ladybug_extensions;
use super::request::LadybugWriteRequest;
use super::schema::schema_statements;
use super::{
    connect_ladybug_database, open_ladybug_database, retry_transient_database, WRITE_RETRY_POLICY,
};
use crate::error::NativeError;
use lbug::Connection;
use std::fs;
use std::path::Path;

pub fn write_database(request: LadybugWriteRequest) -> Result<(), NativeError> {
    retry_transient_database(WRITE_RETRY_POLICY, || write_database_once(&request))
}

fn write_database_once(request: &LadybugWriteRequest) -> Result<(), NativeError> {
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
    let database = open_ladybug_database(Path::new(&request.db_path), false)?;
    let connection = connect_ladybug_database(&database)?;
    for statement in schema_statements(request.include_fts, request.schema_statements.clone()) {
        query_ignoring_existing(&connection, &statement)?;
    }
    for statement in &request.copy_statements {
        connection
            .query(statement)
            .map_err(|error| NativeError::Database(error.to_string()))?;
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
