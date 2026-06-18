use crate::error::NativeError;
use crate::protocol::{ManifestDiff, NativeManifest};
use lbug::{Connection, Database, SystemConfig};
use std::collections::{BTreeMap, BTreeSet};

const DELETE_BATCH_SIZE: usize = 500;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LadybugWriteRequest {
    pub db_path: String,
    pub include_fts: bool,
    pub schema_statements: Vec<String>,
    pub replace_database: bool,
    pub delete_statements: Vec<String>,
    pub copy_statements: Vec<String>,
}

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

fn remove_existing_database(path: &str) -> Result<(), NativeError> {
    let path = std::path::Path::new(path);
    for sidecar in database_sidecar_paths(path) {
        remove_path_if_exists(&sidecar)?;
    }
    remove_path_if_exists(path)
}

fn database_sidecar_paths(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for suffix in ["wal", "tmp", "lock"] {
        paths.push(std::path::PathBuf::from(format!(
            "{}.{suffix}",
            path.to_string_lossy()
        )));
    }
    paths
}

fn remove_path_if_exists(path: &std::path::Path) -> Result<(), NativeError> {
    if !path.exists() {
        return Ok(());
    }
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| {
        NativeError::Database(format!(
            "failed to remove existing database {}: {error}",
            path.display()
        ))
    })
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

pub fn preseed_ladybug_extensions(include_fts: bool) -> Result<(), NativeError> {
    let home = ladybug_home_dir()?;
    let Some(platform) = ladybug_platform() else {
        return Ok(());
    };
    let mut extensions = vec!["json"];
    if include_fts {
        extensions.push("fts");
    }
    for extension in extensions {
        let Some(bytes) = bundled_extension_bytes(extension) else {
            continue;
        };
        let extension_dir = home
            .join(".lbdb")
            .join("extension")
            .join("0.17.0")
            .join(platform)
            .join(extension);
        let extension_path = extension_dir.join(format!("lib{extension}.lbug_extension"));
        if extension_path.exists() {
            continue;
        }
        std::fs::create_dir_all(&extension_dir)?;
        std::fs::write(extension_path, bytes)?;
    }
    Ok(())
}

fn ladybug_home_dir() -> Result<std::path::PathBuf, NativeError> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable)
        .map(std::path::PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            NativeError::Database(format!(
                "LadyBug extension cache cannot be seeded because {variable} is not set"
            ))
        })
}

fn ladybug_platform() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux_amd64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux_arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("osx_amd64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("osx_arm64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("win_amd64")
    } else {
        None
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/linux_amd64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/linux_amd64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/linux_arm64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/linux_arm64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/osx_amd64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/osx_amd64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/osx_arm64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/osx_arm64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn bundled_extension_bytes(extension: &str) -> Option<&'static [u8]> {
    match extension {
        "json" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/win_amd64/json/libjson.lbug_extension"
        )),
        "fts" => Some(include_bytes!(
            "../assets/ladybug-extensions/0.17.0/win_amd64/fts/libfts.lbug_extension"
        )),
        _ => None,
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
fn bundled_extension_bytes(_extension: &str) -> Option<&'static [u8]> {
    None
}

