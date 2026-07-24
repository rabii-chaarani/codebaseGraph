use crate::adapters::cli::format::{materialize_help, plan_help};
use crate::api::{
    contracts::{MaterializationRequest, OperationRequest, OutputFormat},
    materialization::{
        materialization_request as build_public_materialization_request, MaterializeOptions,
    },
    CodebaseGraphApi,
};
use std::io::Write;

pub(in crate::adapters::cli) fn run_materialize<W: Write>(
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

pub(in crate::adapters::cli) fn run_plan<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
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
    build_public_materialization_request(options, output_format)
}
