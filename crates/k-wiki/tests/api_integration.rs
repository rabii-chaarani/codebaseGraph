use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use k_wiki::{
    adapters::{http, mcp},
    api::mcp_operation_descriptor,
    authoring::{
        AuthoringConfig, AuthoringService, BundleRoot, NoopRefreshNotifier, NoopValidator,
        RepositoryRoot,
    },
    service::LocalWikiService,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn mcp_authoring_round_trip_creates_populates_reads_and_searches_a_concept() {
    let temp = TestDir::new("k-wiki-mcp-round-trip");
    let repository = temp.path().join("repository");
    let bundle = repository.join("docs");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
    )
    .expect("write root index");

    let authoring = AuthoringService::new(
        AuthoringConfig {
            repositories: vec![RepositoryRoot {
                id: "repo".into(),
                root_path: repository,
            }],
            bundles: vec![BundleRoot {
                id: "docs".into(),
                repository_id: "repo".into(),
                root_path: bundle.clone(),
            }],
        },
        NoopValidator,
        NoopRefreshNotifier,
    )
    .expect("configure authoring");
    let api = LocalWikiService::new(vec![bundle])
        .with_authoring(authoring)
        .into_api();
    let mut session = mcp_session();

    let created = call_tool(
        &api,
        &mut session,
        1,
        "wiki_create_page",
        json!({
            "bundle_id": "docs",
            "page_path": "guides/getting-started",
            "type": "guide",
            "title": "Getting Started",
            "tags": ["onboarding"],
            "body_markdown": "Initial body.",
            "include_structured_content": true
        }),
    );
    assert_eq!(created["result"]["isError"], false);
    let content_hash = created["result"]["structuredContent"]["result"]["content_hash"]
        .as_str()
        .expect("created content hash");

    let populated = call_tool(
        &api,
        &mut session,
        2,
        "wiki_populate_page",
        json!({
            "bundle_id": "docs",
            "page_path": "guides/getting-started",
            "frontmatter": {
                "type": "guide",
                "title": "Getting Started",
                "tags": ["onboarding"],
                "extensions": {"owner": "platform"}
            },
            "body_markdown": "# Getting Started\n\nFollow the onboarding path.",
            "expected_content_hash": content_hash,
            "include_structured_content": true
        }),
    );
    assert_eq!(populated["result"]["isError"], false);

    let concept = call_tool(
        &api,
        &mut session,
        3,
        "wiki_get_concept",
        json!({
            "bundle_id": "docs",
            "concept_id": "guides/getting-started",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        concept["result"]["structuredContent"]["result"]["id"],
        "guides/getting-started"
    );

    let search = call_tool(
        &api,
        &mut session,
        4,
        "wiki_search_concepts",
        json!({
            "text": "Getting Started",
            "bundle_id": "docs",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        search["result"]["structuredContent"]["result"][0]["concept_id"],
        "guides/getting-started"
    );
}

#[test]
fn mcp_lists_only_configured_bundles_and_keeps_them_readable() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let configured = manifest.join("tests/fixtures/minimal");
    let api = LocalWikiService::new(vec![configured]).into_api();
    let mut session = mcp_session();

    let descriptor = mcp_operation_descriptor("wiki_list_bundles").expect("list bundles tool");
    let schema = (descriptor.request_schema)();
    assert!(schema["properties"].get("repository_roots").is_none());
    assert_eq!(schema["additionalProperties"], false);

    let bundles = call_tool(
        &api,
        &mut session,
        1,
        "wiki_list_bundles",
        json!({"include_structured_content": true}),
    );
    assert_eq!(
        bundles["result"]["structuredContent"]["result"]
            .as_array()
            .expect("configured bundles")
            .iter()
            .map(|bundle| bundle["id"].as_str().expect("bundle id"))
            .collect::<Vec<_>>(),
        vec!["minimal"]
    );

    let search = call_tool(
        &api,
        &mut session,
        2,
        "wiki_search_concepts",
        json!({
            "text": "parser",
            "bundle_id": "minimal",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        search["result"]["structuredContent"]["result"][0]["concept_id"],
        "decisions/adr-001"
    );

    let concept = call_tool(
        &api,
        &mut session,
        3,
        "wiki_get_concept",
        json!({
            "bundle_id": "minimal",
            "concept_id": "decisions/adr-001",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        concept["result"]["structuredContent"]["result"]["id"],
        "decisions/adr-001"
    );

    let directory = call_tool(
        &api,
        &mut session,
        4,
        "wiki_list_directory",
        json!({
            "bundle_id": "minimal",
            "path": "decisions",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        directory["result"]["structuredContent"]["result"]["path"],
        "decisions"
    );

    let diagnostics = call_tool(
        &api,
        &mut session,
        5,
        "wiki_get_diagnostics",
        json!({
            "bundle_id": "minimal",
            "profile": "recommended",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        diagnostics["result"]["structuredContent"]["kind"],
        "diagnostics"
    );

    let recent = call_tool(
        &api,
        &mut session,
        6,
        "wiki_get_recent_changes",
        json!({
            "bundle_id": "minimal",
            "include_structured_content": true
        }),
    );
    assert_eq!(
        recent["result"]["structuredContent"]["kind"],
        "recent_changes"
    );
    assert!(recent["result"]["structuredContent"]["result"].is_array());

    let rejected = call_tool(
        &api,
        &mut session,
        7,
        "wiki_list_bundles",
        json!({"repository_roots": [manifest]}),
    );
    assert_eq!(rejected["error"]["code"], -32602);
    assert_eq!(rejected["error"]["message"], "tool arguments are invalid");
}

#[test]
fn mcp_maintenance_tools_validate_check_links_and_build_a_site() {
    let temp = TestDir::new("k-wiki-mcp-maintenance");
    let bundle = temp.path().join("knowledge");
    let output = temp.path().join(".kwiki/site");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Knowledge\n---\n# Knowledge\n",
    )
    .expect("write root index");
    let api = LocalWikiService::new(vec![bundle.clone()]).into_api();
    let mut session = mcp_session();

    let validation = call_tool(
        &api,
        &mut session,
        1,
        "wiki_validate",
        json!({
            "bundle_root": bundle,
            "profile": "recommended",
            "include_structured_content": true
        }),
    );
    assert_eq!(validation["result"]["isError"], false);
    assert_eq!(
        validation["result"]["structuredContent"]["kind"],
        "validation"
    );

    let links = call_tool(
        &api,
        &mut session,
        2,
        "wiki_check_links",
        json!({
            "bundle_root": bundle,
            "include_structured_content": true
        }),
    );
    assert_eq!(links["result"]["isError"], false);
    assert_eq!(links["result"]["structuredContent"]["kind"], "diagnostics");

    let unconfigured_bundle = temp.path().join("other-knowledge");
    fs::create_dir_all(&unconfigured_bundle).expect("create unconfigured bundle");
    fs::write(
        unconfigured_bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Other\n---\n# Other\n",
    )
    .expect("write unconfigured root index");
    let denied = call_tool(
        &api,
        &mut session,
        3,
        "wiki_validate",
        json!({"bundle_root": unconfigured_bundle, "profile": "recommended"}),
    );
    assert_eq!(denied["error"]["code"], -32602);
    assert_eq!(
        denied["error"]["message"],
        "bundle is not configured for this wiki"
    );

    let build = call_tool(
        &api,
        &mut session,
        4,
        "wiki_build",
        json!({
            "bundle_root": bundle,
            "output_root": output,
            "include_structured_content": true
        }),
    );
    assert_eq!(build["result"]["isError"], false);
    assert_eq!(build["result"]["annotations"]["writeOperation"], true);
    assert_eq!(
        build["result"]["structuredContent"]["kind"],
        "site_rendered"
    );
    assert!(output.join("index.html").is_file());
}

#[test]
fn cli_validate_and_build_use_the_integrated_public_api() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("tests/fixtures/comprehensive");
    let temp = TestDir::new("k-wiki-cli");
    let output = temp.path().join("site");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    let validation = Command::new(binary)
        .args([
            "validate",
            fixture.to_str().expect("fixture path"),
            "--profile",
            "consume",
            "--json",
        ])
        .output()
        .expect("run validation");
    assert!(
        validation.status.success(),
        "{}",
        String::from_utf8_lossy(&validation.stderr)
    );
    let validation_payload: Value =
        serde_json::from_slice(&validation.stdout).expect("validation should emit JSON");
    assert_eq!(validation_payload["kind"], "validation");
    assert!(validation_payload["result"]["accepted"].is_boolean());

    let build = Command::new(binary)
        .args([
            "build",
            fixture.to_str().expect("fixture path"),
            "--out",
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run build");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(output.join("index.html").is_file());
}

#[test]
fn install_command_initializes_repository_local_wiki_state() {
    let temp = TestDir::new("k-wiki-install");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    let install = Command::new(binary)
        .args([
            "install",
            "--repo-root",
            repository.to_str().expect("repository root"),
        ])
        .output()
        .expect("run install");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    for directory in [
        ".kwiki/staging",
        ".kwiki/generations",
        ".kwiki/cache",
        ".kwiki/site",
    ] {
        assert!(repository.join(directory).is_dir(), "missing {directory}");
    }
    let source = fs::read_to_string(repository.join("knowledge/index.md"))
        .expect("read starter bundle index");
    assert!(source.contains("okf_version:"));
    assert!(source.contains("Repository Knowledge"));
    for file_name in ["AGENTS.md", "CLAUDE.md"] {
        let instructions =
            fs::read_to_string(repository.join(file_name)).expect("read installer instructions");
        assert!(instructions.contains("<!-- k-wiki:start -->"));
        assert!(instructions.contains("wiki_validate"));
    }

    let repeat = Command::new(binary)
        .args([
            "install",
            "--repo-root",
            repository.to_str().expect("repository root"),
        ])
        .output()
        .expect("rerun install");
    assert!(repeat.status.success());
    assert!(String::from_utf8_lossy(&repeat.stdout).contains("already initialized"));
}

#[test]
fn mcp_install_command_registers_the_repository_knowledge_bundle() {
    let temp = TestDir::new("k-wiki-mcp-install");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    let client_config = temp.path().join("client").join("mcp.json");
    fs::create_dir_all(client_config.parent().expect("client config parent"))
        .expect("create client config parent");
    fs::write(
        &client_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {"unrelated": {"command": "keep", "args": []}}
        }))
        .expect("serialize client config"),
    )
    .expect("write client config");

    let bootstrap = Command::new(binary)
        .args([
            "install",
            "--repo-root",
            repository.to_str().expect("repository root"),
        ])
        .output()
        .expect("bootstrap repository");
    assert!(bootstrap.status.success());

    let registration = Command::new(binary)
        .args([
            "mcp",
            "install",
            "--client",
            "generic",
            "--repo-root",
            repository.to_str().expect("repository root"),
            "--client-config-path",
            client_config.to_str().expect("client config path"),
        ])
        .output()
        .expect("register MCP client");
    assert!(
        registration.status.success(),
        "{}",
        String::from_utf8_lossy(&registration.stderr)
    );
    let result: Value = serde_json::from_slice(&registration.stdout).expect("registration JSON");
    assert_eq!(result["action"], "updated");
    let server_name = result["server_name"].as_str().expect("server name");
    assert!(server_name.starts_with("k_wiki_repository_"));

    let configured: Value =
        serde_json::from_str(&fs::read_to_string(&client_config).expect("read client config"))
            .expect("parse client config");
    let repository = repository.canonicalize().expect("canonical repository");
    assert_eq!(configured["mcpServers"]["unrelated"]["command"], "keep");
    assert_eq!(configured["mcpServers"][server_name]["command"], "k-wiki");
    assert_eq!(
        configured["mcpServers"][server_name]["args"],
        json!([
            "mcp",
            repository.join("knowledge").to_string_lossy().to_string()
        ])
    );
    for instruction_file in ["AGENTS.md", "CLAUDE.md"] {
        let instructions = fs::read_to_string(repository.join(instruction_file))
            .expect("read updated workflow instructions");
        assert!(
            instructions.contains(&format!("`{server_name}`")),
            "{instruction_file} did not name the effective MCP registration"
        );
    }

    let repeat = Command::new(binary)
        .args([
            "mcp",
            "install",
            "--client",
            "generic",
            "--repo-root",
            repository.to_str().expect("repository root"),
            "--client-config-path",
            client_config.to_str().expect("client config path"),
        ])
        .output()
        .expect("reuse MCP registration");
    assert!(repeat.status.success());
    let repeat: Value = serde_json::from_slice(&repeat.stdout).expect("repeat registration JSON");
    assert_eq!(repeat["action"], "unchanged");

    let dry_run_config = temp.path().join("dry-run").join("mcp.json");
    let dry_run = Command::new(binary)
        .args([
            "mcp",
            "install",
            "--client",
            "generic",
            "--repo-root",
            repository.to_str().expect("repository root"),
            "--client-config-path",
            dry_run_config.to_str().expect("dry-run config path"),
            "--dry-run",
        ])
        .output()
        .expect("dry-run MCP registration");
    assert!(dry_run.status.success());
    let dry_run: Value = serde_json::from_slice(&dry_run.stdout).expect("dry-run JSON");
    assert_eq!(dry_run["action"], "dry_run");
    assert!(!dry_run_config.exists());
}

#[test]
fn mcp_install_command_keeps_codex_project_and_local_registrations_repository_local() {
    let temp = TestDir::new("k-wiki-mcp-install-codex-locality");
    let repository_a = temp.path().join("alpha");
    let repository_b = temp.path().join("beta");
    fs::create_dir_all(&repository_a).expect("create repository a");
    fs::create_dir_all(&repository_b).expect("create repository b");
    let fake_home = temp.path().join("home");
    fs::create_dir_all(&fake_home).expect("create fake home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    bootstrap_repository(binary, &repository_a);
    bootstrap_repository(binary, &repository_b);

    let local = run_mcp_install(
        binary,
        &repository_a,
        &["--client", "codex", "--scope", "local"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("OPENCLAW_HOME", fake_home.join(".openclaw").as_path()),
        ],
    );
    assert!(
        local.status.success(),
        "{}",
        String::from_utf8_lossy(&local.stderr)
    );
    let local_json: Value = serde_json::from_slice(&local.stdout).expect("local JSON");
    assert_eq!(local_json["server_name"], "k_wiki");
    assert_eq!(local_json["target_locality"], "repository_local");

    let project = run_mcp_install(
        binary,
        &repository_b,
        &["--client", "codex", "--scope", "project"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("OPENCLAW_HOME", fake_home.join(".openclaw").as_path()),
        ],
    );
    assert!(
        project.status.success(),
        "{}",
        String::from_utf8_lossy(&project.stderr)
    );
    let project_json: Value = serde_json::from_slice(&project.stdout).expect("project JSON");
    assert_eq!(project_json["server_name"], "k_wiki");
    assert_eq!(project_json["target_locality"], "repository_local");

    for repository in [&repository_a, &repository_b] {
        let config = fs::read_to_string(repository.join(".codex/config.toml"))
            .expect("read repository codex config");
        let canonical = repository.canonicalize().expect("canonical repository");
        assert!(config.contains("[mcp_servers.k_wiki]"));
        assert!(config.contains(canonical.join("knowledge").to_string_lossy().as_ref()));
    }
}

#[test]
fn mcp_install_command_verifies_the_registered_single_bundle_runtime() {
    let temp = TestDir::new("k-wiki-mcp-install-verify");
    let repository = temp.path().join("repository");
    let fake_home = temp.path().join("home");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&fake_home).expect("create fake home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "project", "--verify"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("K_WIKI_SERVER_COMMAND", Path::new(binary)),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("verification JSON");
    assert_eq!(payload["method"], "file_adapter");
    assert_eq!(payload["target_locality"], "repository_local");
    assert_eq!(payload["verification"]["ok"], true);
    assert_eq!(payload["verification"]["checks"]["server_name"], true);
    assert_eq!(payload["verification"]["checks"]["configuration"], true);
    assert_eq!(payload["verification"]["checks"]["mcp_startup"], true);
    assert_eq!(payload["verification"]["checks"]["schema"], true);
    assert_eq!(payload["verification"]["checks"]["bundle"], true);
    assert_eq!(
        payload["verification"]["checks"]["single_bundle_schema"],
        true
    );
    assert_eq!(payload["verification"]["checks"]["configured_bundle"], true);

    let dry_run = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "codex",
            "--scope",
            "project",
            "--dry-run",
            "--verify",
        ],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("K_WIKI_SERVER_COMMAND", Path::new(binary)),
        ],
    );
    assert!(dry_run.status.success());
    let dry_run: Value = serde_json::from_slice(&dry_run.stdout).expect("dry-run JSON");
    assert_eq!(dry_run["verification"]["ok"], true);
    assert_eq!(dry_run["verification"]["skipped"], true);
    assert_eq!(dry_run["verification"]["reason"], "dry_run");
}

#[test]
fn mcp_install_command_keeps_registration_when_runtime_verification_fails() {
    let temp = TestDir::new("k-wiki-mcp-install-unverified");
    let repository = temp.path().join("repository");
    let fake_home = temp.path().join("home");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&fake_home).expect("create fake home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);
    let missing = repository.join("missing-k-wiki");

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "project", "--verify"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("K_WIKI_SERVER_COMMAND", missing.as_path()),
        ],
    );
    assert!(!output.status.success());
    let payload: Value = serde_json::from_slice(&output.stderr).expect("error JSON");
    assert_eq!(payload["error"]["code"], "mcp_installation_unverified");
    assert_eq!(
        payload["error"]["details"]["action"],
        "installed_but_unverified"
    );
    assert_eq!(payload["error"]["details"]["verification"]["ok"], false);
    assert!(repository.join(".codex/config.toml").is_file());
}

#[test]
fn mcp_install_command_uses_distinct_shared_names_for_same_basename_repositories() {
    let temp = TestDir::new("k-wiki-mcp-install-shared-same-basename");
    let repository_a = temp.path().join("group-a").join("repository");
    let repository_b = temp.path().join("group-b").join("repository");
    fs::create_dir_all(&repository_a).expect("create repository a");
    fs::create_dir_all(&repository_b).expect("create repository b");
    let shared_config = temp.path().join("shared").join("mcp.json");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    bootstrap_repository(binary, &repository_a);
    bootstrap_repository(binary, &repository_b);

    let first = run_mcp_install(
        binary,
        &repository_a,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--client-config-path",
            shared_config.to_str().expect("shared config"),
        ],
        &[],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_json: Value = serde_json::from_slice(&first.stdout).expect("first JSON");

    let second = run_mcp_install(
        binary,
        &repository_b,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--client-config-path",
            shared_config.to_str().expect("shared config"),
        ],
        &[],
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_json: Value = serde_json::from_slice(&second.stdout).expect("second JSON");

    let first_name = first_json["server_name"]
        .as_str()
        .expect("first server name");
    let second_name = second_json["server_name"]
        .as_str()
        .expect("second server name");
    assert!(first_name.starts_with("k_wiki_repository_"));
    assert!(second_name.starts_with("k_wiki_repository_"));
    assert_ne!(first_name, second_name);
    assert_eq!(first_json["target_locality"], "shared");
    assert_eq!(second_json["target_locality"], "shared");

    let configured: Value =
        serde_json::from_str(&fs::read_to_string(&shared_config).expect("read shared config"))
            .expect("parse shared config");
    let servers = configured["mcpServers"]
        .as_object()
        .expect("shared servers");
    assert!(servers.get("k_wiki").is_none());
    assert!(servers.get(first_name).is_some());
    assert!(servers.get(second_name).is_some());
}

#[test]
fn mcp_install_command_preserves_explicit_shared_name_and_rejects_shared_k_wiki_name() {
    let temp = TestDir::new("k-wiki-mcp-install-explicit-shared-name");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");
    let shared_config = temp.path().join("shared.json");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    bootstrap_repository(binary, &repository);

    let explicit = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--name",
            "team_docs_wiki",
            "--client-config-path",
            shared_config.to_str().expect("shared config"),
        ],
        &[],
    );
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let explicit_json: Value = serde_json::from_slice(&explicit.stdout).expect("explicit JSON");
    assert_eq!(explicit_json["server_name"], "team_docs_wiki");

    let rejected = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--name",
            "k_wiki",
            "--client-config-path",
            temp.path()
                .join("rejected.json")
                .to_str()
                .expect("rejected config"),
        ],
        &[],
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("reserved for repository-local"));
}

