//! COD-55 exercises the installed CLI's configuration and client-process boundary.
#![cfg(windows)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Case {
    root: PathBuf,
}

impl Case {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cg55-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for name in ["caller", "repo space", "profile space", "home decoy", "bin"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn cli(&self, client: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codebase-graph"));
        command
            .args([
                "mcp",
                "install",
                "--client",
                client,
                "--scope",
                "user",
                "--mcp-transport",
                "stdio",
                "--agent-hooks",
                "none",
                "--name",
                "cod55_selected",
                "--json",
                "--repo-root",
            ])
            .arg(self.path("repo space"))
            .current_dir(self.path("caller"))
            .env("HOME", self.path("home decoy"))
            .env("USERPROFILE", self.path("profile space"))
            .env("PATH", self.path("bin"))
            .env_remove("CODEX_HOME")
            .env_remove("OPENCLAW_HOME")
            .env_remove("CODEBASE_GRAPH_SERVER_COMMAND")
            .env_remove("COD55_ADD_FAIL")
            .env_remove("COD55_LIST_FAIL")
            .env_remove("COD55_CLIENT")
            .env("COD55_LOG", self.path("calls.log"));
        command
    }

    fn fixture(&self) -> PathBuf {
        let source = self.path("fixture.rs");
        let binary = self.path("fixture.exe");
        fs::write(&source, FIXTURE).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2021", "--crate-name", "cod55_fixture"])
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        binary
    }

    fn client(&self, extension: &str, fixture: &Path) {
        self.client_named("codex", extension, fixture);
    }

    fn client_named(&self, client: &str, extension: &str, fixture: &Path) {
        let path = self.path(&format!("bin/{client}.{extension}"));
        if extension == "exe" {
            fs::copy(fixture, path).unwrap();
        } else {
            fs::write(
                path,
                format!(
                    "@echo off\r\n\"{}\" %*\r\nexit /b %errorlevel%\r\n",
                    fixture.display()
                ),
            )
            .unwrap();
        }
    }

    fn calls(&self) -> String {
        fs::read_to_string(self.path("calls.log")).unwrap_or_default()
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn payload(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn cod55_windows_profile_override_explicit_and_project_targets() {
    let case = Case::new();
    let expected = case.path("profile space/.config/mcp/mcp.json");
    let result = payload(case.cli("generic").arg("--dry-run").output().unwrap());
    assert_eq!(Path::new(result["path"].as_str().unwrap()), expected);
    let result = payload(
        case.cli("generic")
            .env_remove("HOME")
            .arg("--dry-run")
            .output()
            .unwrap(),
    );
    assert_eq!(Path::new(result["path"].as_str().unwrap()), expected);

    let decoy = case.path("home decoy/.config/mcp/mcp.json");
    fs::create_dir_all(decoy.parent().unwrap()).unwrap();
    fs::write(&decoy, "unrelated HOME configuration").unwrap();
    payload(case.cli("generic").output().unwrap());
    assert!(fs::read_to_string(&expected)
        .unwrap()
        .contains("cod55_selected"));
    assert_eq!(
        fs::read_to_string(&decoy).unwrap(),
        "unrelated HOME configuration"
    );

    let override_dir = case.path("override space");
    let result = payload(
        case.cli("codex")
            .env("CODEX_HOME", &override_dir)
            .arg("--dry-run")
            .output()
            .unwrap(),
    );
    assert_eq!(
        Path::new(result["path"].as_str().unwrap()),
        override_dir.join("config.toml")
    );
    let explicit = case.path("explicit/other.toml");
    let result = payload(
        case.cli("codex")
            .env("CODEX_HOME", &override_dir)
            .args(["--client-config-path"])
            .arg(&explicit)
            .arg("--dry-run")
            .output()
            .unwrap(),
    );
    assert_eq!(Path::new(result["path"].as_str().unwrap()), explicit);

    let result = payload(
        case.cli("generic")
            .args(["--scope", "project", "--dry-run"])
            .env_remove("HOME")
            .env("USERPROFILE", "relative-invalid-profile")
            .output()
            .unwrap(),
    );
    assert_eq!(
        Path::new(result["path"].as_str().unwrap()),
        case.path("repo space/.mcp.json")
    );

    // Removing USERPROFILE must exercise the current-user token lookup, not
    // the conflicting HOME value or the caller/repository directories.
    let result = payload(
        case.cli("generic")
            .env_remove("USERPROFILE")
            .arg("--dry-run")
            .output()
            .unwrap(),
    );
    let path = Path::new(result["path"].as_str().unwrap());
    let profile = path.ancestors().nth(3).unwrap();
    assert!(profile.is_absolute() && profile.is_dir(), "{path:?}");
    assert!(!profile.starts_with(&case.root), "{path:?}");
}

#[test]
fn cod55_windows_exe_cmd_bat_add_and_list_use_selected_target() {
    for extension in ["exe", "cmd", "bat"] {
        let case = Case::new();
        let fixture = case.fixture();
        case.client(extension, &fixture);
        let decoy = case.path("home decoy/.codex/config.toml");
        fs::create_dir_all(decoy.parent().unwrap()).unwrap();
        fs::write(
            &decoy,
            "cod55_selected decoy must never determine visibility",
        )
        .unwrap();
        let result = payload(
            case.cli("codex")
                .env("CODEBASE_GRAPH_SERVER_COMMAND", &fixture)
                .arg("--verify")
                .output()
                .unwrap(),
        );
        assert_eq!(result["method"], "native_cli", "{extension}: {result}");
        assert_eq!(
            result["verification"]["client_visibility"]["ok"], true,
            "{extension}: {result}"
        );
        assert_eq!(
            result["verification"]["stdio"]["ok"], true,
            "{extension}: {result}"
        );
        let selected = case.path("profile space").join(".codex");
        assert_eq!(
            fs::read_to_string(selected.join("native-server")).unwrap(),
            "cod55_selected"
        );
        let calls = case.calls();
        let selected_env = format!("ENV CODEX_HOME {}", hex(selected.to_str().unwrap()));
        assert_eq!(
            calls.matches(&selected_env).count(),
            2,
            "{extension}: {calls}"
        );
        assert!(calls.contains(&format!("ARG {}", hex("add"))));
        assert!(calls.contains(&format!("ARG {}", hex("list"))));
        assert_eq!(
            fs::read_to_string(decoy).unwrap(),
            "cod55_selected decoy must never determine visibility"
        );
    }
}

#[test]
fn cod55_windows_relative_override_and_native_failure_keep_target() {
    let case = Case::new();
    let fixture = case.fixture();
    case.client("cmd", &fixture);
    let result = payload(
        case.cli("codex")
            .env("CODEBASE_GRAPH_SERVER_COMMAND", &fixture)
            .env("CODEX_HOME", "relative home")
            .env("COD55_ADD_FAIL", "1")
            .arg("--verify")
            .output()
            .unwrap(),
    );
    assert_eq!(result["method"], "file_adapter", "{result}");
    assert!(result["native_error"].as_str().unwrap().contains("23"));
    assert_eq!(
        result["verification"]["client_visibility"]["ok"], true,
        "{result}"
    );
    let selected = case.path("caller").join("relative home");
    assert!(fs::read_to_string(selected.join("config.toml"))
        .unwrap()
        .contains("cod55_selected"));
    assert!(!case.path("repo space/relative home").exists());
    let selected_env = format!("ENV CODEX_HOME {}", hex(selected.to_str().unwrap()));
    assert_eq!(case.calls().matches(&selected_env).count(), 2);
}

#[test]
fn cod55_windows_explicit_file_visibility_never_checks_other_target() {
    let case = Case::new();
    let fixture = case.fixture();
    case.client("exe", &fixture);
    let explicit = case.path("explicit space/custom.toml");
    let result = payload(
        case.cli("codex")
            .env("CODEBASE_GRAPH_SERVER_COMMAND", &fixture)
            .arg("--client-config-path")
            .arg(&explicit)
            .arg("--verify")
            .output()
            .unwrap(),
    );
    assert_eq!(result["method"], "file_adapter");
    assert_eq!(
        result["verification"]["client_visibility"]["skipped"], true,
        "{result}"
    );
    assert!(result["verification"]["client_visibility"]["reason"]
        .as_str()
        .unwrap()
        .contains("selected configuration"));
    assert!(case.calls().is_empty());
    assert!(fs::read_to_string(explicit)
        .unwrap()
        .contains("cod55_selected"));
}

#[test]
fn cod55_windows_claude_project_visibility_runs_in_selected_project() {
    for client in ["claude", "claude-project"] {
        let case = Case::new();
        let fixture = case.fixture();
        case.client_named("claude", "cmd", &fixture);
        let result = payload(
            case.cli(client)
                .args(["--scope", "project", "--verify"])
                .env("CODEBASE_GRAPH_SERVER_COMMAND", &fixture)
                .env("COD55_CLIENT", "claude")
                .output()
                .unwrap(),
        );
        let visibility = &result["verification"]["client_visibility"];
        assert_eq!(visibility["ok"], true, "{client}: {result}");
        assert_ne!(visibility["skipped"], true, "{client}: {result}");
        let calls = case.calls();
        let encoded_cwd = calls
            .lines()
            .find_map(|line| line.strip_prefix("CWD "))
            .unwrap();
        let cwd_bytes = encoded_cwd
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let observed_cwd = PathBuf::from(String::from_utf8(cwd_bytes).unwrap());
        assert_eq!(
            fs::canonicalize(observed_cwd).unwrap(),
            fs::canonicalize(case.path("repo space")).unwrap()
        );

        let result = payload(
            case.cli(client)
                .args(["--scope", "project", "--verify"])
                .env("CODEBASE_GRAPH_SERVER_COMMAND", &fixture)
                .env("COD55_CLIENT", "claude")
                .env("COD55_LIST_FAIL", "1")
                .output()
                .unwrap(),
        );
        assert_eq!(result["verification"]["client_visibility"]["ok"], false);
        assert_ne!(result["verification"]["client_visibility"]["skipped"], true);
    }
}

#[test]
fn cod55_windows_unsafe_batch_input_preserves_existing_configuration() {
    let case = Case::new();
    let fixture = case.fixture();
    case.client("bat", &fixture);
    let existing = case.path("profile space/.codex/config.toml");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, "# unrelated settings\n").unwrap();
    for dry_run in [true, false] {
        let mut command = case.cli("codex");
        command.env("CODEBASE_GRAPH_SERVER_COMMAND", "unsafe\ncommand");
        if dry_run {
            command.arg("--dry-run");
        }
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read_to_string(&existing).unwrap(),
            "# unrelated settings\n"
        );
        assert!(case.calls().is_empty());
    }
}

const FIXTURE: &str = r###"
use std::{env,fs,io::{Read,Write},path::PathBuf};
fn hex(s:&str)->String{s.as_bytes().iter().map(|b|format!("{b:02x}")).collect()}
fn main(){
 let args:Vec<String>=env::args().skip(1).collect();
 if args.get(1).map(String::as_str)==Some("start") {
  let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();
  println!(r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}}"#);
  println!(r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"graph_health"},{"name":"graph_search"}]}}"#);
  println!(r#"{"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"ok":true}}}"#);
  println!(r#"{"jsonrpc":"2.0","id":4,"result":{}}"#);
  println!(r#"{"jsonrpc":"2.0","id":5,"result":{"isError":true}}"#);return
 }
 let mut log=fs::OpenOptions::new().append(true).create(true).open(env::var_os("COD55_LOG").unwrap()).unwrap();
 writeln!(log,"CALL").unwrap();for arg in &args{writeln!(log,"ARG {}",hex(arg)).unwrap();}
 for key in ["HOME","USERPROFILE","CODEX_HOME","OPENCLAW_HOME"]{writeln!(log,"ENV {key} {}",hex(&env::var(key).unwrap_or_default())).unwrap();}
 writeln!(log,"CWD {}",hex(&env::current_dir().unwrap().to_string_lossy())).unwrap();
 let claude=env::var("COD55_CLIENT").as_deref()==Ok("claude");
 let root=if claude{env::current_dir().unwrap()}else{PathBuf::from(env::var_os("CODEX_HOME").unwrap())};
 if args.get(1).map(String::as_str)==Some("add") {
  if env::var_os("COD55_ADD_FAIL").is_some(){std::process::exit(23)}
  fs::create_dir_all(&root).unwrap();fs::write(root.join("native-server"),args.get(2).unwrap()).unwrap();
 }else if args.get(1).map(String::as_str)==Some("list"){
  if env::var_os("COD55_LIST_FAIL").is_some(){std::process::exit(25)}
  let contents=if claude{fs::read_to_string(root.join(".mcp.json")).unwrap_or_default()}else{fs::read_to_string(root.join("native-server")).or_else(|_|fs::read_to_string(root.join("config.toml"))).unwrap_or_default()};print!("{contents}");
 }else{std::process::exit(24)}
}
"###;
