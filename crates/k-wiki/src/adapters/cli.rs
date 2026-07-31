use super::{http::HttpServeOptions, TransportError, TransportPayload};
use std::{io::Write, path::PathBuf};

const HELP_TEXT: &str = "\
k-wiki

USAGE:
  k-wiki validate <bundle> [--profile consume|conformant|recommended] [--json]
  k-wiki install [--bin-dir <directory>] [--force]
  k-wiki build <bundle> --out <directory> [--base-url <path>]
  k-wiki serve <bundle> [--host 127.0.0.1] [--port 4321]
  k-wiki inspect <bundle> --concept <concept-id>
  k-wiki check-links <bundle> [--include-external]
  k-wiki mcp [bundle]
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationProfile {
    Consume,
    Conformant,
    Recommended,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliRequest {
    Install {
        bin_dir: Option<PathBuf>,
        force: bool,
    },
    Validate {
        bundle: PathBuf,
        profile: ValidationProfile,
        json: bool,
    },
    Build {
        bundle: PathBuf,
        out: PathBuf,
        base_url: Option<String>,
    },
    Serve {
        bundle: PathBuf,
        options: HttpServeOptions,
    },
    Inspect {
        bundle: PathBuf,
        concept_id: String,
    },
    CheckLinks {
        bundle: PathBuf,
        include_external: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliAction {
    Help,
    Request(CliRequest),
}

pub fn help_text() -> &'static str {
    HELP_TEXT
}

pub fn parse_args(args: &[String]) -> Result<CliAction, String> {
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => Ok(CliAction::Help),
        Some("install") => parse_install(args),
        Some("validate") => parse_validate(args),
        Some("build") => parse_build(args),
        Some("serve") => parse_serve(args),
        Some("inspect") => parse_inspect(args),
        Some("check-links") => parse_check_links(args),
        Some(command) => Err(format!("unknown command: {command}\n\n{HELP_TEXT}")),
    }
}

fn parse_install(args: &[String]) -> Result<CliAction, String> {
    let mut bin_dir = None;
    let mut force = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--bin-dir" => {
                bin_dir = Some(PathBuf::from(required_value(args, index, "--bin-dir")?));
                index += 2;
            }
            "--force" => {
                force = true;
                index += 1;
            }
            other => return Err(format!("unknown install option: {other}\n\n{HELP_TEXT}")),
        }
    }
    Ok(CliAction::Request(CliRequest::Install { bin_dir, force }))
}

pub fn run<W, E, F>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
    mut dispatch: F,
) -> Result<i32, String>
where
    W: Write,
    E: Write,
    F: FnMut(CliRequest) -> Result<TransportPayload, TransportError>,
{
    match parse_args(args)? {
        CliAction::Help => {
            writeln!(stdout, "{HELP_TEXT}").map_err(|error| error.to_string())?;
            Ok(0)
        }
        CliAction::Request(request) => {
            let machine_readable = matches!(&request, CliRequest::Validate { json: true, .. });
            match dispatch(request) {
                Ok(payload) => {
                    if machine_readable {
                        write_json_line(stdout, &payload.structured)?;
                    } else {
                        writeln!(stdout, "{}", payload.text).map_err(|error| error.to_string())?;
                    }
                    Ok(0)
                }
                Err(error) => {
                    if machine_readable {
                        write_json_line(
                            stderr,
                            &serde_json::json!({
                                "error": {
                                    "code": error.code,
                                    "message": error.message,
                                    "details": error.details,
                                    "retryable": error.retryable,
                                }
                            }),
                        )?;
                    } else {
                        writeln!(stderr, "{}: {}", error.code, error.message)
                            .map_err(|write_error| write_error.to_string())?;
                    }
                    Ok(1)
                }
            }
        }
    }
}

fn write_json_line(writer: &mut impl Write, value: &serde_json::Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
    writeln!(writer).map_err(|error| error.to_string())
}

fn parse_validate(args: &[String]) -> Result<CliAction, String> {
    let bundle = required_path(args.get(1), "validate <bundle>")?;
    let mut profile = ValidationProfile::Recommended;
    let mut json = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                let value = required_value(args, index, "--profile")?;
                profile = match value {
                    "consume" => ValidationProfile::Consume,
                    "conformant" => ValidationProfile::Conformant,
                    "recommended" => ValidationProfile::Recommended,
                    _ => {
                        return Err(
                            "--profile must be one of consume, conformant, or recommended"
                                .to_string(),
                        )
                    }
                };
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            other => return Err(format!("unknown validate option: {other}\n\n{HELP_TEXT}")),
        }
    }
    Ok(CliAction::Request(CliRequest::Validate {
        bundle,
        profile,
        json,
    }))
}

fn parse_build(args: &[String]) -> Result<CliAction, String> {
    let bundle = required_path(args.get(1), "build <bundle>")?;
    let mut out = None;
    let mut base_url = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                out = Some(PathBuf::from(required_value(args, index, "--out")?));
                index += 2;
            }
            "--base-url" => {
                base_url = Some(required_value(args, index, "--base-url")?.to_string());
                index += 2;
            }
            other => return Err(format!("unknown build option: {other}\n\n{HELP_TEXT}")),
        }
    }
    Ok(CliAction::Request(CliRequest::Build {
        bundle,
        out: out.ok_or_else(|| "--out is required".to_string())?,
        base_url,
    }))
}

fn parse_serve(args: &[String]) -> Result<CliAction, String> {
    let bundle = required_path(args.get(1), "serve <bundle>")?;
    let options = HttpServeOptions::parse(&args[2..])?;
    Ok(CliAction::Request(CliRequest::Serve { bundle, options }))
}

fn parse_inspect(args: &[String]) -> Result<CliAction, String> {
    let bundle = required_path(args.get(1), "inspect <bundle>")?;
    let mut concept_id = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--concept" => {
                concept_id = Some(required_value(args, index, "--concept")?.to_string());
                index += 2;
            }
            other => return Err(format!("unknown inspect option: {other}\n\n{HELP_TEXT}")),
        }
    }
    Ok(CliAction::Request(CliRequest::Inspect {
        bundle,
        concept_id: concept_id.ok_or_else(|| "--concept is required".to_string())?,
    }))
}

fn parse_check_links(args: &[String]) -> Result<CliAction, String> {
    let bundle = required_path(args.get(1), "check-links <bundle>")?;
    let mut include_external = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--include-external" => {
                include_external = true;
                index += 1;
            }
            other => {
                return Err(format!(
                    "unknown check-links option: {other}\n\n{HELP_TEXT}"
                ))
            }
        }
    }
    Ok(CliAction::Request(CliRequest::CheckLinks {
        bundle,
        include_external,
    }))
}

fn required_path(value: Option<&String>, usage: &str) -> Result<PathBuf, String> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing required argument: {usage}"))
}

fn required_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}