#[test]
fn mcp_install_command_reports_unique_manual_registration_and_cleanup_instructions() {
    let temp = TestDir::new("k-wiki-mcp-install-manual");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "copilot-studio", "--scope", "user"],
        &[],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("manual result JSON");
    let server_name = payload["server_name"].as_str().expect("manual server name");
    assert!(server_name.starts_with("k_wiki_repository_"));
    assert_eq!(payload["action"], "reported");
    assert_eq!(payload["method"], "manual_metadata");
    assert_eq!(payload["target_locality"], "manual");
    assert_eq!(payload["legacy_cleanup"]["action"], "manual_required");
    assert!(payload["legacy_cleanup"]["instructions"][0]
        .as_str()
        .is_some_and(|instruction| instruction.contains("k_wiki")));
    for instruction_file in ["AGENTS.md", "CLAUDE.md"] {
        let instructions = fs::read_to_string(repository.join(instruction_file))
            .expect("read manual workflow instructions");
        assert!(instructions.contains(&format!("`{server_name}`")));
    }
}

#[test]
fn mcp_install_command_rejects_conflicting_existing_entries_without_rewriting_files() {
    let temp = TestDir::new("k-wiki-mcp-install-conflicts");
    let repository = temp.path().join("repository");
    let fake_home = temp.path().join("home");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&fake_home).expect("create fake home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);

    let json_config = temp.path().join("shared.json");
    fs::write(
        &json_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "team_docs_wiki": {"command": "other", "args": ["mcp", "/tmp/elsewhere"]}
            }
        }))
        .expect("serialize json config"),
    )
    .expect("write json config");
    let before_json = fs::read_to_string(&json_config).expect("read json config before");
    let json_result = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--name",
            "team_docs_wiki",
            "--client-config-path",
            json_config.to_str().expect("json config"),
        ],
        &[],
    );
    assert!(!json_result.status.success());
    assert!(String::from_utf8_lossy(&json_result.stderr).contains("refusing to overwrite"));
    assert_eq!(
        fs::read_to_string(&json_config).expect("read json config after"),
        before_json
    );

    let codex_config = repository.join(".codex/conflict.toml");
    fs::create_dir_all(codex_config.parent().expect("codex config parent"))
        .expect("create codex config parent");
    fs::write(
        &codex_config,
        "[mcp_servers.k_wiki]\ncommand = \"other\"\nargs = [\"mcp\", \"/tmp/elsewhere\"]\nstartup_timeout_sec = 60\n",
    )
    .expect("write codex config");
    let before_toml = fs::read_to_string(&codex_config).expect("read toml before");
    let toml_result = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "codex",
            "--scope",
            "local",
            "--client-config-path",
            codex_config.to_str().expect("codex config"),
        ],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("OPENCLAW_HOME", fake_home.join(".openclaw").as_path()),
        ],
    );
    assert!(!toml_result.status.success());
    assert!(String::from_utf8_lossy(&toml_result.stderr).contains("refusing to overwrite"));
    assert_eq!(
        fs::read_to_string(&codex_config).expect("read toml after"),
        before_toml
    );
}

