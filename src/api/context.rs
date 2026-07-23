use crate::api::contracts::RepoSelector;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct RepoRuntime {
    pub repo_root: PathBuf,
    pub db_path: PathBuf,
    pub manifest_path: PathBuf,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct RepoPaths {
    pub db_path: PathBuf,
    pub manifest_path: PathBuf,
}

impl RepoPaths {
    fn derive(repo_root: &Path) -> Self {
        let repo_name = safe_name(
            repo_root
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("repository"),
        );
        let state_dir = repo_root.join(".codebaseGraph");
        Self {
            db_path: state_dir.join(format!("{repo_name}_graph.ldb")),
            manifest_path: state_dir.join("manifest.json"),
        }
    }
}

pub(crate) fn resolve_runtime(selector: &RepoSelector) -> Result<RepoRuntime, String> {
    let repo_root = resolve_repository_root(selector.repo_root.as_deref())?;
    let paths = RepoPaths::derive(&repo_root);
    let config_path = selector
        .config_path
        .clone()
        .unwrap_or_else(|| repo_root.join(".codebaseGraph").join("config.json"));
    let config = if config_path.exists() {
        Some(read_json_file(&config_path)?)
    } else {
        None
    };
    Ok(RepoRuntime {
        repo_root: repo_root.clone(),
        db_path: selector
            .db_path
            .clone()
            .or_else(|| config_path_value(config.as_ref(), "database_path"))
            .unwrap_or(paths.db_path),
        manifest_path: selector
            .manifest_path
            .clone()
            .or_else(|| config_path_value(config.as_ref(), "manifest_path"))
            .unwrap_or(paths.manifest_path),
        config_path: Some(config_path),
    })
}

pub(crate) fn resolve_repository_root(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return path
            .canonicalize()
            .map_err(|error| format!("failed to resolve repo root: {error}"));
    }
    let current_dir = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    for ancestor in current_dir.ancestors() {
        let config_path = ancestor.join(".codebaseGraph").join("config.json");
        if config_path.exists() {
            let config = read_json_file(&config_path)?;
            if let Some(repo_root) = config.get("repo_root").and_then(serde_json::Value::as_str) {
                let repo_root = PathBuf::from(repo_root);
                return Ok(repo_root.canonicalize().unwrap_or(repo_root));
            }
            return Ok(ancestor.to_path_buf());
        }
    }
    if let Some(git_root) = current_dir
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
    {
        return Ok(git_root.to_path_buf());
    }
    Ok(current_dir)
}

fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}

fn config_path_value(config: Option<&serde_json::Value>, key: &str) -> Option<PathBuf> {
    config
        .and_then(|value| value.get(key))
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
}

fn safe_name(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = normalized.trim_matches(['.', '_', '-']);
    if trimmed.is_empty() {
        "repository".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_runtime, RepoPaths};
    use crate::api::contracts::RepoSelector;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn graph_paths_are_deterministic() {
        let paths = RepoPaths::derive(&std::path::PathBuf::from("/tmp/demo"));
        assert!(paths.db_path.to_string_lossy().ends_with("demo_graph.ldb"));
        assert_eq!(
            paths.manifest_path,
            std::path::PathBuf::from("/tmp/demo/.codebaseGraph/manifest.json")
        );
    }

    #[test]
    fn resolve_runtime_selects_configured_graph_and_manifest_paths() {
        let root = unique_temp_dir("codebase-graph-api-runtime");
        let state = root.join(".codebaseGraph");
        fs::create_dir_all(&state).expect("state directory should be created");
        let graph_path = state.join("configured.ldb");
        let manifest_path = state.join("configured-manifest.json");
        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "database_path": graph_path,
                "manifest_path": manifest_path,
            }))
            .expect("config should serialize"),
        )
        .expect("config should be written");

        let runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        })
        .expect("runtime should resolve");

        assert_eq!(
            runtime.repo_root,
            root.canonicalize().expect("root should canonicalize")
        );
        assert_eq!(runtime.db_path, graph_path);
        assert_eq!(runtime.manifest_path, manifest_path);
        assert_eq!(
            runtime.config_path,
            Some(runtime.repo_root.join(".codebaseGraph").join("config.json"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_runtime_prefers_explicit_graph_and_manifest_paths() {
        let root = unique_temp_dir("codebase-graph-api-runtime-explicit");
        fs::create_dir_all(&root).expect("repository should be created");
        let graph_path = root.join("explicit.ldb");
        let manifest_path = root.join("explicit-manifest.json");

        let runtime = resolve_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: Some(graph_path.clone()),
            manifest_path: Some(manifest_path.clone()),
        })
        .expect("runtime should resolve");

        assert_eq!(runtime.db_path, graph_path);
        assert_eq!(runtime.manifest_path, manifest_path);
        let _ = fs::remove_dir_all(root);
    }
}
