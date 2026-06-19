use super::{
    health::{count_graph_nodes, resolve_health_runtime},
    options::HealthOptions,
    options::{
        ArchitectureQueryOptions, GraphContextOptions, GraphQueryOptions, GraphSearchOptions,
        MetadataOutputOptions,
    },
    query::{execute_read_only_query, validate_read_only_statement},
    search::{execute_graph_context, execute_graph_search},
};
use crate::product_cli::{
    constants::{ARCHITECTURE_QUERIES_JSON, GRAPH_SCHEMA_JSON, QUERY_HELPERS_JSON},
    format::{
        filter_architecture_group, graph_architecture_queries_help, graph_context_help,
        graph_health_help, graph_query_help, graph_query_helpers_help, graph_schema_help,
        graph_search_help, metadata_payload, serialize_architecture_queries_block,
        serialize_context_block, serialize_health_block, serialize_query_block,
        serialize_query_helpers_block, serialize_schema_block, serialize_search_block,
        write_metadata_output,
    },
};
use serde_json::json;
use std::io::Write;

pub(in crate::product_cli) fn run_graph_health<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = HealthOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", graph_health_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let runtime = resolve_health_runtime(&options)?;
    let mut graph_readable = false;
    let mut total_nodes = 0_u64;
    let mut error_message = None;
    let database_exists = runtime.db_path.exists();
    let manifest_exists = runtime.manifest_path.exists();

    if database_exists {
        match count_graph_nodes(&runtime.db_path) {
            Ok(count) => {
                graph_readable = true;
                total_nodes = count;
            }
            Err(error) => {
                error_message = Some(error);
            }
        }
    } else {
        error_message = Some(format!(
            "database file does not exist: {}",
            runtime.db_path.display()
        ));
    }

    let output = json!({
        "ok": database_exists && graph_readable,
        "repo_root": runtime.repo_root,
        "database_path": runtime.db_path,
        "manifest_path": runtime.manifest_path,
        "database_exists": database_exists,
        "manifest_exists": manifest_exists,
        "graph_readable": graph_readable,
        "total_nodes": total_nodes,
        "error": error_message,
    });
    if options.json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string(&output).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
    } else {
        write!(stdout, "{}", serialize_health_block(&output)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(in crate::product_cli) fn run_graph_schema<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "graph-schema")?;
    if options.help {
        writeln!(stdout, "{}", graph_schema_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = metadata_payload(GRAPH_SCHEMA_JSON)?;
    write_metadata_output(stdout, &payload, &options, serialize_schema_block)
}

pub(in crate::product_cli) fn run_graph_query_helpers<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "graph-query-helpers")?;
    if options.help {
        writeln!(stdout, "{}", graph_query_helpers_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = metadata_payload(QUERY_HELPERS_JSON)?;
    write_metadata_output(stdout, &payload, &options, serialize_query_helpers_block)
}

pub(in crate::product_cli) fn run_graph_architecture_queries<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = ArchitectureQueryOptions::parse(args)?;
    if options.output.help {
        writeln!(stdout, "{}", graph_architecture_queries_help())
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let mut payload = metadata_payload(ARCHITECTURE_QUERIES_JSON)?;
    if let Some(group) = options.group {
        filter_architecture_group(&mut payload, &group)?;
    }
    write_metadata_output(
        stdout,
        &payload,
        &options.output,
        serialize_architecture_queries_block,
    )
}

pub(in crate::product_cli) fn run_graph_search<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = GraphSearchOptions::parse(args)?;
    if options.output.help {
        writeln!(stdout, "{}", graph_search_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let runtime = resolve_health_runtime(&HealthOptions {
        repo_root: options.repo_root.clone(),
        config: options.config.clone(),
        db: options.db.clone(),
        manifest: options.manifest.clone(),
        help: false,
        json: false,
    })?;
    let results = execute_graph_search(&runtime.db_path, &options)?;
    let payload = json!({
        "query": options.query,
        "profile": options.profile,
        "limit": options.limit,
        "budget": options.budget,
        "results": results,
    });
    if options.output.format == "json" {
        let text = if options.output.pretty {
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        } else {
            serde_json::to_string(&payload).map_err(|error| error.to_string())?
        };
        writeln!(stdout, "{text}").map_err(|error| error.to_string())
    } else {
        writeln!(stdout, "{}", serialize_search_block(&payload)).map_err(|error| error.to_string())
    }
}

pub(in crate::product_cli) fn run_graph_context<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = GraphContextOptions::parse(args)?;
    if options.search.output.help {
        writeln!(stdout, "{}", graph_context_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let runtime = resolve_health_runtime(&HealthOptions {
        repo_root: options.search.repo_root.clone(),
        config: options.search.config.clone(),
        db: options.search.db.clone(),
        manifest: options.search.manifest.clone(),
        help: false,
        json: false,
    })?;
    if let (Some(node_id), Some(node_type)) = (options.node_id.as_ref(), options.node_type.as_ref())
    {
        let context = execute_graph_context(&runtime.db_path, node_id, node_type, &options.search)?;
        let payload = json!({
            "node_id": node_id,
            "node_type": node_type,
            "profile": options.search.profile,
            "context": context,
        });
        if options.search.output.format == "json" {
            let text = if options.search.output.pretty {
                serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
            } else {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            };
            writeln!(stdout, "{text}").map_err(|error| error.to_string())
        } else {
            writeln!(stdout, "{}", serialize_context_block(&payload))
                .map_err(|error| error.to_string())
        }
    } else {
        let results = execute_graph_search(&runtime.db_path, &options.search)?;
        let payload = json!({
            "query": options.search.query,
            "profile": options.search.profile,
            "limit": options.search.limit,
            "budget": options.search.budget,
            "results": results,
        });
        if options.search.output.format == "json" {
            let text = if options.search.output.pretty {
                serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
            } else {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            };
            writeln!(stdout, "{text}").map_err(|error| error.to_string())
        } else {
            writeln!(stdout, "{}", serialize_search_block(&payload))
                .map_err(|error| error.to_string())
        }
    }
}

pub(in crate::product_cli) fn run_graph_query<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = GraphQueryOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", graph_query_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    validate_read_only_statement(&options.statement)?;
    let runtime = resolve_health_runtime(&HealthOptions {
        repo_root: options.repo_root,
        config: options.config,
        db: options.db,
        manifest: options.manifest,
        help: false,
        json: false,
    })?;
    let (rows, truncated) = execute_read_only_query(
        &runtime.db_path,
        &options.statement,
        &options.parameters,
        options.limit,
    )?;
    let output = json!({
        "statement": options.statement,
        "row_count": rows.len(),
        "rows": rows,
        "truncated": truncated,
    });
    if options.json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?;
    } else {
        write!(stdout, "{}", serialize_query_block(&output)).map_err(|error| error.to_string())?;
    }
    Ok(())
}