#[test]
fn mcp_install_command_removes_legacy_shared_entry_and_reports_dry_run_cleanup() {
    let temp = TestDir::new("k-wiki-mcp-install-legacy-cleanup");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");
    let shared_config = temp.path().join("shared").join("mcp.json");
    fs::create_dir_all(shared_config.parent().expect("shared config parent"))
        .expect("create shared parent");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    bootstrap_repository(binary, &repository);

    let canonical = repository.canonicalize().expect("canonical repository");
    fs::write(
        &shared_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "k_wiki": {
                    "command": "k-wiki",
                    "args": ["mcp", canonical.join("knowledge").to_string_lossy().to_string()]
                },
                "unrelated": {"command": "keep", "args": []}
            }
        }))
        .expect("serialize legacy config"),
    )
    .expect("write legacy config");

    let installed = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--client-config-path",
            shared_config.to_str().expect("shared config"),
        ],
        &[],
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let installed_json: Value = serde_json::from_slice(&installed.stdout).expect("installed JSON");
    assert_eq!(installed_json["legacy_cleanup"]["action"], "removed");
    let configured: Value =
        serde_json::from_str(&fs::read_to_string(&shared_config).expect("read cleaned config"))
            .expect("parse cleaned config");
    assert!(configured["mcpServers"]["k_wiki"].is_null());
    assert_eq!(configured["mcpServers"]["unrelated"]["command"], "keep");

    let dry_run_config = temp.path().join("dry-run").join("mcp.json");
    fs::create_dir_all(dry_run_config.parent().expect("dry run config parent"))
        .expect("create dry run parent");
    fs::write(
        &dry_run_config,
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "k_wiki": {
                    "command": "k-wiki",
                    "args": ["mcp", canonical.join("knowledge").to_string_lossy().to_string()]
                }
            }
        }))
        .expect("serialize dry run config"),
    )
    .expect("write dry run config");
    let before = fs::read_to_string(&dry_run_config).expect("read dry run before");
    let dry_run = run_mcp_install(
        binary,
        &repository,
        &[
            "--client",
            "generic",
            "--scope",
            "local",
            "--client-config-path",
            dry_run_config.to_str().expect("dry run config"),
            "--dry-run",
        ],
        &[],
    );
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let dry_run_json: Value = serde_json::from_slice(&dry_run.stdout).expect("dry run JSON");
    assert_eq!(dry_run_json["action"], "dry_run");
    assert_eq!(dry_run_json["legacy_cleanup"]["action"], "removed");
    assert_eq!(
        fs::read_to_string(&dry_run_config).expect("read dry run after"),
        before
    );
}

