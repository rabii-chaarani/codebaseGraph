use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CONFIRMATIONS: &[&str] = &[
    "release-environment",
    "hosted-ci-green",
    "private-vulnerability-reporting",
];

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("release-gate") => release_gate(args.collect()),
        Some("smoke-artifact") => {
            let executable = args
                .next()
                .ok_or_else(|| "smoke-artifact requires a binary path".to_string())?;
            smoke_artifact(Path::new(&executable))
        }
        Some("smoke-wiki-artifact") => {
            let executable = args
                .next()
                .ok_or_else(|| "smoke-wiki-artifact requires a binary path".to_string())?;
            smoke_wiki_artifact(Path::new(&executable))
        }
        Some("verify-release-version") => {
            let tag = args
                .next()
                .ok_or_else(|| "verify-release-version requires a vX.Y.Z tag".to_string())?;
            verify_release_version(&tag)
        }
        Some(command) => Err(format!("unknown xtask command: {command}")),
        None => Err(
            "usage: cargo run -p xtask -- <release-gate|smoke-artifact|smoke-wiki-artifact|verify-release-version>"
                .to_string(),
        ),
    }
}

fn release_gate(args: Vec<String>) -> Result<(), String> {
    let mut production = false;
    let mut require_conda = false;
    let mut confirmations = BTreeSet::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--production" => production = true,
            "--require-conda" => require_conda = true,
            "--confirm" => {
                index += 1;
                let flag = args
                    .get(index)
                    .ok_or_else(|| "--confirm requires a value".to_string())?;
                confirmations.insert(flag.clone());
            }
            other => return Err(format!("unknown release-gate option: {other}")),
        }
        index += 1;
    }

    let mut issues = Vec::new();
    check_security_policy(&mut issues);
    check_rust_only_files(&mut issues);
    check_cargo_metadata(&mut issues);
    check_release_version_sync(&mut issues);
    check_release_please_config(&mut issues);
    check_workflows(&mut issues);
    check_no_legacy_surfaces(&mut issues);
    if require_conda {
        check_conda_recipe(&mut issues);
    }
    if production {
        for flag in CONFIRMATIONS {
            if !confirmations.contains(*flag) {
                issues.push(format!("FAIL: external-confirmation-missing: production release requires --confirm {flag}."));
            }
        }
    }

    if issues.is_empty() {
        println!("release gate passed");
        Ok(())
    } else {
        for issue in &issues {
            eprintln!("{issue}");
        }
        Err("release gate failed".to_string())
    }
}

fn check_security_policy(issues: &mut Vec<String>) {
    let path = Path::new("SECURITY.md");
    let Ok(text) = fs::read_to_string(path) else {
        issues.push("FAIL: security-policy-missing: SECURITY.md is required.".to_string());
        return;
    };
    for required in ["Reporting a Vulnerability", "graph_query", "--allow-remote"] {
        if !text.contains(required) {
            issues.push(format!(
                "FAIL: security-policy-incomplete: SECURITY.md must mention {required:?}."
            ));
        }
    }
}

fn check_rust_only_files(issues: &mut Vec<String>) {
    for forbidden in ["pyproject.toml", "requirements-dev.txt"] {
        if Path::new(forbidden).exists() {
            issues.push(format!(
                "FAIL: python-tooling-present: {forbidden} must not exist."
            ));
        }
    }
    for directory in ["scripts", "src/codebase_graph"] {
        if Path::new(directory).exists() {
            issues.push(format!(
                "FAIL: python-surface-present: {directory} must not exist."
            ));
        }
    }
    for path in files_under(Path::new(".")) {
        let relative = path.strip_prefix(".").unwrap_or(&path);
        let text_path = relative.to_string_lossy();
        if text_path.contains("/target/")
            || text_path.contains("/.git/")
            || text_path.contains("/.codebaseGraph/")
            || text_path.contains("/.kwiki/")
        {
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "py") {
            issues.push(format!(
                "FAIL: python-file-present: {} must not be maintained source.",
                text_path
            ));
        }
    }
}

