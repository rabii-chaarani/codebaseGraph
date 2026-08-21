use super::{format::mcp_help, install::run_mcp_install};
use crate::daemon_service::{
    start_mcp_daemon, status_mcp_daemon, stop_mcp_daemon, McpDaemonOptions,
};
use std::io::Write;

pub(in crate::adapters::cli) fn run_mcp_command<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("-h" | "--help") | None => {
            writeln!(stdout, "{}", mcp_help()).map_err(|error| error.to_string())?;
            Ok(())
        }
        Some("install") => run_mcp_install(&args[1..], stdout),
        Some("start") => Err("mcp start requires the process stdin/stdout transport; run it through the codebase-graph binary".to_string()),
        Some("http") => Err("mcp http starts a blocking HTTP server; run it through the codebase-graph binary".to_string()),
        Some("daemon") => run_mcp_daemon_command(&args[1..], stdout),
        Some(command) => Err(format!("unknown mcp command: {command}\n\n{}", mcp_help())),
    }
}

fn run_mcp_daemon_command<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let action = args.first().map(String::as_str).ok_or_else(|| {
        format!(
            "mcp daemon requires serve, start, stop, or status\n\n{}",
            mcp_help()
        )
    })?;
    if action == "serve" {
        return Err(
            "mcp daemon serve starts a blocking server; run it through the codebase-graph binary"
                .to_string(),
        );
    }
    let options = McpDaemonOptions::parse(&args[1..])?;
    let payload = match action {
        "start" => start_mcp_daemon(&options)?,
        "stop" => stop_mcp_daemon(&options, false)?,
        "status" => status_mcp_daemon(&options)?,
        other => {
            return Err(format!(
                "unknown mcp daemon command: {other}\n\n{}",
                mcp_help()
            ))
        }
    };
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())
}