#[test]
fn mcp_install_command_dry_run_reports_per_client_targets_for_all_clients() {
    let temp = TestDir::new("k-wiki-mcp-install-all-clients");
    let repository = temp.path().join("repository");
    let fake_home = temp.path().join("home");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&fake_home).expect("create fake home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    bootstrap_repository(binary, &repository);

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "all", "--scope", "local", "--dry-run"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("OPENCLAW_HOME", fake_home.join(".openclaw").as_path()),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("all clients JSON");
    let results = payload["results"].as_array().expect("all-client results");
    assert!(results.iter().any(|result| {
        result["client"] == "codex"
            && result["server_name"] == "k_wiki"
            && result["target_locality"] == "repository_local"
    }));
    assert!(results.iter().any(|result| {
        result["client"] == "generic"
            && result["server_name"]
                .as_str()
                .is_some_and(|name| name.starts_with("k_wiki_repository_"))
            && result["target_locality"] == "shared"
    }));
    assert!(results.iter().any(|result| {
        result["client"] == "copilot-studio" && result["target_locality"] == "manual"
    }));
    assert!(!repository.join(".codex/config.toml").exists());

    let installed = run_mcp_install(
        binary,
        &repository,
        &["--client", "all", "--scope", "local"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", fake_home.join(".codex").as_path()),
            ("OPENCLAW_HOME", fake_home.join(".openclaw").as_path()),
        ],
    );
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let installed_payload: Value =
        serde_json::from_slice(&installed.stdout).expect("installed all-client JSON");
    let installed_results = installed_payload["results"]
        .as_array()
        .expect("installed all-client results");
    assert_eq!(installed_results.len(), results.len());
    assert!(installed_results
        .iter()
        .all(|result| result["action"] != "failed"));
    assert!(repository.join(".codex/config.toml").is_file());
    assert!(repository.join(".mcp.json").is_file());
    assert!(repository.join(".vscode/mcp.json").is_file());
}

