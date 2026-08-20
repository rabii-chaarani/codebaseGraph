use super::{
    http::{handle_mcp_http_request, read_http_request, write_http_json, HttpResponse},
    options::{McpHttpOptions, McpServeOptions},
    refresh::start_configured_api,
    state::McpHttpState,
};
use crate::api::context::{read_install_config, resolve_repository_root};
use crate::storage::atomic::write_json_atomically;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const DAEMON_TRANSPORT_VERSION: &str = "streamable-http-v1";
const DAEMON_HEALTH_PATH: &str = "/_codebasegraph/health";
const DAEMON_SHUTDOWN_PATH: &str = "/_codebasegraph/shutdown";
const DAEMON_STATE_FILE: &str = "mcp-daemon.json";
const DAEMON_LOCK_FILE: &str = "mcp-daemon.lock";
const DAEMON_SERVICE_LOCK_FILE: &str = "mcp-daemon-service.lock";
const CONTROL_HEADER: &str = "x-codebasegraph-control-token";
const START_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub(crate) struct McpDaemonOptions {
    pub(crate) repo_root: Option<PathBuf>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) port: Option<u16>,
}

impl McpDaemonOptions {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: None,
            config: None,
            port: None,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo-root" => {
                    options.repo_root = Some(PathBuf::from(required(args, index, "--repo-root")?));
                    index += 2;
                }
                "--config" => {
                    options.config = Some(PathBuf::from(required(args, index, "--config")?));
                    index += 2;
                }
                "--port" => {
                    let port = required(args, index, "--port")?
                        .parse::<u16>()
                        .map_err(|_| "--port must be between 1 and 65535".to_string())?;
                    if port == 0 {
                        return Err("--port must be between 1 and 65535".to_string());
                    }
                    options.port = Some(port);
                    index += 2;
                }
                other => return Err(format!("unknown mcp daemon option: {other}")),
            }
        }
        Ok(options)
    }

    fn config_path(&self) -> Result<PathBuf, String> {
        if let Some(path) = self.config.as_ref() {
            return Ok(absolutize(path));
        }
        let repo_root = resolve_repository_root(self.repo_root.as_deref())?;
        Ok(repo_root.join(".codebaseGraph/config.json"))
    }
}

fn required<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpDaemonState {
    pub(crate) pid: u32,
    pub(crate) version: String,
    pub(crate) endpoint: String,
    pub(crate) repository_fingerprint: String,
    pub(crate) service_id: String,
    pub(crate) control_token: String,
}

#[derive(Debug, Clone)]
pub(crate) struct McpDaemonSpec {
    pub(crate) config_path: PathBuf,
    pub(crate) repo_root: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) port: u16,
    pub(crate) endpoint: String,
    pub(crate) repository_fingerprint: String,
    pub(crate) service_id: String,
    executable: PathBuf,
}

impl McpDaemonSpec {
    pub(crate) fn from_options(options: &McpDaemonOptions) -> Result<Self, String> {
        Self::from_config(&options.config_path()?, options.port)
    }

    pub(crate) fn from_config(
        config_path: &Path,
        port_override: Option<u16>,
    ) -> Result<Self, String> {
        let config_path = absolutize(config_path);
        let config = read_install_config(&config_path)?;
        let repo_root = config.repo_root.clone().unwrap_or_else(|| {
            config_path
                .parent()
                .and_then(Path::parent)
                .unwrap_or(Path::new("."))
                .to_path_buf()
        });
        let repo_root = repo_root.canonicalize().map_err(|error| {
            format!(
                "failed to resolve daemon repository {}: {error}",
                repo_root.display()
            )
        })?;
        let fingerprint = repository_fingerprint(&repo_root);
        let persisted_http = config.mcp.as_ref().and_then(|mcp| mcp.http.as_ref());
        let persisted_port = persisted_http.and_then(|http| endpoint_port(&http.url));
        let port = port_override
            .or(persisted_port)
            .unwrap_or_else(|| stable_daemon_port(&repo_root));
        let endpoint = format!("http://127.0.0.1:{port}/mcp");
        let service_id = persisted_http
            .map(|http| http.service_id.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| service_id(&fingerprint));
        let state_dir = config
            .state_dir
            .unwrap_or_else(|| repo_root.join(".codebaseGraph"));
        let executable = env::current_exe()
            .map_err(|error| format!("failed to resolve codebase-graph executable: {error}"))?;
        Ok(Self {
            config_path,
            repo_root,
            state_dir,
            port,
            endpoint,
            repository_fingerprint: fingerprint,
            service_id,
            executable,
        })
    }

