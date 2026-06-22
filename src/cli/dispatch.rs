use super::{
    build::{run_materialize, run_plan},
    format::top_level_help,
    graph::{
        run_graph_architecture_queries, run_graph_context, run_graph_health, run_graph_query,
        run_graph_query_helpers, run_graph_schema, run_graph_search,
    },
    mcp::{run_mcp_command, serve_mcp_http, serve_mcp_stdio, McpHttpOptions, McpServeOptions},
    reinstall::run_reinstall,
    setup::run_setup,
    uninstall::run_uninstall,
    watch::run_watch,
};
use std::{
    env,
    io::{self, Write},
};

pub fn run_from_env() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    run_process_args(args)
}

pub fn run_process_args(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return run(args, &mut io::stdout());
    }
    if args.first().map(String::as_str) == Some("mcp") {
        match args.get(1).map(String::as_str) {
            Some("start") => {
                let options = McpServeOptions::parse(&args[2..])?;
                return serve_mcp_stdio(&options, io::stdin().lock(), &mut io::stdout());
            }
            Some("http") => {
                let options = McpHttpOptions::parse(&args[2..])?;
                return serve_mcp_http(&options);
            }
            _ => {}
        }
    }
    run(args, &mut io::stdout())
}

pub fn run<I, S, W>(args: I, stdout: &mut W) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    W: Write,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            writeln!(stdout, "{}", top_level_help()).map_err(|error| error.to_string())?;
            Ok(())
        }
        Some("install") => run_setup(&args[1..], stdout),
        Some("reinstall") => run_reinstall(&args[1..], stdout),
        Some("uninstall") => run_uninstall(&args[1..], stdout),
        Some("build") => run_materialize(&args[1..], stdout),
        Some("plan") => run_plan(&args[1..], stdout),
        Some("watch") => run_watch(&args[1..], stdout),
        Some("check-health") => run_graph_health(&args[1..], stdout),
        Some("schema") => run_graph_schema(&args[1..], stdout),
        Some("query-helpers") => run_graph_query_helpers(&args[1..], stdout),
        Some("codebase-architecture-queries") => run_graph_architecture_queries(&args[1..], stdout),
        Some("codebase-search") => run_graph_search(&args[1..], stdout),
        Some("codebase-context") => run_graph_context(&args[1..], stdout),
        Some("graph-query") => run_graph_query(&args[1..], stdout),
        Some("mcp") => run_mcp_command(&args[1..], stdout),
        Some(command) => Err(format!(
            "unknown command: {command}\n\n{}",
            top_level_help()
        )),
        None => {
            writeln!(stdout, "{}", top_level_help()).map_err(|error| error.to_string())?;
            Ok(())
        }
    }
}

pub fn error_exit_code(error: &str) -> i32 {
    if error.starts_with("graph_query is read-only; blocked keyword:")
        || error.starts_with("graph_query accepts one read-only statement at a time")
        || error.starts_with("graph_query requires a non-empty statement")
        || error.starts_with("graph_query parameters must be a JSON object")
        || error.starts_with("graph-query --parameters must be a JSON object")
        || error.starts_with("failed to resolve repo root")
        || error.starts_with("Repository root may not be inside")
        || error.starts_with("unknown install option:")
        || error.starts_with("unknown reinstall option:")
        || error.starts_with("--mcp-client must be")
        || error.starts_with("--mcp-client requires")
        || error.starts_with("--instructions-target must be")
        || error.starts_with("--instructions-target requires")
    {
        2
    } else {
        1
    }
}
