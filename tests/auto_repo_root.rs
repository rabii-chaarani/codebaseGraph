use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_codebase-graph")
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

fn run_cli<I, S>(cwd: &Path, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn canonical_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().canonicalize().unwrap()
}

fn setup_search_repo(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();
    let output = run_cli(
        root,
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-semantic-enrichment",
            "--json",
        ],
    );
    assert_success(&output);
}

#[test]
fn graph_commands_auto_detect_installed_repo_root_from_nested_directory() {
    let root = unique_temp_dir("codebase-graph-auto-root-graph");
    let nested = root.join("src").join("cli");
    fs::create_dir_all(&nested).unwrap();
    setup_search_repo(&root);

    let search = run_cli(
        &nested,
        ["codebase-search", "SampleService", "--limit", "1", "--json"],
    );
    assert_success(&search);
    let search_value: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap();
    assert_eq!(search_value["results"][0]["label"], "SampleService");

    let health = run_cli(&nested, ["check-health", "--json"]);
    assert_success(&health);
    let health_value: serde_json::Value = serde_json::from_slice(&health.stdout).unwrap();
    assert_eq!(health_value["ok"], true);
    assert_eq!(
        canonical_path(PathBuf::from(health_value["repo_root"].as_str().unwrap())),
        canonical_path(&root)
    );

    let query = run_cli(
        &nested,
        [
            "graph-query",
            "MATCH (n) WHERE n.path = $path RETURN n.path LIMIT 1",
            "--parameters",
            r#"{"path":"service.py"}"#,
            "--json",
        ],
    );
    assert_success(&query);
    let query_value: serde_json::Value = serde_json::from_slice(&query.stdout).unwrap();
    assert_eq!(query_value["rows"][0][0], "service.py");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn build_and_plan_auto_detect_git_root_from_nested_directory() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let root = unique_temp_dir("codebase-graph-auto-root-build");
    let nested = root.join("src").join("cli");
    fs::create_dir_all(&nested).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    assert!(Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap()
        .status
        .success());

    let build = run_cli(
        &nested,
        [
            "build",
            "--mode",
            "full",
            "--no-git",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
    );
    assert_success(&build);
    assert!(root.join(".codebaseGraph").join("manifest.json").exists());
    assert!(!nested.join(".codebaseGraph").exists());

    fs::write(root.join("new.py"), "def new():\n    return 2\n").unwrap();
    let plan = run_cli(&nested, ["plan", "--no-git", "--json"]);
    assert_success(&plan);
    let plan_value: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert!(plan_value["would_rebuild"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == "new.py"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_auto_detects_git_root_from_nested_directory() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let root = unique_temp_dir("codebase-graph-auto-root-install");
    let nested = root.join("src").join("cli");
    fs::create_dir_all(&nested).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    assert!(Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap()
        .status
        .success());

    let install = run_cli(
        &nested,
        [
            "install",
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
    );
    assert_success(&install);
    let install_value: serde_json::Value = serde_json::from_slice(&install.stdout).unwrap();
    assert_eq!(
        canonical_path(PathBuf::from(install_value["repo_root"].as_str().unwrap())),
        canonical_path(&root)
    );
    assert!(root.join(".codebaseGraph").join("config.json").exists());
    assert!(!nested.join(".codebaseGraph").exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_commands_auto_detect_installed_repo_root_from_nested_directory() {
    let root = unique_temp_dir("codebase-graph-auto-root-mcp");
    let nested = root.join("src").join("cli");
    fs::create_dir_all(&nested).unwrap();
    setup_search_repo(&root);

    let client_config = root.join("client").join("mcp.json");
    let install = run_cli(
        &nested,
        [
            "mcp",
            "install",
            "--client",
            "generic",
            "--client-config-path",
            client_config.to_str().unwrap(),
            "--dry-run",
            "--json",
        ],
    );
    assert_success(&install);
    let install_value: serde_json::Value = serde_json::from_slice(&install.stdout).unwrap();
    assert_eq!(
        canonical_path(PathBuf::from(
            install_value["descriptor"]["repo_root"].as_str().unwrap()
        )),
        canonical_path(&root)
    );

    let mut start = Command::new(bin())
        .args(["mcp", "start"])
        .current_dir(&nested)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": "2025-11-25"}
    });
    let request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "graph_health",
            "arguments": {"include_structured_content": true}
        }
    });
    {
        use std::io::Write;
        let stdin = start.stdin.as_mut().unwrap();
        writeln!(stdin, "{initialize}").unwrap();
        writeln!(stdin, "{request}").unwrap();
    }
    let output = start.wait_with_output().unwrap();
    assert_success(&output);
    let responses = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<serde_json::Value>, _>>()
        .unwrap();
    let response = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("graph_health response should be present");
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["structuredContent"]["ok"], true);

    let _ = fs::remove_dir_all(root);
}