    pub(crate) fn state_path(&self) -> PathBuf {
        self.state_dir.join(DAEMON_STATE_FILE)
    }

    fn lock_path(&self) -> PathBuf {
        self.state_dir.join(DAEMON_LOCK_FILE)
    }

    fn launch_args(&self) -> Vec<String> {
        vec![
            "mcp".to_string(),
            "daemon".to_string(),
            "serve".to_string(),
            "--config".to_string(),
            self.config_path.to_string_lossy().to_string(),
            "--port".to_string(),
            self.port.to_string(),
        ]
    }
}

pub(crate) fn repository_fingerprint(repo_root: &Path) -> String {
    let canonical = repo_root
        .canonicalize()
        .unwrap_or_else(|_| absolutize(repo_root));
    hex_digest(canonical.to_string_lossy().as_bytes())
}

pub(crate) fn stable_daemon_port(repo_root: &Path) -> u16 {
    let digest = Sha256::digest(repository_fingerprint(repo_root).as_bytes());
    let value = u16::from_be_bytes([digest[0], digest[1]]);
    41_000 + (value % 8_000)
}

pub(crate) fn service_id(fingerprint: &str) -> String {
    format!("io.codebasegraph.mcp.{}", &fingerprint[..16])
}

pub(crate) fn serve_mcp_daemon(options: &McpDaemonOptions) -> Result<(), String> {
    let spec = McpDaemonSpec::from_options(options)?;
    become_process_group_owner()?;
    fs::create_dir_all(&spec.state_dir).map_err(|error| {
        format!(
            "failed to create daemon state directory {}: {error}",
            spec.state_dir.display()
        )
    })?;
    let lock = open_private_file(&spec.lock_path())?;
    lock.try_lock_exclusive().map_err(|error| {
        format!(
            "MCP daemon is already running for repository {}: {error}",
            spec.repo_root.display()
        )
    })?;
    let listener = TcpListener::bind(("127.0.0.1", spec.port))
        .map_err(|error| format!("failed to bind managed MCP daemon: {error}"))?;
    let serve = McpServeOptions::parse(
        &[
            "--config".to_string(),
            spec.config_path.to_string_lossy().to_string(),
        ],
        "",
    )?;
    let mut http = McpHttpOptions {
        serve,
        host: "127.0.0.1".to_string(),
        port: spec.port,
        endpoint_path: "/mcp".to_string(),
        allow_remote: false,
        auth_token: None,
    };
    http.serve.api = Some(start_configured_api(&http.serve)?);
    let state = McpDaemonState {
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        endpoint: spec.endpoint.clone(),
        repository_fingerprint: spec.repository_fingerprint.clone(),
        service_id: spec.service_id.clone(),
        control_token: rotating_control_token(&spec),
    };
    write_private_state(&spec.state_path(), &state)?;
    let result = daemon_accept_loop(&http, listener, &state);
    remove_state_if_owned(&spec.state_path(), state.pid);
    let _ = FileExt::unlock(&lock);
    result
}

