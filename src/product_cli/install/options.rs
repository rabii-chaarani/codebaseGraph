use super::{expand_path, supported_install_clients, supported_install_clients_with_all};
use crate::product_cli::{format::mcp_install_help, util::required_arg};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(in crate::product_cli) struct McpInstallOptions {
    pub(in crate::product_cli) client: String,
    pub(in crate::product_cli) scope: String,
    pub(in crate::product_cli) name: Option<String>,
    pub(in crate::product_cli) config_path: Option<PathBuf>,
    pub(in crate::product_cli) client_config_path: Option<PathBuf>,
    pub(in crate::product_cli) repo_root: PathBuf,
    pub(in crate::product_cli) dry_run: bool,
    pub(in crate::product_cli) verify: bool,
    pub(in crate::product_cli) json: bool,
    pub(in crate::product_cli) help: bool,
}

impl McpInstallOptions {
    pub(in crate::product_cli) fn parse(args: &[String]) -> Result<Self, String> {
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