#[test]
fn mcp_install_command_preserves_other_repository_legacy_codex_registration() {
    let temp = TestDir::new("k-wiki-mcp-install-cross-file-cleanup");
    let repository = temp.path().join("repository");
    let structural_factory = temp.path().join("StructuralFactory");
    let fake_home = temp.path().join("home");
    let codex_home = fake_home.join(".codex");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&structural_factory).expect("create legacy repository");
    fs::create_dir_all(&codex_home).expect("create fake Codex home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);
    bootstrap_repository(binary, &structural_factory);
    let legacy_bundle = structural_factory
        .canonicalize()
        .expect("canonical legacy repository")
        .join("knowledge");
    fs::write(
        codex_home.join("config.toml"),
        format!(
            "model = \"example\"\n\n[mcp_servers.k_wiki]\ncommand = \"k-wiki\"\nargs = [\"mcp\", {}]\nstartup_timeout_sec = 60\n",
            serde_json::to_string(legacy_bundle.to_string_lossy().as_ref())
                .expect("serialize legacy bundle path"),
        ),
    )
    .expect("seed shared Codex config");

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "local"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", codex_home.as_path()),
        ],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("registration JSON");
    assert_eq!(payload["target_locality"], "repository_local");
    assert_eq!(
        payload["legacy_cleanup"]["shared_target"]["planned_action"],
        "renamed"
    );
    assert_eq!(payload["legacy_cleanup"]["action"], "renamed");
    let preserved_as = payload["legacy_cleanup"]["preserved_as"]
        .as_str()
        .expect("preserved server name");
    assert!(preserved_as.starts_with("k_wiki_structuralfactory_"));
    let local = fs::read_to_string(repository.join(".codex/config.toml"))
        .expect("read repository Codex config");
    assert!(local.contains("[mcp_servers.k_wiki]"));
    assert!(local.contains(
        repository
            .canonicalize()
            .expect("canonical repository")
            .join("knowledge")
            .to_string_lossy()
            .as_ref()
    ));
    let shared = fs::read_to_string(codex_home.join("config.toml"))
        .expect("read cleaned shared Codex config");
    assert!(shared.contains("model = \"example\""));
    assert!(!shared.contains("[mcp_servers.k_wiki]"));
    assert!(shared.contains(&format!("[mcp_servers.{preserved_as}]")));
    assert!(shared.contains(legacy_bundle.to_string_lossy().as_ref()));
}

