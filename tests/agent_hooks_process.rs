//! Process-level coverage for repository-local agent loop hooks.
//!
//! These tests intentionally use a minimal setup config without a graph store.
//! Installation/rendering is configuration-only, while the runner is expected
//! to fail open when the managed loopback daemon is not available.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MANAGED_ID: &str = "codebase-graph-v1";

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_codebase-graph")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_codebase-graph")))
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codebase-graph-agent-hooks-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(path.join(".codebaseGraph")).expect("create temporary repository");
        fs::write(
            path.join(".codebaseGraph/config.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 3,
                "repo_root": path,
                "repo_name": "agent-hooks-test"
            }))
            .unwrap(),
        )
        .expect("write setup config");
        Self { path }
    }

    fn config(&self) -> PathBuf {
        self.path.join(".codebaseGraph/config.json")
    }

    fn hook(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(repo: &TempRepo, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .current_dir(&repo.path)
        .output()
        .expect("run codebase-graph")
}

fn run_with_stdin(repo: &TempRepo, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(binary())
        .args(args)
        .current_dir(&repo.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn codebase-graph");
    {
        let stdin = child.stdin.as_mut().expect("runner stdin");
        std::io::Write::write_all(stdin, input).expect("write runner input");
    }
    child.wait_with_output().expect("wait for codebase-graph")
}

fn json_stdout(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "command failed (status={}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected JSON stdout ({error}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create hook config directory");
    }
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).expect("write hook config");
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).expect("read hook config")
}

fn repository_fingerprint(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap();
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_http_json(stream: &mut TcpStream) -> Value {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).expect("read hook MCP request");
        assert!(read > 0, "hook MCP request ended before its headers");
        request.extend_from_slice(&chunk[..read]);
        if let Some(index) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() - header_end < content_length {
        let read = stream.read(&mut chunk).expect("read hook MCP body");
        assert!(read > 0, "hook MCP request ended before its body");
        request.extend_from_slice(&chunk[..read]);
    }
    if content_length == 0 {
        json!({})
    } else {
        serde_json::from_slice(&request[header_end..header_end + content_length]).unwrap()
    }
}