fn daemon_accept_loop(
    options: &McpHttpOptions,
    listener: TcpListener,
    daemon: &McpDaemonState,
) -> Result<(), String> {
    let mut sessions = McpHttpState::default();
    loop {
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("failed to accept managed MCP request: {error}"))?;
        let request = match read_http_request(&mut stream) {
            Ok(request) => request,
            Err(error) => {
                let _ = write_http_json(&mut stream, 500, &json!({"error": error}), &[]);
                continue;
            }
        };
        if request.path == DAEMON_HEALTH_PATH {
            let status = if request.method == "GET" { 200 } else { 405 };
            write_http_json(
                &mut stream,
                status,
                &json!({
                    "ok": true,
                    "server": "codebase-graph",
                    "pid": daemon.pid,
                    "version": daemon.version,
                    "endpoint": daemon.endpoint,
                    "repository_fingerprint": daemon.repository_fingerprint,
                    "service_id": daemon.service_id,
                    "transport_version": DAEMON_TRANSPORT_VERSION,
                }),
                &[],
            )?;
            continue;
        }
        if request.path == DAEMON_SHUTDOWN_PATH {
            let authorized = request.method == "POST"
                && request.header(CONTROL_HEADER) == Some(daemon.control_token.as_str());
            let response = if authorized {
                HttpResponse::json(200, json!({"ok": true, "pid": daemon.pid}))
            } else {
                HttpResponse::json(401, json!({"ok": false, "error": "unauthorized"}))
            };
            write_http_json(
                &mut stream,
                response.status,
                &response.payload,
                &response.headers,
            )?;
            if authorized {
                break;
            }
            continue;
        }
        let response = handle_mcp_http_request(options, &mut sessions, request);
        write_http_json(
            &mut stream,
            response.status,
            &response.payload,
            &response.headers,
        )?;
    }
    Ok(())
}

