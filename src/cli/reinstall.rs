use super::{format::reinstall_help, watch::SetupOptions};
use crate::api::{
    contracts::{OperationRequest, OutputFormat, RepoSelector, RepositoryLifecycleRequest},
    CodebaseGraphApi,
};
use std::io::Write;

pub(in crate::cli) fn run_reinstall<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = SetupOptions::parse_with_help(args, "reinstall", reinstall_help())?;
    if options.help {
        writeln!(stdout, "{}", reinstall_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let request = RepositoryLifecycleRequest {
        repo: RepoSelector {
            repo_root: options.repo_root.clone(),
            config_path: None,
            db_path: None,
            manifest_path: None,
        },
        action: "reinstall".to_string(),
        output_format: OutputFormat::Typed,
        dry_run: options.dry_run,
        mcp_client: Some(options.mcp_client.clone()),
        mcp_config_path: options.mcp_config_path.clone(),
        instructions_target: Some(options.instructions_target.clone()),
        skip_mcp_config: options.skip_mcp_config,
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
    };
    let payload = CodebaseGraphApi::new()
        .execute_operation(&OperationRequest::Reinstall(request))
        .map_err(|error| error.message)?
        .payload;
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