fn write_http_json(stream: &mut TcpStream, value: &Value, session: bool) {
    let body = serde_json::to_vec(value).unwrap();
    let session_header = if session {
        "Mcp-Session-Id: hook-test-session\r\n"
    } else {
        ""
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{session_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
}

#[test]
fn install_all_clients_preserves_foreign_hooks_and_is_idempotent() {
    let repo = TempRepo::new("merge");
    write_json(
        &repo.hook(".codex/hooks.json"),
        &json!({
            "foreignSetting": "preserved",
            "hooks": {
                "SessionStart": [{"hooks": [{"type": "command", "command": "echo foreign-codex"}]}],
                "ForeignEvent": [{"hooks": [{"type": "command", "command": "echo untouched"}]}]
            }
        }),
    );
    write_json(
        &repo.hook(".claude/settings.json"),
        &json!({
            "permissions": {"allow": ["Read"]},
            "hooks": {"UserPromptSubmit": [{"hooks": [{"type": "command", "command": "echo foreign-claude"}]}]}
        }),
    );
    write_json(
        &repo.hook(".github/hooks/codebase-graph.json"),
        &json!({
            "version": 1,
            "foreign": true,
            "hooks": {"sessionStart": [{"hooks": [{"type": "command", "bash": "echo foreign-copilot"}]}]}
        }),
    );

    let first = run(
        &repo,
        &[
            "agent-hooks",
            "install",
            "--client",
            "all",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
            "--verify",
        ],
    );
    let first_json = json_stdout(&first);
    assert_eq!(first_json["action"], "updated");
    assert_eq!(first_json["managed_id"], MANAGED_ID);
    assert_eq!(first_json["clients"].as_array().unwrap().len(), 3);
    assert_eq!(first_json["verification"]["ok"], true);
    let installed_config: Value =
        serde_json::from_str(&fs::read_to_string(repo.config()).unwrap()).unwrap();
    assert_eq!(
        installed_config["agent_hooks"]["installed_clients"],
        json!(["claude", "codex", "github-copilot"])
    );

    let codex = text(&repo.hook(".codex/hooks.json"));
    let claude = text(&repo.hook(".claude/settings.json"));
    let copilot = text(&repo.hook(".github/hooks/codebase-graph.json"));
    assert!(codex.contains("foreign-codex"));
    assert!(codex.contains("foreignSetting"));
    assert!(codex.contains("echo untouched"));
    assert!(claude.contains("foreign-claude"));
    assert!(claude.contains("\"Read\""));
    assert!(copilot.contains("foreign-copilot"));
    assert!(copilot.contains("\"foreign\": true"));
    for rendered in [&codex, &claude, &copilot] {
        assert!(
            rendered.contains(MANAGED_ID),
            "managed marker missing: {rendered}"
        );
    }
    assert!(copilot.contains("COPILOT_AGENT_PROMPT"));
    assert!(copilot.contains("command -v"));
    assert!(copilot.contains("Get-Command"));
    let copilot_json: Value = serde_json::from_str(&copilot).unwrap();
    assert_eq!(copilot_json["version"], 1);
    assert_eq!(copilot_json["hooks"]["sessionStart"][1]["type"], "command");
    assert!(copilot_json["hooks"]["sessionStart"][1]
        .get("hooks")
        .is_none());

    let before = [codex, claude, copilot];
    let second = run(
        &repo,
        &[
            "agent-hooks",
            "install",
            "--client",
            "all",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
        ],
    );
    let second_json = json_stdout(&second);
    assert_eq!(second_json["action"], "unchanged");
    assert_eq!(text(&repo.hook(".codex/hooks.json")), before[0]);
    assert_eq!(text(&repo.hook(".claude/settings.json")), before[1]);
    assert_eq!(
        text(&repo.hook(".github/hooks/codebase-graph.json")),
        before[2]
    );

    let verify = run(
        &repo,
        &[
            "agent-hooks",
            "verify",
            "--client",
            "all",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
        ],
    );
    assert_eq!(json_stdout(&verify)["ok"], true);

    let remove = run(
        &repo,
        &[
            "agent-hooks",
            "remove",
            "--client",
            "all",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
        ],
    );
    let remove_json = json_stdout(&remove);
    assert_eq!(remove_json["action"], "removed");
    for client in remove_json["clients"].as_array().unwrap() {
        assert_eq!(client["verification"], "managed_handlers_removed");
        assert_eq!(client["restart_required"], true);
        assert_eq!(client["trust_required"], false);
        assert!(client["restart_instructions"].as_str().is_some());
    }
    assert!(text(&repo.hook(".codex/hooks.json")).contains("foreign-codex"));
    assert!(!text(&repo.hook(".codex/hooks.json")).contains(MANAGED_ID));
    assert!(text(&repo.hook(".claude/settings.json")).contains("foreign-claude"));
    assert!(!text(&repo.hook(".claude/settings.json")).contains(MANAGED_ID));
    assert!(text(&repo.hook(".github/hooks/codebase-graph.json")).contains("foreign-copilot"));
    assert!(!text(&repo.hook(".github/hooks/codebase-graph.json")).contains(MANAGED_ID));
    let removed_config: Value =
        serde_json::from_str(&fs::read_to_string(repo.config()).unwrap()).unwrap();
    assert_eq!(
        removed_config["agent_hooks"]["installed_clients"],
        json!([])
    );
}

#[test]
fn install_dry_run_reports_changes_without_creating_hook_files() {
    let repo = TempRepo::new("dry-run");
    let output = run(
        &repo,
        &[
            "agent-hooks",
            "install",
            "--client",
            "all",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
            "--dry-run",
        ],
    );
    let payload = json_stdout(&output);
    assert_eq!(payload["action"], "dry_run");
    assert_eq!(payload["clients"].as_array().unwrap().len(), 3);
    assert!(!repo.hook(".codex/hooks.json").exists());
    assert!(!repo.hook(".claude/settings.json").exists());
    assert!(!repo.hook(".github/hooks/codebase-graph.json").exists());
}

#[test]
fn verify_requires_a_managed_handler_for_every_required_event() {
    let repo = TempRepo::new("verify-per-event");
    let install = run(
        &repo,
        &[
            "agent-hooks",
            "install",
            "--client",
            "codex",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
        ],
    );
    assert_eq!(json_stdout(&install)["action"], "updated");

    let hook_path = repo.hook(".codex/hooks.json");
    let mut hooks: Value = serde_json::from_str(&text(&hook_path)).unwrap();
    let session_start = hooks["hooks"]["SessionStart"]
        .as_array_mut()
        .expect("generated SessionStart hooks");
    session_start.push(session_start[0].clone());
    hooks["hooks"]
        .as_object_mut()
        .expect("generated hooks object")
        .remove("UserPromptSubmit");
    write_json(&hook_path, &hooks);

    let verify = run(
        &repo,
        &[
            "agent-hooks",
            "verify",
            "--client",
            "codex",
            "--repo-root",
            repo.path.to_str().unwrap(),
            "--config",
            repo.config().to_str().unwrap(),
        ],
    );
    let payload = json_stdout(&verify);
    assert_eq!(payload["ok"], false);
    let client = &payload["clients"][0];
    assert_eq!(
        client["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["event"] == "SessionStart")
            .unwrap()["managed_handler_count"],
        2
    );
    let missing = client["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event"] == "UserPromptSubmit")
        .unwrap();
    assert_eq!(missing["managed_handler_count"], 0);
    assert_eq!(missing["ok"], false);
}

#[test]
fn prompt_hook_uses_one_loopback_session_for_health_and_semantic_search() {
    let repo = TempRepo::new("loopback-search");
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind fake hook MCP daemon: {error}"),
    };
    let port = listener.local_addr().unwrap().port();
    write_json(
        &repo.config(),
        &json!({
            "schema_version": 3,
            "repo_root": repo.path.clone(),
            "repo_name": "agent-hooks-test",
            "mcp": {
                "server_name": "codebase_graph",
                "http": {"url": format!("http://127.0.0.1:{port}/mcp")}
            }
        }),
    );
    let fingerprint = repository_fingerprint(&repo.path);
    let repo_root = repo.path.canonicalize().unwrap();
    let server = thread::spawn(move || {
        for request_index in 0..4 {
            let (mut stream, _) = listener.accept().expect("accept hook MCP request");
            let request = read_http_json(&mut stream);
            match request_index {
                0 => write_http_json(
                    &mut stream,
                    &json!({
                        "server": "codebase-graph",
                        "repository_fingerprint": fingerprint,
                    }),
                    false,
                ),
                1 => {
                    assert_eq!(request["method"], "initialize");
                    write_http_json(
                        &mut stream,
                        &json!({"jsonrpc":"2.0","id":1,"result":{}}),
                        true,
                    );
                }
                2 => {
                    assert_eq!(request["params"]["name"], "graph_health");
                    write_http_json(
                        &mut stream,
                        &json!({
                            "jsonrpc":"2.0",
                            "id":2,
                            "result": {
                                "structuredContent": {
                                    "repo_root": repo_root,
                                    "graph_readable": true,
                                    "freshness": "current"
                                },
                                "content": [{"type":"text","text":"health ok"}],
                                "isError": false
                            }
                        }),
                        false,
                    );
                }
                3 => {
                    assert_eq!(request["params"]["name"], "graph_search");
                    assert_eq!(request["params"]["arguments"]["layer"], "semantic");
                    assert_eq!(request["params"]["arguments"]["detail"], "slim");
                    assert_eq!(request["params"]["arguments"]["context_limit"], 1);
                    assert_eq!(request["params"]["arguments"]["limit"], 5);
                    assert_eq!(request["params"]["arguments"]["output_format"], "block");
                    write_http_json(
                        &mut stream,
                        &json!({
                            "jsonrpc":"2.0",
                            "id":2,
                            "result": {
                                "content": [{"type":"text","text":"semantic-hook-result"}],
                                "isError": false
                            }
                        }),
                        false,
                    );
                }
                _ => unreachable!(),
            }
        }
    });

    let output = run_with_stdin(
        &repo,
        &[
            "agent-hooks",
            "run",
            "--client",
            "codex",
            "--config",
            repo.config().to_str().unwrap(),
            "--managed-id",
            MANAGED_ID,
        ],
        br#"{"hook_event_name":"UserPromptSubmit","session_id":"loopback","cwd":".","prompt":"find the graph client"}"#,
    );
    let payload = json_stdout(&output);
    assert!(payload["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("semantic-hook-result"));
    server.join().expect("fake hook MCP daemon");
}

#[test]
fn runner_fail_open_handles_malformed_oversized_and_missing_daemon_inputs() {
    let repo = TempRepo::new("runner");
    let config_path = repo.config();
    let config = config_path.to_str().unwrap();

    let started = std::time::Instant::now();
    let malformed = run_with_stdin(
        &repo,
        &[
            "agent-hooks",
            "run",
            "--client",
            "codex",
            "--config",
            config,
            "--managed-id",
            MANAGED_ID,
        ],
        b"{not-json",
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    let malformed_json = json_stdout(&malformed);
    assert_eq!(malformed_json["advisory"], true);
    assert!(malformed_json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("malformed hook input"));

    let oversized = vec![b'x'; 1024 * 1024 + 1];
    let oversized_output = run_with_stdin(
        &repo,
        &[
            "agent-hooks",
            "run",
            "--client",
            "claude",
            "--config",
            config,
            "--managed-id",
            MANAGED_ID,
        ],
        &oversized,
    );
    let oversized_json = json_stdout(&oversized_output);
    assert_eq!(oversized_json["advisory"], true);
    assert!(oversized_json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("1 MiB bound"));

    let missing_daemon = run_with_stdin(
        &repo,
        &[
            "agent-hooks",
            "run",
            "--client",
            "codex",
            "--config",
            config,
            "--managed-id",
            MANAGED_ID,
        ],
        br#"{"hook_event_name":"UserPromptSubmit","prompt":"find the graph client","session_id":"runner-test"}"#,
    );
    let missing_json = json_stdout(&missing_daemon);
    assert_eq!(missing_json["advisory"], true);
    assert!(missing_json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("Graph lookup was non-blocking"));

    let copilot_native = run_with_stdin(
        &repo,
        &[
            "agent-hooks",
            "run",
            "--client",
            "github-copilot",
            "--config",
            config,
            "--managed-id",
            MANAGED_ID,
        ],
        br#"{"sessionId":"copilot-native","cwd":".","prompt":"find the graph client"}"#,
    );
    let copilot_json = json_stdout(&copilot_native);
    assert_eq!(copilot_json["advisory"], true);
    assert!(copilot_json["additionalContext"]
        .as_str()
        .unwrap()
        .contains("Graph lookup was non-blocking"));
    assert!(!repo.hook(".codebaseGraph/agent-hooks/sessions").exists());
}

#[test]
fn copilot_cloud_environment_is_an_explicit_noop() {
    let repo = TempRepo::new("copilot-cloud");
    let mut command = Command::new(binary());
    command
        .args([
            "agent-hooks",
            "run",
            "--client",
            "github-copilot",
            "--config",
            repo.config().to_str().unwrap(),
            "--managed-id",
            MANAGED_ID,
        ])
        .current_dir(&repo.path)
        .env("COPILOT_AGENT_PROMPT", "cloud prompt must not be persisted");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn copilot cloud hook");
    std::io::Write::write_all(
        child.stdin.as_mut().expect("copilot stdin"),
        br#"{"hookEventName":"userPromptSubmitted","prompt":"cloud prompt must not be persisted"}"#,
    )
    .expect("write copilot input");
    let output = child.wait_with_output().expect("wait for copilot hook");
    let payload = json_stdout(&output);
    assert_eq!(payload, json!({}));
    assert!(!repo.hook(".codebaseGraph/agent-hooks/sessions").exists());
}
