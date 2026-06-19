use crate::cli::{format::mcp_help, install::run_mcp_install};
use std::io::Write;

pub(in crate::cli) fn run_mcp_command<W: Write>(
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
        Some(command) => Err(format!("unknown mcp command: {command}\n\n{}", mcp_help())),
    }
}
