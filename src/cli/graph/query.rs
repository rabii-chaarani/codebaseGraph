use crate::db_writer::{
    connect_ladybug_database, open_ladybug_database, retry_transient_database, READ_RETRY_POLICY,
};
use lbug::Value;
use serde_json::json;
use std::path::Path;

pub(in crate::cli) fn span_json(
    line_start: Option<i64>,
    line_end: Option<i64>,
) -> serde_json::Value {
    let mut span = serde_json::Map::new();
    if let Some(line_start) = line_start {
        span.insert("line_start".to_string(), json!(line_start));
    }
    if let Some(line_end) = line_end {
        span.insert("line_end".to_string(), json!(line_end));
    }
    serde_json::Value::Object(span)
}

pub(in crate::cli) fn cypher_single_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

pub(in crate::cli) fn cypher_identifier(value: &str) -> String {
    value.replace('`', "``")
}

pub(in crate::cli) fn value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Int64(value)) => value.to_string(),
        Some(Value::UInt64(value)) => value.to_string(),
        Some(Value::Int32(value)) => value.to_string(),
        Some(Value::UInt32(value)) => value.to_string(),
        Some(Value::Null(_)) | None => String::new(),
        Some(value) => value.to_string(),
    }
}

pub(in crate::cli) fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int64(value)) => Some(*value),
        Some(Value::Int32(value)) => Some(i64::from(*value)),
        Some(Value::Int16(value)) => Some(i64::from(*value)),
        Some(Value::Int8(value)) => Some(i64::from(*value)),
        Some(Value::UInt64(value)) => i64::try_from(*value).ok(),
        Some(Value::UInt32(value)) => Some(i64::from(*value)),
        Some(Value::UInt16(value)) => Some(i64::from(*value)),
        Some(Value::UInt8(value)) => Some(i64::from(*value)),
        _ => None,
    }
}

pub(in crate::cli) fn value_to_f64(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Double(value)) => *value,
        Some(Value::Float(value)) => f64::from(*value),
        Some(Value::Int64(value)) => *value as f64,
        Some(Value::UInt64(value)) => *value as f64,
        Some(Value::Int32(value)) => f64::from(*value),
        Some(Value::UInt32(value)) => f64::from(*value),
        _ => 0.0,
    }
}

pub(in crate::cli) fn validate_read_only_statement(statement: &str) -> Result<(), String> {
    let stripped = statement.trim().trim_end_matches(';');
    if stripped.contains(';') {
        return Err("graph_query accepts one read-only statement at a time".to_string());
    }
    for keyword in [
        "ALTER", "ATTACH", "CALL", "COPY", "CREATE", "DELETE", "DETACH", "DROP", "EXPORT",
        "IMPORT", "INSERT", "INSTALL", "LOAD", "MERGE", "REMOVE", "RENAME", "SET", "TRUNCATE",
        "UPDATE", "USE",
    ] {
        if contains_keyword(stripped, keyword) {
            return Err(format!(
                "graph_query is read-only; blocked keyword: {keyword}"
            ));
        }
    }
    Ok(())
}

pub(in crate::cli) fn contains_keyword(statement: &str, keyword: &str) -> bool {
    let uppercase = statement.to_ascii_uppercase();
    let mut search_start = 0;
    while let Some(index) = uppercase[search_start..].find(keyword) {
        let absolute_index = search_start + index;
        let before = uppercase[..absolute_index]
            .chars()
            .next_back()
            .map(is_keyword_char)
            .unwrap_or(false);
        let after = uppercase[absolute_index + keyword.len()..]
            .chars()
            .next()
            .map(is_keyword_char)
            .unwrap_or(false);
        if !before && !after {
            return true;
        }
        search_start = absolute_index + keyword.len();
    }
    false
}

pub(in crate::cli) fn is_keyword_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

pub(in crate::cli) fn execute_read_only_query(
    db_path: &Path,
    statement: &str,
    parameters: &serde_json::Map<String, serde_json::Value>,
    limit: usize,
) -> Result<(Vec<Vec<serde_json::Value>>, bool), String> {
    let db = retry_transient_database(READ_RETRY_POLICY, || open_ladybug_database(db_path, true))
        .map_err(|error| error.to_string())?;
    let conn = connect_ladybug_database(&db).map_err(|error| error.to_string())?;
    let mut result = if parameters.is_empty() {
        conn.query(statement)
            .map_err(|error| format!("failed to execute graph query: {error}"))?
    } else {
        let named_parameters = lbug_query_parameters(parameters)?;
        let mut prepared = conn
            .prepare(statement)
            .map_err(|error| format!("failed to prepare graph query: {error}"))?;
        if !prepared.is_read_only() {
            return Err("graph-query prepared statement is not read-only".to_string());
        }
        let execute_parameters = named_parameters
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        conn.execute(&mut prepared, execute_parameters)
            .map_err(|error| format!("failed to execute graph query: {error}"))?
    };
    let mut rows = Vec::new();
    let mut truncated = false;
    for row in result.by_ref().take(limit + 1) {
        if rows.len() == limit {
            truncated = true;
            break;
        }
        rows.push(row.into_iter().map(json_safe_value).collect());
    }
    Ok((rows, truncated))
}

pub(in crate::cli) fn lbug_query_parameters(
    parameters: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<(String, Value)>, String> {
    let mut converted = Vec::with_capacity(parameters.len());
    for (name, value) in parameters {
        if name.trim().is_empty() {
            return Err("graph_query parameter names must not be blank".to_string());
        }
        converted.push((name.clone(), json_parameter_to_lbug_value(value)?));
    }
    Ok(converted)
}

pub(in crate::cli) fn json_parameter_to_lbug_value(
    value: &serde_json::Value,
) -> Result<Value, String> {
    match value {
        serde_json::Value::Bool(value) => Ok(Value::Bool(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Int64(value))
            } else if let Some(value) = value.as_u64() {
                Ok(Value::UInt64(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Double(value))
            } else {
                Err("graph_query numeric parameter is not representable".to_string())
            }
        }
        serde_json::Value::String(value) => Ok(Value::String(value.clone())),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Ok(Value::Json(value.clone()))
        }
    }
}

pub(in crate::cli) fn json_safe_value(value: Value) -> serde_json::Value {
    match value {
        Value::Null(_) => serde_json::Value::Null,
        Value::Bool(value) => json!(value),
        Value::Int64(value) => json!(value),
        Value::Int32(value) => json!(value),
        Value::Int16(value) => json!(value),
        Value::Int8(value) => json!(value),
        Value::UInt64(value) => json!(value),
        Value::UInt32(value) => json!(value),
        Value::UInt16(value) => json!(value),
        Value::UInt8(value) => json!(value),
        Value::Int128(value) => json!(value.to_string()),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| json!(value.to_string())),
        Value::Float(value) => serde_json::Number::from_f64(f64::from(value))
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| json!(value.to_string())),
        Value::String(value) => json!(value),
        Value::Json(value) => value,
        Value::List(_, values) | Value::Array(_, values) => {
            serde_json::Value::Array(values.into_iter().map(json_safe_value).collect())
        }
        Value::Struct(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, json_safe_value(value)))
                .collect(),
        ),
        other => json!(other.to_string()),
    }
}
