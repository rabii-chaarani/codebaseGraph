use super::options::HealthOptions;
use crate::cli::{setup::GraphStatePaths, util::read_json_file};
use lbug::{Connection, Database, SystemConfig, Value};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(in crate::cli) struct HealthRuntime {
    pub(in crate::cli) repo_root: PathBuf,
    pub(in crate::cli) db_path: PathBuf,
    pub(in crate::cli) manifest_path: PathBuf,
}

pub(in crate::cli) fn resolve_health_runtime(
    options: &HealthOptions,
) -> Result<HealthRuntime, String> {
    let repo_root = options
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| options.repo_root.clone());
    let default_paths = GraphStatePaths::derive(&repo_root);
    let config_path = options
        .config
        .clone()
        .unwrap_or_else(|| default_paths.config_path.clone());
    let config = if config_path.exists() {
        Some(read_json_file(&config_path)?)
    } else {
        None
    };
    let db_path = options
        .db
        .clone()
        .or_else(|| {
            config
                .as_ref()
                .and_then(|value| value.get("database_path"))
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
        })
        .unwrap_or(default_paths.db_path);
    let manifest_path = options
        .manifest
        .clone()
        .or_else(|| {
            config
                .as_ref()
                .and_then(|value| value.get("manifest_path"))
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
        })
        .unwrap_or(default_paths.manifest_path);
    Ok(HealthRuntime {
        repo_root,
        db_path,
        manifest_path,
    })
}
pub(in crate::cli) fn count_graph_nodes(db_path: &Path) -> Result<u64, String> {
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    let mut result = conn
        .query("MATCH (n) RETURN count(n) AS total_nodes LIMIT 1")
        .map_err(|error| format!("failed to query graph health: {error}"))?;
    let row = result
        .next()
        .ok_or_else(|| "graph health query returned no rows".to_string())?;
    row.first()
        .and_then(value_to_u64)
        .ok_or_else(|| "graph health query returned a non-numeric node count".to_string())
}

pub(in crate::cli) fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int64(value) if *value >= 0 => Some(*value as u64),
        Value::Int32(value) if *value >= 0 => Some(*value as u64),
        Value::Int16(value) if *value >= 0 => Some(*value as u64),
        Value::Int8(value) if *value >= 0 => Some(*value as u64),
        Value::UInt64(value) => Some(*value),
        Value::UInt32(value) => Some(u64::from(*value)),
        Value::UInt16(value) => Some(u64::from(*value)),
        Value::UInt8(value) => Some(u64::from(*value)),
        _ => None,
    }
}
