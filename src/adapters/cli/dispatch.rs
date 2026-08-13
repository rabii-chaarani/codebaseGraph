use super::{
    format::{
        graph_architecture_queries_help, graph_context_help, graph_health_help, graph_query_help,
        graph_query_helpers_help, graph_schema_help, graph_search_help, graph_syntax_help,
        top_level_help,
    },
    graph::{
        ArchitectureQueryOptions, GraphContextOptions, GraphQueryOptions, GraphSearchOptions,
        HealthOptions, MetadataOutputOptions, SyntaxCatalogOptions,
    },
    materialization::{run_materialize, run_plan},
    mcp_command::run_mcp_command,
    reinstall::run_reinstall,
    setup::run_setup,
    uninstall::run_uninstall,
    watch::run_watch,
};
use crate::api::{
    CodebaseGraphApi, ContextRequest, HealthRequest, OperationRequest, OperationResponse,
    OutputFormat, QueryRequest, RepoSelector, SearchRequest,
};
use std::io::Write;

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
        Some("syntax") => run_graph_syntax(&args[1..], stdout),
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

fn run_graph_health<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = HealthOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", graph_health_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Health(HealthRequest {
        repo: repo_selector_from_options(
            options.repo_root,
            options.config,
            options.db,
            options.manifest,
        ),
        refresh_status: None,
        output_format: output_format(options.json),
    }))?;
    write_api_response(stdout, &response.payload, &options.json, false)
}

fn run_graph_schema<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "schema")?;
    if options.help {
        writeln!(stdout, "{}", graph_schema_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Catalog {
        kind: "schema".to_string(),
        group: None,
        output_format: output_format(options.format == "json"),
    })?;
    write_api_response(
        stdout,
        &response.payload,
        &options.format.eq("json"),
        options.pretty,
    )
}

fn run_graph_syntax<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = SyntaxCatalogOptions::parse(args)?;
    if options.output.help {
        writeln!(stdout, "{}", graph_syntax_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Catalog {
        kind: "syntax".to_string(),
        group: Some(options.language),
        output_format: output_format(options.output.format == "json"),
    })?;
    write_api_response(
        stdout,
        &response.payload,
        &options.output.format.eq("json"),
        options.output.pretty,
    )
}

fn run_graph_query_helpers<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "query-helpers")?;
    if options.help {
        writeln!(stdout, "{}", graph_query_helpers_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Catalog {
        kind: "query-helpers".to_string(),
        group: None,
        output_format: output_format(options.format == "json"),
    })?;
    write_api_response(
        stdout,
        &response.payload,
        &options.format.eq("json"),
        options.pretty,
    )
}

fn run_graph_architecture_queries<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = ArchitectureQueryOptions::parse(args)?;
    if options.output.help {
        writeln!(stdout, "{}", graph_architecture_queries_help())
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Catalog {
        kind: "architecture-queries".to_string(),
        group: options.group,
        output_format: output_format(options.output.format == "json"),
    })?;
    write_api_response(
        stdout,
        &response.payload,
        &options.output.format.eq("json"),
        options.output.pretty,
    )
}

fn run_graph_search<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = GraphSearchOptions::parse(args)?;
    if options.output.help {
        writeln!(stdout, "{}", graph_search_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Search(SearchRequest {
        repo: repo_selector_from_options(
            options.repo_root,
            options.config,
            options.db,
            options.manifest,
        ),
        query: options.query,
        layer: options.layer,
        profile: options.profile,
        limit: options.limit,
        budget: options.budget,
        context_limit: options.context_limit,
        max_depth: options.max_depth,
        detail: options.detail,
        output_format: output_format(options.output.format == "json"),
    }))?;
    write_api_response(
        stdout,
        &response.payload,
        &options.output.format.eq("json"),
        options.output.pretty,
    )
}

fn run_graph_context<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = GraphContextOptions::parse(args)?;
    if options.search.output.help {
        writeln!(stdout, "{}", graph_context_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Context(ContextRequest {
        repo: repo_selector_from_options(
            options.search.repo_root,
            options.search.config,
            options.search.db,
            options.search.manifest,
        ),
        query: if options.search.query.trim().is_empty() {
            None
        } else {
            Some(options.search.query)
        },
        layer: options.search.layer,
        profile: options.search.profile,
        limit: options.search.limit,
        budget: options.search.budget,
        context_limit: options.search.context_limit,
        max_depth: options.search.max_depth,
        detail: options.search.detail,
        node_id: options.node_id,
        node_type: options.node_type,
        output_format: output_format(options.search.output.format == "json"),
    }))?;
    write_api_response(
        stdout,
        &response.payload,
        &options.search.output.format.eq("json"),
        options.search.output.pretty,
    )
}

fn run_graph_query<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = GraphQueryOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", graph_query_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let response = execute_api_request(OperationRequest::Query(QueryRequest {
        repo: repo_selector_from_options(
            options.repo_root,
            options.config,
            options.db,
            options.manifest,
        ),
        statement: options.statement,
        parameters: serde_json::Value::Object(options.parameters),
        limit: options.limit,
        output_format: output_format(options.json),
    }))?;
    write_api_response(stdout, &response.payload, &options.json, false)
}

fn execute_api_request(request: OperationRequest) -> Result<OperationResponse, String> {
    CodebaseGraphApi::new()
        .execute_operation(&request)
        .map_err(|error| error.message)
}

fn output_format(is_json: bool) -> OutputFormat {
    if is_json {
        OutputFormat::Typed
    } else {
        OutputFormat::Block
    }
}

fn write_api_response<W: Write>(
    stdout: &mut W,
    payload: &serde_json::Value,
    is_json: &bool,
    pretty: bool,
) -> Result<(), String> {
    if *is_json {
        let text = if pretty {
            serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?
        } else {
            serde_json::to_string(payload).map_err(|error| error.to_string())?
        };
        writeln!(stdout, "{text}").map_err(|error| error.to_string())
    } else {
        let text = payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "block response did not contain text".to_string())?;
        write!(stdout, "{text}").map_err(|error| error.to_string())
    }
}

fn repo_selector_from_options(
    repo_root: Option<std::path::PathBuf>,
    config_path: Option<std::path::PathBuf>,
    db_path: Option<std::path::PathBuf>,
    manifest_path: Option<std::path::PathBuf>,
) -> RepoSelector {
    RepoSelector {
        repo_root,
        config_path,
        db_path,
        manifest_path,
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
        || error.starts_with("unsupported MCP client:")
        || error.starts_with("MCP install scope must be")
        || error.starts_with("--instructions-target must be")
        || error.starts_with("--instructions-target requires")
    {
        2
    } else {
        1
    }
}
