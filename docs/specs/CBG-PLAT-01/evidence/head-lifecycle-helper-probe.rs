use std::{env,path::{Path,PathBuf}};
fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
fn executable_in_path(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| dir.join(executable).is_file()))
        .unwrap_or(false)
}
fn default_client_config_path(adapter: &str, scope: &str, repo_root: &Path) -> PathBuf {
    let home = home_dir();
    match adapter {
        "codex" => {
            if scope == "user" {
                env::var_os("CODEX_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".codex"))
                    .join("config.toml")
            } else {
                repo_root.join(".codex/config.toml")
            }
        }
        "claude" => {
            if scope == "project" {
                repo_root.join(".mcp.json")
            } else {
                home.join(".claude.json")
            }
        }
        "claude-project" => repo_root.join(".mcp.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "github-copilot" => repo_root.join(".vscode/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"))
            .join("mcp.json5"),
        "generic" => {
            if scope == "project" {
                repo_root.join(".mcp.json")
            } else {
                home.join(".config/mcp/mcp.json")
            }
        }
        _ => home.join(".config/mcp/mcp.json"),
    }
}
fn main() {
 let repo=env::current_dir().unwrap();
 println!("resolved_home={}",home_dir().display());
 println!("codex_user={}",default_client_config_path("codex","user",&repo).display());
 println!("claude_user={}",default_client_config_path("claude","user",&repo).display());
 println!("codex_available={}",executable_in_path("codex"));
 println!("codex_exe_available={}",executable_in_path("codex.exe"));
}
