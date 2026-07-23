use crate::cli::materialization::{
    build_request, read_request, request_manifest_path, write_manifest, MaterializeOptions,
};
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use std::time::Instant;

pub(crate) fn execute_materialization(
    options: &MaterializeOptions,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let request = match options.native_request.as_ref() {
        Some(request_path) => read_request(request_path)?,
        None => build_request(options)?,
    };
    execute_materialization_request(options, request)
}

pub(crate) fn execute_candidate_materialization(
    options: &MaterializeOptions,
    candidate_paths: Vec<String>,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let mut request = build_request(options)?;
    request.candidate_paths = candidate_paths;
    request.atomic_rebuild = false;
    execute_materialization_request(options, request)
}

pub(crate) fn execute_materialization_request(
    options: &MaterializeOptions,
    request: NativeSyntaxMaterializationRequest,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let started = Instant::now();
    let final_request = request;
    let mut response = crate::execute_materialization_pipeline(&final_request)
        .map_err(|error| error.to_string())?;
    response.phase_timings.insert(
        "native_cli_seconds".to_string(),
        started.elapsed().as_secs_f64(),
    );

    if let Some(manifest_path) = request_manifest_path(options).as_ref() {
        write_manifest(
            manifest_path,
            &final_request,
            &response.rebuilt_entries,
            &response.diff,
        )?;
    }

    Ok((final_request, response))
}
