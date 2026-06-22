use super::*;

pub(super) fn setup_fixture_repo(root: &Path) {
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
}

pub(super) fn setup_search_fixture_repo(root: &Path) {
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
}

pub(super) fn test_http_options(root: PathBuf, auth_token: Option<&str>) -> McpHttpOptions {
    McpHttpOptions {
        serve: McpServeOptions {
            repo_root: root,
            config: None,
            db: None,
            manifest: None,
            refresh: None,
        },
        host: "127.0.0.1".to_string(),
        port: 8765,
        endpoint_path: "/mcp".to_string(),
        allow_remote: false,
        auth_token: auth_token.map(str::to_string),
    }
}

pub(super) fn http_json_request(
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    payload: serde_json::Value,
) -> HttpRequest {
    let mut header_map = BTreeMap::new();
    for (name, value) in headers {
        header_map.insert(name.to_ascii_lowercase(), value.to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        headers: header_map,
        body: serde_json::to_vec(&payload).unwrap(),
        body_too_large: false,
    }
}

pub(super) fn assert_json_array_contains(value: &serde_json::Value, expected: &str) {
    assert!(
        json_array_contains(value, expected),
        "expected {value:?} to contain {expected}"
    );
}

pub(super) fn json_array_contains(value: &serde_json::Value, expected: &str) -> bool {
    value
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str() == Some(expected))
}

pub(super) fn watch_filter_for(root: &Path, extra_args: &[&str]) -> WatchEventFilter {
    fs::create_dir_all(root).unwrap();
    let source_root = root.canonicalize().unwrap();
    let mut args = vec![
        "--source-root".to_string(),
        source_root.to_string_lossy().to_string(),
        "--no-git".to_string(),
    ];
    args.extend(extra_args.iter().map(|arg| arg.to_string()));
    let options = MaterializeOptions::parse_with_command(&args, "watch").unwrap();
    WatchEventFilter::from_options(&source_root, &options).unwrap()
}

pub(super) fn watch_test_event(root: &Path, kind: EventKind, paths: &[&str]) -> Event {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    Event {
        kind,
        paths: paths.iter().map(|path| root.join(path)).collect(),
        attrs: Default::default(),
    }
}

pub(super) struct WatchTestEnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Drop for WatchTestEnvGuard {
    fn drop(&mut self) {
        env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS");
        env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE");
    }
}

pub(super) fn watch_test_env_lock() -> WatchTestEnvGuard {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS");
    env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE");
    WatchTestEnvGuard { _guard: guard }
}

pub(super) fn set_test_env(key: &str, value: &str) {
    env::set_var(key, value);
}

pub(super) fn unique_temp_dir(prefix: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

pub(super) fn unique_workspace_dir(prefix: &str) -> PathBuf {
    env::current_dir().unwrap().join(format!(
        ".{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

pub(super) fn json_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
