use super::*;

pub(super) fn run_mcp_install<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = McpInstallOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", mcp_install_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = if options.client == "all" {
        let results = supported_install_clients()
            .into_iter()
            .map(|client| {
                let mut client_options = options.clone();
                client_options.client = client.to_string();
                install_mcp_client(&client_options).unwrap_or_else(|error| {
                    json!({
                        "action": "failed",
                        "client": client,
                        "scope": install_scope(client, &client_options.scope),
                        "server_name": client_options.name.clone().unwrap_or_else(|| "codebase_graph".to_string()),
                        "method": serde_json::Value::Null,
                        "path": serde_json::Value::Null,
                        "command": serde_json::Value::Null,
                        "descriptor": {},
                        "entry": {},
                        "error": error,
                    })
                })
            })
            .collect::<Vec<_>>();
        json!({ "results": results })
    } else {
        install_mcp_client(&options)?
    };
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
#[derive(Debug, Clone)]
pub(super) struct McpInstallOptions {
    pub(super) client: String,
    pub(super) scope: String,
    pub(super) name: Option<String>,
    pub(super) config_path: Option<PathBuf>,
    pub(super) client_config_path: Option<PathBuf>,
    pub(super) repo_root: PathBuf,
    pub(super) dry_run: bool,
    pub(super) verify: bool,
    pub(super) json: bool,
    pub(super) help: bool,
}

impl McpInstallOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            client: "codex".to_string(),
            scope: "local".to_string(),
            name: None,
            config_path: None,
            client_config_path: None,
            repo_root: PathBuf::from("."),
            dry_run: false,
            verify: false,
            json: false,
            help: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--client" => {
                    options.client = required_arg(args, index, "--client")?.to_string();
                    if options.client != "all"
                        && !supported_install_clients().contains(&options.client.as_str())
                    {
                        return Err(format!(
                            "Unsupported MCP client: {}. Supported clients: {}",
                            options.client,
                            supported_install_clients_with_all().join(", ")
                        ));
                    }
                    index += 2;
                }
                "--scope" => {
                    options.scope = required_arg(args, index, "--scope")?.to_string();
                    if !matches!(options.scope.as_str(), "local" | "user" | "project") {
                        return Err(
                            "Unsupported MCP install scope: expected local, user, or project"
                                .to_string(),
                        );
                    }
                    index += 2;
                }
                "--name" => {
                    options.name = Some(required_arg(args, index, "--name")?.to_string());
                    index += 2;
                }
                "--config-path" => {
                    options.config_path =
                        Some(expand_path(required_arg(args, index, "--config-path")?));
                    index += 2;
                }
                "--client-config-path" => {
                    options.client_config_path = Some(expand_path(required_arg(
                        args,
                        index,
                        "--client-config-path",
                    )?));
                    index += 2;
                }
                "--repo-root" => {
                    options.repo_root = expand_path(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--dry-run" => {
                    options.dry_run = true;
                    index += 1;
                }
                "--verify" => {
                    options.verify = true;
                    index += 1;
                }
                "--json" => {
                    options.json = true;
                    index += 1;
                }
                "--format" | "--output-format" => {
                    let value = required_arg(args, index, args[index].as_str())?;
                    if value != "json" && value != "block" {
                        return Err("--format must be json or block".to_string());
                    }
                    options.json = value == "json";
                    index += 2;
                }
                other => {
                    return Err(format!(
                        "unknown mcp install option: {other}\n\n{}",
                        mcp_install_help()
                    ))
                }
            }
        }
        Ok(options)
    }
}

#[derive(Debug, Default)]
pub(super) struct McpHttpState {
    pub(super) sessions: BTreeMap<String, McpSession>,
    pub(super) next_session: u64,
}

impl McpHttpState {
    pub(super) fn next_session_id(&mut self) -> String {
        self.next_session += 1;
        format!("native-http-session-{}", self.next_session)
    }
}

