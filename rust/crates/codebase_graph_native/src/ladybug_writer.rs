use crate::error::NativeError;
use lbug::{Connection, Database, SystemConfig};
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LadybugWriteRequest {
    pub db_path: String,
    pub include_fts: bool,
    pub schema_statements: Vec<String>,
    pub copy_statements: Vec<String>,
}

pub fn write_database(request: LadybugWriteRequest) -> Result<(), NativeError> {
    let database = Database::new(&request.db_path, SystemConfig::default())
        .map_err(|error| NativeError::Database(error.to_string()))?;
    let connection =
        Connection::new(&database).map_err(|error| NativeError::Database(error.to_string()))?;
    for statement in schema_statements(request.include_fts, request.schema_statements) {
        query_ignoring_existing(&connection, &statement)?;
    }
    for statement in request.copy_statements {
        connection
            .query(&statement)
            .map_err(|error| NativeError::Database(error.to_string()))?;
    }
    Ok(())
}

#[cfg(feature = "python-extension")]
pub(crate) fn write_database_for_python(request: LadybugWriteRequest) -> pyo3::PyResult<()> {
    write_database(request)
        .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
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

fn schema_statements(include_fts: bool, provided: Vec<String>) -> Vec<String> {
    if !provided.is_empty() {
        return provided;
    }
    let mut statements = vec!["INSTALL json".to_string(), "LOAD json".to_string()];
    if include_fts {
        statements.extend(["INSTALL fts".to_string(), "LOAD fts".to_string()]);
    }
    statements
}

#[cfg(test)]
fn copy_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_writer_loads_json_staging_through_ladybug_copy() {
        let root = unique_temp_dir("codebase-graph-native-lbug");
        fs::create_dir_all(&root).unwrap();
        let db_path = root.join("graph.lbug");
        let json_path = root.join("thing.json");
        fs::write(
            &json_path,
            r#"[{"id":"one","label":"One","metadata":{"answer":42}}]"#,
        )
        .unwrap();

        let result = write_database(LadybugWriteRequest {
            db_path: db_path.to_string_lossy().to_string(),
            include_fts: false,
            schema_statements: vec![
                "INSTALL json".to_string(),
                "LOAD json".to_string(),
                "CREATE NODE TABLE IF NOT EXISTS `Thing`(
  `id` STRING PRIMARY KEY,
  `label` STRING,
  `metadata` JSON
)"
                .to_string(),
            ],
            copy_statements: vec![format!("COPY `Thing` FROM \"{}\";", copy_path(&json_path))],
        });
        let _ = fs::remove_dir_all(&root);
        result.expect("native writer should execute JSON COPY through Ladybug");
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
