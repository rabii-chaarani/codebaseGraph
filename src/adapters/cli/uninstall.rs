use super::format::uninstall_help;
use crate::adapters::required_arg;
use crate::api::{
    CodebaseGraphApi, OperationRequest, OutputFormat, RepoSelector, RepositoryLifecycleRequest,
};
use std::{io::Write, path::PathBuf};

#[derive(Debug)]
pub(in crate::adapters::cli) struct UninstallOptions {
    repo_root: Option<PathBuf>,
    config: Option<PathBuf>,
    mcp_client: String,
    client_config_path: Option<PathBuf>,
    dry_run: bool,
    json: bool,
    help: bool,
}

impl UninstallOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: None,
            config: None,
            mcp_client: "all".to_string(),
            client_config_path: None,
            dry_run: false,
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
                "--repo-root" | "--source-root" => {
                    options.repo_root =
                        Some(PathBuf::from(required_arg(args, index, "--repo-root")?));
                    index += 2;
                }
                "--config" => {
                    options.config = Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--mcp-client" => {
                    let client = required_arg(args, index, "--mcp-client")?;
                    options.mcp_client = client.to_string();
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
                "--dry-run" => {
                    options.dry_run = true;
                    index += 1;
                }
                "--json" => {
                    options.json = true;
                    index += 1;
                }
                other => {
                    return Err(format!(
                        "unknown uninstall option: {other}\n\n{}",
                        uninstall_help()
                    ));
                }
            }
        }
        if options.client_config_path.is_some() && options.mcp_client == "all" {
            return Err("--client-config-path requires --mcp-client <client>".to_string());
        }
        Ok(options)
    }
}

pub(in crate::adapters::cli) fn run_uninstall<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = UninstallOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", uninstall_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let request = RepositoryLifecycleRequest {
        repo: RepoSelector {
            repo_root: options.repo_root.clone(),
            config_path: options.config.clone(),
            db_path: None,
            manifest_path: None,
        },
        action: "uninstall".to_string(),
        output_format: if options.json {
            OutputFormat::Typed
        } else {
            OutputFormat::Block
        },
        dry_run: options.dry_run,
        mcp_client: Some(options.mcp_client.clone()),
        mcp_config_path: options.client_config_path.clone(),
        instructions_target: None,
        skip_mcp_config: false,
        mode: "changed".to_string(),
        include_fts: true,
        semantic_enrichment: true,
        semantic_provider_mode: "local_only".to_string(),
    };
    let payload = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Uninstall(request))
        .map_err(|error| error.message)?
        .payload;
    if options.json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
    } else {
        let text = payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "block response did not contain text".to_string())?;
        write!(stdout, "{text}").map_err(|error| error.to_string())?;
    }
    Ok(())
}