#[test]
fn mcp_install_command_removes_same_repository_legacy_codex_registration() {
    let temp = TestDir::new("k-wiki-mcp-install-same-repo-legacy");
    let repository = temp.path().join("repository");
    let codex_home = temp.path().join("home/.codex");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&codex_home).expect("create Codex home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);
    let bundle = repository
        .canonicalize()
        .expect("canonical repository")
        .join("knowledge");
    fs::write(
        codex_home.join("config.toml"),
        format!(
            "model = \"example\"\n\n[mcp_servers.k_wiki]\ncommand = \"k-wiki\"\nargs = [\"mcp\", {}]\nstartup_timeout_sec = 60\n",
            serde_json::to_string(bundle.to_string_lossy().as_ref()).expect("serialize bundle"),
        ),
    )
    .expect("seed shared registration");

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "project"],
        &[
            ("HOME", temp.path().join("home").as_path()),
            ("CODEX_HOME", codex_home.as_path()),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).expect("install JSON");
    assert_eq!(payload["legacy_cleanup"]["action"], "removed");
    assert!(repository.join(".codex/config.toml").is_file());
    let shared = fs::read_to_string(codex_home.join("config.toml")).expect("read shared config");
    assert!(shared.contains("model = \"example\""));
    assert!(!shared.contains("[mcp_servers.k_wiki]"));
}