pub(super) fn install_mcp_client(options: &McpInstallOptions) -> Result<serde_json::Value, String> {
    let descriptor = build_mcp_descriptor(options)?;
    if options.client == "copilot-studio" || options.client == "microsoft-copilot" {
        let metadata = copilot_studio_metadata(&descriptor);
        let payload = json!({
            "action": if options.dry_run { "dry_run" } else { "reported" },
            "client": options.client,
            "scope": options.scope,
            "server_name": descriptor.name,
            "method": "manual_metadata",
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": metadata["stdio"].clone(),
            "payload": metadata,
        });
        return attach_install_verification(payload, &descriptor, options);
    }
    let native_command = native_client_command(&options.client, &descriptor, &options.scope);
    let native_available = native_command
        .as_ref()
        .and_then(|command| command.first())
        .is_some_and(|executable| executable_in_path(executable));
    if options.dry_run && options.client_config_path.is_none() && native_available {
        return attach_install_verification(
            json!({
                "action": "dry_run",
                "client": options.client,
                "scope": install_scope(&options.client, &options.scope),
                "server_name": descriptor.name,
                "method": "native_cli",
                "path": serde_json::Value::Null,
                "command": native_command,
                "descriptor": descriptor.as_json(),
                "entry": descriptor.stdio_entry(false, false),
            }),
            &descriptor,
            options,
        );
    }
    if !options.dry_run && options.client_config_path.is_none() && native_available {
        let Some(command) = native_command.clone() else {
            return file_adapter_result(options, &descriptor, native_command, None);
        };
        let completed = Command::new(&command[0])
            .args(&command[1..])
            .output()
            .map_err(|error| format!("failed to run native client installer: {error}"))?;
        if completed.status.success() {
            return attach_install_verification(
                json!({
                    "action": "updated",
                    "client": options.client,
                    "scope": install_scope(&options.client, &options.scope),
                    "server_name": descriptor.name,
                    "method": "native_cli",
                    "path": serde_json::Value::Null,
                    "command": command,
                    "descriptor": descriptor.as_json(),
                    "entry": descriptor.stdio_entry(false, false),
                }),
                &descriptor,
                options,
            );
        }
        let error = subprocess_error(&completed);
        return file_adapter_result(options, &descriptor, Some(command), Some(error));
    }
    let native_error = native_command.as_ref().and_then(|command| {
        command.first().and_then(|executable| {
            if executable_in_path(executable) {
                None
            } else {
                Some(format!("{executable} executable not found"))
            }
        })
    });
    file_adapter_result(options, &descriptor, native_command, native_error)
}

#[derive(Debug, Clone)]
pub(super) struct NativeMcpDescriptor {
    pub(super) name: String,
    pub(super) command: String,
    pub(super) args: Vec<String>,
    pub(super) setup_config_path: String,
    pub(super) repo_root: String,
    pub(super) timeout: u64,
}

impl NativeMcpDescriptor {
    pub(super) fn as_json(&self) -> serde_json::Value {
        json!({
            "name": self.name,
            "transport": "stdio",
            "command": self.command,
            "args": self.args,
            "env": {},
            "cwd": serde_json::Value::Null,
            "setup_config_path": self.setup_config_path,
            "repo_root": self.repo_root,
            "timeout": self.timeout,
            "tool_policy": "graph_query_read_only",
        })
    }

    pub(super) fn stdio_entry(
        &self,
        include_type: bool,
        include_timeout: bool,
    ) -> serde_json::Value {
        let mut entry = serde_json::Map::new();
        entry.insert("command".to_string(), json!(self.command));
        entry.insert("args".to_string(), json!(self.args));
        if include_type {
            entry.insert("type".to_string(), json!("stdio"));
        }
        if include_timeout {
            entry.insert("startup_timeout_sec".to_string(), json!(self.timeout));
        }
        serde_json::Value::Object(entry)
    }
}

pub(super) fn build_mcp_descriptor(
    options: &McpInstallOptions,
) -> Result<NativeMcpDescriptor, String> {
    let config_path = options.config_path.clone().unwrap_or_else(|| {
        GraphStatePaths::derive(&expand_path(options.repo_root.to_string_lossy().as_ref()))
            .config_path
    });
    let setup_config = if config_path.exists() {
        Some(read_json_file(&config_path)?)
    } else {
        None
    };
    let repo_root = setup_config
        .as_ref()
        .and_then(|payload| payload.get("repo_root"))
        .and_then(serde_json::Value::as_str)
        .map(expand_path)
        .unwrap_or_else(|| {
            config_path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| options.repo_root.clone())
        });
    let repo_name = setup_config
        .as_ref()
        .and_then(|payload| payload.get("repo_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            safe_name(
                repo_root
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("repository"),
            )
        });
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| format!("codebase_graph_{}", install_safe_name(&repo_name)));
    let command_from_config = setup_config
        .as_ref()
        .and_then(|payload| payload.pointer("/mcp/command"))
        .and_then(serde_json::Value::as_array)
        .and_then(|values| {
            let command: Option<Vec<String>> = values
                .iter()
                .map(|value| value.as_str().map(str::to_string))
                .collect();
            command.filter(|parts| parts.len() >= 5)
        });
    let (command, args) = if let Some(mut parts) = command_from_config {
        let command = parts.remove(0);
        (command, parts)
    } else {
        (
            server_command(),
            vec![
                "mcp".to_string(),
                "serve".to_string(),
                "--config".to_string(),
                config_path.to_string_lossy().to_string(),
            ],
        )
    };
    Ok(NativeMcpDescriptor {
        name,
        command,
        args,
        setup_config_path: config_path.to_string_lossy().to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        timeout: 60,
    })
}

