use crate::adapters::{
    cli::{format::mcp_help, run},
    mcp::{serve_mcp_http, serve_mcp_stdio, McpHttpOptions, McpServeOptions},
};
use std::{
    env,
    io::{self},
};

pub fn run_from_env() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    run_process_args(args)
}

pub(crate) fn run_process_args(args: Vec<String>) -> Result<(), String> {
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