#[test]
fn mcp_install_command_rejects_conflicting_legacy_preservation_before_writing_local_config() {
    let temp = TestDir::new("k-wiki-mcp-install-preservation-conflict");
    let repository = temp.path().join("repository");
    let legacy_repository = temp.path().join("legacy");
    let codex_home = temp.path().join("home/.codex");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&legacy_repository).expect("create legacy repository");
    fs::create_dir_all(&codex_home).expect("create Codex home");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);
    bootstrap_repository(binary, &legacy_repository);
    let legacy_bundle = legacy_repository
        .canonicalize()
        .expect("canonical legacy repository")
        .join("knowledge");
    let config_path = codex_home.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "[mcp_servers.k_wiki]\ncommand = \"k-wiki\"\nargs = [\"mcp\", {}]\nstartup_timeout_sec = 60\n",
            serde_json::to_string(legacy_bundle.to_string_lossy().as_ref())
                .expect("serialize legacy bundle"),
        ),
    )
    .expect("seed legacy config");
    let envs = [
        ("HOME", temp.path().join("home")),
        ("CODEX_HOME", codex_home.clone()),
    ];
    let env_refs = envs
        .iter()
        .map(|(key, value)| (*key, value.as_path()))
        .collect::<Vec<_>>();
    let dry_run = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "project", "--dry-run"],
        &env_refs,
    );
    assert!(dry_run.status.success());
    let dry_run: Value = serde_json::from_slice(&dry_run.stdout).expect("dry-run JSON");
    let preserved_as = dry_run["legacy_cleanup"]["preserved_as"]
        .as_str()
        .expect("preserved name");
    let mut conflicting = fs::read_to_string(&config_path).expect("read config");
    conflicting.push_str(&format!(
        "\n[mcp_servers.{preserved_as}]\ncommand = \"other\"\nargs = [\"mcp\", \"/other/knowledge\"]\nstartup_timeout_sec = 60\n"
    ));
    fs::write(&config_path, &conflicting).expect("write conflict");

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "project"],
        &env_refs,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("destination"));
    assert_eq!(
        fs::read_to_string(&config_path).expect("read config after"),
        conflicting
    );
    assert!(!repository.join(".codex/config.toml").exists());
}

#[test]
fn mcp_install_command_rejects_unreadable_legacy_config_before_local_install() {
    let temp = TestDir::new("k-wiki-mcp-install-partial-migration");
    let repository = temp.path().join("repository");
    let fake_home = temp.path().join("home");
    let codex_home = fake_home.join(".codex");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(codex_home.join("config.toml"))
        .expect("create unreadable shared config target");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "local"],
        &[
            ("HOME", fake_home.as_path()),
            ("CODEX_HOME", codex_home.as_path()),
        ],
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to read MCP client config"),
        "{stderr}"
    );
    assert!(!repository.join(".codex/config.toml").exists());
}

#[test]
fn mcp_install_command_rejects_malformed_legacy_registration_before_local_install() {
    let temp = TestDir::new("k-wiki-mcp-install-malformed-legacy");
    let repository = temp.path().join("repository");
    let codex_home = temp.path().join("home/.codex");
    fs::create_dir_all(&repository).expect("create repository");
    fs::create_dir_all(&codex_home).expect("create Codex home");
    fs::write(
        codex_home.join("config.toml"),
        "[mcp_servers.k_wiki]\ncommand = \"k-wiki\"\n",
    )
    .expect("write malformed registration");
    let binary = env!("CARGO_BIN_EXE_k-wiki");
    bootstrap_repository(binary, &repository);

    let output = run_mcp_install(
        binary,
        &repository,
        &["--client", "codex", "--scope", "project"],
        &[
            ("HOME", temp.path().join("home").as_path()),
            ("CODEX_HOME", codex_home.as_path()),
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing args"));
    assert!(!repository.join(".codex/config.toml").exists());
}

#[test]
fn mcp_install_command_requires_an_initialized_knowledge_bundle() {
    let temp = TestDir::new("k-wiki-mcp-install-missing-bundle");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");

    let output = Command::new(env!("CARGO_BIN_EXE_k-wiki"))
        .args([
            "mcp",
            "install",
            "--client",
            "generic",
            "--repo-root",
            repository.to_str().expect("repository root"),
            "--client-config-path",
            temp.path()
                .join("mcp.json")
                .to_str()
                .expect("client config path"),
        ])
        .output()
        .expect("run MCP installer");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run k-wiki install first"));
}

#[test]
fn mcp_install_command_rejects_unsupported_clients_and_scopes() {
    let temp = TestDir::new("k-wiki-mcp-install-invalid-options");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository");
    let binary = env!("CARGO_BIN_EXE_k-wiki");

    let bootstrap = Command::new(binary)
        .args([
            "install",
            "--repo-root",
            repository.to_str().expect("repository root"),
        ])
        .output()
        .expect("bootstrap repository");
    assert!(bootstrap.status.success());

    for (option, value, expected_error) in [
        ("--client", "unknown", "unsupported MCP client"),
        (
            "--scope",
            "invalid",
            "MCP install scope must be local, user, or project",
        ),
    ] {
        let output = Command::new(binary)
            .args([
                "mcp",
                "install",
                "--client",
                "generic",
                option,
                value,
                "--repo-root",
                repository.to_str().expect("repository root"),
                "--client-config-path",
                temp.path()
                    .join("mcp.json")
                    .to_str()
                    .expect("client config path"),
            ])
            .output()
            .expect("run MCP installer");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected_error));
    }
}