fn check_cargo_metadata(issues: &mut Vec<String>) {
    let Ok(cargo) = fs::read_to_string("Cargo.toml") else {
        issues.push("FAIL: cargo-missing: root Cargo.toml is required.".to_string());
        return;
    };
    for required in [
        r#"name = "codebase-graph""#,
        r#"name = "codebase_graph""#,
        r#"name = "codebase-graph""#,
        r#"license = "MIT""#,
        r#"repository = "https://github.com/rabii-chaarani/codebaseGraph""#,
        r#"readme = "README.md""#,
    ] {
        if !cargo.contains(required) {
            issues.push(format!(
                "FAIL: cargo-metadata-incomplete: Cargo.toml must contain {required}."
            ));
        }
    }
    let wiki_manifest = Path::new("crates/k-wiki/Cargo.toml");
    match fs::read_to_string(wiki_manifest) {
        Ok(wiki) => {
            for required in [
                r#"name = "k-wiki""#,
                r#"name = "k_wiki""#,
                r#"name = "k-wiki""#,
                r#"license = "MIT""#,
            ] {
                if !wiki.contains(required) {
                    issues.push(format!(
                        "FAIL: cargo-metadata-incomplete: {} must contain {required}.",
                        wiki_manifest.display()
                    ));
                }
            }
        }
        Err(_) => issues.push(format!(
            "FAIL: cargo-missing: {} is required.",
            wiki_manifest.display()
        )),
    }
}

fn check_release_version_sync(issues: &mut Vec<String>) {
    let root = match cargo_version(Path::new("Cargo.toml")) {
        Ok(version) => version,
        Err(error) => {
            issues.push(format!(
                "FAIL: cargo-version-missing: could not read root version: {error}."
            ));
            return;
        }
    };
    let wiki = match cargo_version(Path::new("crates/k-wiki/Cargo.toml")) {
        Ok(version) => version,
        Err(error) => {
            issues.push(format!(
                "FAIL: cargo-version-missing: could not read k-wiki version: {error}."
            ));
            return;
        }
    };
    if root != wiki {
        issues.push(format!(
            "FAIL: release-version-divergence: root Cargo.toml version {root} does not match crates/k-wiki/Cargo.toml version {wiki}."
        ));
    }
    match dependency_version(Path::new("crates/k-wiki/Cargo.toml"), "codebase-graph") {
        Ok(version) if version != root => issues.push(format!(
            "FAIL: release-version-divergence: crates/k-wiki/Cargo.toml depends on codebase-graph {version} but root Cargo.toml is {root}."
        )),
        Ok(_) => {}
        Err(error) => issues.push(format!(
            "FAIL: cargo-version-missing: could not read crates/k-wiki dependency version: {error}."
        )),
    }
}

fn check_release_please_config(issues: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string("release-please-config.json") else {
        issues.push(
            "FAIL: release-please-config-missing: release-please-config.json is required."
                .to_string(),
        );
        return;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
        issues.push(
            "FAIL: release-please-config-invalid: release-please-config.json must be valid JSON."
                .to_string(),
        );
        return;
    };
    let Some(extra_files) = parsed
        .get("packages")
        .and_then(|packages| packages.get("."))
        .and_then(|root| root.get("extra-files"))
        .and_then(Value::as_array)
    else {
        issues.push(
            "FAIL: release-please-config-incomplete: root package must declare extra-files for crates/k-wiki/Cargo.toml."
                .to_string(),
        );
        return;
    };
    let required = [
        ("crates/k-wiki/Cargo.toml", "$.package.version"),
        (
            "crates/k-wiki/Cargo.toml",
            "$.dependencies['codebase-graph'].version",
        ),
    ];
    for (path, jsonpath) in required {
        let present = extra_files.iter().any(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("toml")
                && entry.get("path").and_then(Value::as_str) == Some(path)
                && entry.get("jsonpath").and_then(Value::as_str) == Some(jsonpath)
        });
        if !present {
            issues.push(format!(
                "FAIL: release-please-config-incomplete: root package extra-files must include {path} with {jsonpath}."
            ));
        }
    }
}

