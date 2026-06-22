use super::manifest::{read_request, request_manifest_path, write_manifest};
use super::output::{materialization_payload, serialize_plan_block};
use super::request::{build_request, MaterializeOptions};
use crate::cli::format::{materialize_help, plan_help, schema_statements_from_copy_statements};
use crate::cli::setup::GraphStatePaths;
use crate::db_writer::{write_database, LadybugWriteRequest};
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

pub(in crate::cli) fn run_materialize<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = MaterializeOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", materialize_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let (_, response) = materialize(&options)?;
    let output = serde_json::to_string_pretty(&response).map_err(|error| error.to_string())?;
    writeln!(stdout, "{output}").map_err(|error| error.to_string())?;
    Ok(())
}

pub(in crate::cli) fn materialize(
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
    materialize_request(options, request)
}

pub(in crate::cli) fn materialize_candidate_paths(
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
    materialize_request(options, request)
}

pub(in crate::cli) fn materialize_request(
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
    let mut response =
        crate::materialize_syntax_batch(&final_request).map_err(|error| error.to_string())?;
    if !response.skipped {
        let database_started = Instant::now();
        write_materialized_database(&final_request, &response)?;
        response.phase_timings.insert(
            "database_write_seconds".to_string(),
            database_started.elapsed().as_secs_f64(),
        );
        response.database_written = true;
    }
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

fn write_materialized_database(
    request: &NativeSyntaxMaterializationRequest,
    response: &NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    let schema_statements = if request.schema_statements.is_empty() {
        schema_statements_from_copy_statements(request.include_fts, &response.copy_statements)
    } else {
        request.schema_statements.clone()
    };
    let mut delete_statements = crate::db_writer::partition_delete_statements(
        request.previous_manifest.as_ref(),
        &response.diff,
    );
    delete_statements.extend(crate::db_writer::incoming_row_delete_statements(
        request.previous_manifest.as_ref(),
        &response.diff,
        &response.rebuilt_entries,
    ));
    write_database(LadybugWriteRequest {
        db_path: request.db_path.clone(),
        include_fts: request.include_fts,
        schema_statements,
        replace_database: response.diff.force_rebuild,
        delete_statements,
        copy_statements: response.copy_statements.clone(),
    })
    .map_err(|error| error.to_string())
}

pub(in crate::cli) fn run_plan<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MaterializeOptions::parse_with_command(args, "plan")?;
    if options.help {
        writeln!(stdout, "{}", plan_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let mut request = match options.native_request.as_ref() {
        Some(request_path) => read_request(request_path)?,
        None => build_request(&options)?,
    };
    request.atomic_rebuild = false;
    let response =
        crate::plan_syntax_materialization(&request).map_err(|error| error.to_string())?;
    let paths = GraphStatePaths::derive(Path::new(&request.source_root));
    let payload = materialization_payload(&response, &request.mode, &paths);
    if options.json_output {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())
    } else {
        write!(stdout, "{}", serialize_plan_block(&payload)).map_err(|error| error.to_string())
    }
}