#[test]
fn mcp_stdio_binary_advertises_the_packaged_knowledge_wiki_schema() {
    let temp = TestDir::new("k-wiki-mcp-binary");
    let bundle = temp.path().join("docs");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
    )
    .expect("write bundle index");

    let mut child = Command::new(env!("CARGO_BIN_EXE_k-wiki"))
        .arg("mcp")
        .arg(&bundle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start MCP binary");
    {
        let mut stdin = child.stdin.take().expect("MCP stdin");
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
        .expect("write initialize");
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        )
        .expect("write tools list");
    }

    let output = child.wait_with_output().expect("wait for MCP binary");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .expect("UTF-8 MCP output")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON-RPC response"))
        .collect::<Vec<_>>();
    assert_eq!(
        responses[0]["result"]["serverInfo"]["name"],
        "Knowledge Wiki"
    );
    let tools = responses[1]["result"]["tools"]
        .as_array()
        .expect("tool list");
    assert_eq!(tools.len(), 14);
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "wiki_populate_page"
            && tool["annotations"]["wikiAccess"] == "write"));
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "wiki_build" && tool["annotations"]["wikiAccess"] == "write"));
}

#[tokio::test]
async fn preview_http_dispatches_health_and_serves_static_content_with_security_headers() {
    let temp = TestDir::new("k-wiki-http");
    let bundle = temp.path().join("docs");
    let site = temp.path().join("site");
    fs::create_dir_all(&bundle).expect("create bundle");
    fs::create_dir_all(&site).expect("create site");
    fs::write(
        bundle.join("index.md"),
        "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
    )
    .expect("write root index");
    fs::write(site.join("index.html"), "<h1>Knowledge Wiki</h1>").expect("write site");

    let api = LocalWikiService::new(vec![bundle]).into_api();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind preview");
    let address = listener.local_addr().expect("preview address");
    let server = tokio::spawn(async move {
        axum::serve(listener, http::preview_router(api, site))
            .await
            .expect("serve preview");
    });

    let health = http_request(address, "/healthz").await;
    assert!(health.starts_with("HTTP/1.1 200 OK"));
    assert!(health
        .to_ascii_lowercase()
        .contains("content-security-policy:"));
    assert!(health.contains("\"kind\":\"health\""));

    let page = http_request(address, "/").await;
    assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
    assert!(page.contains("<h1>Knowledge Wiki</h1>"));
    assert!(page.to_ascii_lowercase().contains("x-frame-options: deny"));

    server.abort();
}

#[test]
fn wiki_graph_context_and_service_do_not_import_graph_internals() {
    let graph_context = include_str!("../src/graph_context.rs");
    let refresh = include_str!("../src/refresh.rs");
    let service = include_str!("../src/service.rs");
    for source in [graph_context, refresh, service] {
        assert!(!source.contains("api::core"));
        assert!(!source.contains("api::refresh"));
        assert!(!source.contains("crate::storage"));
        assert!(!source.contains("src/adapters"));
    }
    assert!(refresh.contains("OperationRequest::Refresh"));
    assert!(refresh.contains("CodebaseGraphApi::new"));
}

fn call_tool(
    api: &k_wiki::api::OkfWikiApi<LocalWikiService>,
    session: &mut mcp::McpSession,
    id: u64,
    tool: &str,
    arguments: Value,
) -> Value {
    mcp::handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}
        }),
        session,
        &mut |tool_name| {
            mcp_operation_descriptor(tool_name)
                .map(|descriptor| (descriptor.request_schema)())
                .unwrap_or_else(|| json!({"type": "object"}))
        },
        &mut |tool_name, arguments| mcp::dispatch_api(api, tool_name, arguments),
    )
    .expect("MCP response")
}

fn mcp_session() -> mcp::McpSession {
    mcp::McpSession {
        protocol_version: Some(mcp::protocol_version().to_string()),
        initialized: true,
    }
}

async fn http_request(address: std::net::SocketAddr, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect preview");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .expect("write request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("read response");
    String::from_utf8(response).expect("UTF-8 HTTP response")
}

fn bootstrap_repository(binary: &str, repository: &Path) {
    let output = Command::new(binary)
        .args([
            "install",
            "--repo-root",
            repository.to_str().expect("repository root"),
        ])
        .output()
        .expect("bootstrap repository");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_mcp_install(
    binary: &str,
    repository: &Path,
    extra_args: &[&str],
    envs: &[(&str, &Path)],
) -> std::process::Output {
    let mut command = Command::new(binary);
    command.args([
        "mcp",
        "install",
        "--repo-root",
        repository.to_str().expect("repository root"),
    ]);
    command.args(extra_args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run MCP install")
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