pub(super) fn file_adapter_result(
    options: &McpInstallOptions,
    descriptor: &NativeMcpDescriptor,
    native_command: Option<Vec<String>>,
    native_error: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = options.client_config_path.clone().unwrap_or_else(|| {
        default_client_config_path(
            &options.client,
            &install_scope(&options.client, &options.scope),
            descriptor,
        )
    });
    let existing = fs::read_to_string(&path).ok();
    let rendered = render_client_config(
        &options.client,
        &install_scope(&options.client, &options.scope),
        existing.as_deref(),
        descriptor,
    )?;
    let action = if options.dry_run {
        "dry_run".to_string()
    } else {
        rendered.action.clone()
    };
    if !options.dry_run {
        write_text_atomic(&path, &rendered.text)?;
    }
    let mut payload = json!({
        "action": action,
        "client": options.client,
        "scope": install_scope(&options.client, &options.scope),
        "server_name": descriptor.name,
        "method": "file_adapter",
        "path": path.to_string_lossy(),
        "command": serde_json::Value::Null,
        "descriptor": descriptor.as_json(),
        "entry": rendered.entry,
        "patch": rendered.patch,
        "payload": rendered.payload,
    });
    if let Some(command) = native_command {
        payload["native_command"] = json!(command);
    }
    if let Some(error) = native_error {
        payload["native_error"] = json!(error);
    }
    attach_install_verification(payload, descriptor, options)
}

pub(super) fn attach_install_verification(
    mut payload: serde_json::Value,
    descriptor: &NativeMcpDescriptor,
    options: &McpInstallOptions,
) -> Result<serde_json::Value, String> {
    if options.verify && !options.dry_run {
        payload["verification"] = verify_mcp_install(descriptor, &options.client);
    }
    Ok(payload)
}

pub(super) fn verify_mcp_install(
    descriptor: &NativeMcpDescriptor,
    client: &str,
) -> serde_json::Value {
    let stdio = verify_stdio(descriptor);
    let visibility = verify_client_visibility(client, &descriptor.name);
    json!({
        "ok": stdio.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false)
            && visibility.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(true),
        "stdio": stdio,
        "client_visibility": visibility,
    })
}

pub(super) fn verify_stdio(descriptor: &NativeMcpDescriptor) -> serde_json::Value {
    let command = descriptor_command(descriptor);
    let payload = [
        stdio_json_rpc_message(
            "initialize",
            json!({"protocolVersion": LATEST_PROTOCOL_VERSION}),
            1,
        ),
        stdio_json_rpc_message("tools/list", json!({}), 2),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_health", "arguments": {"include_structured_content": true}}),
            3,
        ),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_search", "arguments": {"query": descriptor.name, "limit": 1}}),
            4,
        ),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_query", "arguments": {"statement": "MATCH (n) DELETE n"}}),
            5,
        ),
    ]
    .join("");
    let mut process = match Command::new(&command[0])
        .args(&command[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(process) => process,
        Err(error) => return json!({"ok": false, "command": command, "error": error.to_string()}),
    };
    if let Some(mut stdin) = process.stdin.take() {
        if let Err(error) = stdin.write_all(payload.as_bytes()) {
            return json!({"ok": false, "command": command, "error": error.to_string()});
        }
    }
    let output = match process.wait_with_output() {
        Ok(output) => output,
        Err(error) => return json!({"ok": false, "command": command, "error": error.to_string()}),
    };
    let responses = parse_stdio_json_lines(&output.stdout);
    if !output.status.success() {
        return json!({
            "ok": false,
            "command": command,
            "returncode": output.status.code().unwrap_or(1),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "responses": responses,
        });
    }
    let checks = stdio_checks(&responses);
    json!({
        "ok": checks.values().all(|value| *value),
        "command": command,
        "checks": checks,
        "responses": responses,
        "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub(super) fn verify_client_visibility(client: &str, server_name: &str) -> serde_json::Value {
    let Some(command) = visibility_command(client) else {
        return json!({"ok": true, "skipped": true, "reason": format!("{client} has no CLI visibility check")});
    };
    if !executable_in_path(&command[0]) {
        return json!({"ok": true, "skipped": true, "reason": format!("{} executable not found", command[0])});
    }
    match Command::new(&command[0]).args(&command[1..]).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let found = stdout.contains(server_name) || stderr.contains(server_name);
            json!({
                "ok": output.status.success() && found,
                "command": command,
                "returncode": output.status.code().unwrap_or(1),
                "found": found,
                "stdout": stdout,
                "stderr": stderr,
            })
        }
        Err(error) => json!({"ok": false, "command": command, "error": error.to_string()}),
    }
}

