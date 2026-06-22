use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub(super) fn snapshot_file(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("failed to snapshot {}: {error}", path.display()))
}

pub(super) fn restore_file(path: &Path, previous: Option<&str>) -> Result<(), String> {
    match previous {
        Some(text) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("failed to restore directory {}: {error}", parent.display())
                })?;
            }
            fs::write(path, text)
                .map_err(|error| format!("failed to restore {}: {error}", path.display()))
        }
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
        },
    }
}

pub(super) fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}

pub(super) fn resolve_repo_root(explicit: Option<&Path>) -> Result<PathBuf, String> {
    resolve_root(explicit, "repo root")
}

pub(super) fn resolve_source_root(explicit: Option<&Path>) -> Result<PathBuf, String> {
    resolve_root(explicit, "source root")
}

fn resolve_root(explicit: Option<&Path>, label: &str) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return path
            .canonicalize()
            .map_err(|error| format!("failed to resolve {label}: {error}"));
    }

    let current_dir =
        env::current_dir().map_err(|error| format!("failed to read current directory: {error}"))?;
    if let Some(root) = discover_codebase_graph_root(&current_dir)? {
        return Ok(root);
    }
    if let Some(root) = discover_git_root(&current_dir) {
        return Ok(root);
    }
    Ok(current_dir)
}

fn discover_codebase_graph_root(start: &Path) -> Result<Option<PathBuf>, String> {
    for ancestor in start.ancestors() {
        let config_path = ancestor.join(".codebaseGraph").join("config.json");
        if !config_path.exists() {
            continue;
        }
        let config = read_json_file(&config_path)?;
        if let Some(repo_root) = config.get("repo_root").and_then(serde_json::Value::as_str) {
            let path = PathBuf::from(repo_root);
            return Ok(Some(path.canonicalize().unwrap_or(path)));
        }
        return Ok(Some(ancestor.to_path_buf()));
    }
    Ok(None)
}

fn discover_git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

pub(super) fn required_arg<'a>(
    args: &'a [String],
    index: usize,
    name: &str,
) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} requires a value"))
}

pub(super) fn parse_usize_arg(args: &[String], index: usize, name: &str) -> Result<usize, String> {
    required_arg(args, index, name)?
        .parse::<usize>()
        .map_err(|error| format!("{name} must be an integer: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn resolve_repo_root_uses_explicit_path() {
        let root = unique_temp_dir("codebase-graph-explicit-root");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_repo_root(Some(&nested)).unwrap();

        assert_eq!(resolved, nested.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_root_finds_codebase_graph_config_ancestor() {
        let root = unique_temp_dir("codebase-graph-config-root");
        let nested = root.join("src").join("cli");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join(".codebaseGraph").join("config.json"), "{}").unwrap();

        let resolved = resolve_root_from(&nested).unwrap();

        assert_eq!(resolved, root.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_root_uses_repo_root_from_config() {
        let root = unique_temp_dir("codebase-graph-config-repo-root");
        let real_root = root.join("real");
        let command_root = root.join("command");
        let nested = command_root.join("src");
        fs::create_dir_all(&real_root).unwrap();
        fs::create_dir_all(command_root.join(".codebaseGraph")).unwrap();
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            command_root.join(".codebaseGraph").join("config.json"),
            serde_json::to_string(&serde_json::json!({
                "repo_root": real_root
            }))
            .unwrap(),
        )
        .unwrap();

        let resolved = resolve_root_from(&nested).unwrap();

        assert_eq!(resolved, real_root.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_root_falls_back_to_git_ancestor() {
        let root = unique_temp_dir("codebase-graph-git-root");
        let nested = root.join("src").join("cli");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_root_from(&nested).unwrap();

        assert_eq!(resolved, root.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_root_falls_back_to_start_directory() {
        let root = unique_temp_dir("codebase-graph-current-root");
        fs::create_dir_all(&root).unwrap();

        let resolved = resolve_root_from(&root).unwrap();

        assert_eq!(resolved, root.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    fn resolve_root_from(start: &Path) -> Result<PathBuf, String> {
        let start = start
            .canonicalize()
            .map_err(|error| format!("failed to resolve test root: {error}"))?;
        if let Some(root) = discover_codebase_graph_root(&start)? {
            return Ok(root);
        }
        if let Some(root) = discover_git_root(&start) {
            return Ok(root);
        }
        Ok(start)
    }
}
