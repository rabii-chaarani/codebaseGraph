use crate::adapters::{
    cli::{format::mcp_help, run},
    mcp::{serve_mcp_http, serve_mcp_stdio, McpHttpOptions, McpServeOptions},
};
use std::{
    env,
    io::{self},
    path::Path,
};

pub fn run_from_env() -> Result<(), String> {
    if let Ok(executable) = env::current_exe() {
        crate::db_writer::register_phase_worker_executable(executable.clone());
        crate::materialization_worker::register_worker_executable(executable);
    }
    let args: Vec<String> = env::args().skip(1).collect();
    run_process_args(args)
}

pub(crate) fn run_process_args(args: Vec<String>) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("__codebase_graph_internal") {
        return run_internal_command(&args[1..]);
    }
    if args.is_empty() {
        return run(args, &mut io::stdout());
    }
    if args.first().map(String::as_str) == Some("mcp") {
        match args.get(1).map(String::as_str) {
            Some("start") => {
                let options = McpServeOptions::parse(&args[2..], mcp_help())?;
                return serve_mcp_stdio(&options, io::stdin().lock(), &mut io::stdout());
            }
            Some("http") => {
                let options = McpHttpOptions::parse(&args[2..], mcp_help())?;
                return serve_mcp_http(&options);
            }
            _ => {}
        }
    }
    run(args, &mut io::stdout())
}

fn run_internal_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("ladybug-write-phase-v1") => {
            let request_path = args
                .get(1)
                .ok_or_else(|| "Ladybug write phase requires a request path".to_string())?;
            if args.len() != 2 {
                return Err("Ladybug write phase accepts exactly one request path".to_string());
            }
            crate::db_writer::execute_phase_file(Path::new(request_path))
        }
        Some("materialization-worker-v1") => {
            let request_path = args.get(1).ok_or_else(|| {
                "materialization worker requires request and result paths".to_string()
            })?;
            let result_path = args.get(2).ok_or_else(|| {
                "materialization worker requires request and result paths".to_string()
            })?;
            if args.len() != 3 {
                return Err(
                    "materialization worker accepts exactly request and result paths".to_string(),
                );
            }
            crate::materialization_worker::execute_worker_file(
                Path::new(request_path),
                Path::new(result_path),
            )
        }
        Some(command) => Err(format!("unknown internal command: {command}")),
        None => Err("missing internal command".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_materialization_worker_mode_is_selected_before_public_commands() {
        let error = run_process_args(vec![
            "__codebase_graph_internal".to_string(),
            "materialization-worker-v1".to_string(),
        ])
        .unwrap_err();
        assert_eq!(
            error,
            "materialization worker requires request and result paths"
        );
    }
}