fn check_workflows(issues: &mut Vec<String>) {
    for workflow in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let Ok(text) = fs::read_to_string(workflow) else {
            issues.push(format!("FAIL: workflow-missing: {workflow} is required."));
            continue;
        };
        if let Some(error) = workflow_yaml_error(&text) {
            issues.push(format!(
                "FAIL: workflow-yaml-invalid: {workflow} is not valid YAML: {error}."
            ));
            continue;
        }
        let workflow_forbidden = [
            concat!("actions/setup-", "python"),
            concat!("python", " "),
            concat!("p", "ip"),
            concat!("py", "test"),
            concat!("ru", "ff"),
            concat!("p", "ip", "-audit"),
            "scripts/",
        ];
        for forbidden in workflow_forbidden {
            if text.contains(forbidden) {
                issues.push(format!(
                    "FAIL: workflow-python-tooling-present: {workflow} contains {forbidden}."
                ));
            }
        }
        for required in [
            "cargo test --workspace --locked",
            "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings",
        ] {
            if workflow.ends_with("ci.yml") && !text.contains(required) {
                issues.push(format!(
                    "FAIL: workflow-rust-gate-missing: {workflow} must run {required}."
                ));
            }
        }
    }
    let release = fs::read_to_string(".github/workflows/release.yml").unwrap_or_default();
    for required in [
        "cargo publish --dry-run --locked",
        "cargo publish --locked",
        "cargo run -p xtask --",
    ] {
        if !release.contains(required) {
            issues.push(format!(
                "FAIL: release-publish-gate-missing: release workflow must run {required}."
            ));
        }
    }
}

fn workflow_yaml_error(text: &str) -> Option<String> {
    yaml_serde::from_str::<yaml_serde::Value>(text)
        .err()
        .map(|error| error.to_string())
}

fn check_no_legacy_surfaces(issues: &mut Vec<String>) {
    let old_crate_name = ["codebase", "graph", "native"].join("_");
    let old_builder = format!("{old_crate_name}_graph_builder");
    let old_workspace_crate = ["rust", "crates", &old_crate_name, "Cargo.toml"].join("/");
    for forbidden in [
        concat!("src/", "legacy", "_cli.rs"),
        "src/ffi.rs",
        &format!("src/bin/{old_builder}.rs"),
        concat!("rust", "/Cargo.toml"),
        &old_workspace_crate,
    ] {
        if Path::new(forbidden).exists() {
            issues.push(format!(
                "FAIL: legacy-surface-present: {forbidden} must not exist."
            ));
        }
    }
    for path in [
        "Cargo.toml",
        "src/lib.rs",
        "src/adapters/cli/mod.rs",
        "src/ladybug_writer.rs",
    ] {
        let text = fs::read_to_string(path).unwrap_or_default();
        let forbidden_tokens = [
            concat!("py", "o3"),
            concat!("python", "-extension"),
            concat!("cdy", "lib"),
            concat!("legacy", "-protocol"),
            concat!("legacy", "_cli"),
            old_builder.as_str(),
        ];
        for forbidden in forbidden_tokens {
            if text.contains(forbidden) {
                issues.push(format!(
                    "FAIL: legacy-token-present: {path} contains {forbidden}."
                ));
            }
        }
    }
}

fn check_conda_recipe(issues: &mut Vec<String>) {
    let Ok(recipe) = fs::read_to_string("conda-forge/recipe/meta.yaml") else {
        issues.push(
            "FAIL: conda-recipe-missing: conda-forge/recipe/meta.yaml is required.".to_string(),
        );
        return;
    };
    for placeholder in [
        "PUT_RELEASE_VERSION_HERE",
        "PUT_RELEASE_ARCHIVE_SHA256_HERE",
        "PUT_SPDX_LICENSE_ID_HERE",
    ] {
        if recipe.contains(placeholder) {
            issues.push(format!(
                "FAIL: conda-placeholder: conda recipe still contains {placeholder}."
            ));
        }
    }
    if recipe.contains(concat!("rust", "/Cargo.toml")) {
        issues.push(
            "FAIL: conda-stale-path: conda recipe must build from root Cargo.toml.".to_string(),
        );
    }
}

