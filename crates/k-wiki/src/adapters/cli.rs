use super::{http::HttpServeOptions, install::McpInstallRequest, TransportError, TransportPayload};
use std::{io::Write, path::PathBuf};

const HELP_TEXT: &str = "\
k-wiki

USAGE:
  k-wiki validate <bundle> [--profile consume|conformant|recommended] [--json]
  k-wiki install [--repo-root <directory>]
  k-wiki build <bundle> --out <directory> [--base-url <path>]
  k-wiki serve <bundle> [--host 127.0.0.1] [--port 4321]
  k-wiki inspect <bundle> --concept <concept-id>
  k-wiki check-links <bundle> [--include-external]
  k-wiki mcp install --client <client> [--repo-root <directory>] [--scope local|user|project] [--name <name>] [--client-config-path <path>] [--dry-run]
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
        repo_root: Option<PathBuf>,
    },
    InstallMcp {
        request: McpInstallRequest,
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
        Some("mcp") => parse_mcp(args),
        Some("validate") => parse_validate(args),
        Some("build") => parse_build(args),
        Some("serve") => parse_serve(args),
        Some("inspect") => parse_inspect(args),
        Some("check-links") => parse_check_links(args),
        Some(command) => Err(format!("unknown command: {command}\n\n{HELP_TEXT}")),
    }
}

fn parse_mcp(args: &[String]) -> Result<CliAction, String> {
    match args.get(1).map(String::as_str) {
        Some("install") => parse_mcp_install(&args[2..]),
        _ => Err(format!("usage: k-wiki mcp [bundle]\n\n{HELP_TEXT}")),
    }
}

fn parse_mcp_install(args: &[String]) -> Result<CliAction, String> {
    let mut client = None;
    let mut scope = "local".to_string();
    let mut name = None;
    let mut client_config_path = None;
    let mut repo_root = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--client" => {
                client = Some(required_value(args, index, "--client")?.to_string());
                index += 2;
            }
            "--scope" => {
                scope = required_value(args, index, "--scope")?.to_string();
                index += 2;
            }
            "--name" => {
                name = Some(required_value(args, index, "--name")?.to_string());
                index += 2;
            }
            "--client-config-path" => {
                client_config_path = Some(PathBuf::from(required_value(
                    args,
                    index,
                    "--client-config-path",
                )?));
                index += 2;
            }
            "--repo-root" => {
                repo_root = Some(PathBuf::from(required_value(args, index, "--repo-root")?));
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            other => {
                return Err(format!(
                    "unknown mcp install option: {other}\n\n{HELP_TEXT}"
                ))
            }
        }
    }
    let client = client.ok_or_else(|| format!("--client is required\n\n{HELP_TEXT}"))?;
    let normalized_client = client.trim().to_ascii_lowercase();
    if normalized_client != "all"
        && !codebase_graph::api::supported_mcp_clients().contains(&normalized_client.as_str())
    {
        return Err(format!("unsupported MCP client: {client}"));
    }
    if !matches!(
        scope.trim().to_ascii_lowercase().as_str(),
        "local" | "user" | "project"
    ) {
        return Err("MCP install scope must be local, user, or project".to_string());
    }
    Ok(CliAction::Request(CliRequest::InstallMcp {
        request: McpInstallRequest {
            client,
            scope,
            name,
            client_config_path,
            repo_root,
            dry_run,
        },
    }))
}

fn parse_install(args: &[String]) -> Result<CliAction, String> {
    let mut repo_root = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--repo-root" => {
                repo_root = Some(PathBuf::from(required_value(args, index, "--repo-root")?));
                index += 2;
            }
            other => return Err(format!("unknown install option: {other}\n\n{HELP_TEXT}")),
        }
    }
    Ok(CliAction::Request(CliRequest::Install { repo_root }))
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
            let machine_readable = matches!(
                &request,
                CliRequest::Validate { json: true, .. } | CliRequest::InstallMcp { .. }
            );
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