pub(super) fn descriptor_command(descriptor: &NativeMcpDescriptor) -> Vec<String> {
    let mut command = vec![descriptor.command.clone()];
    command.extend(descriptor.args.clone());
    command
}

pub(super) fn stdio_json_rpc_message(method: &str, params: serde_json::Value, id: u64) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap_or_else(|_| "{}".to_string())
        + "\n"
}

pub(super) fn parse_stdio_json_lines(data: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(data)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

pub(super) fn stdio_checks(responses: &[serde_json::Value]) -> BTreeMap<String, bool> {
    let mut by_id = BTreeMap::new();
    for response in responses {
        if let Some(id) = response.get("id").and_then(serde_json::Value::as_u64) {
            by_id.insert(id, response);
        }
    }
    let tools = by_id
        .get(&2)
        .and_then(|value| value.pointer("/result/tools"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tool_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut checks = BTreeMap::new();
    checks.insert(
        "initialize".to_string(),
        by_id
            .get(&1)
            .and_then(|value| value.pointer("/result/protocolVersion"))
            .and_then(serde_json::Value::as_str)
            == Some(LATEST_PROTOCOL_VERSION),
    );
    checks.insert(
        "tools_list".to_string(),
        tool_names.contains("graph_health") && tool_names.contains("graph_search"),
    );
    checks.insert(
        "graph_health".to_string(),
        by_id
            .get(&3)
            .and_then(|value| value.pointer("/result/structuredContent/ok"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    );
    checks.insert(
        "graph_search".to_string(),
        by_id
            .get(&4)
            .is_some_and(|value| value.get("error").is_none()),
    );
    checks.insert(
        "tool_error_result".to_string(),
        by_id
            .get(&5)
            .and_then(|value| value.pointer("/result/isError"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    );
    checks
}

pub(super) fn visibility_command(client: &str) -> Option<Vec<String>> {
    match client {
        "codex" => Some(vec![
            "codex".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        "claude" | "claude-project" => Some(vec![
            "claude".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        "openclaw" => Some(vec![
            "openclaw".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        _ => None,
    }
}

pub(super) struct RenderedNativeConfig {
    pub(super) text: String,
    pub(super) action: String,
    pub(super) entry: serde_json::Value,
    pub(super) patch: serde_json::Value,
    pub(super) payload: serde_json::Value,
}

pub(super) fn render_client_config(
    client: &str,
    scope: &str,
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    match adapter_id(client, scope) {
        "codex" => render_codex_config(existing, descriptor),
        "hermes" => render_hermes_config(existing, descriptor),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            render_json_config(adapter_id(client, scope), existing, descriptor)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

pub(super) fn render_json_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let mut payload = existing
        .filter(|text| !text.trim().is_empty())
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
        .unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Err("MCP config must contain a JSON object".to_string());
    }
    let root_path = match adapter {
        "github-copilot" => vec!["servers"],
        "openclaw" => vec!["mcp", "servers"],
        _ => vec!["mcpServers"],
    };
    let include_type = !matches!(adapter, "claude" | "generic");
    let entry = descriptor.stdio_entry(include_type, false);
    let previous = json_container_mut(&mut payload, &root_path)?
        .insert(descriptor.name.clone(), entry.clone());
    let action = action_for_json(previous.as_ref(), &entry, existing.is_some());
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())? + "\n";
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else {
        action
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: payload.clone(),
        payload,
    })
}

pub(super) fn render_codex_config(
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(false, true);
    let patch = codex_toml_block(descriptor);
    let (text, previous) =
        upsert_toml_block(existing.unwrap_or_default(), &descriptor.name, &patch);
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else if previous.is_none() {
        "created".to_string()
    } else if previous.as_deref() == Some(patch.trim_end()) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: json!(patch),
        payload: json!(patch),
    })
}

pub(super) fn render_hermes_config(
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(true, false);
    let patch = hermes_yaml_block(descriptor);
    let (text, previous) = upsert_marked_block(existing.unwrap_or_default(), &patch);
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else if previous.is_none() {
        "created".to_string()
    } else if previous.as_deref() == Some(patch.trim_end()) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: json!(patch),
        payload: json!(patch),
    })
}

pub(super) fn json_container_mut<'a>(
    payload: &'a mut serde_json::Value,
    path: &[&str],
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>, String> {
    let mut cursor = payload
        .as_object_mut()
        .ok_or_else(|| "MCP config must contain a JSON object".to_string())?;
    for key in path {
        let next = cursor
            .entry((*key).to_string())
            .or_insert_with(|| json!({}));
        cursor = next
            .as_object_mut()
            .ok_or_else(|| format!("MCP config key must contain an object: {}", path.join(".")))?;
    }
    Ok(cursor)
}

pub(super) fn action_for_json(
    previous: Option<&serde_json::Value>,
    next_value: &serde_json::Value,
    file_exists: bool,
) -> String {
    if !file_exists {
        "created".to_string()
    } else if previous == Some(next_value) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    }
}

pub(super) fn default_client_config_path(
    client: &str,
    scope: &str,
    descriptor: &NativeMcpDescriptor,
) -> PathBuf {
    let home = home_dir();
    match adapter_id(client, scope) {
        "codex" => env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"))
            .join("config.toml"),
        "claude" => {
            let mac_path =
                home.join("Library/Application Support/Claude/claude_desktop_config.json");
            if mac_path.parent().is_some_and(Path::exists) {
                mac_path
            } else {
                home.join(".config/claude/claude_desktop_config.json")
            }
        }
        "claude-project" => PathBuf::from(&descriptor.repo_root).join(".mcp.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "github-copilot" => PathBuf::from(&descriptor.repo_root).join(".vscode/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"))
            .join("mcp.json5"),
        _ => home.join(".config/mcp/mcp.json"),
    }
}

pub(super) fn supported_install_clients() -> Vec<&'static str> {
    vec![
        "claude",
        "claude-project",
        "codex",
        "copilot-studio",
        "generic",
        "github-copilot",
        "hermes",
        "lmstudio",
        "microsoft-copilot",
        "openclaw",
    ]
}

pub(super) fn install_safe_name(value: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    normalized.trim_matches(['.', '_', '-']).to_string()
}

pub(super) fn supported_install_clients_with_all() -> Vec<&'static str> {
    let mut clients = supported_install_clients();
    clients.push("all");
    clients
}

pub(super) fn install_scope(client: &str, scope: &str) -> String {
    if client == "claude-project" {
        "project".to_string()
    } else {
        scope.to_string()
    }
}

pub(super) fn adapter_id<'a>(client: &'a str, scope: &str) -> &'a str {
    if client == "claude" && scope == "project" {
        "claude-project"
    } else {
        client
    }
}

pub(super) fn native_client_command(
    client: &str,
    descriptor: &NativeMcpDescriptor,
    scope: &str,
) -> Option<Vec<String>> {
    match client {
        "codex" => Some(vec![
            "codex".to_string(),
            "mcp".to_string(),
            "add".to_string(),
            descriptor.name.clone(),
            "--".to_string(),
            descriptor.command.clone(),
            descriptor.args[0].clone(),
            descriptor.args[1].clone(),
            descriptor.args[2].clone(),
            descriptor.args[3].clone(),
        ]),
        "claude" | "claude-project" => Some(vec![
            "claude".to_string(),
            "mcp".to_string(),
            "add".to_string(),
            "--transport".to_string(),
            "stdio".to_string(),
            "--scope".to_string(),
            install_scope(client, scope),
            descriptor.name.clone(),
            "--".to_string(),
            descriptor.command.clone(),
            descriptor.args[0].clone(),
            descriptor.args[1].clone(),
            descriptor.args[2].clone(),
            descriptor.args[3].clone(),
        ]),
        "openclaw" => Some(vec![
            "openclaw".to_string(),
            "mcp".to_string(),
            "set".to_string(),
            descriptor.name.clone(),
            serde_json::to_string(&descriptor.stdio_entry(true, false)).ok()?,
        ]),
        _ => None,
    }
}

pub(super) fn copilot_studio_metadata(descriptor: &NativeMcpDescriptor) -> serde_json::Value {
    json!({
        "kind": "copilot_studio_manual_metadata",
        "stdio": descriptor.stdio_entry(true, false),
        "http": {
            "url": "http://127.0.0.1:8765/mcp",
            "start_command": [
                descriptor.command,
                "mcp",
                "http",
                "--config",
                descriptor.setup_config_path,
                "--host",
                "127.0.0.1",
                "--port",
                "8765",
                "--path",
                "/mcp"
            ],
            "host": "127.0.0.1",
            "port": 8765,
            "path": "/mcp",
        },
        "notes": [
            "No local client configuration file is written for Copilot Studio.",
            "Remote Copilot Studio use requires user-managed endpoint exposure, bearer-token configuration, and TLS.",
        ],
    })
}

pub(super) fn codex_toml_block(descriptor: &NativeMcpDescriptor) -> String {
    format!(
        "[mcp_servers.{}]\ncommand = {}\nargs = {}\nstartup_timeout_sec = {}\n",
        descriptor.name,
        toml_string(&descriptor.command),
        toml_array(&descriptor.args),
        descriptor.timeout
    )
}

pub(super) fn toml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(super) fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(super) fn upsert_toml_block(
    existing: &str,
    server_name: &str,
    block: &str,
) -> (String, Option<String>) {
    let lines = existing.lines().collect::<Vec<_>>();
    let header = format!("[mcp_servers.{server_name}]");
    let env_header = format!("[mcp_servers.{server_name}.env]");
    let start = lines
        .iter()
        .position(|line| line.trim() == header || line.trim() == env_header);
    let Some(start) = start else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[')
                && trimmed.ends_with(']')
                && trimmed != header
                && trimmed != env_header
        })
        .map(|index| start + 1 + index)
        .unwrap_or(lines.len());
    let previous = lines[start..end].join("\n").trim_end().to_string();
    let mut next_lines = Vec::new();
    next_lines.extend(lines[..start].iter().map(|value| (*value).to_string()));
    next_lines.extend(block.trim_end().lines().map(str::to_string));
    next_lines.extend(lines[end..].iter().map(|value| (*value).to_string()));
    (
        next_lines.join("\n").trim_end().to_string() + "\n",
        Some(previous),
    )
}

pub(super) fn hermes_yaml_block(descriptor: &NativeMcpDescriptor) -> String {
    let mut lines = vec![
        "# codebaseGraph MCP server start".to_string(),
        "mcp_servers:".to_string(),
        format!("  {}:", descriptor.name),
        "    type: stdio".to_string(),
        format!("    command: {}", yaml_scalar(&descriptor.command)),
        "    args:".to_string(),
    ];
    for arg in &descriptor.args {
        lines.push(format!("      - {}", yaml_scalar(arg)));
    }
    lines.push("# codebaseGraph MCP server end".to_string());
    lines.join("\n") + "\n"
}

pub(super) fn yaml_scalar(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(super) fn upsert_marked_block(existing: &str, block: &str) -> (String, Option<String>) {
    const START: &str = "# codebaseGraph MCP server start";
    const END: &str = "# codebaseGraph MCP server end";
    let Some(start) = existing.find(START) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    let Some(end) = existing.find(END) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    if end < start {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    }
    let after_end = end + END.len();
    let previous = existing[start..after_end].trim_end().to_string();
    let text = format!(
        "{}\n\n{}\n\n{}",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start()
    )
    .trim()
    .to_string()
        + "\n";
    (text, Some(previous))
}

pub(super) fn write_text_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    ));
    fs::write(&tmp_path, text).map_err(|error| {
        format!(
            "failed to write temporary config {}: {error}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path)
        .map_err(|error| format!("failed to replace config {}: {error}", path.display()))
}

pub(super) fn expand_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

pub(super) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(super) fn executable_in_path(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| dir.join(executable).is_file()))
        .unwrap_or(false)
}

pub(super) fn subprocess_error(completed: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&completed.stdout)
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(&completed.stderr)
        .trim()
        .to_string();
    let output = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let code = completed.status.code().unwrap_or(1);
    if output.is_empty() {
        format!("exit {code}")
    } else {
        format!("exit {code}: {output}")
    }
}