fn smoke_artifact(executable: &Path) -> Result<(), String> {
    if !executable.exists() {
        return Err(format!("binary does not exist: {}", executable.display()));
    }
    let temp = unique_temp_dir("codebase_graph_smoke")?;
    fs::create_dir_all(temp.join("sample")).map_err(|error| error.to_string())?;
    fs::write(
        temp.join("sample/service.py"),
        "def helper():\n    return 1\n",
    )
    .map_err(|error| error.to_string())?;

    run_checked(executable, ["--help"])?;
    let schema = run_capture(executable, ["schema", "--json"])?;
    serde_json::from_str::<Value>(&schema)
        .map_err(|error| format!("schema did not emit JSON: {error}"))?;
    run_checked(
        executable,
        [
            "install",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--dry-run",
            "--json",
        ],
    )?;
    run_checked(
        executable,
        [
            "install",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--json",
        ],
    )?;
    run_checked(
        executable,
        [
            "check-health",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--json",
        ],
    )?;
    run_checked(
        executable,
        [
            "codebase-search",
            "helper",
            "--repo-root",
            temp.join("sample").to_str().ok_or("invalid temp path")?,
            "--no-refresh",
        ],
    )?;
    smoke_mcp_stdio(executable, &temp.join("sample"))?;
    Ok(())
}

fn smoke_wiki_artifact(executable: &Path) -> Result<(), String> {
    if !executable.exists() {
        return Err(format!("binary does not exist: {}", executable.display()));
    }
    let executable = executable.canonicalize().map_err(|error| {
        format!(
            "failed to resolve wiki binary {}: {error}",
            executable.display()
        )
    })?;
    let temp = unique_temp_dir("k_wiki_smoke")?;
    let bundle = temp.join("bundle");
    let concepts = bundle.join("concepts");
    let site = temp.join("site");
    fs::create_dir_all(&concepts).map_err(|error| error.to_string())?;
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: \"0.1\"\ntitle: Smoke Bundle\n---\n# Smoke Bundle\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        concepts.join("welcome.md"),
        "---\ntype: guide\ntitle: Welcome\ntags: [smoke]\n---\n# Welcome\n\nPackaged wiki smoke.\n",
    )
    .map_err(|error| error.to_string())?;
    let repo_root = temp.join("repo");
    let knowledge_root = repo_root.join("knowledge");
    fs::create_dir_all(&knowledge_root).map_err(|error| error.to_string())?;
    fs::write(
        knowledge_root.join("index.md"),
        "---\nokf_version: \"0.1\"\ntitle: Repo Knowledge\n---\n# Repo Knowledge\n",
    )
    .map_err(|error| error.to_string())?;

    let bundle_text = bundle.to_str().ok_or("invalid bundle path")?;
    let site_text = site.to_str().ok_or("invalid site path")?;
    assert_version_surface(&executable, Path::new("crates/k-wiki/Cargo.toml"))?;
    run_checked_in(&executable, ["--help"], &temp)?;
    run_checked_in(&executable, ["validate", bundle_text, "--json"], &temp)?;
    run_checked_in(
        &executable,
        ["build", bundle_text, "--out", site_text],
        &temp,
    )?;
    assert_wiki_codex_project_install(&executable, &repo_root, &temp)?;
    if !site.join("index.html").is_file() {
        return Err("wiki artifact smoke did not generate index.html".to_string());
    }
    smoke_wiki_http(&executable, &bundle, &temp)?;
    smoke_wiki_mcp_stdio(&executable, &bundle, &temp)?;
    Ok(())
}