pub fn partition_delete_statements(
    previous_manifest: Option<&NativeManifest>,
    diff: &ManifestDiff,
) -> Vec<String> {
    let Some(manifest) = previous_manifest else {
        return Vec::new();
    };
    if diff.force_rebuild {
        return Vec::new();
    }
    let touched_paths = diff
        .deleted
        .iter()
        .chain(diff.rebuild_paths().iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    if touched_paths.is_empty() {
        return Vec::new();
    }
    let mut retained_nodes = BTreeSet::new();
    let mut retained_edges = BTreeSet::new();
    for (path, entry) in &manifest.files {
        if touched_paths.contains(path) {
            continue;
        }
        retained_nodes.extend(entry.node_ids.iter().cloned());
        retained_edges.extend(entry.edge_ids.iter().cloned());
    }
    let mut edge_deletes = Vec::new();
    let mut node_deletes = Vec::new();
    for path in touched_paths {
        let Some(entry) = manifest.files.get(&path) else {
            continue;
        };
        edge_deletes.extend(delete_edge_statements(
            &entry.edge_ids,
            &entry.edge_types,
            &retained_edges,
        ));
        node_deletes.extend(delete_node_statements(
            &entry.node_ids,
            &entry.node_types,
            &retained_nodes,
        ));
    }
    edge_deletes.extend(node_deletes);
    edge_deletes
}

fn delete_edge_statements(
    edge_ids: &[String],
    edge_types: &BTreeMap<String, String>,
    retained_edges: &BTreeSet<String>,
) -> Vec<String> {
    let mut ids_by_type: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge_id in edge_ids {
        if retained_edges.contains(edge_id) {
            continue;
        }
        let Some(edge_type) = edge_types.get(edge_id) else {
            continue;
        };
        ids_by_type
            .entry(edge_type.clone())
            .or_default()
            .push(edge_id.clone());
    }
    let mut statements = Vec::new();
    for (edge_type, mut ids) in ids_by_type {
        ids.sort();
        let edge_table = quote_identifier(&edge_type);
        let from_table = quote_identifier(&format!("FROM_{edge_type}"));
        let to_table = quote_identifier(&format!("TO_{edge_type}"));
        for chunk in ids.chunks(DELETE_BATCH_SIZE) {
            let id_list = cypher_string_list(chunk);
            statements.push(format!(
                "MATCH ()-[r:{from_table}]->(edge:{edge_table}) WHERE edge.id IN [{id_list}] DELETE r"
            ));
            statements.push(format!(
                "MATCH (edge:{edge_table})-[r:{to_table}]->() WHERE edge.id IN [{id_list}] DELETE r"
            ));
            statements.push(format!(
                "MATCH (edge:{edge_table}) WHERE edge.id IN [{id_list}] DELETE edge"
            ));
        }
    }
    statements
}

fn delete_node_statements(
    node_ids: &[String],
    node_types: &BTreeMap<String, String>,
    retained_nodes: &BTreeSet<String>,
) -> Vec<String> {
    let mut ids_by_type: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node_id in node_ids {
        if retained_nodes.contains(node_id) {
            continue;
        }
        let Some(node_type) = node_types.get(node_id) else {
            continue;
        };
        ids_by_type
            .entry(node_type.clone())
            .or_default()
            .push(node_id.clone());
    }
    let mut statements = Vec::new();
    for (node_type, mut ids) in ids_by_type {
        ids.sort();
        let node_table = quote_identifier(&node_type);
        for chunk in ids.chunks(DELETE_BATCH_SIZE) {
            statements.push(format!(
                "MATCH (node:{node_table}) WHERE node.id IN [{}] DELETE node",
                cypher_string_list(chunk)
            ));
        }
    }
    statements
}

fn quote_identifier(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn cypher_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn cypher_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", cypher_string(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
fn copy_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ManifestDiff, ManifestEntry, NativeManifest};
    use std::collections::BTreeMap;

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
            replace_database: false,
            delete_statements: Vec::new(),
            copy_statements: vec![format!("COPY `Thing` FROM \"{}\";", copy_path(&json_path))],
        });
        let _ = fs::remove_dir_all(&root);
        result.expect("native writer should execute JSON COPY through Ladybug");
    }

    #[test]
    fn partition_delete_statements_skip_retained_shared_ids() {
        let mut files = BTreeMap::new();
        files.insert(
            "changed.py".to_string(),
            entry(
                &["Function:changed", "Symbol:shared"],
                &[
                    ("Function:changed", "Function"),
                    ("Symbol:shared", "Symbol"),
                ],
                &["Contains:changed", "References:shared"],
                &[
                    ("Contains:changed", "Contains"),
                    ("References:shared", "References"),
                ],
            ),
        );
        files.insert(
            "unchanged.py".to_string(),
            entry(
                &["Function:unchanged", "Symbol:shared"],
                &[
                    ("Function:unchanged", "Function"),
                    ("Symbol:shared", "Symbol"),
                ],
                &["References:shared"],
                &[("References:shared", "References")],
            ),
        );
        let manifest = NativeManifest {
            schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "test".to_string(),
            files,
        };
        let statements = partition_delete_statements(
            Some(&manifest),
            &ManifestDiff {
                added: Vec::new(),
                modified: vec!["changed.py".to_string()],
                unchanged: vec!["unchanged.py".to_string()],
                deleted: Vec::new(),
                force_rebuild: false,
            },
        );
        let joined = statements.join("\n");

        assert!(joined.contains("Function:changed"));
        assert!(joined.contains("Contains:changed"));
        assert!(!joined.contains("Symbol:shared"));
        assert!(!joined.contains("References:shared"));
    }

    fn entry(
        node_ids: &[&str],
        node_types: &[(&str, &str)],
        edge_ids: &[&str],
        edge_types: &[(&str, &str)],
    ) -> ManifestEntry {
        ManifestEntry {
            path: "path.py".to_string(),
            content_hash: "hash".to_string(),
            language: "python".to_string(),
            partition_id: "partition".to_string(),
            node_ids: node_ids.iter().map(|value| value.to_string()).collect(),
            edge_ids: edge_ids.iter().map(|value| value.to_string()).collect(),
            node_types: node_types
                .iter()
                .map(|(id, table)| (id.to_string(), table.to_string()))
                .collect(),
            edge_types: edge_types
                .iter()
                .map(|(id, table)| (id.to_string(), table.to_string()))
                .collect(),
            materialized_at: "now".to_string(),
        }
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
