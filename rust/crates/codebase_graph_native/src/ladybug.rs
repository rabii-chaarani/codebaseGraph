use crate::error::NativeError;
use lbug::{Connection, Database, LogicalType, SystemConfig, Value as LbugValue};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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
        if is_json_extension_statement(&statement) {
            continue;
        }
        query_ignoring_existing(&connection, &statement)?;
    }
    for statement in request.copy_statements {
        if execute_json_copy_statement(&connection, &statement)? {
            continue;
        }
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

fn is_json_extension_statement(statement: &str) -> bool {
    matches!(
        statement.trim().to_ascii_uppercase().as_str(),
        "INSTALL JSON" | "LOAD JSON"
    )
}

fn execute_json_copy_statement(
    connection: &Connection<'_>,
    statement: &str,
) -> Result<bool, NativeError> {
    let Some((table_expression, path)) = parse_simple_json_copy_statement(statement) else {
        return Ok(false);
    };
    let rows = read_json_rows(&path)?;
    insert_json_rows(connection, &table_expression, &rows)?;
    Ok(true)
}

fn parse_simple_json_copy_statement(statement: &str) -> Option<(String, PathBuf)> {
    let trimmed = statement.trim();
    let body = trimmed.strip_suffix(';')?.trim();
    let marker = " FROM \"";
    let from_index = body.find(marker)?;
    let copy_prefix = body[..from_index].trim();
    let table_expression = copy_prefix.strip_prefix("COPY ")?.trim();
    let path_start = from_index + marker.len();
    let path_tail = &body[path_start..];
    let path_end = path_tail.find('"')?;
    let path_text = &path_tail[..path_end];
    if !path_text.ends_with(".json") || !path_tail[path_end + 1..].trim().is_empty() {
        return None;
    }
    Some((table_expression.to_string(), PathBuf::from(path_text)))
}

fn read_json_rows(path: &Path) -> Result<Vec<BTreeMap<String, JsonValue>>, NativeError> {
    let data = fs::read_to_string(path)?;
    serde_json::from_str(&data).map_err(NativeError::from)
}

fn insert_json_rows(
    connection: &Connection<'_>,
    table_expression: &str,
    rows: &[BTreeMap<String, JsonValue>],
) -> Result<(), NativeError> {
    let mut columns = BTreeSet::new();
    for row in rows {
        columns.extend(row.keys().cloned());
    }
    let columns = columns.into_iter().collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok(());
    }
    let properties = columns
        .iter()
        .map(|column| format!("{column}: ${column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!("CREATE (:{table_expression} {{{properties}}});");
    let mut prepared = connection
        .prepare(&query)
        .map_err(|error| NativeError::Database(error.to_string()))?;
    for row in rows {
        let values = columns
            .iter()
            .map(|column| {
                (
                    column.clone(),
                    lbug_value(column, row.get(column).unwrap_or(&JsonValue::Null)),
                )
            })
            .collect::<Vec<_>>();
        let params = values
            .iter()
            .map(|(column, value)| (column.as_str(), value.clone()))
            .collect::<Vec<_>>();
        connection
            .execute(&mut prepared, params)
            .map_err(|error| NativeError::Database(error.to_string()))?;
    }
    Ok(())
}

fn lbug_value(column: &str, value: &JsonValue) -> LbugValue {
    match value {
        JsonValue::Null => LbugValue::Null(null_type_for_column(column)),
        JsonValue::Bool(value) => LbugValue::Bool(*value),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                LbugValue::Int64(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                LbugValue::Int64(value)
            } else if let Some(value) = value.as_f64() {
                LbugValue::Double(value)
            } else {
                LbugValue::Null(null_type_for_column(column))
            }
        }
        JsonValue::String(value) => LbugValue::String(value.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => LbugValue::Json(value.clone()),
    }
}

fn null_type_for_column(column: &str) -> LogicalType {
    match column {
        "metadata" => LogicalType::Json,
        "line_start" | "line_end" | "byte_start" | "byte_end" | "size_bytes" => LogicalType::Int64,
        "confidence" => LogicalType::Double,
        _ => LogicalType::String,
    }
}

#[cfg(test)]
fn copy_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_writer_loads_json_staging_without_json_extension() {
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
        result.unwrap();
    }

    #[test]
    fn recognizes_simple_json_copy_statement() {
        let root = unique_temp_dir("codebase-graph-native-parse-copy");
        let json_path = root.join("file.json");
        let parsed = parse_simple_json_copy_statement(&format!(
            "COPY `File` FROM \"{}\";",
            copy_path(&json_path)
        ))
        .unwrap();

        assert_eq!(parsed.0, "`File`");
        assert_eq!(parsed.1, json_path);
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