fn smoke_wiki_http(executable: &Path, bundle: &Path, current_dir: &Path) -> Result<(), String> {
    let mut child = Command::new(executable)
        .args(["serve"])
        .arg(bundle)
        .args(["--host", "127.0.0.1", "--port", "0"])
        .current_dir(current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn wiki preview: {error}"))?;

    let result = (|| {
        let stderr = child.stderr.take().ok_or("missing wiki preview stderr")?;
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read wiki preview address: {error}"))?;
        let address = line
            .trim()
            .strip_prefix("Knowledge Wiki preview: http://")
            .ok_or_else(|| format!("wiki preview did not report its address: {line:?}"))?;

        for path in ["/healthz", "/", "/assets/wiki.css"] {
            let response = wiki_http_get(address, path)?;
            if !response.starts_with("HTTP/1.1 200") {
                return Err(format!(
                    "wiki preview returned a non-success response for {path}: {}",
                    response.lines().next().unwrap_or_default()
                ));
            }
        }
        Ok(())
    })();

    let _ = child.kill();
    let _ = child.wait();
    result
}

fn wiki_http_get(address: &str, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(address)
        .map_err(|error| format!("failed to connect to wiki preview: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| error.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;
    Ok(response)
}

fn smoke_wiki_mcp_stdio(
    executable: &Path,
    bundle: &Path,
    current_dir: &Path,
) -> Result<(), String> {
    let mut child = Command::new(executable)
        .arg("mcp")
        .arg(bundle)
        .current_dir(current_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn wiki MCP server: {error}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("missing wiki MCP stdin")?;
        writeln!(
            stdin,
            "{}",
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"}
            })
        )
        .map_err(|error| error.to_string())?;
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        )
        .map_err(|error| error.to_string())?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "wiki MCP smoke failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    if !stdout.contains(r#""name":"Knowledge Wiki""#)
        || !stdout.contains(r#""name":"wiki_get_concept""#)
        || !stdout.contains(r#""name":"wiki_populate_page""#)
        || !stdout.contains(r#""name":"wiki_validate""#)
        || !stdout.contains(r#""name":"wiki_check_links""#)
        || !stdout.contains(r#""name":"wiki_build""#)
    {
        return Err("wiki MCP smoke did not advertise the Knowledge Wiki schema".to_string());
    }
    Ok(())
}

fn assert_version_surface(executable: &Path, manifest: &Path) -> Result<(), String> {
    let expected = cargo_version(manifest)?;
    let output = run_capture_in(executable, ["--version"], Path::new("."))?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let actual = stdout.trim();
    let expected_line = format!("k-wiki {expected}");
    if actual != expected_line {
        return Err(format!(
            "unexpected version output: expected {expected_line:?}, got {actual:?}"
        ));
    }
    Ok(())
}

fn assert_wiki_codex_project_install(
    executable: &Path,
    repo_root: &Path,
    current_dir: &Path,
) -> Result<(), String> {
    let home = current_dir.join("isolated-home");
    let codex_home = home.join(".codex");
    let empty_path = current_dir.join("empty-path");
    let legacy_repo = current_dir.join("StructuralFactory");
    let legacy_bundle = legacy_repo.join("knowledge");
    fs::create_dir_all(&codex_home).map_err(|error| error.to_string())?;
    fs::create_dir_all(&empty_path).map_err(|error| error.to_string())?;
    fs::create_dir_all(&legacy_bundle).map_err(|error| error.to_string())?;
    fs::write(
        legacy_bundle.join("index.md"),
        "---\nokf_version: \"0.1\"\ntitle: Legacy Knowledge\n---\n# Legacy Knowledge\n",
    )
    .map_err(|error| error.to_string())?;
    let legacy_bundle = legacy_bundle
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize legacy bundle: {error}"))?;
    fs::write(
        codex_home.join("config.toml"),
        format!(
            "model = \"preserved\"\n\n[mcp_servers.k_wiki]\ncommand = \"legacy-k-wiki\"\nargs = [\"mcp\", {}]\nstartup_timeout_sec = 60\n",
            serde_json::to_string(legacy_bundle.to_string_lossy().as_ref())
                .map_err(|error| error.to_string())?,
        ),
    )
    .map_err(|error| error.to_string())?;
    let home_text = home.to_string_lossy().to_string();
    let codex_home_text = codex_home.to_string_lossy().to_string();
    let empty_path_text = empty_path.to_string_lossy().to_string();
    let executable_text = executable.to_string_lossy().to_string();
    let environment = [
        ("HOME", home_text.as_str()),
        ("CODEX_HOME", codex_home_text.as_str()),
        ("PATH", empty_path_text.as_str()),
        ("K_WIKI_SERVER_COMMAND", executable_text.as_str()),
    ];
    let output = run_capture_in_with_env(
        executable,
        [
            "mcp",
            "install",
            "--client",
            "codex",
            "--scope",
            "project",
            "--repo-root",
            repo_root.to_str().ok_or("invalid repo root path")?,
            "--dry-run",
            "--verify",
        ],
        current_dir,
        &environment,
    )?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let payload: Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!("dry-run MCP install did not return valid JSON: {error}; stdout: {stdout}")
    })?;
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .ok_or("dry-run MCP install did not report method")?;
    if method != "file_adapter" {
        return Err(format!(
            "dry-run MCP install expected method=file_adapter, got {method}"
        ));
    }
    let locality = payload
        .get("target_locality")
        .and_then(Value::as_str)
        .ok_or("dry-run MCP install did not report target_locality")?;
    if locality != "repository_local" {
        return Err(format!(
            "dry-run MCP install expected repository_local locality, got {locality}"
        ));
    }
    let expected_path = repo_root
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize repo root for dry-run smoke: {error}"))?
        .join(".codex/config.toml");
    let reported_path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or("dry-run MCP install did not report path")?;
    if Path::new(reported_path) != expected_path {
        return Err(format!(
            "dry-run MCP install expected path {}, got {}",
            expected_path.display(),
            reported_path
        ));
    }
    if payload
        .pointer("/verification/reason")
        .and_then(Value::as_str)
        != Some("dry_run")
    {
        return Err("dry-run MCP install did not report verification as skipped".to_string());
    }

    let install_args = [
        "mcp",
        "install",
        "--client",
        "codex",
        "--scope",
        "project",
        "--repo-root",
        repo_root.to_str().ok_or("invalid repo root path")?,
        "--verify",
    ];
    let installed = run_capture_in_with_env(executable, install_args, current_dir, &environment)?;
    let installed_stdout =
        String::from_utf8(installed.stdout).map_err(|error| error.to_string())?;
    let installed: Value = serde_json::from_str(installed_stdout.trim()).map_err(|error| {
        format!("MCP install did not return valid JSON: {error}; stdout: {installed_stdout}")
    })?;
    if !matches!(
        installed.get("action").and_then(Value::as_str),
        Some("created" | "updated")
    ) || installed
        .pointer("/verification/ok")
        .and_then(Value::as_bool)
        != Some(true)
        || installed
            .pointer("/legacy_cleanup/action")
            .and_then(Value::as_str)
            != Some("renamed")
    {
        return Err(format!(
            "packaged MCP install did not update, verify, and preserve the legacy entry: {installed}"
        ));
    }
    let preserved_as = installed
        .pointer("/legacy_cleanup/preserved_as")
        .and_then(Value::as_str)
        .ok_or("packaged MCP install did not report the preserved legacy name")?;
    if !preserved_as.starts_with("k_wiki_structuralfactory_") {
        return Err(format!("unexpected preserved legacy name: {preserved_as}"));
    }
    let local_config = fs::read_to_string(&expected_path).map_err(|error| error.to_string())?;
    let expected_command = format!(
        "command = {}",
        serde_json::to_string(&executable_text).map_err(|error| error.to_string())?
    );
    if !local_config.contains("[mcp_servers.k_wiki]")
        || !local_config
            .lines()
            .any(|line| line.trim() == expected_command)
    {
        return Err("packaged MCP install did not write the repository-local registration".into());
    }
    let shared_config =
        fs::read_to_string(codex_home.join("config.toml")).map_err(|error| error.to_string())?;
    if !shared_config.contains("model = \"preserved\"")
        || shared_config.contains("[mcp_servers.k_wiki]")
        || !shared_config.contains(&format!("[mcp_servers.{preserved_as}]"))
        || !shared_config.contains("command = \"legacy-k-wiki\"")
    {
        return Err("packaged MCP install did not preserve the shared legacy registration".into());
    }

    let repeated = run_capture_in_with_env(executable, install_args, current_dir, &environment)?;
    let repeated_stdout = String::from_utf8(repeated.stdout).map_err(|error| error.to_string())?;
    let repeated: Value = serde_json::from_str(repeated_stdout.trim()).map_err(|error| {
        format!("repeat MCP install did not return valid JSON: {error}; stdout: {repeated_stdout}")
    })?;
    if repeated.get("action").and_then(Value::as_str) != Some("unchanged")
        || repeated
            .pointer("/verification/ok")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err(format!(
            "packaged MCP install was not idempotent: {repeated}"
        ));
    }
    Ok(())
}

fn smoke_mcp_stdio(executable: &Path, repo_root: &Path) -> Result<(), String> {
    let mut child = Command::new(executable)
        .args(["mcp", "start", "--repo-root"])
        .arg(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn MCP server: {error}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("missing MCP stdin")?;
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "xtask-smoke", "version": "0"}
            }
        });
        writeln!(stdin, "{}", initialize).map_err(|error| error.to_string())?;
        let tools = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
        writeln!(stdin, "{}", tools).map_err(|error| error.to_string())?;
    }
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut output).ok();
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    if !status.success() && output.is_empty() {
        return Err(format!("MCP stdio smoke failed with status {status}"));
    }
    if !output.contains("graph_search") {
        return Err("MCP stdio smoke did not list graph_search".to_string());
    }
    Ok(())
}

