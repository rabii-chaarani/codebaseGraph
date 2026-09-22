use crate::agent_hooks::{
    reconcile_agent_hooks, remove_agent_hooks, resolve_agent_hook_clients, run_agent_hook_stdin,
    verify_agent_hooks, AgentHookClient, MANAGED_ID,
};
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct AgentHooksOptions {
    client: String,
    repo_root: Option<PathBuf>,
    config: Option<PathBuf>,
    dry_run: bool,
    verify: bool,
    managed_id: String,
    help: bool,
}

impl AgentHooksOptions {
    fn parse(args: &[String], command: &str) -> Result<Self, String> {
        let mut options = Self {
            client: "all".to_string(),
            repo_root: None,
            config: None,
            dry_run: false,
            verify: false,
            managed_id: MANAGED_ID.to_string(),
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
                    options.client = required(args, index, "--client")?.to_string();
                    index += 2;
                }
                "--repo-root" => {
                    options.repo_root = Some(PathBuf::from(required(args, index, "--repo-root")?));
                    index += 2;
                }
                "--config" | "--config-path" => {
                    options.config = Some(PathBuf::from(required(args, index, "--config")?));
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
                "--managed-id" => {
                    options.managed_id = required(args, index, "--managed-id")?.to_string();
                    index += 2;
                }
                "--json" => index += 1,
                other => {
                    return Err(format!(
                        "unknown agent-hooks {command} option: {other}\n\n{}",
                        agent_hooks_help()
                    ));
                }
            }
        }
        Ok(options)
    }

    fn paths(&self) -> Result<(PathBuf, PathBuf), String> {
        let config = self
            .config
            .clone()
            .or_else(|| {
                self.repo_root
                    .as_ref()
                    .map(|root| root.join(".codebaseGraph/config.json"))
            })
            .unwrap_or_else(|| PathBuf::from(".codebaseGraph/config.json"));
        let config = absolutize(config);
        let root = self
            .repo_root
            .clone()
            .or_else(|| {
                config
                    .parent()
                    .and_then(|parent| parent.parent())
                    .map(PathBuf::from)
            })
            .unwrap_or(env::current_dir().map_err(|error| error.to_string())?);
        Ok((absolutize(root), config))
    }
}

fn required<'a>(args: &'a [String], index: usize, name: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} requires a value"))
}

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or_else(|_| path)
    }
}

fn clients(selection: &str) -> Result<Vec<AgentHookClient>, String> {
    resolve_agent_hook_clients(selection, selection)
}

pub(in crate::adapters::cli) fn run_agent_hooks_command<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let action = args.first().map(String::as_str).unwrap_or("help");
    if matches!(action, "-h" | "--help" | "help") {
        writeln!(stdout, "{}", agent_hooks_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    match action {
        "install" | "remove" | "verify" => {
            let options = AgentHooksOptions::parse(&args[1..], action)?;
            if options.help {
                writeln!(stdout, "{}", agent_hooks_help()).map_err(|error| error.to_string())?;
                return Ok(());
            }
            if options.managed_id != MANAGED_ID {
                return Err(format!("--managed-id must be {MANAGED_ID}"));
            }
            let (root, config) = options.paths()?;
            let selected = clients(&options.client)?;
            let mut payload = match action {
                "install" => reconcile_agent_hooks(&root, &config, &selected, options.dry_run)?,
                "remove" => remove_agent_hooks(&root, &config, &selected, options.dry_run)?,
                "verify" => verify_agent_hooks(&root, &config, &selected)?,
                _ => unreachable!(),
            };
            if action == "install" && options.verify {
                payload["verification"] = verify_agent_hooks(&root, &config, &selected)?;
            }
            writeln!(
                stdout,
                "{}",
                serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())
        }
        "run" => {
            let options = AgentHooksOptions::parse(&args[1..], action)?;
            if options.help {
                writeln!(stdout, "{}", agent_hooks_help()).map_err(|error| error.to_string())?;
                return Ok(());
            }
            if options.managed_id != MANAGED_ID {
                return Ok(());
            }
            let client = AgentHookClient::parse_for_cli(&options.client)?;
            let (_, config) = options.paths()?;
            let mut stdin = io::stdin().lock();
            run_agent_hook_stdin(client, &config, &mut stdin, stdout)
        }
        other => Err(format!(
            "unknown agent-hooks command: {other}\n\n{}",
            agent_hooks_help()
        )),
    }
}

pub(in crate::adapters::cli) fn agent_hooks_help() -> &'static str {
    "codebase-graph agent-hooks\n\nUSAGE:\n  codebase-graph agent-hooks install --client <codex|claude|github-copilot|all> [--repo-root <path>] [--config <path>] [--dry-run]\n  codebase-graph agent-hooks remove --client <codex|claude|github-copilot|all> [--repo-root <path>] [--config <path>] [--dry-run]\n  codebase-graph agent-hooks verify --client <codex|claude|github-copilot|all> [--repo-root <path>] [--config <path>]\n  codebase-graph agent-hooks run --client <codex|claude|github-copilot> --config <path> --managed-id codebase-graph-v1\n\nHooks are advisory, fail open within a bounded deadline, and use the existing managed loopback MCP daemon."
}

trait CliClient {
    fn parse_for_cli(value: &str) -> Result<Self, String>
    where
        Self: Sized;
}

impl CliClient for AgentHookClient {
    fn parse_for_cli(value: &str) -> Result<Self, String> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude" | "claude-project" => Ok(Self::Claude),
            "github-copilot" | "copilot" => Ok(Self::GithubCopilot),
            other => Err(format!("unsupported agent hook client: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_resolve_repo_local_config_by_default() {
        let expected_root = env::temp_dir().join("codebase-graph-agent-hooks-options-repo");
        let options = AgentHooksOptions::parse(
            &[
                "--repo-root".to_string(),
                expected_root.to_string_lossy().into_owned(),
            ],
            "install",
        )
        .unwrap();
        let (root, config) = options.paths().unwrap();
        assert!(root.is_absolute());
        assert_eq!(root, expected_root);
        assert_eq!(config, expected_root.join(".codebaseGraph/config.json"));
    }
}
