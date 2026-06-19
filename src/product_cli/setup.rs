use super::*;

pub(super) fn run_setup<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = SetupOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", setup_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }

    let source_root = options
        .repo_root
        .canonicalize()
        .map_err(|error| format!("failed to resolve repo root: {error}"))?;
    let paths = GraphStatePaths::derive(&source_root);
    if source_root
        .components()
        .any(|component| component.as_os_str() == ".codebaseGraph")
    {
        return Err(format!(
            "Repository root may not be inside a .codebaseGraph state directory: {}",
            source_root.display()
        ));
    }
    let materialize_options = MaterializeOptions {
        source_root: Some(source_root.clone()),
        db: Some(paths.db_path.clone()),
        manifest: Some(paths.manifest_path.clone()),
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: true,
        ..MaterializeOptions::default()
    };
    let config_payload = setup_config_payload(&paths, &source_root);
    let instructions_path = instruction_target_path(&source_root, &options.instructions_target)?;
    let state_dir_existed = paths.state_dir.exists();
    let previous_config = snapshot_file(&paths.config_path)?;
    let previous_instructions = match instructions_path.as_ref() {
        Some(path) => Some((path.clone(), snapshot_file(path)?)),
        None => None,
    };
    let (config_action, instructions, mcp_config, materialization) = if options.dry_run {
        let request = build_request(&materialize_options)?;
        let materialization = dry_run_materialization_payload(&request, &paths);
        let config_action = if json_file_would_change(&paths.config_path, &config_payload)? {
            "dry_run"
        } else {
            "unchanged"
        };
        let instructions = json!({
            "action": if instructions_path.is_some() { "dry_run" } else { "skipped" },
            "path": instructions_path.as_ref().map(|path| path.to_string_lossy().to_string()),
        });
        let mcp_config = setup_mcp_config(&options, &paths, true)?;
        (
            config_action.to_string(),
            instructions,
            mcp_config,
            materialization,
        )
    } else {
        fs::create_dir_all(&paths.state_dir).map_err(|error| {
            format!(
                "failed to create state directory {}: {error}",
                paths.state_dir.display()
            )
        })?;
        let result = (|| {
            let config_action = write_setup_config(&paths, &source_root)?;
            let instructions = upsert_instruction_block(
                &source_root,
                &options.instructions_target,
                &paths.config_path,
            )?;
            let (_, response) = materialize(&materialize_options)?;
            let mcp_config = setup_mcp_config(&options, &paths, false)?;
            Ok::<_, String>((
                config_action.to_string(),
                instructions,
                mcp_config,
                materialization_payload(&response, &materialize_options.mode, &paths),
            ))
        })();
        match result {
            Ok(result) => result,
            Err(error) => {
                restore_file(&paths.config_path, previous_config.as_deref())?;
                if let Some((path, previous)) = previous_instructions.as_ref() {
                    restore_file(path, previous.as_deref())?;
                }
                if !state_dir_existed {
                    let _ = fs::remove_dir_all(&paths.state_dir);
                }
                return Err(error);
            }
        }
    };
    let output = json!({
        "ok": true,
        "repo_root": source_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "db_path": paths.db_path,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "config_path": paths.config_path,
        "config_action": config_action,
        "mcp_config": mcp_config,
        "instructions": instructions,
        "materialization": materialization,
        "database_written": materialization.get("database_written").cloned().unwrap_or(json!(false)),
        "skipped": materialization.get("skipped").cloned().unwrap_or(json!(0)),
        "node_rows": materialization.get("node_rows").cloned().unwrap_or(json!(0)),
        "edge_rows": materialization.get("edge_rows").cloned().unwrap_or(json!(0)),
        "connector_rows": materialization.get("connector_rows").cloned().unwrap_or(json!(0)),
        "diagnostics": materialization.get("diagnostics").cloned().unwrap_or(json!([])),
    });
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
pub(super) fn setup_mcp_config(
    options: &SetupOptions,
    paths: &GraphStatePaths,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let descriptor = build_mcp_descriptor(&McpInstallOptions {
        client: "generic".to_string(),
        scope: "local".to_string(),
        name: Some("codebase_graph".to_string()),
        config_path: Some(paths.config_path.clone()),
        client_config_path: None,
        repo_root: paths
            .state_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        dry_run: true,
        verify: false,
        json: true,
        help: false,
    })?;
    if options.skip_mcp_config || options.mcp_client == "none" {
        return Ok(json!({
            "action": "skipped",
            "client": options.mcp_client,
            "scope": "local",
            "server_name": descriptor.name,
            "method": serde_json::Value::Null,
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": descriptor.stdio_entry(false, true),
        }));
    }
    install_mcp_client(&McpInstallOptions {
        client: options.mcp_client.clone(),
        scope: if options.mcp_client == "claude-project" {
            "project".to_string()
        } else {
            "local".to_string()
        },
        name: Some("codebase_graph".to_string()),
        config_path: Some(paths.config_path.clone()),
        client_config_path: options.mcp_config_path.clone(),
        repo_root: paths
            .state_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        dry_run,
        verify: false,
        json: true,
        help: false,
    })
}
pub(super) fn setup_config_payload(paths: &GraphStatePaths, repo_root: &Path) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "repo_root": repo_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "ontology_version": "code_ontology_v1",
        "package_version": env!("CARGO_PKG_VERSION"),
        "materialization": {
            "include": [],
            "exclude": []
        },
        "mcp": {
            "server_name": "codebase_graph",
            "command": [
                server_command(),
                "mcp",
                "serve",
                "--config",
                paths.config_path.to_string_lossy()
            ]
        }
    })
}