fn verify_release_version(tag: &str) -> Result<(), String> {
    let expected = tag
        .strip_prefix('v')
        .ok_or_else(|| format!("release tag must match vX.Y.Z, got {tag:?}"))?;
    if expected.split('.').count() != 3
        || !expected
            .chars()
            .all(|item| item.is_ascii_digit() || item == '.')
    {
        return Err(format!("release tag must match vX.Y.Z, got {tag:?}"));
    }
    let actual = cargo_version(Path::new("Cargo.toml"))?;
    if actual != expected {
        return Err(format!(
            "Cargo package version {actual} does not match release tag {tag}"
        ));
    }
    let wiki = cargo_version(Path::new("crates/k-wiki/Cargo.toml"))?;
    if wiki != expected {
        return Err(format!(
            "Knowledge Wiki package version {wiki} does not match release tag {tag}"
        ));
    }
    let dependency = dependency_version(Path::new("crates/k-wiki/Cargo.toml"), "codebase-graph")?;
    if dependency != expected {
        return Err(format!(
            "Knowledge Wiki depends on codebase-graph {dependency} but release tag is {tag}"
        ));
    }
    println!("{actual}");
    Ok(())
}

fn cargo_version(manifest: &Path) -> Result<String, String> {
    let cargo = fs::read_to_string(manifest).map_err(|error| error.to_string())?;
    for line in cargo.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("version = ") {
            return Ok(value.trim_matches('"').to_string());
        }
    }
    Err(format!(
        "{} does not contain package version",
        manifest.display()
    ))
}

