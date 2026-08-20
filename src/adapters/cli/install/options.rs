use crate::adapters::{cli::format::mcp_install_help, required_arg};
use crate::api::McpTransport;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(in crate::adapters::cli) struct McpInstallOptions {
    pub(in crate::adapters::cli) client: String,
    pub(in crate::adapters::cli) scope: String,
    pub(in crate::adapters::cli) name: Option<String>,
    pub(in crate::adapters::cli) config_path: Option<PathBuf>,
    pub(in crate::adapters::cli) client_config_path: Option<PathBuf>,
    pub(in crate::adapters::cli) repo_root: Option<PathBuf>,
    pub(in crate::adapters::cli) dry_run: bool,
    pub(in crate::adapters::cli) verify: bool,
    pub(in crate::adapters::cli) transport: McpTransport,
    pub(in crate::adapters::cli) daemon_port: Option<u16>,
    pub(in crate::adapters::cli) json: bool,
    pub(in crate::adapters::cli) help: bool,
}

impl McpInstallOptions {
    pub(in crate::adapters::cli) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            client: "codex".to_string(),
            scope: "local".to_string(),
            name: None,
            config_path: None,
            client_config_path: None,
            repo_root: None,
            dry_run: false,
            verify: false,
            transport: McpTransport::Auto,
            daemon_port: None,
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
                    index += 2;
                }
                "--scope" => {
                    options.scope = required_arg(args, index, "--scope")?.to_string();
                    index += 2;
                }
                "--name" => {
                    options.name = Some(required_arg(args, index, "--name")?.to_string());
                    index += 2;
                }
                "--config-path" => {
                    options.config_path =
                        Some(PathBuf::from(required_arg(args, index, "--config-path")?));
                    index += 2;
                }
                "--client-config-path" => {
                    options.client_config_path = Some(PathBuf::from(required_arg(
                        args,
                        index,
                        "--client-config-path",
                    )?));
                    index += 2;
                }
                "--repo-root" => {
                    options.repo_root =
                        Some(PathBuf::from(required_arg(args, index, "--repo-root")?));
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
                "--mcp-transport" => {
                    options.transport =
                        McpTransport::parse(required_arg(args, index, "--mcp-transport")?)?;
                    index += 2;
                }
                "--mcp-daemon-port" => {
                    let port = required_arg(args, index, "--mcp-daemon-port")?
                        .parse::<u16>()
                        .map_err(|_| "--mcp-daemon-port must be between 1 and 65535".to_string())?;
                    if port == 0 {
                        return Err("--mcp-daemon-port must be between 1 and 65535".to_string());
                    }
                    options.daemon_port = Some(port);
                    index += 2;
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
