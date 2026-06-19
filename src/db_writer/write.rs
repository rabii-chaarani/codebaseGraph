use super::cleanup::remove_existing_database;
use super::extensions::preseed_ladybug_extensions;
use super::request::LadybugWriteRequest;
use super::schema::schema_statements;
use crate::error::NativeError;
use lbug::{Connection, Database, SystemConfig};

pub fn write_database(request: LadybugWriteRequest) -> Result<(), NativeError> {
    preseed_ladybug_extensions(request.include_fts)?;
    if request.replace_database {
        remove_existing_database(&request.db_path)?;
    }
    let database = Database::new(&request.db_path, SystemConfig::default())
        .map_err(|error| NativeError::Database(error.to_string()))?;
    let connection =
        Connection::new(&database).map_err(|error| NativeError::Database(error.to_string()))?;
    for statement in schema_statements(request.include_fts, request.schema_statements) {
        query_ignoring_existing(&connection, &statement)?;
    }
    for statement in request.delete_statements {
        query_ignoring_missing(&connection, &statement)?;
    }
    for statement in request.copy_statements {
        connection
            .query(&statement)
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

fn query_ignoring_missing(connection: &Connection<'_>, statement: &str) -> Result<(), NativeError> {
    match connection.query(statement) {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string().to_lowercase();
            if message.contains("does not exist")
                || message.contains("not found")
                || message.contains("no such")
            {
                Ok(())
            } else {
                Err(NativeError::Database(error.to_string()))
            }
        }
    }
}