fn dependency_version(manifest: &Path, dependency: &str) -> Result<String, String> {
    let cargo = fs::read_to_string(manifest).map_err(|error| error.to_string())?;
    for line in cargo.lines() {
        let line = line.trim();
        if line.starts_with(&format!("{dependency} =")) {
            if let Some(version_index) = line.find("version = ") {
                let value = &line[(version_index + "version = ".len())..];
                if let Some(first_quote) = value.find('"') {
                    let rest = &value[(first_quote + 1)..];
                    if let Some(end_quote) = rest.find('"') {
                        return Ok(rest[..end_quote].to_string());
                    }
                }
            }
        }
    }
    Err(format!(
        "{} does not contain dependency version for {}",
        manifest.display(),
        dependency
    ))
}

fn run_checked<'a, I>(executable: &Path, args: I) -> Result<(), String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            executable.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_capture_in<'a, I>(
    executable: &Path,
    args: I,
    current_dir: &Path,
) -> Result<std::process::Output, String>
where
    I: IntoIterator<Item = &'a str>,
{
    Command::new(executable)
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(|error| error.to_string())
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(format!(
                    "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
                    output.status,
                    executable.display(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        })
}

fn run_capture_in_with_env<'a, I>(
    executable: &Path,
    args: I,
    current_dir: &Path,
    env_pairs: &[(&str, &str)],
) -> Result<std::process::Output, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut command = Command::new(executable);
    command.args(args).current_dir(current_dir);
    for (key, value) in env_pairs {
        command.env(key, value);
    }
    command
        .output()
        .map_err(|error| error.to_string())
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(format!(
                    "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
                    output.status,
                    executable.display(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ))
            }
        })
}

fn run_checked_in<'a, I>(executable: &Path, args: I, current_dir: &Path) -> Result<(), String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new(executable)
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            executable.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_capture<'a, I>(executable: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = &'a str>,
{
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "command failed with status {}: {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            executable.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn unique_temp_dir(prefix: &str) -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let path = env::temp_dir().join(format!("{prefix}_{nanos}"));
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|item| item.to_str())
            .unwrap_or_default();
        if name == ".git" || name == "target" || name == ".codebaseGraph" || name == ".kwiki" {
            continue;
        }
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::workflow_yaml_error;

    #[test]
    fn workflow_yaml_validation_rejects_unindented_heredoc_body() {
        let invalid =
            "jobs:\n  build:\n    steps:\n      - run: |\n          cat <<'EOF'\nbody\nEOF\n";
        let valid = "jobs:\n  build:\n    steps:\n      - run: |\n          cat <<'EOF'\n          body\n          EOF\n";

        assert!(workflow_yaml_error(invalid).is_some());
        assert!(workflow_yaml_error(valid).is_none());
    }
}