pub(crate) fn start_mcp_daemon(options: &McpDaemonOptions) -> Result<serde_json::Value, String> {
    let spec = McpDaemonSpec::from_options(options)?;
    fs::create_dir_all(&spec.state_dir).map_err(|error| {
        format!(
            "failed to create daemon state directory {}: {error}",
            spec.state_dir.display()
        )
    })?;
    let _provision_lock = acquire_bounded_lock(
        &spec.state_dir.join(DAEMON_SERVICE_LOCK_FILE),
        START_TIMEOUT,
    )?;
    let service = PlatformService::for_spec(&spec)?;
    if let Ok(state) = read_daemon_state(&spec.state_path()) {
        if probe_health(&state).is_ok() {
            if !service.manifest_path().exists() {
                return Err(format!(
                    "an unmanaged MCP daemon is already running as PID {}; stop it before installing the user service",
                    state.pid
                ));
            }
            verify_daemon_endpoint(&state.endpoint, Some(&spec.repository_fingerprint))?;
            return Ok(json!({
                "action": "unchanged",
                "running": true,
                "pid": state.pid,
                "endpoint": state.endpoint,
                "service_id": state.service_id,
                "repository_fingerprint": state.repository_fingerprint,
            }));
        }
    }
    if let Err(start_error) = service.install_and_start() {
        let cleanup_error = service.stop(true).err();
        let _ = fs::remove_file(service.manifest_path());
        return Err(match cleanup_error {
            Some(cleanup_error) => format!(
                "{start_error}; additionally failed to roll back the service: {cleanup_error}"
            ),
            None => start_error,
        });
    }
    let deadline = Instant::now() + START_TIMEOUT;
    let mut verification_error = None;
    while Instant::now() < deadline {
        if let Ok(state) = read_daemon_state(&spec.state_path()) {
            if probe_health(&state).is_ok() {
                match verify_daemon_endpoint(&state.endpoint, Some(&spec.repository_fingerprint)) {
                    Ok(_) => {
                        return Ok(json!({
                            "action": "started",
                            "running": true,
                            "pid": state.pid,
                            "endpoint": state.endpoint,
                            "service_id": state.service_id,
                            "repository_fingerprint": state.repository_fingerprint,
                            "service_manifest": service.manifest_path(),
                        }));
                    }
                    Err(error) => verification_error = Some(error),
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    let message = format!(
        "managed MCP daemon {} did not become healthy within {} seconds",
        spec.service_id,
        START_TIMEOUT.as_secs()
    );
    let message = match verification_error {
        Some(error) => format!("{message}: last MCP verification error: {error}"),
        None => message,
    };
    let cleanup_error = service.stop(true).err();
    let _ = fs::remove_file(service.manifest_path());
    Err(match cleanup_error {
        Some(error) => format!("{message}; additionally failed to roll back the service: {error}"),
        None => message,
    })
}

fn acquire_bounded_lock(path: &Path, timeout: Duration) -> Result<File, String> {
    let file = open_private_file(path)?;
    let deadline = Instant::now() + timeout;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(format!(
                    "timed out waiting for daemon service lock {}",
                    path.display()
                ));
            }
            Err(error) => {
                return Err(format!(
                    "failed to acquire daemon service lock {}: {error}",
                    path.display()
                ));
            }
        }
    }
}

pub(crate) fn stop_mcp_daemon(
    options: &McpDaemonOptions,
    remove_service: bool,
) -> Result<serde_json::Value, String> {
    let spec = McpDaemonSpec::from_options(options)?;
    let previous = read_daemon_state(&spec.state_path()).ok();
    let verified_owner = previous
        .as_ref()
        .is_some_and(|state| probe_health(state).is_ok());
    if let Some(state) = previous.as_ref() {
        if verified_owner {
            let _ = request_shutdown(state);
        }
        let deadline = Instant::now() + STOP_TIMEOUT;
        while Instant::now() < deadline && pid_is_alive(state.pid) {
            thread::sleep(Duration::from_millis(50));
        }
    }
    let service = PlatformService::for_spec(&spec)?;
    let service_error = service.stop(remove_service).err();
    if let Some(state) = previous.as_ref() {
        if verified_owner {
            if service_error.is_some() && pid_is_alive(state.pid) {
                force_stop_process_group(state.pid)?;
            }
            let deadline = Instant::now() + STOP_TIMEOUT;
            while Instant::now() < deadline && pid_is_alive(state.pid) {
                thread::sleep(Duration::from_millis(50));
            }
            if pid_is_alive(state.pid) {
                return Err(format!("managed MCP daemon PID {} did not stop", state.pid));
            }
        } else if let Some(error) = service_error.as_ref() {
            return Err(error.clone());
        }
        remove_state_if_owned(&spec.state_path(), state.pid);
    }
    if let Some(error) = service_error {
        return Err(error);
    }
    Ok(json!({
        "action": if previous.is_some() { "stopped" } else { "unchanged" },
        "running": false,
        "service_removed": remove_service,
        "service_id": spec.service_id,
        "endpoint": spec.endpoint,
    }))
}

#[cfg(unix)]
fn become_process_group_owner() -> Result<(), String> {
    let result = unsafe { libc::setpgid(0, 0) };
    if result == 0 || unsafe { libc::getpgrp() == libc::getpid() } {
        Ok(())
    } else {
        Err(format!(
            "failed to establish managed MCP daemon process group: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(unix))]
fn become_process_group_owner() -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn force_stop_process_group(pid: u32) -> Result<(), String> {
    let pid = i32::try_from(pid).map_err(|_| "daemon PID is outside platform range".to_string())?;
    let _ = unsafe { libc::kill(-pid, libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && pid_is_alive(pid as u32) {
        thread::sleep(Duration::from_millis(25));
    }
    if pid_is_alive(pid as u32) {
        let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
        if result != 0 && pid_is_alive(pid as u32) {
            return Err(format!(
                "failed to kill managed MCP daemon process group {pid}: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn force_stop_process_group(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return if pid_is_alive(pid) {
            Err(format!(
                "failed to open managed MCP daemon PID {pid} for termination"
            ))
        } else {
            Ok(())
        };
    }
    let terminated = unsafe { TerminateProcess(handle, 1) };
    unsafe { CloseHandle(handle) };
    if terminated == 0 && pid_is_alive(pid) {
        Err(format!("failed to terminate managed MCP daemon PID {pid}"))
    } else {
        Ok(())
    }
}

pub(crate) fn status_mcp_daemon(options: &McpDaemonOptions) -> Result<serde_json::Value, String> {
    let spec = McpDaemonSpec::from_options(options)?;
    let service = PlatformService::for_spec(&spec)?;
    let state = read_daemon_state(&spec.state_path()).ok();
    let running = state
        .as_ref()
        .is_some_and(|state| probe_health(state).is_ok());
    Ok(json!({
        "running": running,
        "pid": state.as_ref().map(|state| state.pid),
        "endpoint": state.as_ref().map(|state| state.endpoint.clone()).unwrap_or_else(|| spec.endpoint.clone()),
        "service_id": spec.service_id,
        "repository_fingerprint": spec.repository_fingerprint,
        "state_path": spec.state_path(),
        "managed_service_installed": service.manifest_path().exists(),
        "service_manifest": service.manifest_path(),
    }))
}

pub(crate) fn read_daemon_state(path: &Path) -> Result<McpDaemonState, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read daemon state {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to decode daemon state {}: {error}", path.display()))
}

pub(crate) fn probe_daemon_endpoint(endpoint: &str) -> Result<serde_json::Value, String> {
    let port = endpoint_port(endpoint).ok_or_else(|| "invalid daemon endpoint URL".to_string())?;
    http_json_request(port, "GET", DAEMON_HEALTH_PATH, &[], None)
}

pub(crate) fn verify_daemon_endpoint(
    endpoint: &str,
    expected_fingerprint: Option<&str>,
) -> Result<serde_json::Value, String> {
    let health = probe_daemon_endpoint(endpoint)?;
    if health.get("server").and_then(serde_json::Value::as_str) != Some("codebase-graph") {
        return Err("HTTP endpoint did not identify itself as codebase-graph".to_string());
    }
    if let Some(expected) = expected_fingerprint {
        if health
            .get("repository_fingerprint")
            .and_then(serde_json::Value::as_str)
            != Some(expected)
        {
            return Err(
                "HTTP endpoint repository fingerprint does not match setup config".to_string(),
            );
        }
    }
    let port = endpoint_port(endpoint).ok_or_else(|| "invalid daemon endpoint URL".to_string())?;
    let initialized = http_json_response(
        port,
        "POST",
        "/mcp",
        &[],
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": crate::api::CodebaseGraphApi::latest_mcp_protocol_version()}
        })),
    )?;
    let session_id = initialized
        .headers
        .get("mcp-session-id")
        .ok_or_else(|| "HTTP initialize response did not return an MCP session ID".to_string())?;
    let tools = http_json_response(
        port,
        "POST",
        "/mcp",
        &[
            ("mcp-session-id", session_id.as_str()),
            (
                "mcp-protocol-version",
                crate::api::CodebaseGraphApi::latest_mcp_protocol_version(),
            ),
        ],
        Some(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        })),
    )?;
    let listed = tools
        .payload
        .pointer("/result/tools")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "HTTP tools/list response did not contain tool schemas".to_string())?;
    let has_health = listed.iter().any(|tool| {
        tool.get("name").and_then(serde_json::Value::as_str) == Some("graph_health")
            && tool.get("inputSchema").is_some()
    });
    let has_search = listed.iter().any(|tool| {
        tool.get("name").and_then(serde_json::Value::as_str) == Some("graph_search")
            && tool.get("inputSchema").is_some()
    });
    if !has_health || !has_search {
        return Err("HTTP endpoint is missing required graph tool schemas".to_string());
    }
    Ok(json!({
        "ok": true,
        "health": health,
        "initialize": initialized.payload,
        "tool_count": listed.len(),
        "checks": {
            "server_identity": true,
            "repository_fingerprint": expected_fingerprint.is_none_or(|expected| health["repository_fingerprint"] == expected),
            "initialize": true,
            "tool_schemas": true,
        }
    }))
}

fn probe_health(state: &McpDaemonState) -> Result<serde_json::Value, String> {
    let health = probe_daemon_endpoint(&state.endpoint)?;
    if health.get("server").and_then(serde_json::Value::as_str) != Some("codebase-graph")
        || health.get("pid").and_then(serde_json::Value::as_u64) != Some(u64::from(state.pid))
        || health
            .get("repository_fingerprint")
            .and_then(serde_json::Value::as_str)
            != Some(state.repository_fingerprint.as_str())
        || health.get("service_id").and_then(serde_json::Value::as_str)
            != Some(state.service_id.as_str())
    {
        return Err("daemon health identity does not match persisted state".to_string());
    }
    Ok(health)
}

fn request_shutdown(state: &McpDaemonState) -> Result<(), String> {
    let port =
        endpoint_port(&state.endpoint).ok_or_else(|| "invalid daemon endpoint URL".to_string())?;
    http_json_request(
        port,
        "POST",
        DAEMON_SHUTDOWN_PATH,
        &[(CONTROL_HEADER, state.control_token.as_str())],
        Some(&json!({})),
    )?;
    Ok(())
}

fn http_json_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Ok(http_json_response(port, method, path, headers, body)?.payload)
}

struct HttpClientResponse {
    payload: serde_json::Value,
    headers: std::collections::BTreeMap<String, String>,
}

fn http_json_response(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&serde_json::Value>,
) -> Result<HttpClientResponse, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("failed to connect to managed MCP daemon: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .map_err(|error| error.to_string())?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").map_err(|error| error.to_string())?;
    }
    write!(stream, "\r\n").map_err(|error| error.to_string())?;
    stream.write_all(&body).map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("failed to read managed MCP daemon response: {error}"))?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "managed MCP daemon returned an invalid HTTP response".to_string())?;
    let head = String::from_utf8_lossy(&response[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(500);
    let payload = serde_json::from_slice::<serde_json::Value>(&response[split + 4..])
        .unwrap_or_else(|_| json!({}));
    if status / 100 != 2 {
        return Err(format!(
            "managed MCP daemon returned HTTP {status}: {payload}"
        ));
    }
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    Ok(HttpClientResponse { payload, headers })
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    let authority = endpoint.strip_prefix("http://127.0.0.1:")?;
    authority.split('/').next()?.parse::<u16>().ok()
}

fn write_private_state(path: &Path, state: &McpDaemonState) -> Result<(), String> {
    let value = serde_json::to_value(state).map_err(|error| error.to_string())?;
    write_json_atomically(path, &value)
        .map_err(|error| format!("failed to write daemon state {}: {error}", path.display()))?;
    set_private_permissions(path)?;
    Ok(())
}

fn open_private_file(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("failed to open daemon lock {}: {error}", path.display()))?;
    set_private_permissions(path)?;
    Ok(file)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to protect daemon state {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn remove_state_if_owned(path: &Path, pid: u32) {
    if read_daemon_state(path)
        .ok()
        .is_some_and(|state| state.pid == pid)
    {
        let _ = fs::remove_file(path);
    }
}

fn rotating_control_token(spec: &McpDaemonSpec) -> String {
    #[cfg(unix)]
    {
        let mut entropy = [0_u8; 32];
        if File::open("/dev/urandom")
            .and_then(|mut source| source.read_exact(&mut entropy))
            .is_ok()
        {
            return entropy.iter().map(|byte| format!("{byte:02x}")).collect();
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    hex_digest(
        format!(
            "{}:{}:{}:{}",
            spec.repository_fingerprint,
            std::process::id(),
            now,
            spec.endpoint
        )
        .as_bytes(),
    )
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        false
    } else {
        unsafe { CloseHandle(handle) };
        true
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum PlatformService {
    Launchd { spec: McpDaemonSpec, path: PathBuf },
    Systemd { spec: McpDaemonSpec, path: PathBuf },
    TaskScheduler { spec: McpDaemonSpec, path: PathBuf },
}

impl PlatformService {
    fn for_spec(spec: &McpDaemonSpec) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            let path = home_dir()
                .join("Library/LaunchAgents")
                .join(format!("{}.plist", spec.service_id));
            return Ok(Self::Launchd {
                spec: spec.clone(),
                path,
            });
        }
        #[cfg(target_os = "linux")]
        {
            let path = home_dir()
                .join(".config/systemd/user")
                .join(format!("{}.service", spec.service_id));
            return Ok(Self::Systemd {
                spec: spec.clone(),
                path,
            });
        }
        #[cfg(windows)]
        {
            let path = spec.state_dir.join("mcp-daemon-task.xml");
            return Ok(Self::TaskScheduler {
                spec: spec.clone(),
                path,
            });
        }
        #[allow(unreachable_code)]
        Err("managed MCP daemon services are unsupported on this platform".to_string())
    }

    fn manifest_path(&self) -> &Path {
        match self {
            Self::Launchd { path, .. }
            | Self::Systemd { path, .. }
            | Self::TaskScheduler { path, .. } => path,
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Launchd { spec, .. } => render_launchd(spec),
            Self::Systemd { spec, .. } => render_systemd(spec),
            Self::TaskScheduler { spec, .. } => render_task_scheduler(spec),
        }
    }

    fn install_and_start(&self) -> Result<(), String> {
        let manager = match self {
            Self::Launchd { .. } => require_executable("launchctl")?,
            Self::Systemd { .. } => require_executable("systemctl")?,
            Self::TaskScheduler { .. } => require_executable("schtasks.exe")?,
        };
        let path = self.manifest_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create service directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        fs::write(path, self.render()).map_err(|error| {
            format!(
                "failed to write service manifest {}: {error}",
                path.display()
            )
        })?;
        match self {
            Self::Launchd { spec, path } => {
                let domain = format!("gui/{}", effective_user_id());
                let _ = run_output_bounded(
                    &manager,
                    &["bootout", &format!("{domain}/{}", spec.service_id)],
                    STOP_TIMEOUT,
                );
                run_checked(&manager, &["bootstrap", &domain, &path.to_string_lossy()])?;
                run_checked(
                    &manager,
                    &["kickstart", "-k", &format!("{domain}/{}", spec.service_id)],
                )
            }
            Self::Systemd { spec, .. } => {
                run_checked(&manager, &["--user", "daemon-reload"])?;
                run_checked(
                    &manager,
                    &[
                        "--user",
                        "enable",
                        "--now",
                        &format!("{}.service", spec.service_id),
                    ],
                )
            }
            Self::TaskScheduler { spec, path } => {
                run_checked(
                    &manager,
                    &[
                        "/Create",
                        "/TN",
                        &spec.service_id,
                        "/XML",
                        &path.to_string_lossy(),
                        "/F",
                    ],
                )?;
                run_checked(&manager, &["/Run", "/TN", &spec.service_id])
            }
        }
    }

    fn stop(&self, remove: bool) -> Result<(), String> {
        match self {
            Self::Launchd { spec, path } => {
                let launchctl = require_executable("launchctl")?;
                let target = format!("gui/{}/{}", effective_user_id(), spec.service_id);
                let output = run_output_bounded(&launchctl, &["bootout", &target], STOP_TIMEOUT)?;
                if !output.status.success() && path.exists() && !service_not_loaded(&output) {
                    return Err(command_failure(&launchctl, &output));
                }
            }
            Self::Systemd { spec, .. } => {
                let systemctl = require_executable("systemctl")?;
                let unit = format!("{}.service", spec.service_id);
                let output =
                    run_output_bounded(&systemctl, &["--user", "stop", &unit], STOP_TIMEOUT)?;
                if !output.status.success() && self.manifest_path().exists() {
                    return Err(command_failure(&systemctl, &output));
                }
                if remove {
                    let _ =
                        run_output_bounded(&systemctl, &["--user", "disable", &unit], STOP_TIMEOUT);
                    let _ =
                        run_output_bounded(&systemctl, &["--user", "daemon-reload"], STOP_TIMEOUT);
                }
            }
            Self::TaskScheduler { spec, .. } => {
                let schtasks = require_executable("schtasks.exe")?;
                let _ =
                    run_output_bounded(&schtasks, &["/End", "/TN", &spec.service_id], STOP_TIMEOUT);
                if remove {
                    let output = run_output_bounded(
                        &schtasks,
                        &["/Delete", "/TN", &spec.service_id, "/F"],
                        STOP_TIMEOUT,
                    )?;
                    if !output.status.success()
                        && self.manifest_path().exists()
                        && !service_not_loaded(&output)
                    {
                        return Err(command_failure(&schtasks, &output));
                    }
                }
            }
        }
        if remove {
            let _ = fs::remove_file(self.manifest_path());
        }
        Ok(())
    }
}

fn render_launchd(spec: &McpDaemonSpec) -> String {
    let mut args = vec![spec.executable.to_string_lossy().to_string()];
    args.extend(spec.launch_args());
    let arguments = args
        .iter()
        .map(|arg| format!("    <string>{}</string>", xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{}</string>\n  <key>ProgramArguments</key>\n  <array>\n{}\n  </array>\n  <key>RunAtLoad</key><true/>\n  <key>ProcessType</key><string>Background</string>\n  <key>KeepAlive</key>\n  <dict><key>SuccessfulExit</key><false/></dict>\n</dict>\n</plist>\n",
        xml_escape(&spec.service_id), arguments
    )
}

fn render_systemd(spec: &McpDaemonSpec) -> String {
    let command = std::iter::once(spec.executable.to_string_lossy().to_string())
        .chain(spec.launch_args())
        .map(|arg| systemd_quote(&arg))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "[Unit]\nDescription=CodebaseGraph MCP daemon for {}\n\n[Service]\nType=simple\nExecStart={}\nRestart=on-failure\nRestartSec=1\nTimeoutStopSec=5s\nKillMode=control-group\n\n[Install]\nWantedBy=default.target\n",
        spec.repository_fingerprint, command
    )
}

fn render_task_scheduler(spec: &McpDaemonSpec) -> String {
    let arguments = spec
        .launch_args()
        .iter()
        .map(|arg| windows_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings>\n  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>\n  <Actions Context=\"Author\"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions>\n</Task>\n",
        xml_escape(&spec.executable.to_string_lossy()),
        xml_escape(&arguments)
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn systemd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn windows_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn run_checked(program: &Path, args: &[&str]) -> Result<(), String> {
    let output = run_output_bounded(program, args, START_TIMEOUT)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failure(program, &output))
    }
}

fn run_output_bounded(
    program: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run {}: {error}", program.display()))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("failed to wait for {}: {error}", program.display()))?
        {
            Some(_) => {
                return child.wait_with_output().map_err(|error| {
                    format!("failed to collect {} output: {error}", program.display())
                })
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} did not finish within {} seconds",
                    program.display(),
                    timeout.as_secs()
                ));
            }
        }
    }
}

