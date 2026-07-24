use super::{attach_install_verification, options::McpInstallOptions};
use crate::adapters::cli::format::mcp_install_help;
use crate::api::{
    CodebaseGraphApi, McpInstallRequest, OperationRequest, OutputFormat, RepoSelector,
};
use std::io::Write;

pub(in crate::adapters::cli) fn run_mcp_install<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = McpInstallOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", mcp_install_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let request = OperationRequest::InstallMcp(McpInstallRequest {
        repo: RepoSelector {
            repo_root: options.repo_root.clone(),
            config_path: options.config_path.clone(),
            db_path: None,
            manifest_path: None,
        },
        client: options.client.clone(),
        scope: options.scope.clone(),
        name: options.name.clone(),
        client_config_path: options.client_config_path.clone(),
        dry_run: options.dry_run,
        output_format: OutputFormat::Typed,
    });
    let response = CodebaseGraphApi::new()
        .execute_operation(&request)
        .map_err(|error| error.message)?;
    let payload = attach_install_verification(response.payload, &options);
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
