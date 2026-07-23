use super::manifest::read_request;
use super::request::{build_request, MaterializeOptions};
use crate::api::{
    contracts::{MaterializationRequest, OperationRequest, OutputFormat, RepoSelector},
    CodebaseGraphApi,
};
use crate::cli::format::{materialize_help, plan_help};
use std::io::Write;
use std::path::Path;

pub(in crate::cli) fn run_materialize<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = MaterializeOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", materialize_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let api = CodebaseGraphApi::new();
    let request = materialize_request(&options, OutputFormat::Typed)?;
    let response = api
        .execute_operation(&OperationRequest::Materialize(request))
        .map_err(|error| error.message)?;
    let output =
        serde_json::to_string_pretty(&response.payload).map_err(|error| error.to_string())?;
    writeln!(stdout, "{output}").map_err(|error| error.to_string())?;
    Ok(())
}

pub(in crate::cli) fn run_plan<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MaterializeOptions::parse_with_command(args, "plan")?;
    if options.help {
        writeln!(stdout, "{}", plan_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let api = CodebaseGraphApi::new();
    let request = materialize_request(
        &options,
        if options.json_output {
            OutputFormat::Typed
        } else {
            OutputFormat::Block
        },
    )?;
    let response = api
        .execute_operation(&OperationRequest::Plan(request))
        .map_err(|error| error.message)?;
    let payload = response.payload;
    if options.json_output {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())
    } else {
        let text = payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "block response did not contain text".to_string())?;
        write!(stdout, "{text}").map_err(|error| error.to_string())
    }
}

fn materialize_request(
    options: &MaterializeOptions,
    output_format: OutputFormat,
) -> Result<MaterializationRequest, String> {
    let request = match options.native_request.as_ref() {
        Some(request_path) => {
            let native = read_request(request_path)?;
            MaterializationRequest {
                repo: RepoSelector {
                    repo_root: Some(Path::new(&native.source_root).to_path_buf()),
                    config_path: None,
                    db_path: Some(Path::new(&native.db_path).to_path_buf()),
                    manifest_path: options.manifest.clone(),
                },
                native_request_path: Some(request_path.clone()),
                source_root: Some(native.source_root),
                mode: native.mode,
                include_fts: native.include_fts,
                semantic_enrichment: native.semantic_enrichment,
                semantic_provider_mode: native.semantic_provider_mode,
                use_git: options.use_git,
                git_diff: options.git_diff,
                git_base: options.git_base.clone(),
                include_patterns: native.include_patterns,
                exclude_patterns: native.exclude_patterns,
                candidate_paths: native.candidate_paths,
                parallel: native.parallel,
                progress: native.progress,
                output_format,
            }
        }
        None => {
            let materialization_request = build_request(options)
                .map_err(|error| format!("failed to build materialization request: {error}"))?;
            MaterializationRequest {
                repo: RepoSelector {
                    repo_root: options.source_root.clone(),
                    config_path: None,
                    db_path: options.db.clone(),
                    manifest_path: options.manifest.clone(),
                },
                native_request_path: None,
                source_root: options
                    .source_root
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                mode: materialization_request.mode,
                include_fts: materialization_request.include_fts,
                semantic_enrichment: materialization_request.semantic_enrichment,
                semantic_provider_mode: materialization_request.semantic_provider_mode,
                use_git: options.use_git,
                git_diff: options.git_diff,
                git_base: options.git_base.clone(),
                include_patterns: materialization_request.include_patterns,
                exclude_patterns: materialization_request.exclude_patterns,
                candidate_paths: materialization_request.candidate_paths,
                parallel: materialization_request.parallel,
                progress: materialization_request.progress,
                output_format,
            }
        }
    };
    Ok(request)
}