pub(super) fn write_setup_config(
    paths: &GraphStatePaths,
    repo_root: &Path,
) -> Result<&'static str, String> {
    let payload = setup_config_payload(paths, repo_root);
    let mut action = "created";
    if paths.config_path.exists() {
        let previous = read_json_file(&paths.config_path)?;
        if previous == payload {
            return Ok("unchanged");
        }
        action = "updated";
    }
    if let Some(parent) = paths.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    fs::write(&paths.config_path, format!("{text}\n")).map_err(|error| {
        format!(
            "failed to write setup config {}: {error}",
            paths.config_path.display()
        )
    })?;
    Ok(action)
}

pub(super) fn json_file_would_change(
    path: &Path,
    payload: &serde_json::Value,
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(true);
    }
    Ok(read_json_file(path)? != *payload)
}

pub(super) fn instruction_target_path(
    repo_root: &Path,
    target: &str,
) -> Result<Option<PathBuf>, String> {
    match target {
        "skip" => Ok(None),
        "agents" => Ok(Some(repo_root.join("AGENTS.md"))),
        "claude" => Ok(Some(repo_root.join("CLAUDE.md"))),
        "auto" => {
            let agents = repo_root.join("AGENTS.md");
            if agents.exists() {
                return Ok(Some(agents));
            }
            let claude = repo_root.join("CLAUDE.md");
            if claude.exists() {
                return Ok(Some(claude));
            }
            Ok(Some(agents))
        }
        other => Err(format!("Unsupported instruction target: {other}")),
    }
}

pub(super) fn upsert_instruction_block(
    repo_root: &Path,
    target: &str,
    config_path: &Path,
) -> Result<serde_json::Value, String> {
    let Some(path) = instruction_target_path(repo_root, target)? else {
        return Ok(json!({"action": "skipped", "path": serde_json::Value::Null}));
    };
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let created = !path.exists();
    let block = instruction_block(config_path);
    let (next_text, action) = upsert_instruction_text(&existing, &block, created);
    if next_text == existing {
        return Ok(json!({"action": "unchanged", "path": path.to_string_lossy()}));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create instruction directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, next_text)
        .map_err(|error| format!("failed to write instructions {}: {error}", path.display()))?;
    Ok(json!({"action": action, "path": path.to_string_lossy()}))
}

pub(super) fn instruction_block(config_path: &Path) -> String {
    format!(
        "<!-- codebaseGraph:start -->\n\
## codebaseGraph workflow\n\
- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.\n\
- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.\n\
- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: \"json\"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.\n\
- Use MCP `graph_search` with `detail: \"slim\"` and `context_limit: 1` before answering repo-structure questions or performing coding tasks.\n\
- Use MCP `graph_context` with `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.\n\
- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.\n\
- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.\n\
- If MCP tools are unavailable, fall back to CLI: `{command} graph-search <query> --repo-root . --no-refresh --detail slim --context-limit 1`, `{command} graph-context <query> --repo-root . --profile <profile> --no-refresh --detail slim --context-limit 2`, `{command} graph-architecture-queries`, `{command} graph-query \"<statement>\" --repo-root .`, `{command} graph-schema`, and `{command} graph-query-helpers`.\n\
- Refresh the graph with `{command} setup --repo-root . --mcp-client none` when files change materially. Setup config: `{config_path}`.\n\
<!-- codebaseGraph:end -->\n",
        command = server_command(),
        config_path = config_path.to_string_lossy(),
    )
}

pub(super) fn upsert_instruction_text(
    existing: &str,
    block: &str,
    created: bool,
) -> (String, &'static str) {
    const START: &str = "<!-- codebaseGraph:start -->";
    const END: &str = "<!-- codebaseGraph:end -->";
    if existing.trim().is_empty() {
        return (block.to_string(), "created");
    }
    let Some(start) = existing.find(START) else {
        let separator = if existing.ends_with('\n') { "" } else { "\n" };
        let action = if created { "created" } else { "updated" };
        return (
            format!("{}{separator}\n{}", existing.trim_end(), block),
            action,
        );
    };
    let Some(end) = existing.find(END) else {
        return (
            format!("{}\n\n{}", existing.trim_end(), block),
            if created { "created" } else { "updated" },
        );
    };
    if end < start {
        return (
            format!("{}\n\n{}", existing.trim_end(), block),
            if created { "created" } else { "updated" },
        );
    }
    let after_end = end + END.len();
    let text = format!(
        "{}\n\n{}\n\n{}",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start()
    )
    .trim()
    .to_string()
        + "\n";
    (text, "updated")
}
#[derive(Debug)]
pub(super) struct GraphStatePaths {
    pub(super) repo_name: String,
    pub(super) state_dir: PathBuf,
    pub(super) db_path: PathBuf,
    pub(super) manifest_path: PathBuf,
    pub(super) config_path: PathBuf,
}

impl GraphStatePaths {
    pub(super) fn derive(repo_root: &Path) -> Self {
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
            config_path: state_dir.join("config.json"),
            state_dir,
            repo_name,
        }
    }
}

pub(super) fn safe_name(value: &str) -> String {
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