fn command_failure(program: &Path, output: &std::process::Output) -> String {
    format!(
        "{} failed with status {}: {}",
        program.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn service_not_loaded(output: &std::process::Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    stderr.contains("could not find service")
        || stderr.contains("no such process")
        || stderr.contains("cannot find the file")
        || stderr.contains("does not exist")
}

fn require_executable(name: &str) -> Result<PathBuf, String> {
    executable_in_path(name).ok_or_else(|| {
        format!(
            "platform service manager executable {name} is unavailable; refusing to fall back to stdio"
        )
    })
}

fn executable_in_path(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 && candidate.is_file() {
        return Some(candidate.to_path_buf());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(name))
            .find(|path| path.is_file())
    })
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(unix)]
fn effective_user_id() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn effective_user_id() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> McpDaemonSpec {
        McpDaemonSpec {
            config_path: PathBuf::from("/tmp/repo with spaces/.codebaseGraph/config.json"),
            repo_root: PathBuf::from("/tmp/repo with spaces"),
            state_dir: PathBuf::from("/tmp/repo with spaces/.codebaseGraph"),
            port: 43123,
            endpoint: "http://127.0.0.1:43123/mcp".to_string(),
            repository_fingerprint: "0123456789abcdef0123456789abcdef".to_string(),
            service_id: "io.codebasegraph.mcp.0123456789abcdef".to_string(),
            executable: PathBuf::from("/Applications/Codebase Graph/codebase-graph"),
        }
    }

    #[test]
    fn stable_ports_and_service_ids_are_repository_scoped() {
        let left = Path::new("/tmp/repository-a");
        let right = Path::new("/tmp/repository-b");
        assert_eq!(stable_daemon_port(left), stable_daemon_port(left));
        assert_ne!(repository_fingerprint(left), repository_fingerprint(right));
        assert_ne!(
            service_id(&repository_fingerprint(left)),
            service_id(&repository_fingerprint(right))
        );
    }

    #[test]
    fn service_manifests_quote_paths_and_request_single_instance_restart() {
        let spec = spec();
        let launchd = render_launchd(&spec);
        assert!(launchd.contains("SuccessfulExit"));
        assert!(launchd.contains("repo with spaces"));
        let systemd = render_systemd(&spec);
        assert!(systemd.contains("Restart=on-failure"));
        assert!(systemd.contains("\"/Applications/Codebase Graph/codebase-graph\""));
        let task = render_task_scheduler(&spec);
        assert!(task.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(task.contains("<RestartOnFailure>"));
    }
}
