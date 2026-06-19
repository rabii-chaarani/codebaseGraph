use crate::ladybug_writer::{write_database, LadybugWriteRequest};
use crate::protocol::{
    NativeManifest, NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse,
};
use lbug::{Connection, Database, SystemConfig, Value};
use notify::{
    event::{AccessKind, AccessMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const GRAPH_SCHEMA_JSON: &str = include_str!("../assets/graph_schema.json");
const QUERY_HELPERS_JSON: &str = include_str!("../assets/query_helpers.json");
const ARCHITECTURE_QUERIES_JSON: &str = include_str!("../assets/architecture_queries.json");
const MCP_TOOL_SPECS_JSON: &str = include_str!("../assets/mcp_tool_specs.json");
const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_HTTP_BODY_BYTES: usize = 1_000_000;

fn server_command() -> String {
    env::var("CODEBASE_GRAPH_SERVER_COMMAND").unwrap_or_else(|_| "codebase-graph".to_string())
}

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
            Some("serve") => {
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
        Some("setup") => run_setup(&args[1..], stdout),
        Some("materialize") => run_materialize(&args[1..], stdout),
        Some("plan") => run_plan(&args[1..], stdout),
        Some("watch") => run_watch(&args[1..], stdout),
        Some("graph-health") => run_graph_health(&args[1..], stdout),
        Some("graph-schema") => run_graph_schema(&args[1..], stdout),
        Some("graph-query-helpers") => run_graph_query_helpers(&args[1..], stdout),
        Some("graph-architecture-queries") => run_graph_architecture_queries(&args[1..], stdout),
        Some("graph-search" | "search") => run_graph_search(&args[1..], stdout),
        Some("graph-context" | "context") => run_graph_context(&args[1..], stdout),
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
        || error.starts_with("unknown setup option:")
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

fn run_materialize<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MaterializeOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", materialize_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let (_, response) = materialize(&options)?;
    let output = serde_json::to_string_pretty(&response).map_err(|error| error.to_string())?;
    writeln!(stdout, "{output}").map_err(|error| error.to_string())?;
    Ok(())
}

fn materialize(
    options: &MaterializeOptions,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let request = match options.native_request.as_ref() {
        Some(request_path) => read_request(request_path)?,
        None => build_request(options)?,
    };
    let started = Instant::now();
    let mut response =
        crate::materialize_syntax_batch(&request).map_err(|error| error.to_string())?;
    if !response.skipped {
        let database_started = Instant::now();
        let schema_statements = if request.schema_statements.is_empty() {
            schema_statements_from_copy_statements(request.include_fts, &response.copy_statements)
        } else {
            request.schema_statements.clone()
        };
        write_database(LadybugWriteRequest {
            db_path: request.db_path.clone(),
            include_fts: request.include_fts,
            schema_statements,
            replace_database: response.diff.force_rebuild,
            delete_statements: crate::ladybug_writer::partition_delete_statements(
                request.previous_manifest.as_ref(),
                &response.diff,
            ),
            copy_statements: response.copy_statements.clone(),
        })
        .map_err(|error| error.to_string())?;
        response.phase_timings.insert(
            "database_write_seconds".to_string(),
            database_started.elapsed().as_secs_f64(),
        );
        response.database_written = true;
    }
    response.phase_timings.insert(
        "native_cli_seconds".to_string(),
        started.elapsed().as_secs_f64(),
    );

    if let Some(manifest_path) = request_manifest_path(options).as_ref() {
        write_manifest(
            manifest_path,
            &request,
            &response.rebuilt_entries,
            &response.diff,
        )?;
    }

    Ok((request, response))
}

fn run_plan<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MaterializeOptions::parse_with_command(args, "plan")?;
    if options.help {
        writeln!(stdout, "{}", plan_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let mut request = match options.native_request.as_ref() {
        Some(request_path) => read_request(request_path)?,
        None => build_request(&options)?,
    };
    request.atomic_rebuild = false;
    let response =
        crate::plan_syntax_materialization(&request).map_err(|error| error.to_string())?;
    let paths = GraphStatePaths::derive(Path::new(&request.source_root));
    let payload = materialization_payload(&response, &request.mode, &paths);
    if options.json_output {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())
    } else {
        write!(stdout, "{}", serialize_plan_block(&payload)).map_err(|error| error.to_string())
    }
}

fn run_watch<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = WatchOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", watch_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let backend = options.backend;
    let loop_config = WatchLoopConfig {
        poll_ms: options.poll_ms,
        debounce_ms: options.debounce_ms,
        max_iterations: options.max_iterations,
    };
    let once = options.once;
    let mut materialize_options = options.materialize;
    let source_root = materialize_options
        .source_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .map_err(|error| format!("failed to resolve source root: {error}"))?;
    materialize_options.source_root = Some(source_root.clone());
    let filter = WatchEventFilter::from_options(&source_root, &materialize_options)?;
    if once {
        let (_, response) = materialize(&materialize_options)?;
        write_watch_event(stdout, "refreshed", None, 0, 0, &response)?;
        return Ok(());
    }
    match backend {
        WatchBackend::Poll => run_poll_watch(stdout, loop_config, &materialize_options, &filter),
        WatchBackend::Native => {
            let (watcher, rx) = start_native_watcher(&source_root)?;
            run_native_watch(
                stdout,
                loop_config,
                &materialize_options,
                &filter,
                watcher,
                rx,
                VecDeque::new(),
            )
        }
        WatchBackend::Auto => match start_native_watcher(&source_root) {
            Ok((watcher, rx)) => {
                let probe = probe_native_watcher(&source_root, &filter, &rx)?;
                if probe.delivered {
                    run_native_watch(
                        stdout,
                        loop_config,
                        &materialize_options,
                        &filter,
                        watcher,
                        rx,
                        probe.queued,
                    )
                } else {
                    drop(watcher);
                    write_watch_status(stdout, "fallback", "poll", probe.reason.as_deref())?;
                    run_poll_watch(stdout, loop_config, &materialize_options, &filter)
                }
            }
            Err(error) => {
                write_watch_status(stdout, "fallback", "poll", Some("watcher_start_failed"))?;
                let _ = error;
                run_poll_watch(stdout, loop_config, &materialize_options, &filter)
            }
        },
    }
}

fn start_native_watcher(
    source_root: &Path,
) -> Result<(notify::RecommendedWatcher, Receiver<WatchMessage>), String> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let message = match result {
            Ok(event) => WatchMessage::Event(event),
            Err(error) => WatchMessage::Error(error.to_string()),
        };
        let _ = tx.send(message);
    })
    .map_err(|error| format!("failed to start filesystem watcher: {error}"))?;
    watcher
        .watch(source_root, RecursiveMode::Recursive)
        .map_err(|error| format!("failed to watch {}: {error}", source_root.display()))?;
    Ok((watcher, rx))
}

fn run_native_watch<W: Write>(
    stdout: &mut W,
    loop_config: WatchLoopConfig,
    materialize_options: &MaterializeOptions,
    filter: &WatchEventFilter,
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    mut queued: VecDeque<WatchMessage>,
) -> Result<(), String> {
    let mut refreshes = 0_usize;
    loop {
        let first = match queued.pop_front() {
            Some(message) => message,
            None => rx
                .recv()
                .map_err(|error| format!("filesystem watcher stopped: {error}"))?,
        };
        let batch = match collect_watch_batch(
            first,
            &rx,
            &mut queued,
            filter,
            Duration::from_millis(loop_config.debounce_ms),
            watch_max_wait(loop_config.debounce_ms),
        )? {
            Some(batch) => batch,
            None => continue,
        };
        let (_, response) = materialize(materialize_options)?;
        write_watch_event(
            stdout,
            "refreshed",
            Some("native"),
            batch.event_count,
            batch.paths.len(),
            &response,
        )?;
        refreshes += 1;
        if loop_config
            .max_iterations
            .is_some_and(|max| refreshes >= max)
        {
            return Ok(());
        }
    }
}

fn run_poll_watch<W: Write>(
    stdout: &mut W,
    loop_config: WatchLoopConfig,
    materialize_options: &MaterializeOptions,
    filter: &WatchEventFilter,
) -> Result<(), String> {
    let mut previous_snapshot = watch_file_snapshot(filter)?;
    let mut refreshes = 0_usize;
    loop {
        let batch = collect_poll_batch(
            filter,
            &mut previous_snapshot,
            Duration::from_millis(loop_config.poll_ms),
            Duration::from_millis(loop_config.debounce_ms),
            watch_max_wait(loop_config.debounce_ms),
        )?;
        let (_, response) = materialize(materialize_options)?;
        write_watch_event(
            stdout,
            "refreshed",
            Some("poll"),
            batch.event_count,
            batch.paths.len(),
            &response,
        )?;
        refreshes += 1;
        if loop_config
            .max_iterations
            .is_some_and(|max| refreshes >= max)
        {
            return Ok(());
        }
    }
}

fn run_setup<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = SetupOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", setup_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }

    let source_root = options
        .repo_root
        .canonicalize()
        .map_err(|error| format!("failed to resolve repo root: {error}"))?;
    let paths = GraphStatePaths::derive(&source_root);
    if source_root
        .components()
        .any(|component| component.as_os_str() == ".codebaseGraph")
    {
        return Err(format!(
            "Repository root may not be inside a .codebaseGraph state directory: {}",
            source_root.display()
        ));
    }
    let materialize_options = MaterializeOptions {
        source_root: Some(source_root.clone()),
        db: Some(paths.db_path.clone()),
        manifest: Some(paths.manifest_path.clone()),
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: true,
        ..MaterializeOptions::default()
    };
    let config_payload = setup_config_payload(&paths, &source_root);
    let instructions_path = instruction_target_path(&source_root, &options.instructions_target)?;
    let state_dir_existed = paths.state_dir.exists();
    let previous_config = snapshot_file(&paths.config_path)?;
    let previous_instructions = match instructions_path.as_ref() {
        Some(path) => Some((path.clone(), snapshot_file(path)?)),
        None => None,
    };
    let (config_action, instructions, mcp_config, materialization) = if options.dry_run {
        let request = build_request(&materialize_options)?;
        let materialization = dry_run_materialization_payload(&request, &paths);
        let config_action = if json_file_would_change(&paths.config_path, &config_payload)? {
            "dry_run"
        } else {
            "unchanged"
        };
        let instructions = json!({
            "action": if instructions_path.is_some() { "dry_run" } else { "skipped" },
            "path": instructions_path.as_ref().map(|path| path.to_string_lossy().to_string()),
        });
        let mcp_config = setup_mcp_config(&options, &paths, true)?;
        (
            config_action.to_string(),
            instructions,
            mcp_config,
            materialization,
        )
    } else {
        fs::create_dir_all(&paths.state_dir).map_err(|error| {
            format!(
                "failed to create state directory {}: {error}",
                paths.state_dir.display()
            )
        })?;
        let result = (|| {
            let config_action = write_setup_config(&paths, &source_root)?;
            let instructions = upsert_instruction_block(
                &source_root,
                &options.instructions_target,
                &paths.config_path,
            )?;
            let (_, response) = materialize(&materialize_options)?;
            let mcp_config = setup_mcp_config(&options, &paths, false)?;
            Ok::<_, String>((
                config_action.to_string(),
                instructions,
                mcp_config,
                materialization_payload(&response, &materialize_options.mode, &paths),
            ))
        })();
        match result {
            Ok(result) => result,
            Err(error) => {
                restore_file(&paths.config_path, previous_config.as_deref())?;
                if let Some((path, previous)) = previous_instructions.as_ref() {
                    restore_file(path, previous.as_deref())?;
                }
                if !state_dir_existed {
                    let _ = fs::remove_dir_all(&paths.state_dir);
                }
                return Err(error);
            }
        }
    };
    let output = json!({
        "ok": true,
        "repo_root": source_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "db_path": paths.db_path,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "config_path": paths.config_path,
        "config_action": config_action,
        "mcp_config": mcp_config,
        "instructions": instructions,
        "materialization": materialization,
        "database_written": materialization.get("database_written").cloned().unwrap_or(json!(false)),
        "skipped": materialization.get("skipped").cloned().unwrap_or(json!(0)),
        "node_rows": materialization.get("node_rows").cloned().unwrap_or(json!(0)),
        "edge_rows": materialization.get("edge_rows").cloned().unwrap_or(json!(0)),
        "connector_rows": materialization.get("connector_rows").cloned().unwrap_or(json!(0)),
        "diagnostics": materialization.get("diagnostics").cloned().unwrap_or(json!([])),
    });
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn run_graph_health<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

fn run_graph_schema<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "graph-schema")?;
    if options.help {
        writeln!(stdout, "{}", graph_schema_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = metadata_payload(GRAPH_SCHEMA_JSON)?;
    write_metadata_output(stdout, &payload, &options, serialize_schema_block)
}

fn run_graph_query_helpers<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "graph-query-helpers")?;
    if options.help {
        writeln!(stdout, "{}", graph_query_helpers_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = metadata_payload(QUERY_HELPERS_JSON)?;
    write_metadata_output(stdout, &payload, &options, serialize_query_helpers_block)
}

fn run_graph_architecture_queries<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

fn run_graph_search<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

fn run_graph_context<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

fn run_graph_query<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

fn run_mcp_command<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("-h" | "--help") | None => {
            writeln!(stdout, "{}", mcp_help()).map_err(|error| error.to_string())?;
            Ok(())
        }
        Some("install") => run_mcp_install(&args[1..], stdout),
        Some("serve") => Err("mcp serve requires the process stdin/stdout transport; run it through the codebase-graph binary".to_string()),
        Some("http") => Err("mcp http starts a blocking HTTP server; run it through the codebase-graph binary".to_string()),
        Some(command) => Err(format!("unknown mcp command: {command}\n\n{}", mcp_help())),
    }
}

fn run_mcp_install<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = McpInstallOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", mcp_install_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = if options.client == "all" {
        let results = supported_install_clients()
            .into_iter()
            .map(|client| {
                let mut client_options = options.clone();
                client_options.client = client.to_string();
                install_mcp_client(&client_options).unwrap_or_else(|error| {
                    json!({
                        "action": "failed",
                        "client": client,
                        "scope": install_scope(client, &client_options.scope),
                        "server_name": client_options.name.clone().unwrap_or_else(|| "codebase_graph".to_string()),
                        "method": serde_json::Value::Null,
                        "path": serde_json::Value::Null,
                        "command": serde_json::Value::Null,
                        "descriptor": {},
                        "entry": {},
                        "error": error,
                    })
                })
            })
            .collect::<Vec<_>>();
        json!({ "results": results })
    } else {
        install_mcp_client(&options)?
    };
    writeln!(
        stdout,
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn serve_mcp_stdio<R: BufRead, W: Write>(
    options: &McpServeOptions,
    mut input: R,
    output: &mut W,
) -> Result<(), String> {
    let mut session = McpSession::default();
    while let Some(message) = read_mcp_message(&mut input, output)? {
        if let Some(response) = handle_mcp_message(message, &mut session, options) {
            write_mcp_message(output, &response)?;
        }
    }
    Ok(())
}

fn serve_mcp_http(options: &McpHttpOptions) -> Result<(), String> {
    let listener = options.bind_listener()?;
    serve_mcp_http_listener(options, listener, None)
}

fn serve_mcp_http_listener(
    options: &McpHttpOptions,
    listener: TcpListener,
    max_requests: Option<usize>,
) -> Result<(), String> {
    let mut state = McpHttpState::default();
    let mut handled = 0_usize;
    loop {
        if max_requests.is_some_and(|limit| handled >= limit) {
            break;
        }
        let (mut stream, _) = listener
            .accept()
            .map_err(|error| format!("failed to accept MCP HTTP request: {error}"))?;
        if let Err(error) = handle_mcp_http_stream(options, &mut state, &mut stream) {
            let _ = write_http_json(
                &mut stream,
                500,
                &rpc_error(serde_json::Value::Null, -32000, &error),
                &[],
            );
        }
        handled += 1;
    }
    Ok(())
}

fn handle_mcp_http_stream(
    options: &McpHttpOptions,
    state: &mut McpHttpState,
    stream: &mut TcpStream,
) -> Result<(), String> {
    let request = read_http_request(stream)?;
    let response = handle_mcp_http_request(options, state, request);
    write_http_json(
        stream,
        response.status,
        &response.payload,
        &response.headers,
    )
}

fn handle_mcp_http_request(
    options: &McpHttpOptions,
    state: &mut McpHttpState,
    request: HttpRequest,
) -> HttpResponse {
    if request.path != options.endpoint_path {
        return HttpResponse::json(
            404,
            rpc_error(serde_json::Value::Null, -32601, "MCP endpoint not found"),
        );
    }
    if request.method != "POST" {
        return HttpResponse {
            status: 405,
            payload: json!({}),
            headers: vec![("Allow".to_string(), "POST".to_string())],
        };
    }
    if !valid_http_origin(request.header("origin")) {
        return HttpResponse::json(
            403,
            rpc_error(serde_json::Value::Null, -32000, "Forbidden origin"),
        );
    }
    if let Some(auth_token) = options.auth_token.as_deref() {
        let authorization = request.header("authorization").unwrap_or("");
        if authorization.strip_prefix("Bearer ") != Some(auth_token) {
            return HttpResponse {
                status: 401,
                payload: rpc_error(serde_json::Value::Null, -32000, "Unauthorized"),
                headers: vec![("WWW-Authenticate".to_string(), "Bearer".to_string())],
            };
        }
    }
    if let Some(protocol) = request.header("mcp-protocol-version") {
        if !is_supported_protocol_version(protocol) {
            return HttpResponse::json(
                400,
                json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32602,
                        "message": "Unsupported MCP protocol version",
                        "data": {
                            "supported": ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"],
                            "requested": protocol,
                        },
                    },
                }),
            );
        }
    }
    if request.body_too_large {
        return HttpResponse::json(
            413,
            json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32000,
                    "message": "MCP request body is too large",
                    "data": {"max_bytes": MAX_HTTP_BODY_BYTES},
                },
            }),
        );
    }
    let message = match parse_mcp_payload(&request.body) {
        Ok(message) => message,
        Err(error) => {
            return HttpResponse::json(
                400,
                rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )
        }
    };
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let request_id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let session_id = request.header("mcp-session-id");
    let (resolved_session_id, session) = if method == "initialize" {
        let id = session_id
            .filter(|id| state.sessions.contains_key(*id))
            .map(str::to_string)
            .unwrap_or_else(|| state.next_session_id());
        let session = state.sessions.entry(id.clone()).or_default();
        (id, session)
    } else {
        match session_id.and_then(|id| {
            state
                .sessions
                .get_mut(id)
                .map(|session| (id.to_string(), session))
        }) {
            Some((id, session)) => (id, session),
            None => {
                return HttpResponse::json(
                    400,
                    rpc_error(request_id, -32002, "MCP session is not initialized"),
                )
            }
        }
    };
    match handle_mcp_message(message, session, &options.serve) {
        Some(payload) => {
            let headers = if method == "initialize" {
                vec![("Mcp-Session-Id".to_string(), resolved_session_id)]
            } else {
                Vec::new()
            };
            HttpResponse {
                status: 200,
                payload,
                headers,
            }
        }
        None => HttpResponse {
            status: 202,
            payload: json!({}),
            headers: Vec::new(),
        },
    }
}

fn handle_mcp_message(
    message: serde_json::Value,
    session: &mut McpSession,
    options: &McpServeOptions,
) -> Option<serde_json::Value> {
    let request_id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if method == "notifications/initialized" {
        session.initialized = true;
        return None;
    }
    if method.starts_with("notifications/") {
        return None;
    }
    if matches!(method, "tools/list" | "tools/call") && session.protocol_version.is_none() {
        return Some(rpc_error(
            request_id,
            -32002,
            "MCP session is not initialized",
        ));
    }
    let result = match method {
        "initialize" => {
            let requested = message
                .get("params")
                .and_then(|params| params.get("protocolVersion"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let protocol_version = negotiate_protocol_version(requested);
            session.protocol_version = Some(protocol_version.to_string());
            Ok(json!({
                "protocolVersion": protocol_version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "codebase_graph", "version": env!("CARGO_PKG_VERSION")},
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => metadata_payload(MCP_TOOL_SPECS_JSON),
        "tools/call" => {
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool_name = params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            mcp_call_tool_result(tool_name, &arguments, options)
        }
        _ => {
            return Some(rpc_error(
                request_id,
                -32601,
                &format!("Unsupported MCP method: {method}"),
            ));
        }
    };
    match result {
        Ok(result) => Some(json!({"jsonrpc": "2.0", "id": request_id, "result": result})),
        Err(error) => Some(rpc_error(request_id, -32602, &error)),
    }
}

fn mcp_call_tool_result(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    let payload = mcp_tool_payload(tool_name, arguments, options);
    let output_format = arguments
        .get("output_format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("block");
    let include_structured = arguments
        .get("include_structured_content")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    match payload {
        Ok(payload) => {
            let text = if output_format == "json" {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            } else {
                mcp_block_text(tool_name, &payload)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": false,
            });
            if include_structured {
                result["structuredContent"] = payload;
            }
            Ok(result)
        }
        Err(error)
            if tool_name.is_empty() || error.starts_with("Unknown codebaseGraph MCP tool") =>
        {
            Err(error)
        }
        Err(error) => {
            let payload = json!({
                "error": {
                    "tool": tool_name,
                    "type": "ValueError",
                    "message": error,
                }
            });
            let text = if output_format == "json" {
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            } else {
                serialize_error_block(&payload)
            };
            let mut result = json!({
                "content": [{"type": "text", "text": text}],
                "isError": true,
            });
            if include_structured {
                result["structuredContent"] = payload;
            }
            Ok(result)
        }
    }
}

fn mcp_tool_payload(
    tool_name: &str,
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<serde_json::Value, String> {
    match tool_name {
        "graph_health" => graph_health_payload(options),
        "graph_schema" => metadata_payload(GRAPH_SCHEMA_JSON),
        "graph_query_helpers" => metadata_payload(QUERY_HELPERS_JSON),
        "graph_architecture_queries" => {
            let mut payload = metadata_payload(ARCHITECTURE_QUERIES_JSON)?;
            if let Some(group) = arguments.get("group").and_then(serde_json::Value::as_str) {
                filter_architecture_group(&mut payload, group)?;
            }
            Ok(payload)
        }
        "graph_search" => {
            let search = graph_search_options_from_mcp(arguments, options, true)?;
            let runtime = resolve_health_runtime(&options.health_options())?;
            let results = execute_graph_search(&runtime.db_path, &search)?;
            Ok(json!({
                "query": search.query,
                "profile": search.profile,
                "limit": search.limit,
                "budget": search.budget,
                "results": results,
            }))
        }
        "graph_context" => {
            let context = graph_context_options_from_mcp(arguments, options)?;
            let runtime = resolve_health_runtime(&options.health_options())?;
            if let (Some(node_id), Some(node_type)) =
                (context.node_id.as_ref(), context.node_type.as_ref())
            {
                let rows =
                    execute_graph_context(&runtime.db_path, node_id, node_type, &context.search)?;
                Ok(json!({
                    "node_id": node_id,
                    "node_type": node_type,
                    "profile": context.search.profile,
                    "context": rows,
                }))
            } else {
                let results = execute_graph_search(&runtime.db_path, &context.search)?;
                Ok(json!({
                    "query": context.search.query,
                    "profile": context.search.profile,
                    "limit": context.search.limit,
                    "budget": context.search.budget,
                    "results": results,
                }))
            }
        }
        "graph_query" => {
            let statement = arguments
                .get("statement")
                .or_else(|| arguments.get("query"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();
            if statement.is_empty() {
                return Err("graph_query requires a non-empty statement".to_string());
            }
            validate_read_only_statement(statement)?;
            let parameters = arguments
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let parameters = parameters
                .as_object()
                .ok_or_else(|| "graph_query parameters must be a JSON object".to_string())?;
            let limit = arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(100) as usize;
            if limit == 0 || limit > 1000 {
                return Err("graph_query limit must be between 1 and 1000".to_string());
            }
            let runtime = resolve_health_runtime(&options.health_options())?;
            let (rows, truncated) =
                execute_read_only_query(&runtime.db_path, statement, parameters, limit)?;
            Ok(json!({
                "statement": statement,
                "row_count": rows.len(),
                "rows": rows,
                "truncated": truncated,
            }))
        }
        _ => Err(format!("Unknown codebaseGraph MCP tool: {tool_name}")),
    }
}

fn graph_health_payload(options: &McpServeOptions) -> Result<serde_json::Value, String> {
    let runtime = resolve_health_runtime(&options.health_options())?;
    let database_exists = runtime.db_path.exists();
    let manifest_exists = runtime.manifest_path.exists();
    let mut graph_readable = false;
    let mut total_nodes = 0_u64;
    let mut error_message = None;
    if database_exists {
        match count_graph_nodes(&runtime.db_path) {
            Ok(count) => {
                graph_readable = true;
                total_nodes = count;
            }
            Err(error) => error_message = Some(error),
        }
    }
    Ok(json!({
        "ok": database_exists && graph_readable,
        "repo_root": runtime.repo_root,
        "database_path": runtime.db_path,
        "manifest_path": runtime.manifest_path,
        "database_exists": database_exists,
        "manifest_exists": manifest_exists,
        "graph_readable": graph_readable,
        "total_nodes": total_nodes,
        "error": error_message,
    }))
}

fn graph_search_options_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
    require_query: bool,
) -> Result<GraphSearchOptions, String> {
    let query = arguments
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    if require_query && query.trim().is_empty() {
        return Err("Search query must not be empty".to_string());
    }
    let detail = arguments
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("standard");
    if detail != "standard" && detail != "slim" {
        return Err("--detail must be standard or slim".to_string());
    }
    Ok(GraphSearchOptions {
        query,
        limit: json_usize(arguments, "limit", 3),
        profile: arguments
            .get("profile")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("brief")
            .to_string(),
        budget: json_usize(arguments, "budget", 600),
        context_limit: json_usize(arguments, "context_limit", 3),
        max_depth: arguments
            .get("max_depth")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize),
        detail: detail.to_string(),
        repo_root: options.repo_root.clone(),
        config: options.config.clone(),
        db: options.db.clone(),
        manifest: options.manifest.clone(),
        output: MetadataOutputOptions {
            format: arguments
                .get("output_format")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("block")
                .to_string(),
            pretty: false,
            help: false,
        },
    })
}

fn graph_context_options_from_mcp(
    arguments: &serde_json::Value,
    options: &McpServeOptions,
) -> Result<GraphContextOptions, String> {
    let node_id = arguments
        .get("node_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let node_type = arguments
        .get("node_type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if node_id.is_some() != node_type.is_some() {
        return Err(
            "graph-context explicit lookup requires both --node-id and --node-type".to_string(),
        );
    }
    let search = graph_search_options_from_mcp(arguments, options, node_id.is_none())?;
    Ok(GraphContextOptions {
        search,
        node_id,
        node_type,
    })
}

fn json_usize(arguments: &serde_json::Value, key: &str, default: usize) -> usize {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn mcp_block_text(tool_name: &str, payload: &serde_json::Value) -> String {
    match tool_name {
        "graph_health" => serialize_health_block(payload),
        "graph_schema" => serialize_schema_block(payload),
        "graph_query_helpers" => serialize_query_helpers_block(payload),
        "graph_architecture_queries" => serialize_architecture_queries_block(payload),
        "graph_search" => serialize_search_block(payload),
        "graph_context" => {
            if payload.get("context").is_some() {
                serialize_context_block(payload)
            } else {
                serialize_search_block(payload)
            }
        }
        "graph_query" => serialize_query_block(payload),
        _ => serde_json::to_string(payload).unwrap_or_default(),
    }
}

fn read_mcp_message<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
) -> Result<Option<serde_json::Value>, String> {
    let mut line = String::new();
    let bytes = input
        .read_line(&mut line)
        .map_err(|error| format!("failed to read MCP frame: {error}"))?;
    if bytes == 0 {
        return Ok(None);
    }
    if line.to_ascii_lowercase().starts_with("content-length:") {
        let length = match line
            .split_once(':')
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        {
            Some(length) => length,
            None => {
                write_mcp_message(
                    output,
                    &rpc_error(
                        serde_json::Value::Null,
                        -32700,
                        "Invalid JSON-RPC payload: Content-Length must be an integer",
                    ),
                )?;
                return Ok(None);
            }
        };
        loop {
            line.clear();
            let bytes = input
                .read_line(&mut line)
                .map_err(|error| format!("failed to read MCP headers: {error}"))?;
            if bytes == 0 || line == "\n" || line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0_u8; length];
        input.read_exact(&mut body).map_err(|error| {
            format!("Body ended before Content-Length bytes were read: {error}")
        })?;
        return parse_mcp_payload(&body).map(Some).or_else(|error| {
            log_mcp_stdio_parse_error(&error);
            write_mcp_message(
                output,
                &rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )?;
            Ok(None)
        });
    }
    parse_mcp_payload(line.as_bytes())
        .map(Some)
        .or_else(|error| {
            log_mcp_stdio_parse_error(&error);
            write_mcp_message(
                output,
                &rpc_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("Invalid JSON-RPC payload: {error}"),
                ),
            )?;
            Ok(None)
        })
}

fn log_mcp_stdio_parse_error(error: &str) {
    eprintln!(
        "{}",
        json!({
            "event": "mcp.stdio_parse_error",
            "message": error,
        })
    );
}

fn parse_mcp_payload(data: &[u8]) -> Result<serde_json::Value, String> {
    let payload: serde_json::Value =
        serde_json::from_slice(data).map_err(|error| error.to_string())?;
    if !payload.is_object() {
        return Err("JSON-RPC payload must be an object".to_string());
    }
    Ok(payload)
}

fn write_mcp_message<W: Write>(output: &mut W, message: &serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_string(message).map_err(|error| error.to_string())?;
    writeln!(output, "{body}").map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

fn negotiate_protocol_version(requested: &str) -> String {
    match requested {
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05" => requested.to_string(),
        _ => LATEST_PROTOCOL_VERSION.to_string(),
    }
}

fn rpc_error(request_id: serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn is_supported_protocol_version(version: &str) -> bool {
    matches!(
        version,
        "2025-11-25" | "2025-06-18" | "2025-03-26" | "2024-11-05"
    )
}

fn valid_http_origin(origin: Option<&str>) -> bool {
    match origin.and_then(http_origin_host) {
        None => true,
        Some(host) => matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"),
    }
}

fn http_origin_host(origin: &str) -> Option<String> {
    let after_scheme = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    if authority.starts_with('[') {
        return authority
            .split_once(']')
            .map(|(host, _)| host.trim_start_matches('[').to_string());
    }
    let host = authority.split(':').next().unwrap_or(authority).trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read HTTP request: {error}"))?;
        if read == 0 {
            return Err("HTTP request ended before headers were complete".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() > MAX_HTTP_BODY_BYTES {
            return Err("HTTP headers exceed maximum MCP request size".to_string());
        }
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = headers.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "HTTP request is missing a request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let raw_path = request_parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or(raw_path).to_string();
    let mut header_map = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            header_map.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let length = match header_map.get("content-length") {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| "Content-Length must be an integer".to_string())?,
        None => 0,
    };
    if length > MAX_HTTP_BODY_BYTES {
        return Ok(HttpRequest {
            method,
            path,
            headers: header_map,
            body: Vec::new(),
            body_too_large: true,
        });
    }
    let body_start = header_end + 4;
    let mut body = buffer.get(body_start..).unwrap_or(&[]).to_vec();
    while body.len() < length {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("failed to read HTTP body: {error}"))?;
        if read == 0 {
            return Err("HTTP request ended before Content-Length bytes were read".to_string());
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);
    Ok(HttpRequest {
        method,
        path,
        headers: header_map,
        body,
        body_too_large: false,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_http_json(
    stream: &mut TcpStream,
    status: u16,
    payload: &serde_json::Value,
    headers: &[(String, String)],
) -> Result<(), String> {
    let body = if status == 202 || status == 405 {
        Vec::new()
    } else {
        serde_json::to_vec(payload).map_err(|error| error.to_string())?
    };
    let reason = http_reason(status);
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    )
    .map_err(|error| error.to_string())?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").map_err(|error| error.to_string())?;
    }
    write!(stream, "\r\n").map_err(|error| error.to_string())?;
    stream.write_all(&body).map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())
}

fn http_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Internal Server Error",
    }
}

fn build_request(
    options: &MaterializeOptions,
) -> Result<NativeSyntaxMaterializationRequest, String> {
    let source_root = options
        .source_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .map_err(|error| format!("failed to resolve source root: {error}"))?;
    let paths = GraphStatePaths::derive(&source_root);
    let db_path = options.db.clone().unwrap_or_else(|| paths.db_path.clone());
    let manifest_path = options
        .manifest
        .clone()
        .unwrap_or_else(|| paths.manifest_path.clone());
    let previous_manifest = if manifest_path.exists() {
        Some(read_manifest(&manifest_path)?)
    } else {
        None
    };
    let config_rules = read_materialization_config_rules(&paths.config_path)?;
    let mut include_patterns = config_rules.include_patterns;
    include_patterns.extend(options.include_patterns.clone());
    let mut exclude_patterns = config_rules.exclude_patterns;
    exclude_patterns.extend(options.exclude_patterns.clone());
    let ignore_patterns = read_codebase_graph_ignore(&source_root)?;
    let candidate_paths = git_candidate_paths(&source_root, options)?;
    let staging_dir = paths.state_dir.join("native-staging");
    Ok(NativeSyntaxMaterializationRequest {
        source_root: source_root.to_string_lossy().to_string(),
        repository_label: paths.repo_name,
        mode: options.mode.clone(),
        parser_version: "native-rust-cli-v1".to_string(),
        manifest_schema_version: 1,
        ontology: "code_ontology_v1".to_string(),
        ontology_schema: Default::default(),
        previous_manifest,
        profiles: Vec::new(),
        excluded_parts: default_excluded_parts(),
        include_patterns,
        exclude_patterns,
        ignore_patterns,
        candidate_paths,
        db_path: db_path.to_string_lossy().to_string(),
        include_fts: options.include_fts,
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        schema_statements: Vec::new(),
        staging_dir: staging_dir.to_string_lossy().to_string(),
        atomic_rebuild: true,
        strict: true,
        parallel: options.parallel,
        progress: options.progress,
    })
}

#[derive(Default)]
struct ConfigScanRules {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
}

fn read_materialization_config_rules(path: &Path) -> Result<ConfigScanRules, String> {
    if !path.exists() {
        return Ok(ConfigScanRules::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse config {}: {error}", path.display()))?;
    let materialization = value
        .get("materialization")
        .and_then(serde_json::Value::as_object);
    Ok(ConfigScanRules {
        include_patterns: materialization
            .and_then(|payload| payload.get("include"))
            .map(json_string_array)
            .unwrap_or_default(),
        exclude_patterns: materialization
            .and_then(|payload| payload.get("exclude"))
            .map(json_string_array)
            .unwrap_or_default(),
    })
}

fn json_string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn read_codebase_graph_ignore(source_root: &Path) -> Result<Vec<String>, String> {
    let path = source_root.join(".codebaseGraphignore");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

fn git_candidate_paths(
    source_root: &Path,
    options: &MaterializeOptions,
) -> Result<Vec<String>, String> {
    if !options.use_git {
        return Ok(Vec::new());
    }
    let mut paths = if options.git_diff && options.plan_only {
        let base = options.git_base.as_deref().unwrap_or("HEAD");
        git_paths(
            source_root,
            &["diff", "--name-only", "--diff-filter=ACMRTD", base, "--"],
        )
        .unwrap_or_default()
    } else {
        git_paths(
            source_root,
            &["ls-files", "--cached", "--others", "--exclude-standard"],
        )
        .unwrap_or_default()
    };
    if options.git_diff && options.plan_only {
        if let Ok(untracked) =
            git_paths(source_root, &["ls-files", "--others", "--exclude-standard"])
        {
            paths.extend(untracked);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn git_paths(source_root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(source_root)
        .output()
        .map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.replace('\\', "/"))
        .collect())
}

fn read_manifest(path: &Path) -> Result<NativeManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read manifest {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse manifest {}: {error}", path.display()))
}

fn request_manifest_path(options: &MaterializeOptions) -> Option<PathBuf> {
    if options.native_request.is_some() {
        return options.manifest.clone();
    }
    let source_root = options
        .source_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let source_root = source_root.canonicalize().unwrap_or(source_root);
    Some(
        options
            .manifest
            .clone()
            .unwrap_or_else(|| GraphStatePaths::derive(&source_root).manifest_path),
    )
}

fn default_excluded_parts() -> Vec<String> {
    [
        ".bzr",
        ".cache",
        ".codebaseGraph",
        ".direnv",
        ".eggs",
        ".git",
        ".hg",
        ".mypy_cache",
        ".nox",
        ".svn",
        ".tox",
        ".venv",
        "build",
        "dist",
        "node_modules",
        "target",
        "venv",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn read_request(path: &Path) -> Result<NativeSyntaxMaterializationRequest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read native request {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse native request {}: {error}", path.display()))
}

fn write_manifest(
    path: &Path,
    request: &NativeSyntaxMaterializationRequest,
    rebuilt_entries: &BTreeMap<String, crate::protocol::ManifestEntry>,
    diff: &crate::protocol::ManifestDiff,
) -> Result<(), String> {
    let mut files = if diff.force_rebuild {
        BTreeMap::new()
    } else {
        request
            .previous_manifest
            .as_ref()
            .map(|manifest| manifest.files.clone())
            .unwrap_or_default()
    };
    let removed: BTreeSet<String> = diff
        .deleted
        .iter()
        .chain(diff.rebuild_paths().iter())
        .cloned()
        .collect();
    files.retain(|path, _| !removed.contains(path));
    files.extend(
        rebuilt_entries
            .iter()
            .map(|(path, entry)| (path.clone(), entry.clone())),
    );

    let manifest = NativeManifest {
        schema_version: request.manifest_schema_version,
        ontology: request.ontology.clone(),
        parser_version: request.parser_version.clone(),
        files,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create manifest directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("failed to write manifest {}: {error}", path.display()))
}

#[derive(Debug, Default)]
struct MaterializeOptions {
    native_request: Option<PathBuf>,
    source_root: Option<PathBuf>,
    db: Option<PathBuf>,
    manifest: Option<PathBuf>,
    mode: String,
    include_fts: bool,
    semantic_enrichment: bool,
    semantic_provider_mode: String,
    use_git: bool,
    git_diff: bool,
    git_base: Option<String>,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    parallel: bool,
    progress: bool,
    plan_only: bool,
    help: bool,
    json_output: bool,
}

impl MaterializeOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        Self::parse_with_command(args, "materialize")
    }

    fn parse_with_command(args: &[String], command_name: &str) -> Result<Self, String> {
        let mut options = Self {
            mode: "changed".to_string(),
            include_fts: true,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            use_git: true,
            plan_only: command_name == "plan",
            ..Self::default()
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--native-request" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--native-request requires a path".to_string())?;
                    options.native_request = Some(PathBuf::from(value));
                    index += 2;
                }
                "--source-root" | "--repo-root" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| format!("{} requires a path", args[index]))?;
                    options.source_root = Some(PathBuf::from(value));
                    index += 2;
                }
                "--db" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--db requires a path".to_string())?;
                    options.db = Some(PathBuf::from(value));
                    index += 2;
                }
                "--manifest" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--manifest requires a path".to_string())?;
                    options.manifest = Some(PathBuf::from(value));
                    index += 2;
                }
                "--mode" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mode requires full or changed".to_string())?;
                    if value != "full" && value != "changed" {
                        return Err("--mode must be full or changed".to_string());
                    }
                    options.mode = value.clone();
                    index += 2;
                }
                "--no-fts" => {
                    options.include_fts = false;
                    index += 1;
                }
                "--no-semantic-enrichment" => {
                    options.semantic_enrichment = false;
                    index += 1;
                }
                "--semantic-provider-mode" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--semantic-provider-mode requires local_only".to_string()
                    })?;
                    if value != "local_only" {
                        return Err("--semantic-provider-mode must be local_only".to_string());
                    }
                    options.semantic_provider_mode = value.clone();
                    index += 2;
                }
                "--no-git" => {
                    options.use_git = false;
                    index += 1;
                }
                "--git-diff" => {
                    options.git_diff = true;
                    index += 1;
                }
                "--git-base" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--git-base requires a revision".to_string())?;
                    options.git_base = Some(value.clone());
                    options.git_diff = true;
                    index += 2;
                }
                "--include" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--include requires a glob pattern".to_string())?;
                    options.include_patterns.push(value.clone());
                    index += 2;
                }
                "--exclude" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--exclude requires a glob pattern".to_string())?;
                    options.exclude_patterns.push(value.clone());
                    index += 2;
                }
                "--single-thread" => {
                    options.parallel = false;
                    index += 1;
                }
                "--parallel" => {
                    options.parallel = true;
                    index += 1;
                }
                "--progress" => {
                    options.progress = true;
                    index += 1;
                }
                "--json" => {
                    options.json_output = true;
                    index += 1;
                }
                other => {
                    return Err(format!(
                        "unknown {command_name} option: {other}\n\n{}",
                        materialize_like_help(command_name)
                    ));
                }
            }
        }
        Ok(options)
    }
}

fn materialize_like_help(command_name: &str) -> &'static str {
    match command_name {
        "plan" => plan_help(),
        "watch" => watch_help(),
        _ => materialize_help(),
    }
}

#[derive(Debug)]
struct WatchOptions {
    materialize: MaterializeOptions,
    backend: WatchBackend,
    poll_ms: u64,
    debounce_ms: u64,
    max_iterations: Option<usize>,
    once: bool,
    help: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchBackend {
    Auto,
    Native,
    Poll,
}

#[derive(Clone, Copy, Debug)]
struct WatchLoopConfig {
    poll_ms: u64,
    debounce_ms: u64,
    max_iterations: Option<usize>,
}

impl WatchBackend {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "native" => Ok(Self::Native),
            "poll" => Ok(Self::Poll),
            _ => Err("--watch-backend must be auto, native, or poll".to_string()),
        }
    }
}

impl WatchOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut materialize_args = Vec::new();
        let mut backend = WatchBackend::Auto;
        let mut poll_ms = 500_u64;
        let mut debounce_ms = 250_u64;
        let mut max_iterations = None;
        let mut once = false;
        let mut help = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    help = true;
                    index += 1;
                }
                "--poll-ms" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--poll-ms requires an integer".to_string())?;
                    poll_ms = value
                        .parse()
                        .map_err(|error| format!("--poll-ms must be an integer: {error}"))?;
                    index += 2;
                }
                "--watch-backend" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--watch-backend requires auto, native, or poll".to_string()
                    })?;
                    backend = WatchBackend::parse(value)?;
                    index += 2;
                }
                "--debounce-ms" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--debounce-ms requires an integer".to_string())?;
                    debounce_ms = value
                        .parse()
                        .map_err(|error| format!("--debounce-ms must be an integer: {error}"))?;
                    index += 2;
                }
                "--max-iterations" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--max-iterations requires an integer".to_string())?;
                    max_iterations = Some(value.parse().map_err(|error| {
                        format!("--max-iterations must be an integer: {error}")
                    })?);
                    index += 2;
                }
                "--once" => {
                    once = true;
                    index += 1;
                }
                _ => {
                    materialize_args.push(args[index].clone());
                    index += 1;
                }
            }
        }
        Ok(Self {
            materialize: MaterializeOptions::parse_with_command(&materialize_args, "watch")?,
            backend,
            poll_ms,
            debounce_ms,
            max_iterations,
            once,
            help,
        })
    }
}

#[derive(Debug)]
struct SetupOptions {
    repo_root: PathBuf,
    mode: String,
    include_fts: bool,
    semantic_enrichment: bool,
    semantic_provider_mode: String,
    mcp_client: String,
    mcp_config_path: Option<PathBuf>,
    skip_mcp_config: bool,
    dry_run: bool,
    instructions_target: String,
    help: bool,
}

impl SetupOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: PathBuf::from("."),
            mode: "changed".to_string(),
            include_fts: true,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            mcp_client: "codex".to_string(),
            mcp_config_path: None,
            skip_mcp_config: false,
            dry_run: false,
            instructions_target: "auto".to_string(),
            help: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--repo-root" | "--source-root" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--repo-root requires a path".to_string())?;
                    options.repo_root = PathBuf::from(value);
                    index += 2;
                }
                "--mode" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mode requires full or changed".to_string())?;
                    if value != "full" && value != "changed" {
                        return Err("--mode must be full or changed".to_string());
                    }
                    options.mode = value.clone();
                    index += 2;
                }
                "--mcp-client" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mcp-client requires a client id".to_string())?;
                    if value != "none" && !supported_install_clients().contains(&value.as_str()) {
                        return Err(format!(
                            "--mcp-client must be none or one of {}",
                            supported_install_clients().join(", ")
                        ));
                    }
                    options.mcp_client = value.clone();
                    index += 2;
                }
                "--mcp-config-path" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mcp-config-path requires a path".to_string())?;
                    options.mcp_config_path = Some(PathBuf::from(value));
                    index += 2;
                }
                "--skip-mcp-config" => {
                    options.skip_mcp_config = true;
                    index += 1;
                }
                "--dry-run" => {
                    options.dry_run = true;
                    index += 1;
                }
                "--instructions-target" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--instructions-target requires auto, agents, claude, or skip".to_string()
                    })?;
                    if !matches!(value.as_str(), "auto" | "agents" | "claude" | "skip") {
                        return Err(
                            "--instructions-target must be auto, agents, claude, or skip"
                                .to_string(),
                        );
                    }
                    options.instructions_target = value.clone();
                    index += 2;
                }
                "--no-fts" => {
                    options.include_fts = false;
                    index += 1;
                }
                "--no-semantic-enrichment" => {
                    options.semantic_enrichment = false;
                    index += 1;
                }
                "--semantic-provider-mode" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--semantic-provider-mode requires local_only".to_string()
                    })?;
                    if value != "local_only" {
                        return Err("--semantic-provider-mode must be local_only".to_string());
                    }
                    options.semantic_provider_mode = value.clone();
                    index += 2;
                }
                "--json" => {
                    index += 1;
                }
                other => {
                    return Err(format!("unknown setup option: {other}\n\n{}", setup_help()));
                }
            }
        }
        Ok(options)
    }
}

fn setup_mcp_config(
    options: &SetupOptions,
    paths: &GraphStatePaths,
    dry_run: bool,
) -> Result<serde_json::Value, String> {
    let descriptor = build_mcp_descriptor(&McpInstallOptions {
        client: "generic".to_string(),
        scope: "local".to_string(),
        name: Some("codebase_graph".to_string()),
        config_path: Some(paths.config_path.clone()),
        client_config_path: None,
        repo_root: paths
            .state_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        dry_run: true,
        verify: false,
        json: true,
        help: false,
    })?;
    if options.skip_mcp_config || options.mcp_client == "none" {
        return Ok(json!({
            "action": "skipped",
            "client": options.mcp_client,
            "scope": "local",
            "server_name": descriptor.name,
            "method": serde_json::Value::Null,
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": descriptor.stdio_entry(false, true),
        }));
    }
    install_mcp_client(&McpInstallOptions {
        client: options.mcp_client.clone(),
        scope: if options.mcp_client == "claude-project" {
            "project".to_string()
        } else {
            "local".to_string()
        },
        name: Some("codebase_graph".to_string()),
        config_path: Some(paths.config_path.clone()),
        client_config_path: options.mcp_config_path.clone(),
        repo_root: paths
            .state_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        dry_run,
        verify: false,
        json: true,
        help: false,
    })
}

fn materialization_payload(
    response: &NativeSyntaxMaterializationResponse,
    mode: &str,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    let rebuilt_paths = response.diff.rebuild_paths();
    let skipped_paths = response
        .snapshots
        .iter()
        .filter_map(|(path, snapshot)| {
            if snapshot.language.is_none() {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let ignored_paths = response
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.strip_prefix("Ignored file: "))
        .map(str::to_string)
        .collect::<Vec<_>>();
    json!({
        "mode": mode,
        "scanned": response.snapshots.len(),
        "rebuilt": rebuilt_paths.len(),
        "skipped": skipped_paths.len(),
        "ignored": ignored_paths.len(),
        "deleted": response.diff.deleted.len(),
        "diagnostics": response.diagnostics,
        "manifest_path": paths.manifest_path,
        "rebuilt_paths": rebuilt_paths,
        "skipped_paths": skipped_paths.clone(),
        "ignored_paths": ignored_paths,
        "deleted_paths": response.diff.deleted.clone(),
        "would_rebuild": response.diff.rebuild_paths(),
        "would_delete": response.diff.deleted,
        "would_skip": skipped_paths,
        "graph_summary": response.graph_summary,
        "node_rows": response.node_rows,
        "edge_rows": response.edge_rows,
        "connector_rows": response.connector_rows,
        "database_written": response.database_written,
        "progress_events": response.progress_events,
        "phase_timings": response.phase_timings,
    })
}

fn dry_run_materialization_payload(
    request: &NativeSyntaxMaterializationRequest,
    paths: &GraphStatePaths,
) -> serde_json::Value {
    let snapshots = scan_source_snapshots(Path::new(&request.source_root));
    let scanned = snapshots.len();
    let skipped_paths = snapshots
        .into_iter()
        .filter_map(|(path, language)| if language.is_none() { Some(path) } else { None })
        .collect::<Vec<_>>();
    json!({
        "mode": "dry_run",
        "scanned": scanned,
        "rebuilt": 0,
        "skipped": skipped_paths.len(),
        "deleted": 0,
        "diagnostics": [],
        "manifest_path": paths.manifest_path,
        "rebuilt_paths": [],
        "skipped_paths": skipped_paths,
        "deleted_paths": [],
        "graph_summary": {},
    })
}

fn serialize_plan_block(payload: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "plan mode={} scanned={} rebuild={} delete={} skip={} ignored={}",
        block_value(value_str(payload, "mode")),
        payload
            .get("scanned")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("rebuilt")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("deleted")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("skipped")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        payload
            .get("ignored")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    )];
    append_plan_path_lines(&mut lines, "rebuild", value_array(payload, "would_rebuild"));
    append_plan_path_lines(&mut lines, "delete", value_array(payload, "would_delete"));
    append_plan_path_lines(&mut lines, "skip", value_array(payload, "would_skip"));
    append_plan_path_lines(&mut lines, "ignore", value_array(payload, "ignored_paths"));
    format!("{}\n", lines.join("\n"))
}

fn append_plan_path_lines(lines: &mut Vec<String>, label: &str, paths: &[serde_json::Value]) {
    for path in paths {
        if let Some(path) = path.as_str() {
            lines.push(format!("{label} {}", block_value(path)));
        }
    }
}

#[derive(Debug)]
enum WatchMessage {
    Event(Event),
    Error(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WatchChangeBatch {
    paths: BTreeSet<String>,
    event_count: usize,
}

#[derive(Debug, Default)]
struct WatchProbeOutcome {
    delivered: bool,
    queued: VecDeque<WatchMessage>,
    reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WatchFileState {
    modified_nanos: u128,
    len: u64,
}

type WatchFileSnapshot = BTreeMap<String, WatchFileState>;

#[derive(Debug)]
struct WatchEventFilter {
    source_root: PathBuf,
    current_dir: PathBuf,
    excluded_parts: BTreeSet<String>,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    ignore_patterns: Vec<String>,
}

impl WatchEventFilter {
    fn from_options(source_root: &Path, options: &MaterializeOptions) -> Result<Self, String> {
        let paths = GraphStatePaths::derive(source_root);
        let config_rules = read_materialization_config_rules(&paths.config_path)?;
        let mut include_patterns = config_rules.include_patterns;
        include_patterns.extend(options.include_patterns.clone());
        let mut exclude_patterns = config_rules.exclude_patterns;
        exclude_patterns.extend(options.exclude_patterns.clone());
        Ok(Self {
            source_root: source_root.to_path_buf(),
            current_dir: env::current_dir().unwrap_or_else(|_| source_root.to_path_buf()),
            excluded_parts: default_excluded_parts().into_iter().collect(),
            include_patterns,
            exclude_patterns,
            ignore_patterns: read_codebase_graph_ignore(source_root)?,
        })
    }

    fn relevant_paths(&self, event: &Event) -> BTreeSet<String> {
        if !watch_event_refreshes(event) {
            return BTreeSet::new();
        }
        event
            .paths
            .iter()
            .filter_map(|path| self.relevant_path(path))
            .collect()
    }

    fn relevant_path(&self, path: &Path) -> Option<String> {
        let relative = self.relative_event_path(path)?;
        if relative.as_os_str().is_empty() {
            return None;
        }
        if relative.components().any(|component| {
            self.excluded_parts
                .contains(component.as_os_str().to_string_lossy().as_ref())
        }) {
            return None;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        if self.ignored_by_patterns(&relative) {
            None
        } else {
            Some(relative)
        }
    }

    fn relative_event_path(&self, path: &Path) -> Option<PathBuf> {
        if let Ok(relative) = path.strip_prefix(&self.source_root) {
            return Some(relative.to_path_buf());
        }
        if path.is_relative() {
            let absolute = self.current_dir.join(path);
            if let Ok(relative) = absolute.strip_prefix(&self.source_root) {
                return Some(relative.to_path_buf());
            }
            return Some(path.to_path_buf());
        }
        None
    }

    fn ignored_by_patterns(&self, relative_path: &str) -> bool {
        if !self.include_patterns.is_empty()
            && !watch_matches_any_pattern(relative_path, &self.include_patterns)
        {
            return true;
        }
        watch_matches_any_pattern(relative_path, &self.ignore_patterns)
            || watch_matches_any_pattern(relative_path, &self.exclude_patterns)
    }
}

fn watch_event_refreshes(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Other
            | EventKind::Access(AccessKind::Close(AccessMode::Write))
    )
}

fn probe_native_watcher(
    source_root: &Path,
    filter: &WatchEventFilter,
    rx: &Receiver<WatchMessage>,
) -> Result<WatchProbeOutcome, String> {
    let timeout = watch_probe_timeout();
    let probe_dir = source_root.join(".codebaseGraph").join("watch-probe");
    let probe_path = probe_dir.join(format!(
        "probe-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    if !watch_probe_skip_write() {
        fs::create_dir_all(&probe_dir)
            .map_err(|error| format!("failed to create watch probe directory: {error}"))?;
        fs::write(&probe_path, b"probe")
            .map_err(|error| format!("failed to write watch probe: {error}"))?;
    }

    let started = Instant::now();
    let mut outcome = WatchProbeOutcome::default();
    while started.elapsed() < timeout {
        let remaining = timeout.saturating_sub(started.elapsed());
        match rx.recv_timeout(remaining) {
            Ok(WatchMessage::Event(event)) => {
                outcome.delivered = true;
                if !watch_event_is_under_dir(&event, &probe_dir, source_root, &filter.current_dir) {
                    outcome.queued.push_back(WatchMessage::Event(event));
                }
            }
            Ok(WatchMessage::Error(error)) => {
                outcome.reason = Some("watcher_error".to_string());
                outcome.queued.push_back(WatchMessage::Error(error));
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("filesystem watcher stopped during health probe".to_string())
            }
        }
    }
    let _ = fs::remove_file(&probe_path);
    if !outcome.delivered && outcome.reason.is_none() {
        outcome.reason = Some("probe_timeout".to_string());
    }
    Ok(outcome)
}

fn watch_probe_timeout() -> Duration {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(750))
}

fn watch_probe_skip_write() -> bool {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE").is_ok_and(|value| value == "1")
}

fn watch_event_is_under_dir(
    event: &Event,
    directory: &Path,
    source_root: &Path,
    current_dir: &Path,
) -> bool {
    !event.paths.is_empty()
        && event
            .paths
            .iter()
            .all(|path| watch_path_is_under_dir(path, directory, source_root, current_dir))
}

fn watch_path_is_under_dir(
    path: &Path,
    directory: &Path,
    source_root: &Path,
    current_dir: &Path,
) -> bool {
    if path.starts_with(directory) {
        return true;
    }
    if path.is_relative() {
        return current_dir.join(path).starts_with(directory)
            || source_root.join(path).starts_with(directory);
    }
    false
}

fn collect_watch_batch(
    first: WatchMessage,
    rx: &Receiver<WatchMessage>,
    queued: &mut VecDeque<WatchMessage>,
    filter: &WatchEventFilter,
    debounce: Duration,
    max_wait: Duration,
) -> Result<Option<WatchChangeBatch>, String> {
    let mut batch = WatchChangeBatch::default();
    apply_watch_message(first, filter, &mut batch)?;
    if batch.paths.is_empty() {
        return Ok(None);
    }

    let started = Instant::now();
    let mut last_relevant = started;
    loop {
        let elapsed = started.elapsed();
        if elapsed >= max_wait {
            return Ok(Some(batch));
        }
        let quiet_elapsed = last_relevant.elapsed();
        if quiet_elapsed >= debounce {
            return Ok(Some(batch));
        }
        let timeout = debounce
            .saturating_sub(quiet_elapsed)
            .min(max_wait.saturating_sub(elapsed));
        let message = match queued.pop_front() {
            Some(message) => Ok(message),
            None => rx.recv_timeout(timeout),
        };
        match message {
            Ok(message) => {
                let before = batch.paths.len();
                let before_events = batch.event_count;
                apply_watch_message(message, filter, &mut batch)?;
                if batch.paths.len() != before || batch.event_count != before_events {
                    last_relevant = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(Some(batch)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("filesystem watcher stopped".to_string())
            }
        }
    }
}

fn apply_watch_message(
    message: WatchMessage,
    filter: &WatchEventFilter,
    batch: &mut WatchChangeBatch,
) -> Result<(), String> {
    match message {
        WatchMessage::Event(event) => {
            let paths = filter.relevant_paths(&event);
            if !paths.is_empty() {
                batch.event_count += 1;
                batch.paths.extend(paths);
            }
            Ok(())
        }
        WatchMessage::Error(error) => Err(format!("filesystem watcher error: {error}")),
    }
}

fn collect_poll_batch(
    filter: &WatchEventFilter,
    previous_snapshot: &mut WatchFileSnapshot,
    poll_interval: Duration,
    debounce: Duration,
    max_wait: Duration,
) -> Result<WatchChangeBatch, String> {
    loop {
        std::thread::sleep(poll_interval);
        let current_snapshot = watch_file_snapshot(filter)?;
        let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
        *previous_snapshot = current_snapshot;
        if changed_paths.is_empty() {
            continue;
        }

        let started = Instant::now();
        let mut last_relevant = started;
        let mut batch = WatchChangeBatch {
            paths: changed_paths,
            event_count: 1,
        };
        loop {
            let elapsed = started.elapsed();
            if elapsed >= max_wait {
                return Ok(batch);
            }
            let quiet_elapsed = last_relevant.elapsed();
            if quiet_elapsed >= debounce {
                return Ok(batch);
            }
            let timeout = poll_interval
                .min(debounce.saturating_sub(quiet_elapsed))
                .min(max_wait.saturating_sub(elapsed));
            std::thread::sleep(timeout);
            let current_snapshot = watch_file_snapshot(filter)?;
            let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
            *previous_snapshot = current_snapshot;
            if !changed_paths.is_empty() {
                batch.paths.extend(changed_paths);
                batch.event_count += 1;
                last_relevant = Instant::now();
            }
        }
    }
}

fn watch_file_snapshot(filter: &WatchEventFilter) -> Result<WatchFileSnapshot, String> {
    let mut snapshot = BTreeMap::new();
    watch_file_snapshot_inner(filter, &filter.source_root, &mut snapshot)?;
    Ok(snapshot)
}

fn watch_file_snapshot_inner(
    filter: &WatchEventFilter,
    directory: &Path,
    snapshot: &mut WatchFileSnapshot,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read directory {}: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if filter.excluded_parts.contains(name) {
                continue;
            }
            watch_file_snapshot_inner(filter, &path, snapshot)?;
        } else if path.is_file() {
            let Some(relative_path) = filter.relevant_path(&path) else {
                continue;
            };
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified_nanos = metadata
                .modified()
                .ok()
                .and_then(|modified| {
                    modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_nanos())
                })
                .unwrap_or(0);
            snapshot.insert(
                relative_path,
                WatchFileState {
                    modified_nanos,
                    len: metadata.len(),
                },
            );
        }
    }
    Ok(())
}

fn watch_snapshot_diff(
    previous: &WatchFileSnapshot,
    current: &WatchFileSnapshot,
) -> BTreeSet<String> {
    let mut changed_paths = BTreeSet::new();
    for (path, state) in current {
        if previous.get(path) != Some(state) {
            changed_paths.insert(path.clone());
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            changed_paths.insert(path.clone());
        }
    }
    changed_paths
}

fn watch_max_wait(debounce_ms: u64) -> Duration {
    Duration::from_secs(5).max(Duration::from_millis(debounce_ms.saturating_mul(10)))
}

fn watch_matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|pattern| pattern.trim())
        .filter(|pattern| !pattern.is_empty() && !pattern.starts_with('#'))
        .any(|pattern| watch_glob_matches(path, pattern))
}

fn watch_glob_matches(path: &str, pattern: &str) -> bool {
    let pattern = watch_normalize_pattern(pattern);
    if pattern.ends_with('/') {
        return path.starts_with(pattern.trim_end_matches('/'));
    }
    if !pattern.contains('/')
        && watch_wildcard_match(path.rsplit('/').next().unwrap_or(path), &pattern)
    {
        return true;
    }
    watch_wildcard_match(path, &pattern)
}

fn watch_normalize_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_string()
}

fn watch_wildcard_match(text: &str, pattern: &str) -> bool {
    let (mut text_index, mut pattern_index) = (0_usize, 0_usize);
    let mut star_index = None;
    let mut match_index = 0_usize;
    let text = text.as_bytes();
    let pattern = pattern.as_bytes();
    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            text_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            match_index = text_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            text_index = match_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn write_watch_event<W: Write>(
    stdout: &mut W,
    event: &str,
    backend: Option<&str>,
    event_count: usize,
    changed_paths: usize,
    response: &NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    let backend = backend
        .map(|backend| format!(" backend={backend}"))
        .unwrap_or_default();
    writeln!(
        stdout,
        "watch event={}{} event_count={} changed_paths={} rebuilt={} deleted={} skipped={} database_written={}",
        event,
        backend,
        event_count,
        changed_paths,
        response.diff.rebuild_paths().len(),
        response.diff.deleted.len(),
        response.skipped,
        response.database_written
    )
    .map_err(|error| error.to_string())
}

fn write_watch_status<W: Write>(
    stdout: &mut W,
    event: &str,
    backend: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    if let Some(reason) = reason {
        writeln!(
            stdout,
            "watch event={event} backend={backend} reason={reason}"
        )
    } else {
        writeln!(stdout, "watch event={event} backend={backend}")
    }
    .map_err(|error| error.to_string())
}

fn scan_source_snapshots(root: &Path) -> Vec<(String, Option<&'static str>)> {
    let mut snapshots = Vec::new();
    scan_source_snapshots_inner(root, root, &mut snapshots);
    snapshots.sort_by(|left, right| left.0.cmp(&right.0));
    snapshots
}

fn scan_source_snapshots_inner(
    root: &Path,
    directory: &Path,
    snapshots: &mut Vec<(String, Option<&'static str>)>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if default_excluded_parts().iter().any(|part| part == name) {
            continue;
        }
        if path.is_dir() {
            scan_source_snapshots_inner(root, &path, snapshots);
        } else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
            snapshots.push((relative.to_string(), language_for_path(&path)));
        }
    }
}

fn language_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("py") => Some("python"),
        Some("rs") => Some("rust"),
        Some("go") => Some("go"),
        Some("c") | Some("h") => Some("c"),
        Some("cc") | Some("cpp") | Some("cxx") | Some("hpp") | Some("hh") => Some("cpp"),
        Some("f") | Some("f90") | Some("f95") | Some("for") => Some("fortran"),
        _ => None,
    }
}

#[derive(Debug)]
struct HealthOptions {
    repo_root: PathBuf,
    config: Option<PathBuf>,
    db: Option<PathBuf>,
    manifest: Option<PathBuf>,
    help: bool,
    json: bool,
}

impl HealthOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: PathBuf::from("."),
            config: None,
            db: None,
            manifest: None,
            help: false,
            json: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--repo-root" | "--source-root" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--repo-root requires a path".to_string())?;
                    options.repo_root = PathBuf::from(value);
                    index += 2;
                }
                "--config" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--config requires a path".to_string())?;
                    options.config = Some(PathBuf::from(value));
                    index += 2;
                }
                "--db" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--db requires a path".to_string())?;
                    options.db = Some(PathBuf::from(value));
                    index += 2;
                }
                "--manifest" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--manifest requires a path".to_string())?;
                    options.manifest = Some(PathBuf::from(value));
                    index += 2;
                }
                "--json" => {
                    options.json = true;
                    index += 1;
                }
                other => {
                    return Err(format!(
                        "unknown graph-health option: {other}\n\n{}",
                        graph_health_help()
                    ));
                }
            }
        }
        Ok(options)
    }
}

#[derive(Debug)]
struct GraphQueryOptions {
    statement: String,
    parameters: serde_json::Map<String, serde_json::Value>,
    limit: usize,
    repo_root: PathBuf,
    config: Option<PathBuf>,
    db: Option<PathBuf>,
    manifest: Option<PathBuf>,
    help: bool,
    json: bool,
}

impl GraphQueryOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut statement = None;
        let mut parameters = serde_json::Map::new();
        let mut limit = 100_usize;
        let mut repo_root = PathBuf::from(".");
        let mut config = None;
        let mut db = None;
        let mut manifest = None;
        let mut help = false;
        let mut json = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    help = true;
                    index += 1;
                }
                "--parameters" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--parameters requires a JSON object".to_string())?;
                    let parsed: serde_json::Value =
                        serde_json::from_str(value).map_err(|error| {
                            format!("graph-query --parameters must be a JSON object: {error}")
                        })?;
                    parameters = parsed.as_object().cloned().ok_or_else(|| {
                        "graph-query --parameters must be a JSON object".to_string()
                    })?;
                    index += 2;
                }
                "--limit" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--limit requires an integer".to_string())?;
                    limit = value
                        .parse::<usize>()
                        .map_err(|error| format!("--limit must be an integer: {error}"))?;
                    if limit == 0 {
                        return Err("graph-query limit must be greater than zero".to_string());
                    }
                    if limit > 1000 {
                        return Err("graph-query limit must be 1000 or less".to_string());
                    }
                    index += 2;
                }
                "--repo-root" | "--source-root" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--repo-root requires a path".to_string())?;
                    repo_root = PathBuf::from(value);
                    index += 2;
                }
                "--config" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--config requires a path".to_string())?;
                    config = Some(PathBuf::from(value));
                    index += 2;
                }
                "--db" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--db requires a path".to_string())?;
                    db = Some(PathBuf::from(value));
                    index += 2;
                }
                "--manifest" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--manifest requires a path".to_string())?;
                    manifest = Some(PathBuf::from(value));
                    index += 2;
                }
                "--json" => {
                    json = true;
                    index += 1;
                }
                "--pretty" => {
                    index += 1;
                }
                "--output-format" | "--format" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--output-format requires json or block".to_string())?;
                    if value != "json" && value != "block" {
                        return Err("--output-format must be json or block".to_string());
                    }
                    json = value == "json";
                    index += 2;
                }
                "--context" | "--output" => {
                    if args.get(index + 1).is_some() {
                        index += 2;
                    } else {
                        return Err(format!("{} requires a value", args[index]));
                    }
                }
                other if other.starts_with('-') => {
                    return Err(format!(
                        "unknown graph-query option: {other}\n\n{}",
                        graph_query_help()
                    ));
                }
                value => {
                    if statement.is_some() {
                        return Err("graph-query accepts exactly one statement".to_string());
                    }
                    statement = Some(value.to_string());
                    index += 1;
                }
            }
        }
        if help {
            return Ok(Self {
                statement: String::new(),
                parameters,
                limit,
                repo_root,
                config,
                db,
                manifest,
                help,
                json,
            });
        }
        let statement = statement
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "graph-query requires a non-empty statement".to_string())?;
        Ok(Self {
            statement,
            parameters,
            limit,
            repo_root,
            config,
            db,
            manifest,
            help,
            json,
        })
    }
}

#[derive(Debug)]
struct MetadataOutputOptions {
    format: String,
    pretty: bool,
    help: bool,
}

impl MetadataOutputOptions {
    fn parse(args: &[String], command_name: &str) -> Result<Self, String> {
        let mut options = Self {
            format: "block".to_string(),
            pretty: false,
            help: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--format" | "--output-format" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--format requires json or block".to_string())?;
                    if value != "json" && value != "block" {
                        return Err("--format must be json or block".to_string());
                    }
                    options.format = value.clone();
                    index += 2;
                }
                "--json" => {
                    options.format = "json".to_string();
                    index += 1;
                }
                "--pretty" => {
                    options.pretty = true;
                    index += 1;
                }
                other => {
                    return Err(format!(
                        "unknown {command_name} option: {other}\n\n{}",
                        metadata_help(command_name)
                    ));
                }
            }
        }
        Ok(options)
    }
}

#[derive(Debug)]
struct ArchitectureQueryOptions {
    output: MetadataOutputOptions,
    group: Option<String>,
}

impl ArchitectureQueryOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut metadata_args = Vec::new();
        let mut group = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--group" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--group requires a group name".to_string())?;
                    group = Some(value.clone());
                    index += 2;
                }
                _ => {
                    metadata_args.push(args[index].clone());
                    index += 1;
                }
            }
        }
        Ok(Self {
            output: MetadataOutputOptions::parse(&metadata_args, "graph-architecture-queries")?,
            group,
        })
    }
}

#[derive(Debug)]
struct GraphSearchOptions {
    query: String,
    limit: usize,
    profile: String,
    budget: usize,
    context_limit: usize,
    max_depth: Option<usize>,
    detail: String,
    repo_root: PathBuf,
    config: Option<PathBuf>,
    db: Option<PathBuf>,
    manifest: Option<PathBuf>,
    output: MetadataOutputOptions,
}

impl GraphSearchOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut query = None;
        let mut limit = 3_usize;
        let mut profile = "brief".to_string();
        let mut budget = 600_usize;
        let mut context_limit = 3_usize;
        let mut max_depth = None;
        let mut detail = "standard".to_string();
        let mut repo_root = PathBuf::from(".");
        let mut config = None;
        let mut db = None;
        let mut manifest = None;
        let mut output_args = Vec::new();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" | "--json" | "--pretty" | "--format" | "--output-format" => {
                    output_args.push(args[index].clone());
                    if matches!(args[index].as_str(), "--format" | "--output-format") {
                        if let Some(value) = args.get(index + 1) {
                            output_args.push(value.clone());
                        }
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                "--limit" => {
                    limit = parse_usize_arg(args, index, "--limit")?;
                    if limit == 0 {
                        return Err("Search limit must be greater than zero".to_string());
                    }
                    index += 2;
                }
                "--profile" => {
                    profile = required_arg(args, index, "--profile")?.to_string();
                    index += 2;
                }
                "--budget" => {
                    budget = parse_usize_arg(args, index, "--budget")?;
                    index += 2;
                }
                "--context-limit" => {
                    context_limit = parse_usize_arg(args, index, "--context-limit")?;
                    index += 2;
                }
                "--max-depth" => {
                    max_depth = Some(parse_usize_arg(args, index, "--max-depth")?);
                    index += 2;
                }
                "--snippet-context-lines" => {
                    let _ = required_arg(args, index, args[index].as_str())?;
                    index += 2;
                }
                "--include-snippets" => {
                    index += 1;
                }
                "--no-semantic" => {
                    index += 1;
                }
                "--no-confidence" => {
                    index += 1;
                }
                "--include-evidence" => {
                    index += 1;
                }
                "--detail" => {
                    let value = required_arg(args, index, "--detail")?;
                    if value != "standard" && value != "slim" {
                        return Err("--detail must be standard or slim".to_string());
                    }
                    detail = value.to_string();
                    index += 2;
                }
                "--repo-root" | "--source-root" => {
                    repo_root = PathBuf::from(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--config" => {
                    config = Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--db" => {
                    db = Some(PathBuf::from(required_arg(args, index, "--db")?));
                    index += 2;
                }
                "--manifest" => {
                    manifest = Some(PathBuf::from(required_arg(args, index, "--manifest")?));
                    index += 2;
                }
                "--no-refresh" | "--include-structured-content" => {
                    index += 1;
                }
                "--context" | "--output" => {
                    let _ = required_arg(args, index, args[index].as_str())?;
                    index += 2;
                }
                other if other.starts_with('-') => {
                    return Err(format!(
                        "unknown graph-search option: {other}\n\n{}",
                        graph_search_help()
                    ));
                }
                value => {
                    if query.is_some() {
                        return Err("graph-search accepts exactly one query".to_string());
                    }
                    query = Some(value.to_string());
                    index += 1;
                }
            }
        }
        let output = MetadataOutputOptions::parse(&output_args, "graph-search")?;
        if output.help {
            return Ok(Self {
                query: String::new(),
                limit,
                profile,
                budget,
                context_limit,
                max_depth,
                detail,
                repo_root,
                config,
                db,
                manifest,
                output,
            });
        }
        let query = query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Search query must not be empty".to_string())?;
        Ok(Self {
            query,
            limit,
            profile,
            budget,
            context_limit,
            max_depth,
            detail,
            repo_root,
            config,
            db,
            manifest,
            output,
        })
    }
}

#[derive(Debug)]
struct GraphContextOptions {
    search: GraphSearchOptions,
    node_id: Option<String>,
    node_type: Option<String>,
}

impl GraphContextOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut search_args = Vec::new();
        let mut node_id = None;
        let mut node_type = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--node-id" => {
                    node_id = Some(required_arg(args, index, "--node-id")?.to_string());
                    index += 2;
                }
                "--node-type" => {
                    node_type = Some(required_arg(args, index, "--node-type")?.to_string());
                    index += 2;
                }
                other => {
                    search_args.push(other.to_string());
                    index += 1;
                }
            }
        }
        if node_id.is_some()
            && node_type.is_some()
            && !search_args.iter().any(|arg| arg == "-h" || arg == "--help")
        {
            search_args.push("__explicit_context__".to_string());
        }
        let mut search = GraphSearchOptions::parse(&search_args)?;
        if node_id.is_some() && node_type.is_some() {
            search.query.clear();
        } else if node_id.is_some() || node_type.is_some() {
            return Err(
                "graph-context explicit lookup requires both --node-id and --node-type".to_string(),
            );
        }
        Ok(Self {
            search,
            node_id,
            node_type,
        })
    }
}

#[derive(Debug, Default)]
struct McpSession {
    protocol_version: Option<String>,
    initialized: bool,
}

#[derive(Debug)]
struct McpServeOptions {
    repo_root: PathBuf,
    config: Option<PathBuf>,
    db: Option<PathBuf>,
    manifest: Option<PathBuf>,
}

impl McpServeOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: PathBuf::from("."),
            config: None,
            db: None,
            manifest: None,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo-root" => {
                    options.repo_root = PathBuf::from(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--config" => {
                    options.config = Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--db" => {
                    options.db = Some(PathBuf::from(required_arg(args, index, "--db")?));
                    index += 2;
                }
                "--manifest" => {
                    options.manifest =
                        Some(PathBuf::from(required_arg(args, index, "--manifest")?));
                    index += 2;
                }
                other => {
                    return Err(format!(
                        "unknown mcp serve option: {other}\n\n{}",
                        mcp_help()
                    ));
                }
            }
        }
        Ok(options)
    }

    fn health_options(&self) -> HealthOptions {
        HealthOptions {
            repo_root: self.repo_root.clone(),
            config: self.config.clone(),
            db: self.db.clone(),
            manifest: self.manifest.clone(),
            help: false,
            json: false,
        }
    }
}

#[derive(Debug)]
struct McpHttpOptions {
    serve: McpServeOptions,
    host: String,
    port: u16,
    endpoint_path: String,
    allow_remote: bool,
    auth_token: Option<String>,
}

impl McpHttpOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            serve: McpServeOptions {
                repo_root: PathBuf::from("."),
                config: None,
                db: None,
                manifest: None,
            },
            host: "127.0.0.1".to_string(),
            port: 8765,
            endpoint_path: "/mcp".to_string(),
            allow_remote: false,
            auth_token: None,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo-root" => {
                    options.serve.repo_root =
                        PathBuf::from(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--config" => {
                    options.serve.config =
                        Some(PathBuf::from(required_arg(args, index, "--config")?));
                    index += 2;
                }
                "--db" => {
                    options.serve.db = Some(PathBuf::from(required_arg(args, index, "--db")?));
                    index += 2;
                }
                "--manifest" => {
                    options.serve.manifest =
                        Some(PathBuf::from(required_arg(args, index, "--manifest")?));
                    index += 2;
                }
                "--host" => {
                    options.host = required_arg(args, index, "--host")?.to_string();
                    index += 2;
                }
                "--port" => {
                    options.port = required_arg(args, index, "--port")?
                        .parse::<u16>()
                        .map_err(|_| "--port must be between 0 and 65535".to_string())?;
                    index += 2;
                }
                "--path" => {
                    options.endpoint_path = required_arg(args, index, "--path")?.to_string();
                    if !options.endpoint_path.starts_with('/') {
                        return Err("--path must start with /".to_string());
                    }
                    index += 2;
                }
                "--allow-remote" => {
                    options.allow_remote = true;
                    index += 1;
                }
                "--auth-token" => {
                    options.auth_token =
                        Some(required_arg(args, index, "--auth-token")?.to_string());
                    index += 2;
                }
                "--auth-token-env" => {
                    let name = required_arg(args, index, "--auth-token-env")?;
                    let value = env::var(name).map_err(|_| {
                        format!("Environment variable {name:?} must contain the HTTP bearer token")
                    })?;
                    options.auth_token = Some(value);
                    index += 2;
                }
                other => {
                    return Err(format!(
                        "unknown mcp http option: {other}\n\n{}",
                        mcp_help()
                    ));
                }
            }
        }
        options.validate()?;
        Ok(options)
    }

    fn validate(&self) -> Result<(), String> {
        if self
            .auth_token
            .as_deref()
            .is_some_and(|token| token.trim().is_empty())
        {
            return Err("MCP HTTP auth token must not be blank".to_string());
        }
        if self.allow_remote && self.auth_token.is_none() {
            return Err("MCP HTTP remote bind requires an auth token".to_string());
        }
        if !self.allow_remote && !is_local_host(&self.host) {
            return Err(
                "MCP HTTP transport may only bind to localhost unless allow_remote is enabled"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn bind_listener(&self) -> Result<TcpListener, String> {
        self.validate()?;
        TcpListener::bind((self.host.as_str(), self.port))
            .map_err(|error| format!("failed to bind MCP HTTP server: {error}"))
    }
}

#[derive(Debug, Clone)]
struct McpInstallOptions {
    client: String,
    scope: String,
    name: Option<String>,
    config_path: Option<PathBuf>,
    client_config_path: Option<PathBuf>,
    repo_root: PathBuf,
    dry_run: bool,
    verify: bool,
    json: bool,
    help: bool,
}

impl McpInstallOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            client: "codex".to_string(),
            scope: "local".to_string(),
            name: None,
            config_path: None,
            client_config_path: None,
            repo_root: PathBuf::from("."),
            dry_run: false,
            verify: false,
            json: false,
            help: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--client" => {
                    options.client = required_arg(args, index, "--client")?.to_string();
                    if options.client != "all"
                        && !supported_install_clients().contains(&options.client.as_str())
                    {
                        return Err(format!(
                            "Unsupported MCP client: {}. Supported clients: {}",
                            options.client,
                            supported_install_clients_with_all().join(", ")
                        ));
                    }
                    index += 2;
                }
                "--scope" => {
                    options.scope = required_arg(args, index, "--scope")?.to_string();
                    if !matches!(options.scope.as_str(), "local" | "user" | "project") {
                        return Err(
                            "Unsupported MCP install scope: expected local, user, or project"
                                .to_string(),
                        );
                    }
                    index += 2;
                }
                "--name" => {
                    options.name = Some(required_arg(args, index, "--name")?.to_string());
                    index += 2;
                }
                "--config-path" => {
                    options.config_path =
                        Some(expand_path(required_arg(args, index, "--config-path")?));
                    index += 2;
                }
                "--client-config-path" => {
                    options.client_config_path = Some(expand_path(required_arg(
                        args,
                        index,
                        "--client-config-path",
                    )?));
                    index += 2;
                }
                "--repo-root" => {
                    options.repo_root = expand_path(required_arg(args, index, "--repo-root")?);
                    index += 2;
                }
                "--dry-run" => {
                    options.dry_run = true;
                    index += 1;
                }
                "--verify" => {
                    options.verify = true;
                    index += 1;
                }
                "--json" => {
                    options.json = true;
                    index += 1;
                }
                "--format" | "--output-format" => {
                    let value = required_arg(args, index, args[index].as_str())?;
                    if value != "json" && value != "block" {
                        return Err("--format must be json or block".to_string());
                    }
                    options.json = value == "json";
                    index += 2;
                }
                other => {
                    return Err(format!(
                        "unknown mcp install option: {other}\n\n{}",
                        mcp_install_help()
                    ))
                }
            }
        }
        Ok(options)
    }
}

#[derive(Debug, Default)]
struct McpHttpState {
    sessions: BTreeMap<String, McpSession>,
    next_session: u64,
}

impl McpHttpState {
    fn next_session_id(&mut self) -> String {
        self.next_session += 1;
        format!("native-http-session-{}", self.next_session)
    }
}

fn install_mcp_client(options: &McpInstallOptions) -> Result<serde_json::Value, String> {
    let descriptor = build_mcp_descriptor(options)?;
    if options.client == "copilot-studio" || options.client == "microsoft-copilot" {
        let metadata = copilot_studio_metadata(&descriptor);
        let payload = json!({
            "action": if options.dry_run { "dry_run" } else { "reported" },
            "client": options.client,
            "scope": options.scope,
            "server_name": descriptor.name,
            "method": "manual_metadata",
            "path": serde_json::Value::Null,
            "command": serde_json::Value::Null,
            "descriptor": descriptor.as_json(),
            "entry": metadata["stdio"].clone(),
            "payload": metadata,
        });
        return attach_install_verification(payload, &descriptor, options);
    }
    let native_command = native_client_command(&options.client, &descriptor, &options.scope);
    let native_available = native_command
        .as_ref()
        .and_then(|command| command.first())
        .is_some_and(|executable| executable_in_path(executable));
    if options.dry_run && options.client_config_path.is_none() && native_available {
        return attach_install_verification(
            json!({
                "action": "dry_run",
                "client": options.client,
                "scope": install_scope(&options.client, &options.scope),
                "server_name": descriptor.name,
                "method": "native_cli",
                "path": serde_json::Value::Null,
                "command": native_command,
                "descriptor": descriptor.as_json(),
                "entry": descriptor.stdio_entry(false, false),
            }),
            &descriptor,
            options,
        );
    }
    if !options.dry_run && options.client_config_path.is_none() && native_available {
        let Some(command) = native_command.clone() else {
            return file_adapter_result(options, &descriptor, native_command, None);
        };
        let completed = Command::new(&command[0])
            .args(&command[1..])
            .output()
            .map_err(|error| format!("failed to run native client installer: {error}"))?;
        if completed.status.success() {
            return attach_install_verification(
                json!({
                    "action": "updated",
                    "client": options.client,
                    "scope": install_scope(&options.client, &options.scope),
                    "server_name": descriptor.name,
                    "method": "native_cli",
                    "path": serde_json::Value::Null,
                    "command": command,
                    "descriptor": descriptor.as_json(),
                    "entry": descriptor.stdio_entry(false, false),
                }),
                &descriptor,
                options,
            );
        }
        let error = subprocess_error(&completed);
        return file_adapter_result(options, &descriptor, Some(command), Some(error));
    }
    let native_error = native_command.as_ref().and_then(|command| {
        command.first().and_then(|executable| {
            if executable_in_path(executable) {
                None
            } else {
                Some(format!("{executable} executable not found"))
            }
        })
    });
    file_adapter_result(options, &descriptor, native_command, native_error)
}

#[derive(Debug, Clone)]
struct NativeMcpDescriptor {
    name: String,
    command: String,
    args: Vec<String>,
    setup_config_path: String,
    repo_root: String,
    timeout: u64,
}

impl NativeMcpDescriptor {
    fn as_json(&self) -> serde_json::Value {
        json!({
            "name": self.name,
            "transport": "stdio",
            "command": self.command,
            "args": self.args,
            "env": {},
            "cwd": serde_json::Value::Null,
            "setup_config_path": self.setup_config_path,
            "repo_root": self.repo_root,
            "timeout": self.timeout,
            "tool_policy": "graph_query_read_only",
        })
    }

    fn stdio_entry(&self, include_type: bool, include_timeout: bool) -> serde_json::Value {
        let mut entry = serde_json::Map::new();
        entry.insert("command".to_string(), json!(self.command));
        entry.insert("args".to_string(), json!(self.args));
        if include_type {
            entry.insert("type".to_string(), json!("stdio"));
        }
        if include_timeout {
            entry.insert("startup_timeout_sec".to_string(), json!(self.timeout));
        }
        serde_json::Value::Object(entry)
    }
}

fn build_mcp_descriptor(options: &McpInstallOptions) -> Result<NativeMcpDescriptor, String> {
    let config_path = options.config_path.clone().unwrap_or_else(|| {
        GraphStatePaths::derive(&expand_path(options.repo_root.to_string_lossy().as_ref()))
            .config_path
    });
    let setup_config = if config_path.exists() {
        Some(read_json_file(&config_path)?)
    } else {
        None
    };
    let repo_root = setup_config
        .as_ref()
        .and_then(|payload| payload.get("repo_root"))
        .and_then(serde_json::Value::as_str)
        .map(expand_path)
        .unwrap_or_else(|| {
            config_path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| options.repo_root.clone())
        });
    let repo_name = setup_config
        .as_ref()
        .and_then(|payload| payload.get("repo_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            safe_name(
                repo_root
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("repository"),
            )
        });
    let name = options
        .name
        .clone()
        .unwrap_or_else(|| format!("codebase_graph_{}", install_safe_name(&repo_name)));
    let command_from_config = setup_config
        .as_ref()
        .and_then(|payload| payload.pointer("/mcp/command"))
        .and_then(serde_json::Value::as_array)
        .and_then(|values| {
            let command: Option<Vec<String>> = values
                .iter()
                .map(|value| value.as_str().map(str::to_string))
                .collect();
            command.filter(|parts| parts.len() >= 5)
        });
    let (command, args) = if let Some(mut parts) = command_from_config {
        let command = parts.remove(0);
        (command, parts)
    } else {
        (
            server_command(),
            vec![
                "mcp".to_string(),
                "serve".to_string(),
                "--config".to_string(),
                config_path.to_string_lossy().to_string(),
            ],
        )
    };
    Ok(NativeMcpDescriptor {
        name,
        command,
        args,
        setup_config_path: config_path.to_string_lossy().to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        timeout: 60,
    })
}

fn file_adapter_result(
    options: &McpInstallOptions,
    descriptor: &NativeMcpDescriptor,
    native_command: Option<Vec<String>>,
    native_error: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = options.client_config_path.clone().unwrap_or_else(|| {
        default_client_config_path(
            &options.client,
            &install_scope(&options.client, &options.scope),
            descriptor,
        )
    });
    let existing = fs::read_to_string(&path).ok();
    let rendered = render_client_config(
        &options.client,
        &install_scope(&options.client, &options.scope),
        existing.as_deref(),
        descriptor,
    )?;
    let action = if options.dry_run {
        "dry_run".to_string()
    } else {
        rendered.action.clone()
    };
    if !options.dry_run {
        write_text_atomic(&path, &rendered.text)?;
    }
    let mut payload = json!({
        "action": action,
        "client": options.client,
        "scope": install_scope(&options.client, &options.scope),
        "server_name": descriptor.name,
        "method": "file_adapter",
        "path": path.to_string_lossy(),
        "command": serde_json::Value::Null,
        "descriptor": descriptor.as_json(),
        "entry": rendered.entry,
        "patch": rendered.patch,
        "payload": rendered.payload,
    });
    if let Some(command) = native_command {
        payload["native_command"] = json!(command);
    }
    if let Some(error) = native_error {
        payload["native_error"] = json!(error);
    }
    attach_install_verification(payload, descriptor, options)
}

fn attach_install_verification(
    mut payload: serde_json::Value,
    descriptor: &NativeMcpDescriptor,
    options: &McpInstallOptions,
) -> Result<serde_json::Value, String> {
    if options.verify && !options.dry_run {
        payload["verification"] = verify_mcp_install(descriptor, &options.client);
    }
    Ok(payload)
}

fn verify_mcp_install(descriptor: &NativeMcpDescriptor, client: &str) -> serde_json::Value {
    let stdio = verify_stdio(descriptor);
    let visibility = verify_client_visibility(client, &descriptor.name);
    json!({
        "ok": stdio.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false)
            && visibility.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(true),
        "stdio": stdio,
        "client_visibility": visibility,
    })
}

fn verify_stdio(descriptor: &NativeMcpDescriptor) -> serde_json::Value {
    let command = descriptor_command(descriptor);
    let payload = [
        stdio_json_rpc_message(
            "initialize",
            json!({"protocolVersion": LATEST_PROTOCOL_VERSION}),
            1,
        ),
        stdio_json_rpc_message("tools/list", json!({}), 2),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_health", "arguments": {"include_structured_content": true}}),
            3,
        ),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_search", "arguments": {"query": descriptor.name, "limit": 1}}),
            4,
        ),
        stdio_json_rpc_message(
            "tools/call",
            json!({"name": "graph_query", "arguments": {"statement": "MATCH (n) DELETE n"}}),
            5,
        ),
    ]
    .join("");
    let mut process = match Command::new(&command[0])
        .args(&command[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(process) => process,
        Err(error) => return json!({"ok": false, "command": command, "error": error.to_string()}),
    };
    if let Some(mut stdin) = process.stdin.take() {
        if let Err(error) = stdin.write_all(payload.as_bytes()) {
            return json!({"ok": false, "command": command, "error": error.to_string()});
        }
    }
    let output = match process.wait_with_output() {
        Ok(output) => output,
        Err(error) => return json!({"ok": false, "command": command, "error": error.to_string()}),
    };
    let responses = parse_stdio_json_lines(&output.stdout);
    if !output.status.success() {
        return json!({
            "ok": false,
            "command": command,
            "returncode": output.status.code().unwrap_or(1),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "responses": responses,
        });
    }
    let checks = stdio_checks(&responses);
    json!({
        "ok": checks.values().all(|value| *value),
        "command": command,
        "checks": checks,
        "responses": responses,
        "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn verify_client_visibility(client: &str, server_name: &str) -> serde_json::Value {
    let Some(command) = visibility_command(client) else {
        return json!({"ok": true, "skipped": true, "reason": format!("{client} has no CLI visibility check")});
    };
    if !executable_in_path(&command[0]) {
        return json!({"ok": true, "skipped": true, "reason": format!("{} executable not found", command[0])});
    }
    match Command::new(&command[0]).args(&command[1..]).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let found = stdout.contains(server_name) || stderr.contains(server_name);
            json!({
                "ok": output.status.success() && found,
                "command": command,
                "returncode": output.status.code().unwrap_or(1),
                "found": found,
                "stdout": stdout,
                "stderr": stderr,
            })
        }
        Err(error) => json!({"ok": false, "command": command, "error": error.to_string()}),
    }
}

fn descriptor_command(descriptor: &NativeMcpDescriptor) -> Vec<String> {
    let mut command = vec![descriptor.command.clone()];
    command.extend(descriptor.args.clone());
    command
}

fn stdio_json_rpc_message(method: &str, params: serde_json::Value, id: u64) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap_or_else(|_| "{}".to_string())
        + "\n"
}

fn parse_stdio_json_lines(data: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(data)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

fn stdio_checks(responses: &[serde_json::Value]) -> BTreeMap<String, bool> {
    let mut by_id = BTreeMap::new();
    for response in responses {
        if let Some(id) = response.get("id").and_then(serde_json::Value::as_u64) {
            by_id.insert(id, response);
        }
    }
    let tools = by_id
        .get(&2)
        .and_then(|value| value.pointer("/result/tools"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tool_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(serde_json::Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut checks = BTreeMap::new();
    checks.insert(
        "initialize".to_string(),
        by_id
            .get(&1)
            .and_then(|value| value.pointer("/result/protocolVersion"))
            .and_then(serde_json::Value::as_str)
            == Some(LATEST_PROTOCOL_VERSION),
    );
    checks.insert(
        "tools_list".to_string(),
        tool_names.contains("graph_health") && tool_names.contains("graph_search"),
    );
    checks.insert(
        "graph_health".to_string(),
        by_id
            .get(&3)
            .and_then(|value| value.pointer("/result/structuredContent/ok"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    );
    checks.insert(
        "graph_search".to_string(),
        by_id
            .get(&4)
            .is_some_and(|value| value.get("error").is_none()),
    );
    checks.insert(
        "tool_error_result".to_string(),
        by_id
            .get(&5)
            .and_then(|value| value.pointer("/result/isError"))
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    );
    checks
}

fn visibility_command(client: &str) -> Option<Vec<String>> {
    match client {
        "codex" => Some(vec![
            "codex".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        "claude" | "claude-project" => Some(vec![
            "claude".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        "openclaw" => Some(vec![
            "openclaw".to_string(),
            "mcp".to_string(),
            "list".to_string(),
        ]),
        _ => None,
    }
}

struct RenderedNativeConfig {
    text: String,
    action: String,
    entry: serde_json::Value,
    patch: serde_json::Value,
    payload: serde_json::Value,
}

fn render_client_config(
    client: &str,
    scope: &str,
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    match adapter_id(client, scope) {
        "codex" => render_codex_config(existing, descriptor),
        "hermes" => render_hermes_config(existing, descriptor),
        "claude" | "claude-project" | "lmstudio" | "github-copilot" | "openclaw" | "generic" => {
            render_json_config(adapter_id(client, scope), existing, descriptor)
        }
        other => Err(format!("Unsupported MCP client adapter: {other}")),
    }
}

fn render_json_config(
    adapter: &str,
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let mut payload = existing
        .filter(|text| !text.trim().is_empty())
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()
        .map_err(|error| format!("MCP config must contain a JSON object: {error}"))?
        .unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Err("MCP config must contain a JSON object".to_string());
    }
    let root_path = match adapter {
        "github-copilot" => vec!["servers"],
        "openclaw" => vec!["mcp", "servers"],
        _ => vec!["mcpServers"],
    };
    let include_type = !matches!(adapter, "claude" | "generic");
    let entry = descriptor.stdio_entry(include_type, false);
    let previous = json_container_mut(&mut payload, &root_path)?
        .insert(descriptor.name.clone(), entry.clone());
    let action = action_for_json(previous.as_ref(), &entry, existing.is_some());
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())? + "\n";
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else {
        action
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: payload.clone(),
        payload,
    })
}

fn render_codex_config(
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(false, true);
    let patch = codex_toml_block(descriptor);
    let (text, previous) =
        upsert_toml_block(existing.unwrap_or_default(), &descriptor.name, &patch);
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else if previous.is_none() {
        "created".to_string()
    } else if previous.as_deref() == Some(patch.trim_end()) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: json!(patch),
        payload: json!(patch),
    })
}

fn render_hermes_config(
    existing: Option<&str>,
    descriptor: &NativeMcpDescriptor,
) -> Result<RenderedNativeConfig, String> {
    let entry = descriptor.stdio_entry(true, false);
    let patch = hermes_yaml_block(descriptor);
    let (text, previous) = upsert_marked_block(existing.unwrap_or_default(), &patch);
    let action = if existing == Some(text.as_str()) {
        "unchanged".to_string()
    } else if previous.is_none() {
        "created".to_string()
    } else if previous.as_deref() == Some(patch.trim_end()) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    };
    Ok(RenderedNativeConfig {
        text,
        action,
        entry,
        patch: json!(patch),
        payload: json!(patch),
    })
}

fn json_container_mut<'a>(
    payload: &'a mut serde_json::Value,
    path: &[&str],
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>, String> {
    let mut cursor = payload
        .as_object_mut()
        .ok_or_else(|| "MCP config must contain a JSON object".to_string())?;
    for key in path {
        let next = cursor
            .entry((*key).to_string())
            .or_insert_with(|| json!({}));
        cursor = next
            .as_object_mut()
            .ok_or_else(|| format!("MCP config key must contain an object: {}", path.join(".")))?;
    }
    Ok(cursor)
}

fn action_for_json(
    previous: Option<&serde_json::Value>,
    next_value: &serde_json::Value,
    file_exists: bool,
) -> String {
    if !file_exists {
        "created".to_string()
    } else if previous == Some(next_value) {
        "unchanged".to_string()
    } else {
        "updated".to_string()
    }
}

fn default_client_config_path(
    client: &str,
    scope: &str,
    descriptor: &NativeMcpDescriptor,
) -> PathBuf {
    let home = home_dir();
    match adapter_id(client, scope) {
        "codex" => env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"))
            .join("config.toml"),
        "claude" => {
            let mac_path =
                home.join("Library/Application Support/Claude/claude_desktop_config.json");
            if mac_path.parent().is_some_and(Path::exists) {
                mac_path
            } else {
                home.join(".config/claude/claude_desktop_config.json")
            }
        }
        "claude-project" => PathBuf::from(&descriptor.repo_root).join(".mcp.json"),
        "lmstudio" => home.join(".lmstudio/mcp.json"),
        "github-copilot" => PathBuf::from(&descriptor.repo_root).join(".vscode/mcp.json"),
        "hermes" => home.join(".hermes/config.yaml"),
        "openclaw" => env::var_os("OPENCLAW_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".openclaw"))
            .join("mcp.json5"),
        _ => home.join(".config/mcp/mcp.json"),
    }
}

fn supported_install_clients() -> Vec<&'static str> {
    vec![
        "claude",
        "claude-project",
        "codex",
        "copilot-studio",
        "generic",
        "github-copilot",
        "hermes",
        "lmstudio",
        "microsoft-copilot",
        "openclaw",
    ]
}

fn install_safe_name(value: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    normalized.trim_matches(['.', '_', '-']).to_string()
}

fn supported_install_clients_with_all() -> Vec<&'static str> {
    let mut clients = supported_install_clients();
    clients.push("all");
    clients
}

fn install_scope(client: &str, scope: &str) -> String {
    if client == "claude-project" {
        "project".to_string()
    } else {
        scope.to_string()
    }
}

fn adapter_id<'a>(client: &'a str, scope: &str) -> &'a str {
    if client == "claude" && scope == "project" {
        "claude-project"
    } else {
        client
    }
}

fn native_client_command(
    client: &str,
    descriptor: &NativeMcpDescriptor,
    scope: &str,
) -> Option<Vec<String>> {
    match client {
        "codex" => Some(vec![
            "codex".to_string(),
            "mcp".to_string(),
            "add".to_string(),
            descriptor.name.clone(),
            "--".to_string(),
            descriptor.command.clone(),
            descriptor.args[0].clone(),
            descriptor.args[1].clone(),
            descriptor.args[2].clone(),
            descriptor.args[3].clone(),
        ]),
        "claude" | "claude-project" => Some(vec![
            "claude".to_string(),
            "mcp".to_string(),
            "add".to_string(),
            "--transport".to_string(),
            "stdio".to_string(),
            "--scope".to_string(),
            install_scope(client, scope),
            descriptor.name.clone(),
            "--".to_string(),
            descriptor.command.clone(),
            descriptor.args[0].clone(),
            descriptor.args[1].clone(),
            descriptor.args[2].clone(),
            descriptor.args[3].clone(),
        ]),
        "openclaw" => Some(vec![
            "openclaw".to_string(),
            "mcp".to_string(),
            "set".to_string(),
            descriptor.name.clone(),
            serde_json::to_string(&descriptor.stdio_entry(true, false)).ok()?,
        ]),
        _ => None,
    }
}

fn copilot_studio_metadata(descriptor: &NativeMcpDescriptor) -> serde_json::Value {
    json!({
        "kind": "copilot_studio_manual_metadata",
        "stdio": descriptor.stdio_entry(true, false),
        "http": {
            "url": "http://127.0.0.1:8765/mcp",
            "start_command": [
                descriptor.command,
                "mcp",
                "http",
                "--config",
                descriptor.setup_config_path,
                "--host",
                "127.0.0.1",
                "--port",
                "8765",
                "--path",
                "/mcp"
            ],
            "host": "127.0.0.1",
            "port": 8765,
            "path": "/mcp",
        },
        "notes": [
            "No local client configuration file is written for Copilot Studio.",
            "Remote Copilot Studio use requires user-managed endpoint exposure, bearer-token configuration, and TLS.",
        ],
    })
}

fn codex_toml_block(descriptor: &NativeMcpDescriptor) -> String {
    format!(
        "[mcp_servers.{}]\ncommand = {}\nargs = {}\nstartup_timeout_sec = {}\n",
        descriptor.name,
        toml_string(&descriptor.command),
        toml_array(&descriptor.args),
        descriptor.timeout
    )
}

fn toml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| toml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn upsert_toml_block(existing: &str, server_name: &str, block: &str) -> (String, Option<String>) {
    let lines = existing.lines().collect::<Vec<_>>();
    let header = format!("[mcp_servers.{server_name}]");
    let env_header = format!("[mcp_servers.{server_name}.env]");
    let start = lines
        .iter()
        .position(|line| line.trim() == header || line.trim() == env_header);
    let Some(start) = start else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('[')
                && trimmed.ends_with(']')
                && trimmed != header
                && trimmed != env_header
        })
        .map(|index| start + 1 + index)
        .unwrap_or(lines.len());
    let previous = lines[start..end].join("\n").trim_end().to_string();
    let mut next_lines = Vec::new();
    next_lines.extend(lines[..start].iter().map(|value| (*value).to_string()));
    next_lines.extend(block.trim_end().lines().map(str::to_string));
    next_lines.extend(lines[end..].iter().map(|value| (*value).to_string()));
    (
        next_lines.join("\n").trim_end().to_string() + "\n",
        Some(previous),
    )
}

fn hermes_yaml_block(descriptor: &NativeMcpDescriptor) -> String {
    let mut lines = vec![
        "# codebaseGraph MCP server start".to_string(),
        "mcp_servers:".to_string(),
        format!("  {}:", descriptor.name),
        "    type: stdio".to_string(),
        format!("    command: {}", yaml_scalar(&descriptor.command)),
        "    args:".to_string(),
    ];
    for arg in &descriptor.args {
        lines.push(format!("      - {}", yaml_scalar(arg)));
    }
    lines.push("# codebaseGraph MCP server end".to_string());
    lines.join("\n") + "\n"
}

fn yaml_scalar(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn upsert_marked_block(existing: &str, block: &str) -> (String, Option<String>) {
    const START: &str = "# codebaseGraph MCP server start";
    const END: &str = "# codebaseGraph MCP server end";
    let Some(start) = existing.find(START) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    let Some(end) = existing.find(END) else {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    };
    if end < start {
        let prefix = existing.trim_end();
        let separator = if prefix.is_empty() { "" } else { "\n\n" };
        return (format!("{prefix}{separator}{block}"), None);
    }
    let after_end = end + END.len();
    let previous = existing[start..after_end].trim_end().to_string();
    let text = format!(
        "{}\n\n{}\n\n{}",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start()
    )
    .trim()
    .to_string()
        + "\n";
    (text, Some(previous))
}

fn write_text_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    ));
    fs::write(&tmp_path, text).map_err(|error| {
        format!(
            "failed to write temporary config {}: {error}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path)
        .map_err(|error| format!("failed to replace config {}: {error}", path.display()))
}

fn expand_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn executable_in_path(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.components().count() > 1 {
        return path.is_file();
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| dir.join(executable).is_file()))
        .unwrap_or(false)
}

fn subprocess_error(completed: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&completed.stdout)
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(&completed.stderr)
        .trim()
        .to_string();
    let output = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let code = completed.status.code().unwrap_or(1);
    if output.is_empty() {
        format!("exit {code}")
    } else {
        format!("exit {code}: {output}")
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    body_too_large: bool,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    payload: serde_json::Value,
    headers: Vec<(String, String)>,
}

impl HttpResponse {
    fn json(status: u16, payload: serde_json::Value) -> Self {
        Self {
            status,
            payload,
            headers: Vec::new(),
        }
    }
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[derive(Debug)]
struct HealthRuntime {
    repo_root: PathBuf,
    db_path: PathBuf,
    manifest_path: PathBuf,
}

fn resolve_health_runtime(options: &HealthOptions) -> Result<HealthRuntime, String> {
    let repo_root = options
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| options.repo_root.clone());
    let default_paths = GraphStatePaths::derive(&repo_root);
    let config_path = options
        .config
        .clone()
        .unwrap_or_else(|| default_paths.config_path.clone());
    let config = if config_path.exists() {
        Some(read_json_file(&config_path)?)
    } else {
        None
    };
    let db_path = options
        .db
        .clone()
        .or_else(|| {
            config
                .as_ref()
                .and_then(|value| value.get("database_path"))
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
        })
        .unwrap_or(default_paths.db_path);
    let manifest_path = options
        .manifest
        .clone()
        .or_else(|| {
            config
                .as_ref()
                .and_then(|value| value.get("manifest_path"))
                .and_then(serde_json::Value::as_str)
                .map(PathBuf::from)
        })
        .unwrap_or(default_paths.manifest_path);
    Ok(HealthRuntime {
        repo_root,
        db_path,
        manifest_path,
    })
}

fn setup_config_payload(paths: &GraphStatePaths, repo_root: &Path) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "repo_root": repo_root,
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir,
        "database_path": paths.db_path,
        "manifest_path": paths.manifest_path,
        "ontology_version": "code_ontology_v1",
        "package_version": env!("CARGO_PKG_VERSION"),
        "materialization": {
            "include": [],
            "exclude": []
        },
        "mcp": {
            "server_name": "codebase_graph",
            "command": [
                server_command(),
                "mcp",
                "serve",
                "--config",
                paths.config_path.to_string_lossy()
            ]
        }
    })
}

fn write_setup_config(paths: &GraphStatePaths, repo_root: &Path) -> Result<&'static str, String> {
    let payload = setup_config_payload(paths, repo_root);
    let mut action = "created";
    if paths.config_path.exists() {
        let previous = read_json_file(&paths.config_path)?;
        if previous == payload {
            return Ok("unchanged");
        }
        action = "updated";
    }
    if let Some(parent) = paths.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create config directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    fs::write(&paths.config_path, format!("{text}\n")).map_err(|error| {
        format!(
            "failed to write setup config {}: {error}",
            paths.config_path.display()
        )
    })?;
    Ok(action)
}

fn json_file_would_change(path: &Path, payload: &serde_json::Value) -> Result<bool, String> {
    if !path.exists() {
        return Ok(true);
    }
    Ok(read_json_file(path)? != *payload)
}

fn instruction_target_path(repo_root: &Path, target: &str) -> Result<Option<PathBuf>, String> {
    match target {
        "skip" => Ok(None),
        "agents" => Ok(Some(repo_root.join("AGENTS.md"))),
        "claude" => Ok(Some(repo_root.join("CLAUDE.md"))),
        "auto" => {
            let agents = repo_root.join("AGENTS.md");
            if agents.exists() {
                return Ok(Some(agents));
            }
            let claude = repo_root.join("CLAUDE.md");
            if claude.exists() {
                return Ok(Some(claude));
            }
            Ok(Some(agents))
        }
        other => Err(format!("Unsupported instruction target: {other}")),
    }
}

fn upsert_instruction_block(
    repo_root: &Path,
    target: &str,
    config_path: &Path,
) -> Result<serde_json::Value, String> {
    let Some(path) = instruction_target_path(repo_root, target)? else {
        return Ok(json!({"action": "skipped", "path": serde_json::Value::Null}));
    };
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let created = !path.exists();
    let block = instruction_block(config_path);
    let (next_text, action) = upsert_instruction_text(&existing, &block, created);
    if next_text == existing {
        return Ok(json!({"action": "unchanged", "path": path.to_string_lossy()}));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create instruction directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, next_text)
        .map_err(|error| format!("failed to write instructions {}: {error}", path.display()))?;
    Ok(json!({"action": action, "path": path.to_string_lossy()}))
}

fn instruction_block(config_path: &Path) -> String {
    format!(
        "<!-- codebaseGraph:start -->\n\
## codebaseGraph workflow\n\
- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.\n\
- Prefer the `codebase_graph` MCP server tools over shell commands whenever they are exposed in the current agent session.\n\
- AI agents receive block output by default for graph CLI and MCP tools; request `output_format: \"json\"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.\n\
- Use MCP `graph_search` with `detail: \"slim\"` and `context_limit: 1` before answering repo-structure questions or performing coding tasks.\n\
- Use MCP `graph_context` with `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.\n\
- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.\n\
- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.\n\
- If MCP tools are unavailable, fall back to CLI: `{command} graph-search <query> --repo-root . --no-refresh --detail slim --context-limit 1`, `{command} graph-context <query> --repo-root . --profile <profile> --no-refresh --detail slim --context-limit 2`, `{command} graph-architecture-queries`, `{command} graph-query \"<statement>\" --repo-root .`, `{command} graph-schema`, and `{command} graph-query-helpers`.\n\
- Refresh the graph with `{command} setup --repo-root . --mcp-client none` when files change materially. Setup config: `{config_path}`.\n\
<!-- codebaseGraph:end -->\n",
        command = server_command(),
        config_path = config_path.to_string_lossy(),
    )
}

fn upsert_instruction_text(existing: &str, block: &str, created: bool) -> (String, &'static str) {
    const START: &str = "<!-- codebaseGraph:start -->";
    const END: &str = "<!-- codebaseGraph:end -->";
    if existing.trim().is_empty() {
        return (block.to_string(), "created");
    }
    let Some(start) = existing.find(START) else {
        let separator = if existing.ends_with('\n') { "" } else { "\n" };
        let action = if created { "created" } else { "updated" };
        return (
            format!("{}{separator}\n{}", existing.trim_end(), block),
            action,
        );
    };
    let Some(end) = existing.find(END) else {
        return (
            format!("{}\n\n{}", existing.trim_end(), block),
            if created { "created" } else { "updated" },
        );
    };
    if end < start {
        return (
            format!("{}\n\n{}", existing.trim_end(), block),
            if created { "created" } else { "updated" },
        );
    }
    let after_end = end + END.len();
    let text = format!(
        "{}\n\n{}\n\n{}",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start()
    )
    .trim()
    .to_string()
        + "\n";
    (text, "updated")
}

fn snapshot_file(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("failed to snapshot {}: {error}", path.display()))
}

fn restore_file(path: &Path, previous: Option<&str>) -> Result<(), String> {
    match previous {
        Some(text) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("failed to restore directory {}: {error}", parent.display())
                })?;
            }
            fs::write(path, text)
                .map_err(|error| format!("failed to restore {}: {error}", path.display()))
        }
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
        },
    }
}

fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}

fn count_graph_nodes(db_path: &Path) -> Result<u64, String> {
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    let mut result = conn
        .query("MATCH (n) RETURN count(n) AS total_nodes LIMIT 1")
        .map_err(|error| format!("failed to query graph health: {error}"))?;
    let row = result
        .next()
        .ok_or_else(|| "graph health query returned no rows".to_string())?;
    row.first()
        .and_then(value_to_u64)
        .ok_or_else(|| "graph health query returned a non-numeric node count".to_string())
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int64(value) if *value >= 0 => Some(*value as u64),
        Value::Int32(value) if *value >= 0 => Some(*value as u64),
        Value::Int16(value) if *value >= 0 => Some(*value as u64),
        Value::Int8(value) if *value >= 0 => Some(*value as u64),
        Value::UInt64(value) => Some(*value),
        Value::UInt32(value) => Some(u64::from(*value)),
        Value::UInt16(value) => Some(u64::from(*value)),
        Value::UInt8(value) => Some(u64::from(*value)),
        _ => None,
    }
}

fn execute_graph_search(
    db_path: &Path,
    options: &GraphSearchOptions,
) -> Result<Vec<serde_json::Value>, String> {
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    crate::ladybug_writer::preseed_ladybug_extensions(true).map_err(|error| error.to_string())?;
    conn.query("LOAD fts")
        .map_err(|error| format!("failed to load FTS extension for graph search: {error}"))?;
    let schema = metadata_payload(GRAPH_SCHEMA_JSON)?;
    let mut hits = Vec::new();
    let candidate_limit = options.limit.clamp(10, 50);
    let mut order = 0_usize;
    for index in value_array(&schema, "search_indexes") {
        let index_name = value_str(index, "name");
        for node_type in index
            .get("node_types")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            let full_index_name = format!("{index_name}_{node_type}");
            hits.extend(search_fts_index(
                &conn,
                node_type,
                &full_index_name,
                &options.query,
                candidate_limit,
                order,
            )?);
            order += 1;
        }
    }
    rank_search_hits(&mut hits, &options.query);
    hits.sort_by(|left, right| {
        right
            .rank_score
            .partial_cmp(&left.rank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.index_order.cmp(&right.index_order))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line_start.cmp(&right.line_start))
            .then_with(|| left.id.cmp(&right.id))
    });
    hits.dedup_by(|left, right| left.id == right.id);
    let mut payloads = Vec::new();
    for hit in hits.into_iter().take(options.limit) {
        let context = if options.context_limit > 0 && options.budget > 0 {
            execute_graph_context(db_path, &hit.id, &hit.node_type, options)?
        } else {
            Vec::new()
        };
        let mut payload = hit.into_json(options);
        if payload
            .get("context")
            .and_then(serde_json::Value::as_array)
            .is_some()
        {
            payload["context"] = serde_json::Value::Array(context);
        }
        payloads.push(payload);
    }
    Ok(payloads)
}

fn execute_graph_context(
    db_path: &Path,
    node_id: &str,
    node_type: &str,
    options: &GraphSearchOptions,
) -> Result<Vec<serde_json::Value>, String> {
    if options.context_limit == 0 || options.budget == 0 {
        return Ok(Vec::new());
    }
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    let schema = metadata_payload(GRAPH_SCHEMA_JSON)?;
    let profile = schema
        .get("context_profiles")
        .and_then(|profiles| profiles.get(&options.profile))
        .ok_or_else(|| format!("Unknown context profile: {}", options.profile))?;
    let relations = profile
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("Context profile {} has no relations", options.profile))?;
    let depth_limit = options.max_depth.unwrap_or_else(|| {
        profile
            .get("max_depth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize
    });
    if depth_limit == 0 {
        return Ok(Vec::new());
    }
    let mut context = Vec::new();
    let mut seen = BTreeSet::new();
    let mut frontier = vec![(node_id.to_string(), node_type.to_string(), 0_usize)];
    while let Some((current_id, current_type, depth)) = frontier.first().cloned() {
        frontier.remove(0);
        if depth >= depth_limit || context.len() >= options.context_limit {
            continue;
        }
        for relation in relations.iter().filter_map(serde_json::Value::as_str) {
            for direction in ["outgoing", "incoming"] {
                let remaining = options.context_limit.saturating_sub(context.len());
                if remaining == 0 {
                    return Ok(context);
                }
                let neighbors = query_relation_neighbors(
                    &conn,
                    &schema,
                    &current_id,
                    &current_type,
                    relation,
                    direction,
                    remaining,
                )?;
                for neighbor in neighbors {
                    let neighbor_id = value_str(&neighbor, "id").to_string();
                    let neighbor_type = value_str(&neighbor, "type").to_string();
                    if neighbor_id.is_empty() || neighbor_type.is_empty() {
                        continue;
                    }
                    let dedupe_key =
                        format!("{direction}:{relation}:{neighbor_type}:{neighbor_id}");
                    if !seen.insert(dedupe_key) {
                        continue;
                    }
                    if depth + 1 < depth_limit {
                        frontier.push((neighbor_id, neighbor_type, depth + 1));
                    }
                    context.push(neighbor);
                    if context.len() >= options.context_limit {
                        return Ok(context);
                    }
                }
            }
        }
    }
    Ok(context)
}

fn query_relation_neighbors(
    conn: &Connection,
    schema: &serde_json::Value,
    node_id: &str,
    node_type: &str,
    relation: &str,
    direction: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let Some(relation_type) = relation_type(schema, relation) else {
        return Ok(Vec::new());
    };
    let source_types = relation_type
        .get("source_types")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    let target_types = relation_type
        .get("target_types")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    let neighbor_types = if direction == "outgoing" {
        if !source_types.contains(&node_type) {
            return Ok(Vec::new());
        }
        target_types
    } else {
        if !target_types.contains(&node_type) {
            return Ok(Vec::new());
        }
        source_types
    };
    let mut neighbors = Vec::new();
    for neighbor_type in neighbor_types {
        if neighbors.len() >= limit {
            break;
        }
        let remaining = limit - neighbors.len();
        let statement = neighbor_statement(
            node_type,
            neighbor_type,
            relation,
            direction,
            node_id,
            remaining,
        );
        let mut result = match conn.query(&statement) {
            Ok(result) => result,
            Err(error) if is_missing_search_target_error(&error.to_string()) => continue,
            Err(error) => {
                return Err(format!(
                    "failed to query {direction} {relation} neighbors for {node_type}: {error}"
                ));
            }
        };
        for row in result.by_ref() {
            let label = value_to_string(row.get(1));
            let label = if label.is_empty() {
                value_to_string(row.get(2))
            } else {
                label
            };
            let summary = value_to_string(row.get(6));
            let mut payload = json!({
                "direction": direction,
                "relation": relation,
                "type": neighbor_type,
                "label": label.clone(),
                "path": value_to_string(row.get(3)),
                "span": span_json(value_to_i64(row.get(4)), value_to_i64(row.get(5))),
                "id": value_to_string(row.first()),
            });
            if !summary.is_empty() && summary != label {
                payload["summary"] = json!(summary);
            }
            let edge_id = value_to_string(row.get(7));
            if !edge_id.is_empty() {
                payload["evidence_path"] = json!({
                    "chain": format!("{}:{}->{}", relation, value_to_string(row.get(9)), value_to_string(row.get(10)))
                });
            }
            neighbors.push(payload);
            if neighbors.len() >= limit {
                break;
            }
        }
    }
    Ok(neighbors)
}

fn relation_type<'a>(
    schema: &'a serde_json::Value,
    relation: &str,
) -> Option<&'a serde_json::Value> {
    value_array(schema, "relation_types")
        .iter()
        .find(|value| value_str(value, "name") == relation)
}

fn neighbor_statement(
    node_type: &str,
    neighbor_type: &str,
    relation: &str,
    direction: &str,
    node_id: &str,
    limit: usize,
) -> String {
    if direction == "outgoing" {
        format!(
            "MATCH (source:`{}` {{id: '{}'}})-[:`FROM_{}`]->(edge:`{}`)-[:`TO_{}`]->(neighbor:`{}`) RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, neighbor.line_start, neighbor.line_end, neighbor.summary, edge.id, edge.kind, edge.source_id, edge.target_id, edge.confidence, edge.metadata LIMIT {}",
            cypher_identifier(node_type),
            cypher_single_quoted(node_id),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(neighbor_type),
            limit,
        )
    } else {
        format!(
            "MATCH (neighbor:`{}`)-[:`FROM_{}`]->(edge:`{}`)-[:`TO_{}`]->(target:`{}` {{id: '{}'}}) RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, neighbor.line_start, neighbor.line_end, neighbor.summary, edge.id, edge.kind, edge.source_id, edge.target_id, edge.confidence, edge.metadata LIMIT {}",
            cypher_identifier(neighbor_type),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(node_type),
            cypher_single_quoted(node_id),
            limit,
        )
    }
}

fn search_fts_index(
    conn: &Connection,
    node_type: &str,
    index_name: &str,
    query: &str,
    limit: usize,
    index_order: usize,
) -> Result<Vec<SearchHitRow>, String> {
    let statement = format!(
        "CALL QUERY_FTS_INDEX('{}', '{}', '{}', TOP := {}) RETURN node.id, node.label, node.qualified_name, node.path, node.line_start, node.line_end, node.summary, score",
        cypher_single_quoted(node_type),
        cypher_single_quoted(index_name),
        cypher_single_quoted(query),
        limit
    );
    let mut result = match conn.query(&statement) {
        Ok(result) => result,
        Err(error) if is_missing_search_target_error(&error.to_string()) => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "failed to search FTS index {index_name} for {node_type}: {error}"
            ));
        }
    };
    let mut rows = Vec::new();
    for row in result.by_ref() {
        rows.push(SearchHitRow {
            id: value_to_string(row.first()),
            node_type: node_type.to_string(),
            label: value_to_string(row.get(1)),
            qualified_name: value_to_string(row.get(2)),
            path: value_to_string(row.get(3)),
            line_start: value_to_i64(row.get(4)),
            line_end: value_to_i64(row.get(5)),
            summary: value_to_string(row.get(6)),
            score: value_to_f64(row.get(7)),
            rank_score: 0.0,
            index_order,
        });
    }
    Ok(rows)
}

fn is_missing_search_target_error(error: &str) -> bool {
    error.contains("does not exist")
        || error.contains("doesn't have an index")
        || error.contains("Index not found")
}

#[derive(Debug, Clone)]
struct SearchHitRow {
    id: String,
    node_type: String,
    label: String,
    qualified_name: String,
    path: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    summary: String,
    score: f64,
    rank_score: f64,
    index_order: usize,
}

impl SearchHitRow {
    fn into_json(self, options: &GraphSearchOptions) -> serde_json::Value {
        let span = span_json(self.line_start, self.line_end);
        if options.detail == "slim" {
            let mut payload = json!({
                "id": self.id,
                "type": self.node_type,
                "label": self.label,
                "rank_score": self.rank_score,
            });
            if !self.path.is_empty() {
                payload["path"] = json!(self.path);
            }
            if !span.as_object().is_none_or(serde_json::Map::is_empty) {
                payload["span"] = span;
            }
            if !self.summary.is_empty() && self.summary != self.label {
                payload["summary"] = json!(self.summary);
            }
            return payload;
        }
        json!({
            "id": self.id,
            "type": self.node_type,
            "label": self.label,
            "qualified_name": self.qualified_name,
            "path": self.path,
            "span": span,
            "score": self.score,
            "rank_score": self.rank_score,
            "score_components": {
                "fts": self.score,
                "lexical": lexical_score(&options.query, &self),
                "entity": entity_priority_score(&self.node_type),
            },
            "summary": self.summary,
            "context": [],
        })
    }
}

fn rank_search_hits(hits: &mut [SearchHitRow], query: &str) {
    let max_score = hits.iter().map(|hit| hit.score).fold(0.0, f64::max);
    for hit in hits {
        let fts_score = if max_score > 0.0 {
            hit.score / max_score
        } else {
            0.0
        };
        let lexical = lexical_score(query, hit);
        hit.rank_score = round6(
            (0.25 * fts_score) + (0.25 * lexical) + (0.50 * entity_priority_score(&hit.node_type)),
        );
    }
}

fn lexical_score(query: &str, hit: &SearchHitRow) -> f64 {
    let normalized_query = query.to_ascii_lowercase();
    if normalized_query.is_empty() {
        return 0.0;
    }
    let label = hit.label.to_ascii_lowercase();
    let qualified_name = hit.qualified_name.to_ascii_lowercase();
    let path = hit.path.to_ascii_lowercase();
    if label == normalized_query || qualified_name == normalized_query {
        1.0
    } else if label.contains(&normalized_query) || qualified_name.contains(&normalized_query) {
        0.8
    } else if path.contains(&normalized_query) {
        0.5
    } else {
        0.0
    }
}

fn entity_priority_score(node_type: &str) -> f64 {
    match node_type {
        "Class" | "Function" | "Method" | "Module" | "Variable" | "Parameter" | "Field"
        | "Enum" | "Interface" | "Trait" | "Struct" => 1.0,
        "File" | "DocumentationChunk" | "DocumentationSource" => 0.8,
        "CallExpression" | "Reference" | "ImportDeclaration" | "Assignment" => 0.6,
        "Symbol" => 0.25,
        "Dependency" => 0.2,
        "SyntaxCapture" => 0.1,
        _ => 0.5,
    }
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn span_json(line_start: Option<i64>, line_end: Option<i64>) -> serde_json::Value {
    let mut span = serde_json::Map::new();
    if let Some(line_start) = line_start {
        span.insert("line_start".to_string(), json!(line_start));
    }
    if let Some(line_end) = line_end {
        span.insert("line_end".to_string(), json!(line_end));
    }
    serde_json::Value::Object(span)
}

fn cypher_single_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn cypher_identifier(value: &str) -> String {
    value.replace('`', "``")
}

fn value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Int64(value)) => value.to_string(),
        Some(Value::UInt64(value)) => value.to_string(),
        Some(Value::Int32(value)) => value.to_string(),
        Some(Value::UInt32(value)) => value.to_string(),
        Some(Value::Null(_)) | None => String::new(),
        Some(value) => value.to_string(),
    }
}

fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int64(value)) => Some(*value),
        Some(Value::Int32(value)) => Some(i64::from(*value)),
        Some(Value::Int16(value)) => Some(i64::from(*value)),
        Some(Value::Int8(value)) => Some(i64::from(*value)),
        Some(Value::UInt64(value)) => i64::try_from(*value).ok(),
        Some(Value::UInt32(value)) => Some(i64::from(*value)),
        Some(Value::UInt16(value)) => Some(i64::from(*value)),
        Some(Value::UInt8(value)) => Some(i64::from(*value)),
        _ => None,
    }
}

fn value_to_f64(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Double(value)) => *value,
        Some(Value::Float(value)) => f64::from(*value),
        Some(Value::Int64(value)) => *value as f64,
        Some(Value::UInt64(value)) => *value as f64,
        Some(Value::Int32(value)) => f64::from(*value),
        Some(Value::UInt32(value)) => f64::from(*value),
        _ => 0.0,
    }
}

fn validate_read_only_statement(statement: &str) -> Result<(), String> {
    let stripped = statement.trim().trim_end_matches(';');
    if stripped.contains(';') {
        return Err("graph_query accepts one read-only statement at a time".to_string());
    }
    for keyword in [
        "ALTER", "ATTACH", "CALL", "COPY", "CREATE", "DELETE", "DETACH", "DROP", "EXPORT",
        "IMPORT", "INSERT", "INSTALL", "LOAD", "MERGE", "REMOVE", "RENAME", "SET", "TRUNCATE",
        "UPDATE", "USE",
    ] {
        if contains_keyword(stripped, keyword) {
            return Err(format!(
                "graph_query is read-only; blocked keyword: {keyword}"
            ));
        }
    }
    Ok(())
}

fn contains_keyword(statement: &str, keyword: &str) -> bool {
    let uppercase = statement.to_ascii_uppercase();
    let mut search_start = 0;
    while let Some(index) = uppercase[search_start..].find(keyword) {
        let absolute_index = search_start + index;
        let before = uppercase[..absolute_index]
            .chars()
            .next_back()
            .map(is_keyword_char)
            .unwrap_or(false);
        let after = uppercase[absolute_index + keyword.len()..]
            .chars()
            .next()
            .map(is_keyword_char)
            .unwrap_or(false);
        if !before && !after {
            return true;
        }
        search_start = absolute_index + keyword.len();
    }
    false
}

fn is_keyword_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn execute_read_only_query(
    db_path: &Path,
    statement: &str,
    parameters: &serde_json::Map<String, serde_json::Value>,
    limit: usize,
) -> Result<(Vec<Vec<serde_json::Value>>, bool), String> {
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    let mut result = if parameters.is_empty() {
        conn.query(statement)
            .map_err(|error| format!("failed to execute graph query: {error}"))?
    } else {
        let named_parameters = lbug_query_parameters(parameters)?;
        let mut prepared = conn
            .prepare(statement)
            .map_err(|error| format!("failed to prepare graph query: {error}"))?;
        if !prepared.is_read_only() {
            return Err("graph-query prepared statement is not read-only".to_string());
        }
        let execute_parameters = named_parameters
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        conn.execute(&mut prepared, execute_parameters)
            .map_err(|error| format!("failed to execute graph query: {error}"))?
    };
    let mut rows = Vec::new();
    let mut truncated = false;
    for row in result.by_ref().take(limit + 1) {
        if rows.len() == limit {
            truncated = true;
            break;
        }
        rows.push(row.into_iter().map(json_safe_value).collect());
    }
    Ok((rows, truncated))
}

fn lbug_query_parameters(
    parameters: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<(String, Value)>, String> {
    let mut converted = Vec::with_capacity(parameters.len());
    for (name, value) in parameters {
        if name.trim().is_empty() {
            return Err("graph_query parameter names must not be blank".to_string());
        }
        converted.push((name.clone(), json_parameter_to_lbug_value(value)?));
    }
    Ok(converted)
}

fn json_parameter_to_lbug_value(value: &serde_json::Value) -> Result<Value, String> {
    match value {
        serde_json::Value::Bool(value) => Ok(Value::Bool(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Int64(value))
            } else if let Some(value) = value.as_u64() {
                Ok(Value::UInt64(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Double(value))
            } else {
                Err("graph_query numeric parameter is not representable".to_string())
            }
        }
        serde_json::Value::String(value) => Ok(Value::String(value.clone())),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Ok(Value::Json(value.clone()))
        }
    }
}

fn json_safe_value(value: Value) -> serde_json::Value {
    match value {
        Value::Null(_) => serde_json::Value::Null,
        Value::Bool(value) => json!(value),
        Value::Int64(value) => json!(value),
        Value::Int32(value) => json!(value),
        Value::Int16(value) => json!(value),
        Value::Int8(value) => json!(value),
        Value::UInt64(value) => json!(value),
        Value::UInt32(value) => json!(value),
        Value::UInt16(value) => json!(value),
        Value::UInt8(value) => json!(value),
        Value::Int128(value) => json!(value.to_string()),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| json!(value.to_string())),
        Value::Float(value) => serde_json::Number::from_f64(f64::from(value))
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| json!(value.to_string())),
        Value::String(value) => json!(value),
        Value::Json(value) => value,
        Value::List(_, values) | Value::Array(_, values) => {
            serde_json::Value::Array(values.into_iter().map(json_safe_value).collect())
        }
        Value::Struct(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, json_safe_value(value)))
                .collect(),
        ),
        other => json!(other.to_string()),
    }
}

fn required_arg<'a>(args: &'a [String], index: usize, name: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} requires a value"))
}

fn parse_usize_arg(args: &[String], index: usize, name: &str) -> Result<usize, String> {
    required_arg(args, index, name)?
        .parse::<usize>()
        .map_err(|error| format!("{name} must be an integer: {error}"))
}

fn metadata_payload(source: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(source)
        .map_err(|error| format!("failed to parse embedded metadata: {error}"))
}

fn write_metadata_output<W: Write>(
    stdout: &mut W,
    payload: &serde_json::Value,
    options: &MetadataOutputOptions,
    block_serializer: fn(&serde_json::Value) -> String,
) -> Result<(), String> {
    let text = if options.format == "json" {
        if options.pretty {
            serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?
        } else {
            serde_json::to_string(payload).map_err(|error| error.to_string())?
        }
    } else {
        block_serializer(payload)
    };
    writeln!(stdout, "{text}").map_err(|error| error.to_string())
}

fn filter_architecture_group(payload: &mut serde_json::Value, group: &str) -> Result<(), String> {
    let groups = payload
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected: Vec<serde_json::Value> = groups
        .iter()
        .filter(|value| value.get("name").and_then(serde_json::Value::as_str) == Some(group))
        .cloned()
        .collect();
    if selected.is_empty() {
        let valid = groups
            .iter()
            .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown architecture query group: {group}. Valid groups: {valid}"
        ));
    }
    if let Some(object) = payload.as_object_mut() {
        object.insert("groups".to_string(), serde_json::Value::Array(selected));
    }
    Ok(())
}

fn serialize_schema_block(payload: &serde_json::Value) -> String {
    let node_types = value_array(payload, "node_types");
    let relation_types = value_array(payload, "relation_types");
    let parser_mappings = value_array(payload, "parser_node_mappings");
    let search_indexes = value_array(payload, "search_indexes");
    let profiles = payload
        .get("context_profiles")
        .and_then(serde_json::Value::as_object)
        .map(serde_json::Map::len)
        .unwrap_or_default();
    let query_helpers = value_array(payload, "query_helpers");
    let mut lines = vec![format!(
        "schema {} version={} nodes={} relations={} parser_mappings={} indexes={} profiles={} helpers={}",
        block_value(value_str(payload, "ontology")),
        block_value(value_str(payload, "version")),
        node_types.len(),
        relation_types.len(),
        parser_mappings.len(),
        search_indexes.len(),
        profiles,
        query_helpers.len()
    )];
    if !node_types.is_empty() {
        lines.push(format!("node_types {}", csv_names(node_types)));
    }
    if !relation_types.is_empty() {
        lines.push(format!("relation_types {}", csv_names(relation_types)));
    }
    for index in search_indexes {
        lines.push(format!(
            "index {} node_types={} fields={}",
            block_value(value_str(index, "name")),
            csv_values(index.get("node_types")),
            csv_values(index.get("fields"))
        ));
    }
    if let Some(context_profiles) = payload
        .get("context_profiles")
        .and_then(serde_json::Value::as_object)
    {
        for (name, profile) in context_profiles {
            let relations = csv_values(profile.get("relations"));
            if relations.is_empty() {
                lines.push(format!("profile {}", block_value(name)));
            } else {
                lines.push(format!(
                    "profile {} relations={relations}",
                    block_value(name)
                ));
            }
        }
    }
    format!("{}\n", lines.join("\n"))
}

fn serialize_query_helpers_block(payload: &serde_json::Value) -> String {
    let helpers = value_array(payload, "query_helpers");
    let mut lines = vec![format!("query_helpers count={}", helpers.len())];
    for helper in helpers {
        append_query_spec_lines(&mut lines, helper, "");
    }
    format!("{}\n", lines.join("\n"))
}

fn serialize_architecture_queries_block(payload: &serde_json::Value) -> String {
    let groups = value_array(payload, "groups");
    let mut lines = vec![format!(
        "architecture_queries workflow={} execution_tool={} groups={}",
        block_value(value_str(payload, "workflow")),
        block_value(value_str(payload, "execution_tool")),
        groups.len()
    )];
    let recommended_order = csv_values(payload.get("recommended_order"));
    if !recommended_order.is_empty() {
        lines.push(format!("recommended_order {recommended_order}"));
    }
    for group in groups {
        lines.push(format!(
            "group {} goal={}",
            block_value(value_str(group, "name")),
            block_value(value_str(group, "goal"))
        ));
        for query in value_array(group, "queries") {
            append_query_spec_lines(&mut lines, query, "  ");
        }
    }
    format!("{}\n", lines.join("\n"))
}

fn serialize_health_block(payload: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "health ok={} database_exists={} manifest_exists={} graph_readable={} total_nodes={}",
        value_bool(payload, "ok"),
        value_bool(payload, "database_exists"),
        value_bool(payload, "manifest_exists"),
        value_bool(payload, "graph_readable"),
        payload
            .get("total_nodes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
    )];
    for key in ["repo_root", "database_path", "manifest_path"] {
        lines.push(format!("{key} {}", block_value(value_str(payload, key))));
    }
    if let Some(error) = payload.get("error").and_then(serde_json::Value::as_str) {
        lines.push(format!("error {}", block_value(error)));
    }
    format!("{}\n", lines.join("\n"))
}

fn serialize_search_block(payload: &serde_json::Value) -> String {
    let results = value_array(payload, "results");
    let mut lines = vec![format!("q {}", block_value(value_str(payload, "query")))];
    let mut current_path: Option<String> = None;
    for result in results {
        let result_path = value_str(result, "path").to_string();
        if current_path.as_deref() != Some(result_path.as_str()) {
            if lines.len() > 1 {
                lines.push(String::new());
            }
            lines.push(format!("file path {}", block_value(&result_path)));
            current_path = Some(result_path);
        }
        let mut result_parts = vec![
            format!("- {}", value_str(result, "type")),
            block_value(value_str(result, "label")),
            block_span(result.get("span")),
        ];
        if let Some(rank_score) = result.get("rank_score").and_then(serde_json::Value::as_f64) {
            result_parts.push(format!("rank_score={rank_score:.2}"));
        }
        if let Some(id) = result.get("id").and_then(serde_json::Value::as_str) {
            result_parts.push(format!("id={}", block_value(id)));
        }
        let summary = value_str(result, "summary");
        if !summary.is_empty() && summary != value_str(result, "label") {
            result_parts.push(format!("summary={}", block_value(summary)));
        }
        lines.push(result_parts.join(" "));
    }
    format!("{}\n", lines.join("\n"))
}

fn serialize_query_block(payload: &serde_json::Value) -> String {
    let rows = value_array(payload, "rows");
    let columns = query_columns(value_str(payload, "statement"));
    let mut lines = vec![format!(
        "query rows={} truncated={}",
        payload
            .get("row_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(rows.len() as u64),
        value_bool(payload, "truncated")
    )];
    lines.push(format!(
        "statement {}",
        block_value(value_str(payload, "statement"))
    ));
    if !columns.is_empty() {
        lines.push(format!("columns {}", columns.join(",")));
    }
    for (index, row) in rows.iter().enumerate() {
        let values = row
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![(*row).clone()]);
        let row_text = if !columns.is_empty() && columns.len() == values.len() {
            columns
                .iter()
                .zip(values.iter())
                .map(|(column, value)| format!("{column}={}", block_json_value(value)))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            values
                .iter()
                .map(block_json_value)
                .collect::<Vec<_>>()
                .join(" ")
        };
        lines.push(
            format!("row {} {}", index + 1, row_text)
                .trim_end()
                .to_string(),
        );
    }
    format!("{}\n", lines.join("\n"))
}

fn query_columns(statement: &str) -> Vec<String> {
    let upper = statement.to_ascii_uppercase();
    let Some(return_index) = upper.find("RETURN") else {
        return Vec::new();
    };
    let after_return = return_index + "RETURN".len();
    let end = ["ORDER BY", "LIMIT"]
        .iter()
        .filter_map(|marker| {
            upper[after_return..]
                .find(marker)
                .map(|index| after_return + index)
        })
        .min()
        .unwrap_or(statement.len());
    split_return_expressions(&statement[after_return..end])
        .into_iter()
        .filter_map(|expression| query_column_label(&expression))
        .collect()
}

fn split_return_expressions(text: &str) -> Vec<String> {
    let mut expressions = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    for character in text.chars() {
        match character {
            '(' => {
                depth += 1;
                current.push(character);
            }
            ')' => {
                depth = (depth - 1).max(0);
                current.push(character);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    expressions.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        expressions.push(trimmed.to_string());
    }
    expressions
}

fn query_column_label(expression: &str) -> Option<String> {
    let parts = expression.split_whitespace().collect::<Vec<_>>();
    for index in 0..parts.len().saturating_sub(1) {
        if parts[index].eq_ignore_ascii_case("AS") && is_identifier(parts[index + 1]) {
            return Some(parts[index + 1].to_string());
        }
    }
    let label = expression.rsplit('.').next()?.trim();
    if is_identifier(label) {
        Some(label.to_string())
    } else {
        None
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn block_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => block_value(value),
        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn serialize_error_block(payload: &serde_json::Value) -> String {
    let error = payload.get("error").unwrap_or(payload);
    format!(
        "error tool={} type={} message={}\n",
        block_value(value_str(error, "tool")),
        block_value(value_str(error, "type")),
        block_value(value_str(error, "message"))
    )
}

fn serialize_context_block(payload: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "context {} id={} profile={}",
        value_str(payload, "node_type"),
        block_value(value_str(payload, "node_id")),
        block_value(value_str(payload, "profile"))
    )];
    let mut current_path: Option<String> = None;
    for context in value_array(payload, "context") {
        let context_path = value_str(context, "path").to_string();
        if current_path.as_deref() != Some(context_path.as_str()) {
            if lines.len() > 1 {
                lines.push(String::new());
            }
            lines.push(format!("file path {}", block_value(&context_path)));
            current_path = Some(context_path);
        }
        let mut parts = vec![
            value_str(context, "direction").to_string(),
            value_str(context, "relation").to_string(),
            value_str(context, "type").to_string(),
            block_value(value_str(context, "label")),
            block_span(context.get("span")),
        ];
        let summary = value_str(context, "summary");
        if !summary.is_empty() && summary != value_str(context, "label") {
            parts.push(format!("summary={}", block_value(summary)));
        }
        lines.push(parts.join(" ").trim_end().to_string());
    }
    format!("{}\n", lines.join("\n"))
}

fn block_span(value: Option<&serde_json::Value>) -> String {
    let Some(span) = value.and_then(serde_json::Value::as_object) else {
        return String::new();
    };
    let start = span
        .get("line_start")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default();
    let end = span
        .get("line_end")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(start);
    if start <= 0 && end <= 0 {
        String::new()
    } else {
        format!("L{start}-L{end}")
    }
}

fn append_query_spec_lines(lines: &mut Vec<String>, query: &serde_json::Value, indent: &str) {
    lines.push(format!(
        "{indent}query {} description={}",
        block_value(value_str(query, "name")),
        block_value(value_str(query, "description"))
    ));
    let parameters = csv_values(query.get("parameters"));
    if !parameters.is_empty() {
        lines.push(format!("{indent}parameters {parameters}"));
    }
    let returns = csv_values(query.get("returns"));
    if !returns.is_empty() {
        lines.push(format!("{indent}returns {returns}"));
    }
    if let Some(statement) = query
        .get("statement")
        .or_else(|| query.get("query"))
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("{indent}statement {}", block_value(statement)));
    }
}

fn value_array<'a>(payload: &'a serde_json::Value, key: &str) -> &'a [serde_json::Value] {
    payload
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn value_str<'a>(payload: &'a serde_json::Value, key: &str) -> &'a str {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn value_bool(payload: &serde_json::Value, key: &str) -> bool {
    payload
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn csv_names(values: &[serde_json::Value]) -> String {
    values
        .iter()
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(block_value)
        .collect::<Vec<_>>()
        .join(",")
}

fn csv_values(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(block_value)
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

fn block_value(value: &str) -> String {
    if value.is_empty() {
        "\"\"".to_string()
    } else if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/' | ':')
    }) {
        value.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    }
}

fn top_level_help() -> &'static str {
    "codebase-graph native CLI\n\nUSAGE:\n  codebase-graph <command> [options]\n\nCOMMANDS:\n  setup                       Materialize graph state and write .codebaseGraph/config.json\n  materialize                 Materialize a graph through the Rust native engine\n  plan                        Preview files that would rebuild, delete, skip, or ignore\n  watch                       Watch for file changes and refresh after a debounce window\n  graph-health                Check whether the native graph database is readable\n  graph-schema                Return ontology schema, indexes, profiles, and helpers\n  graph-query-helpers         Return named read-only graph query helpers\n  graph-architecture-queries  Return the architecture-discovery query catalog\n  graph-search, search        Search the code graph with compact context\n  graph-context, context      Return compact graph context\n  graph-query                 Execute a restricted read-only graph query\n  mcp                         Serve codebaseGraph MCP over stdio or HTTP\n\nRun `codebase-graph <command> --help` for command options."
}

fn mcp_help() -> &'static str {
    "codebase-graph mcp\n\nUSAGE:\n  codebase-graph mcp install [--client <client>] [--scope <scope>] [--config-path <path>] [--client-config-path <path>] [--dry-run] [--json]\n  codebase-graph mcp serve [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>]\n  codebase-graph mcp http [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--host <host>] [--port <port>] [--path <path>] [--allow-remote] [--auth-token <token>|--auth-token-env <name>]\n\nOPTIONS:\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --host <host>             HTTP bind host; defaults to 127.0.0.1\n  --port <port>             HTTP bind port; defaults to 8765\n  --path <path>             HTTP endpoint path; defaults to /mcp\n  --allow-remote            Permit non-local HTTP bind when an auth token is supplied\n  --auth-token <token>      Bearer token required for HTTP requests\n  --auth-token-env <name>   Environment variable containing the bearer token"
}

fn mcp_install_help() -> &'static str {
    "codebase-graph mcp install\n\nUSAGE:\n  codebase-graph mcp install [--client <client>] [--scope local|user|project] [--name <name>] [--config-path <path>] [--client-config-path <path>] [--repo-root <path>] [--dry-run] [--verify] [--json]\n\nOPTIONS:\n  --client <client>             codex, claude, claude-project, lmstudio, github-copilot, hermes, openclaw, generic, copilot-studio, microsoft-copilot, or all\n  --scope <scope>               local, user, or project; defaults to local\n  --name <name>                 MCP server name; defaults to codebase_graph_<repo>\n  --config-path <path>          Path to .codebaseGraph/config.json\n  --client-config-path <path>   Override the target MCP client config path\n  --repo-root <path>            Repository root used to find .codebaseGraph/config.json\n  --dry-run                     Show install action without writing files or invoking CLIs\n  --verify                      Accepted for compatibility\n  --json                        Emit JSON output"
}

fn materialize_help() -> &'static str {
    "codebase-graph materialize\n\nUSAGE:\n  codebase-graph materialize [--source-root <path>|--repo-root <path>] [--db <path>] [--manifest <path>] [--mode full|changed] [--json]\n  codebase-graph materialize --native-request <path> [--manifest <path>] [--json]\n\nOPTIONS:\n  --source-root <path>      Repository or source root to scan\n  --repo-root <path>        Alias for --source-root\n  --db <path>               Ladybug database path; defaults under .codebaseGraph\n  --manifest <path>         Manifest path; defaults under .codebaseGraph\n  --mode <mode>             full or changed; defaults to changed\n  --no-git                  Disable Git file discovery and scan the filesystem\n  --git-diff                Materialize files from git diff plus untracked files\n  --git-base <rev>          Revision used by --git-diff; defaults to HEAD\n  --include <glob>          Include only paths matching the glob; repeatable\n  --exclude <glob>          Exclude paths matching the glob; repeatable\n  --parallel                Parse independent files concurrently\n  --single-thread           Force single-thread parsing\n  --progress                Include progress events in JSON output\n  --no-fts                  Skip FTS extension loading and index creation\n  --no-semantic-enrichment  Skip semantic enrichment\n  --semantic-provider-mode  local_only only; provider-backed modes are not supported by Rust-only production\n  --native-request <path>   JSON NativeSyntaxMaterializationRequest payload\n  --json                    Emit JSON output"
}

fn plan_help() -> &'static str {
    "codebase-graph plan\n\nUSAGE:\n  codebase-graph plan [--source-root <path>|--repo-root <path>] [--manifest <path>] [--mode full|changed] [--json]\n\nOPTIONS:\n  --source-root <path>      Repository or source root to scan\n  --repo-root <path>        Alias for --source-root\n  --manifest <path>         Manifest path; defaults under .codebaseGraph\n  --mode <mode>             full or changed; defaults to changed\n  --no-git                  Disable Git file discovery and scan the filesystem\n  --git-diff                Plan files from git diff plus untracked files\n  --git-base <rev>          Revision used by --git-diff; defaults to HEAD\n  --include <glob>          Include only paths matching the glob; repeatable\n  --exclude <glob>          Exclude paths matching the glob; repeatable\n  --native-request <path>   JSON NativeSyntaxMaterializationRequest payload\n  --json                    Emit JSON output"
}

fn watch_help() -> &'static str {
    "codebase-graph watch\n\nUSAGE:\n  codebase-graph watch [--source-root <path>|--repo-root <path>] [--mode full|changed] [--watch-backend auto|native|poll] [--poll-ms <n>] [--debounce-ms <n>]\n\nOPTIONS:\n  --source-root <path>      Repository or source root to watch recursively\n  --repo-root <path>        Alias for --source-root\n  --mode <mode>             full or changed; defaults to changed\n  --watch-backend <backend> auto, native, or poll; defaults to auto\n  --poll-ms <n>             Poll interval for poll backend or auto fallback; defaults to 500\n  --debounce-ms <n>         Quiet-window debounce interval in milliseconds; defaults to 250\n  --max-iterations <n>      Stop after n refreshes, useful for tests\n  --once                    Run one refresh immediately and exit\n  --no-git                  Disable Git file discovery and scan the filesystem\n  --git-diff                Refresh files from git diff plus untracked files\n  --git-base <rev>          Revision used by --git-diff; defaults to HEAD\n  --include <glob>          Include only paths matching the glob; repeatable\n  --exclude <glob>          Exclude paths matching the glob; repeatable\n  --parallel                Parse independent files concurrently\n  --progress                Include progress events in JSON output"
}

fn setup_help() -> &'static str {
    "codebase-graph setup\n\nUSAGE:\n  codebase-graph setup [--repo-root <path>] [--mode full|changed] [--mcp-client <client>] [--mcp-config-path <path>] [--skip-mcp-config] [--dry-run] [--instructions-target auto|agents|claude|skip] [--json]\n\nOPTIONS:\n  --repo-root <path>          Repository root to initialize\n  --mode <mode>               full or changed; defaults to changed\n  --mcp-client <client>       codex, claude, claude-project, lmstudio, github-copilot, hermes, openclaw, generic, copilot-studio, microsoft-copilot, or none\n  --mcp-config-path <path>    Override MCP client config path\n  --skip-mcp-config           Do not write MCP client config\n  --dry-run                   Report setup changes without writing repo or client state\n  --instructions-target <t>   auto, agents, claude, or skip\n  --no-fts                    Skip FTS extension loading and index creation\n  --no-semantic-enrichment    Skip semantic enrichment\n  --semantic-provider-mode    local_only only; provider-backed modes are not supported by Rust-only production\n  --json                      Emit JSON output"
}

fn graph_health_help() -> &'static str {
    "codebase-graph graph-health\n\nUSAGE:\n  codebase-graph graph-health [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--json]\n\nOPTIONS:\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --json                    Emit JSON output"
}

fn graph_schema_help() -> &'static str {
    "codebase-graph graph-schema\n\nUSAGE:\n  codebase-graph graph-schema [--format json|block] [--json] [--pretty]\n\nOPTIONS:\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output\n  --pretty                  Pretty-print JSON output"
}

fn graph_query_helpers_help() -> &'static str {
    "codebase-graph graph-query-helpers\n\nUSAGE:\n  codebase-graph graph-query-helpers [--format json|block] [--json] [--pretty]\n\nOPTIONS:\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output\n  --pretty                  Pretty-print JSON output"
}

fn graph_architecture_queries_help() -> &'static str {
    "codebase-graph graph-architecture-queries\n\nUSAGE:\n  codebase-graph graph-architecture-queries [--group <name>] [--format json|block] [--json] [--pretty]\n\nOPTIONS:\n  --group <name>            Optional architecture query group to return\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output\n  --pretty                  Pretty-print JSON output"
}

fn graph_search_help() -> &'static str {
    "codebase-graph graph-search\n\nUSAGE:\n  codebase-graph graph-search <query> [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--limit <n>] [--profile <name>] [--detail standard|slim] [--format json|block] [--json]\n\nOPTIONS:\n  <query>                   Search query\n  --limit <n>               Maximum search hits; defaults to 3\n  --profile <name>          Context profile label; defaults to brief\n  --budget <n>              Context budget retained in output payload; defaults to 600\n  --context-limit <n>       Context item limit retained for compatibility\n  --detail <level>          standard or slim; defaults to standard\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output"
}

fn graph_context_help() -> &'static str {
    "codebase-graph graph-context\n\nUSAGE:\n  codebase-graph graph-context [query] [--node-id <id> --node-type <type>] [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--limit <n>] [--context-limit <n>] [--profile <name>] [--detail standard|slim] [--format json|block] [--json]\n\nOPTIONS:\n  [query]                   Search query used when explicit node lookup is not supplied\n  --node-id <id>            Explicit graph node id\n  --node-type <type>        Explicit graph node type\n  --limit <n>               Maximum search hits in query mode; defaults to 3\n  --context-limit <n>       Maximum explicit context rows; defaults to 3\n  --profile <name>          Context profile label; defaults to brief\n  --budget <n>              Context budget retained in output payload; defaults to 600\n  --detail <level>          standard or slim; defaults to standard\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output"
}

fn metadata_help(command_name: &str) -> &'static str {
    match command_name {
        "graph-schema" => graph_schema_help(),
        "graph-query-helpers" => graph_query_helpers_help(),
        "graph-architecture-queries" => graph_architecture_queries_help(),
        "graph-search" => graph_search_help(),
        "graph-context" => graph_context_help(),
        _ => "codebase-graph metadata command",
    }
}

fn graph_query_help() -> &'static str {
    "codebase-graph graph-query\n\nUSAGE:\n  codebase-graph graph-query <statement> [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--limit <rows>] [--parameters <json>] [--json]\n\nOPTIONS:\n  <statement>               Restricted read-only Cypher statement\n  --parameters <json>       JSON object with named query parameters\n  --limit <rows>            Maximum rows to return; defaults to 100 and caps at 1000\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --json                    Emit JSON output"
}

#[derive(Debug)]
struct GraphStatePaths {
    repo_name: String,
    state_dir: PathBuf,
    db_path: PathBuf,
    manifest_path: PathBuf,
    config_path: PathBuf,
}

impl GraphStatePaths {
    fn derive(repo_root: &Path) -> Self {
        let repo_name = safe_name(
            repo_root
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("repository"),
        );
        let state_dir = repo_root.join(".codebaseGraph");
        Self {
            db_path: state_dir.join(format!("{repo_name}_graph.ldb")),
            manifest_path: state_dir.join("manifest.json"),
            config_path: state_dir.join("config.json"),
            state_dir,
            repo_name,
        }
    }
}

fn safe_name(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = normalized.trim_matches(['.', '_', '-']);
    if trimmed.is_empty() {
        "repository".to_string()
    } else {
        trimmed.to_string()
    }
}

fn schema_statements_from_copy_statements(
    include_fts: bool,
    copy_statements: &[String],
) -> Vec<String> {
    let tables = copy_tables(copy_statements);
    let relation_names = relation_names(&tables);
    let mut node_tables: Vec<String> = tables
        .iter()
        .filter(|table| {
            !table.starts_with("FROM_")
                && !table.starts_with("TO_")
                && !relation_names.contains(*table)
        })
        .cloned()
        .collect();
    let mut relation_tables: Vec<String> = relation_names.into_iter().collect();
    node_tables.sort();
    relation_tables.sort();

    let mut statements = vec!["INSTALL json".to_string(), "LOAD json".to_string()];
    if include_fts {
        statements.extend(["INSTALL fts".to_string(), "LOAD fts".to_string()]);
    }
    statements.extend(
        node_tables
            .iter()
            .map(|table| node_table_sql(table, node_fields(table))),
    );
    statements.extend(
        relation_tables
            .iter()
            .map(|table| node_table_sql(table, edge_fields())),
    );
    for relation in &relation_tables {
        statements.push(relation_table_sql(
            &format!("FROM_{relation}"),
            &node_tables,
            &[relation.to_string()],
            "source",
        ));
        statements.push(relation_table_sql(
            &format!("TO_{relation}"),
            &[relation.to_string()],
            &node_tables,
            "target",
        ));
    }
    if include_fts {
        statements.extend(fts_index_statements(&node_tables));
    }
    statements
}

fn fts_index_statements(node_tables: &[String]) -> Vec<String> {
    let Ok(schema) = metadata_payload(GRAPH_SCHEMA_JSON) else {
        return Vec::new();
    };
    let present_tables: BTreeSet<&str> = node_tables.iter().map(String::as_str).collect();
    let mut statements = Vec::new();
    for index in value_array(&schema, "search_indexes") {
        let index_name = value_str(index, "name");
        let fields = index
            .get("fields")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(|field| format!("'{}'", cypher_single_quoted(field)))
            .collect::<Vec<_>>()
            .join(", ");
        for node_type in index
            .get("node_types")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|node_type| present_tables.contains(*node_type))
        {
            statements.push(format!(
                "CALL CREATE_FTS_INDEX('{}', '{}_{}', [{}])",
                cypher_single_quoted(node_type),
                cypher_single_quoted(index_name),
                cypher_single_quoted(node_type),
                fields
            ));
        }
    }
    statements
}

fn copy_tables(copy_statements: &[String]) -> BTreeSet<String> {
    copy_statements
        .iter()
        .filter_map(|statement| {
            let start = statement.find('`')?;
            let rest = &statement[start + 1..];
            let end = rest.find('`')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

fn relation_names(tables: &BTreeSet<String>) -> BTreeSet<String> {
    let mut relations = BTreeSet::new();
    for table in tables {
        if let Some(name) = table.strip_prefix("FROM_") {
            relations.insert(name.to_string());
        }
        if let Some(name) = table.strip_prefix("TO_") {
            relations.insert(name.to_string());
        }
    }
    relations
}

fn node_table_sql(table: &str, fields: Vec<(&'static str, &'static str)>) -> String {
    let columns: Vec<String> = fields
        .into_iter()
        .map(|(name, value_type)| {
            let primary_key = if name == "id" { " PRIMARY KEY" } else { "" };
            format!("  `{name}` {value_type}{primary_key}")
        })
        .collect();
    format!(
        "CREATE NODE TABLE IF NOT EXISTS `{table}`(\n{}\n)",
        columns.join(",\n")
    )
}

fn relation_table_sql(
    table: &str,
    from_tables: &[String],
    to_tables: &[String],
    role: &str,
) -> String {
    let endpoints: Vec<String> = from_tables
        .iter()
        .flat_map(|from_table| {
            to_tables
                .iter()
                .map(move |to_table| format!("  FROM `{from_table}` TO `{to_table}`"))
        })
        .collect();
    let mut columns = endpoints;
    columns.push(format!("  `role` STRING DEFAULT '{role}'"));
    format!(
        "CREATE REL TABLE IF NOT EXISTS `{table}`(\n{}\n)",
        columns.join(",\n")
    )
}

fn node_fields(table: &str) -> Vec<(&'static str, &'static str)> {
    let mut fields = common_node_fields();
    if table == "File" {
        fields.push(("content_hash", "STRING"));
    }
    fields
}

fn common_node_fields() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "STRING"),
        ("label", "STRING"),
        ("kind", "STRING"),
        ("language", "STRING"),
        ("path", "STRING"),
        ("qualified_name", "STRING"),
        ("scope_id", "STRING"),
        ("line_start", "INT64"),
        ("line_end", "INT64"),
        ("byte_start", "INT64"),
        ("byte_end", "INT64"),
        ("tree_sitter_node_type", "STRING"),
        ("capture_name", "STRING"),
        ("summary", "STRING"),
        ("metadata", "JSON"),
    ]
}

fn edge_fields() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "STRING"),
        ("kind", "STRING"),
        ("source_id", "STRING"),
        ("target_id", "STRING"),
        ("confidence", "DOUBLE"),
        ("line_start", "INT64"),
        ("line_end", "INT64"),
        ("byte_start", "INT64"),
        ("byte_end", "INT64"),
        ("metadata", "JSON"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_top_level_help() {
        let mut output = Vec::new();
        run(["--help"], &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("codebase-graph native CLI"));
        assert!(text.contains("materialize"));
    }

    #[test]
    fn prints_top_level_help_without_args() {
        let mut output = Vec::new();
        run(std::iter::empty::<&str>(), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("codebase-graph native CLI"));
        assert!(text.contains("mcp"));
    }

    #[test]
    fn materialize_help_is_product_command_help() {
        let mut output = Vec::new();
        run(["materialize", "--help"], &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("codebase-graph materialize"));
        assert!(text.contains("--native-request"));
        assert!(text.contains("local_only only"));
        assert!(!text.contains("opportunistic"));
        assert!(!text.contains("provider_first"));
    }

    #[test]
    fn setup_help_is_product_command_help() {
        let mut output = Vec::new();
        run(["setup", "--help"], &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("codebase-graph setup"));
        assert!(text.contains("--mcp-client"));
        assert!(text.contains("local_only only"));
        assert!(!text.contains("opportunistic"));
        assert!(!text.contains("provider_first"));
    }

    #[test]
    fn materialize_rejects_provider_backed_semantic_modes() {
        let args = vec![
            "--semantic-provider-mode".to_string(),
            "provider_first".to_string(),
        ];
        let error = MaterializeOptions::parse(&args).unwrap_err();

        assert!(error.contains("--semantic-provider-mode must be local_only"));
    }

    #[test]
    fn setup_rejects_provider_backed_semantic_modes() {
        let args = vec![
            "--semantic-provider-mode".to_string(),
            "opportunistic".to_string(),
        ];
        let error = SetupOptions::parse(&args).unwrap_err();

        assert!(error.contains("--semantic-provider-mode must be local_only"));
    }

    #[test]
    fn graph_schema_outputs_block_and_json() {
        let mut block = Vec::new();
        run(["graph-schema"], &mut block).unwrap();
        let block_text = String::from_utf8(block).unwrap();
        assert!(block_text.starts_with("schema "));
        assert!(block_text.contains("helpers=8"));
        assert!(!block_text.trim_start().starts_with('{'));

        let mut json_output = Vec::new();
        run(["graph-schema", "--json"], &mut json_output).unwrap();
        let json_text = String::from_utf8(json_output).unwrap();
        assert!(!json_text.contains("\n  "));
        let value: serde_json::Value = serde_json::from_str(&json_text).unwrap();
        assert_eq!(value["ontology"], "code_ontology_v1");
        assert!(value["context_profiles"].is_object());
    }

    #[test]
    fn graph_query_helpers_outputs_helper_catalog() {
        let mut block = Vec::new();
        run(["graph-query-helpers"], &mut block).unwrap();
        let block_text = String::from_utf8(block).unwrap();
        assert!(block_text.starts_with("query_helpers count=8"));
        assert!(block_text.contains("repository_overview"));

        let mut json_output = Vec::new();
        run(["graph-query-helpers", "--json"], &mut json_output).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json_output).unwrap();
        assert!(value["query_helpers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|helper| helper["name"] == "repository_overview"));
    }

    #[test]
    fn graph_architecture_queries_filters_by_group() {
        let mut block = Vec::new();
        run(
            [
                "graph-architecture-queries",
                "--group",
                "overview",
                "--format",
                "block",
            ],
            &mut block,
        )
        .unwrap();
        let block_text = String::from_utf8(block).unwrap();
        assert!(block_text.starts_with("architecture_queries "));
        assert!(block_text.contains("group overview "));
        assert!(!block_text.contains("group public_surface "));

        let mut json_output = Vec::new();
        run(
            [
                "graph-architecture-queries",
                "--group",
                "overview",
                "--json",
            ],
            &mut json_output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json_output).unwrap();
        assert_eq!(value["execution_tool"], "graph_query");
        assert_eq!(value["groups"].as_array().unwrap().len(), 1);
        assert_eq!(value["groups"][0]["name"], "overview");
    }

    #[test]
    fn graph_search_reads_native_fts_indexes() {
        let root = unique_temp_dir("codebase-graph-rust-search");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("service.py"),
            "class SampleService:\n    def helper(self):\n        return 1\n",
        )
        .unwrap();

        setup_search_fixture_repo(&root);

        let mut output = Vec::new();
        run(
            [
                "graph-search",
                "SampleService",
                "--repo-root",
                root.to_str().unwrap(),
                "--limit",
                "3",
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["query"], "SampleService");
        assert!(value["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["label"] == "SampleService"));

        let mut top_output = Vec::new();
        run(
            [
                "graph-search",
                "SampleService",
                "--repo-root",
                root.to_str().unwrap(),
                "--limit",
                "1",
                "--json",
            ],
            &mut top_output,
        )
        .unwrap();
        let top_value: serde_json::Value = serde_json::from_slice(&top_output).unwrap();
        assert_eq!(top_value["results"][0]["type"], "Class");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn setup_indexes_documented_language_defaults() {
        let root = unique_temp_dir("codebase-graph-language-defaults");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub struct RustService;\nimpl RustService { pub fn run(&self) {} }\npub fn rust_helper() { RustService.run(); }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main.go"),
            "package main\nimport \"fmt\"\nfunc GoHelper() { fmt.Println(\"ok\") }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/service.c"),
            "#include <stdio.h>\nstruct CService { int id; };\nint c_helper() { printf(\"ok\"); return 1; }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/service.cpp"),
            "#include <iostream>\nclass CppService { public: void run() { cpp_helper(); } };\nint cpp_helper() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/solver.f90"),
            "module fortran_service\ncontains\nsubroutine fortran_helper()\nuse iso_fortran_env\ncall run()\nend subroutine fortran_helper\nend module fortran_service\n",
        )
        .unwrap();

        let mut setup_output = Vec::new();
        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut setup_output,
        )
        .unwrap();
        let setup_value: serde_json::Value = serde_json::from_slice(&setup_output).unwrap();
        assert_eq!(setup_value["ok"], true);
        let diagnostics = setup_value["diagnostics"].as_array().unwrap();
        assert!(
            diagnostics.iter().all(|diagnostic| !diagnostic
                .as_str()
                .unwrap()
                .contains("Skipped unsupported file: src/")),
            "supported language files should not be skipped: {diagnostics:?}"
        );

        let manifest_text = fs::read_to_string(root.join(".codebaseGraph/manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
        for path in [
            "src/lib.rs",
            "src/main.go",
            "src/service.c",
            "src/service.cpp",
            "src/solver.f90",
        ] {
            assert!(
                manifest["files"].get(path).is_some(),
                "{path} should be materialized"
            );
        }

        for symbol in [
            "RustService",
            "GoHelper",
            "CService",
            "CppService",
            "fortran_service",
        ] {
            let mut search_output = Vec::new();
            run(
                [
                    "graph-search",
                    symbol,
                    "--repo-root",
                    root.to_str().unwrap(),
                    "--limit",
                    "5",
                    "--json",
                ],
                &mut search_output,
            )
            .unwrap();
            let search_value: serde_json::Value = serde_json::from_slice(&search_output).unwrap();
            assert!(
                search_value["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|hit| hit["label"] == symbol),
                "{symbol} should be searchable: {search_value}"
            );
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_search_default_output_is_block() {
        let root = unique_temp_dir("codebase-graph-rust-search-block");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        setup_search_fixture_repo(&root);

        let mut output = Vec::new();
        run(
            [
                "graph-search",
                "helper",
                "--repo-root",
                root.to_str().unwrap(),
            ],
            &mut output,
        )
        .unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("q helper\n"));
        assert!(text.contains("file path "));
        assert!(!text.trim_start().starts_with('{'));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_context_explicit_node_reads_neighbors() {
        let root = unique_temp_dir("codebase-graph-rust-context");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("service.py"),
            "class SampleService:\n    def helper(self):\n        return 1\n",
        )
        .unwrap();

        setup_search_fixture_repo(&root);
        let mut search_output = Vec::new();
        run(
            [
                "graph-search",
                "SampleService",
                "--repo-root",
                root.to_str().unwrap(),
                "--limit",
                "1",
                "--json",
            ],
            &mut search_output,
        )
        .unwrap();
        let search: serde_json::Value = serde_json::from_slice(&search_output).unwrap();
        let hit = &search["results"][0];
        let node_id = hit["id"].as_str().unwrap();
        let node_type = hit["type"].as_str().unwrap();

        let mut output = Vec::new();
        run(
            [
                "graph-context",
                "--node-id",
                node_id,
                "--node-type",
                node_type,
                "--repo-root",
                root.to_str().unwrap(),
                "--profile",
                "brief",
                "--context-limit",
                "5",
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["node_id"], node_id);
        assert_eq!(value["node_type"], node_type);
        assert!(value["context"].as_array().unwrap().iter().any(|context| {
            context["relation"] == "Contains" && context["label"].as_str().unwrap_or("") == "helper"
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_context_query_mode_uses_search_payload() {
        let root = unique_temp_dir("codebase-graph-rust-context-query");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        setup_search_fixture_repo(&root);

        let mut output = Vec::new();
        run(
            [
                "graph-context",
                "helper",
                "--repo-root",
                root.to_str().unwrap(),
                "--limit",
                "1",
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["query"], "helper");
        assert_eq!(value["results"].as_array().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialize_empty_project_from_native_request() {
        let root = unique_temp_dir("codebase-graph-native-cli");
        fs::create_dir_all(&root).unwrap();
        let request_path = root.join("request.json");
        let manifest_path = root.join("manifest.json");
        let db_path = root.join("graph.lbug");
        let staging_dir = root.join("staging");
        fs::write(
            &request_path,
            format!(
                r#"{{
  "source_root": "{root}",
  "repository_label": "empty",
  "mode": "full",
  "parser_version": "native-test",
  "manifest_schema_version": 1,
  "ontology": "code_ontology_v1",
  "previous_manifest": null,
  "profiles": [],
  "excluded_parts": [],
  "db_path": "{db}",
  "include_fts": false,
  "semantic_enrichment": false,
  "semantic_provider_mode": "local_only",
  "schema_statements": [],
  "staging_dir": "{staging}",
  "atomic_rebuild": true,
  "strict": true
}}"#,
                root = json_path(&root),
                db = json_path(&db_path),
                staging = json_path(&staging_dir),
            ),
        )
        .unwrap();

        let mut output = Vec::new();
        run(
            [
                "materialize",
                "--native-request",
                request_path.to_str().unwrap(),
                "--manifest",
                manifest_path.to_str().unwrap(),
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["skipped"], true);
        assert!(manifest_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialize_python_source_root_without_python_request() {
        let root = unique_temp_dir("codebase-graph-rust-source-root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
        let db_path = root.join(".codebaseGraph").join("graph.ldb");
        let manifest_path = root.join(".codebaseGraph").join("manifest.json");

        let mut output = Vec::new();
        run(
            [
                "materialize",
                "--source-root",
                root.to_str().unwrap(),
                "--db",
                db_path.to_str().unwrap(),
                "--manifest",
                manifest_path.to_str().unwrap(),
                "--mode",
                "full",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["database_written"], true);
        assert_eq!(value["skipped"], false);
        assert!(db_path.exists());
        assert!(manifest_path.exists());
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert!(manifest["files"].get("service.py").is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plan_lists_rebuild_delete_skip_and_ignore_paths() {
        let root = unique_temp_dir("codebase-graph-rust-plan");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
        fs::write(root.join("old.py"), "def old():\n    return 1\n").unwrap();
        fs::write(root.join("notes.txt"), "not source\n").unwrap();
        fs::write(root.join("ignored.py"), "def ignored():\n    return 1\n").unwrap();
        fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
        setup_fixture_repo(&root);

        fs::write(root.join("service.py"), "def helper():\n    return 2\n").unwrap();
        fs::write(root.join("new.py"), "def new():\n    return 3\n").unwrap();
        fs::remove_file(root.join("old.py")).unwrap();

        let mut output = Vec::new();
        run(
            [
                "plan",
                "--source-root",
                root.to_str().unwrap(),
                "--no-git",
                "--json",
            ],
            &mut output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_json_array_contains(&value["would_rebuild"], "new.py");
        assert_json_array_contains(&value["would_rebuild"], "service.py");
        assert_json_array_contains(&value["would_delete"], "old.py");
        assert_json_array_contains(&value["would_skip"], "notes.txt");
        assert_json_array_contains(&value["ignored_paths"], "ignored.py");
        assert_eq!(value["database_written"], false);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialize_honors_config_excludes() {
        let root = unique_temp_dir("codebase-graph-rust-config-excludes");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        fs::write(root.join("keep.py"), "def keep():\n    return 1\n").unwrap();
        fs::write(root.join("skip.py"), "def skip():\n    return 1\n").unwrap();
        fs::write(
            root.join(".codebaseGraph").join("config.json"),
            r#"{"materialization":{"exclude":["skip.py"]}}"#,
        )
        .unwrap();

        let mut output = Vec::new();
        run(
            [
                "plan",
                "--source-root",
                root.to_str().unwrap(),
                "--no-git",
                "--json",
            ],
            &mut output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_json_array_contains(&value["would_rebuild"], "keep.py");
        assert_json_array_contains(&value["ignored_paths"], "skip.py");
        assert!(!json_array_contains(&value["would_rebuild"], "skip.py"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_filter_ignores_excluded_parts_and_access_events() {
        let root = unique_temp_dir("codebase-graph-rust-watch-filter-excluded");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        let filter = watch_filter_for(&root, &[]);

        let read_access = watch_test_event(
            &root,
            EventKind::Access(notify::event::AccessKind::Open(
                notify::event::AccessMode::Read,
            )),
            &["src/lib.rs"],
        );
        assert!(filter.relevant_paths(&read_access).is_empty());

        let write_close = watch_test_event(
            &root,
            EventKind::Access(notify::event::AccessKind::Close(
                notify::event::AccessMode::Write,
            )),
            &["src/lib.rs"],
        );
        assert_eq!(
            filter.relevant_paths(&write_close),
            BTreeSet::from(["src/lib.rs".to_string()])
        );

        let backend_other = watch_test_event(&root, EventKind::Other, &["src/lib.rs"]);
        assert_eq!(
            filter.relevant_paths(&backend_other),
            BTreeSet::from(["src/lib.rs".to_string()])
        );

        let state_dir = watch_test_event(
            &root,
            EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            &[".codebaseGraph/manifest.json"],
        );
        assert!(filter.relevant_paths(&state_dir).is_empty());

        let target_dir = watch_test_event(
            &root,
            EventKind::Create(notify::event::CreateKind::File),
            &["target/debug/build.log"],
        );
        assert!(filter.relevant_paths(&target_dir).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_filter_honors_ignore_config_and_cli_excludes() {
        let root = unique_temp_dir("codebase-graph-rust-watch-filter-rules");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
        fs::write(
            root.join(".codebaseGraph").join("config.json"),
            r#"{"materialization":{"exclude":["config_skip.py"]}}"#,
        )
        .unwrap();
        let filter = watch_filter_for(&root, &["--exclude", "cli_skip.py"]);

        for path in ["ignored.py", "config_skip.py", "cli_skip.py"] {
            let event = watch_test_event(
                &root,
                EventKind::Modify(notify::event::ModifyKind::Data(
                    notify::event::DataChange::Content,
                )),
                &[path],
            );
            assert!(filter.relevant_paths(&event).is_empty());
        }

        let event = watch_test_event(
            &root,
            EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            &["keep.py"],
        );
        assert_eq!(
            filter.relevant_paths(&event),
            BTreeSet::from(["keep.py".to_string()])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_filter_keeps_unsupported_files_when_unignored() {
        let root = unique_temp_dir("codebase-graph-rust-watch-filter-unsupported");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let event = watch_test_event(
            &root,
            EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            &["notes.txt"],
        );

        assert_eq!(
            filter.relevant_paths(&event),
            BTreeSet::from(["notes.txt".to_string()])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_filter_accepts_relative_notify_paths() {
        let root = unique_workspace_dir("codebase-graph-rust-watch-relative");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let cwd_relative_path = root
            .strip_prefix(env::current_dir().unwrap())
            .unwrap()
            .join("cwd_relative.py");

        let cwd_relative = Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![cwd_relative_path],
            attrs: Default::default(),
        };
        assert_eq!(
            filter.relevant_paths(&cwd_relative),
            BTreeSet::from(["cwd_relative.py".to_string()])
        );

        let root_relative = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("root_relative.py")],
            attrs: Default::default(),
        };
        assert_eq!(
            filter.relevant_paths(&root_relative),
            BTreeSet::from(["root_relative.py".to_string()])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_batch_coalesces_burst_events_until_quiet() {
        let root = unique_temp_dir("codebase-graph-rust-watch-burst");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (tx, rx) = mpsc::channel();
        tx.send(WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            &["b.py"],
        )))
        .unwrap();
        let mut queued = VecDeque::new();

        let batch = collect_watch_batch(
            WatchMessage::Event(watch_test_event(
                &root,
                EventKind::Create(notify::event::CreateKind::File),
                &["a.py"],
            )),
            &rx,
            &mut queued,
            &filter,
            Duration::from_millis(10),
            Duration::from_secs(1),
        )
        .unwrap()
        .unwrap();

        assert_eq!(batch.event_count, 2);
        assert_eq!(
            batch.paths,
            BTreeSet::from(["a.py".to_string(), "b.py".to_string()])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_batch_flushes_under_sustained_churn() {
        let root = unique_temp_dir("codebase-graph-rust-watch-churn");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (tx, rx) = mpsc::channel();
        let sender_root = root.clone();
        let sender = std::thread::spawn(move || {
            for index in 0..20 {
                tx.send(WatchMessage::Event(watch_test_event(
                    &sender_root,
                    EventKind::Modify(notify::event::ModifyKind::Data(
                        notify::event::DataChange::Content,
                    )),
                    &[&format!("churn-{index}.py")],
                )))
                .unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        });

        let started = Instant::now();
        let mut queued = VecDeque::new();
        let batch = collect_watch_batch(
            WatchMessage::Event(watch_test_event(
                &root,
                EventKind::Create(notify::event::CreateKind::File),
                &["initial.py"],
            )),
            &rx,
            &mut queued,
            &filter,
            Duration::from_millis(100),
            Duration::from_millis(30),
        )
        .unwrap()
        .unwrap();
        sender.join().unwrap();

        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(batch.event_count > 1);
        assert!(batch.paths.contains("initial.py"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_batch_coalesces_queued_events_into_follow_up_refresh() {
        let root = unique_temp_dir("codebase-graph-rust-watch-queued");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (tx, rx) = mpsc::channel();
        for path in ["during-a.py", "during-b.py", "during-c.py"] {
            tx.send(WatchMessage::Event(watch_test_event(
                &root,
                EventKind::Modify(notify::event::ModifyKind::Data(
                    notify::event::DataChange::Content,
                )),
                &[path],
            )))
            .unwrap();
        }
        let mut queued = VecDeque::new();

        let batch = collect_watch_batch(
            rx.recv().unwrap(),
            &rx,
            &mut queued,
            &filter,
            Duration::from_millis(10),
            Duration::from_secs(1),
        )
        .unwrap()
        .unwrap();

        assert_eq!(batch.event_count, 3);
        assert_eq!(
            batch.paths,
            BTreeSet::from([
                "during-a.py".to_string(),
                "during-b.py".to_string(),
                "during-c.py".to_string()
            ])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_batch_propagates_watcher_errors() {
        let root = unique_temp_dir("codebase-graph-rust-watch-error");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (_tx, rx) = mpsc::channel();
        let mut queued = VecDeque::new();
        let error = collect_watch_batch(
            WatchMessage::Error("backend failed".to_string()),
            &rx,
            &mut queued,
            &filter,
            Duration::from_millis(1),
            Duration::from_millis(1),
        )
        .unwrap_err();

        assert!(error.contains("filesystem watcher error: backend failed"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_probe_succeeds_when_notify_event_arrives() {
        let _guard = watch_test_env_lock();
        set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "5");
        let root = unique_temp_dir("codebase-graph-rust-watch-probe-success");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (tx, rx) = mpsc::channel();
        tx.send(WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Create(notify::event::CreateKind::File),
            &[".codebaseGraph/watch-probe/probe-test.tmp"],
        )))
        .unwrap();

        let outcome = probe_native_watcher(&root.canonicalize().unwrap(), &filter, &rx).unwrap();

        assert!(outcome.delivered);
        assert!(outcome.queued.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_probe_falls_back_after_timeout() {
        let _guard = watch_test_env_lock();
        set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "1");
        let root = unique_temp_dir("codebase-graph-rust-watch-probe-timeout");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (_tx, rx) = mpsc::channel();

        let outcome = probe_native_watcher(&root.canonicalize().unwrap(), &filter, &rx).unwrap();

        assert!(!outcome.delivered);
        assert_eq!(outcome.reason.as_deref(), Some("probe_timeout"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_probe_discards_probe_events_and_queues_real_events() {
        let _guard = watch_test_env_lock();
        set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "5");
        let root = unique_temp_dir("codebase-graph-rust-watch-probe-queue");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let (tx, rx) = mpsc::channel();
        tx.send(WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Create(notify::event::CreateKind::File),
            &[".codebaseGraph/watch-probe/probe-test.tmp"],
        )))
        .unwrap();
        tx.send(WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Create(notify::event::CreateKind::File),
            &["src/lib.rs"],
        )))
        .unwrap();

        let outcome = probe_native_watcher(&root.canonicalize().unwrap(), &filter, &rx).unwrap();

        assert!(outcome.delivered);
        assert_eq!(outcome.queued.len(), 1);
        let mut batch = WatchChangeBatch::default();
        apply_watch_message(
            outcome.queued.into_iter().next().unwrap(),
            &filter,
            &mut batch,
        )
        .unwrap();
        assert_eq!(batch.paths, BTreeSet::from(["src/lib.rs".to_string()]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_poll_snapshot_honors_filters() {
        let root = unique_temp_dir("codebase-graph-rust-watch-poll-filter");
        fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("keep.py"), "def keep():\n    return 1\n").unwrap();
        fs::write(root.join("ignored.py"), "def ignored():\n    return 1\n").unwrap();
        fs::write(root.join("config_skip.py"), "def skip():\n    return 1\n").unwrap();
        fs::write(root.join("cli_skip.py"), "def skip():\n    return 1\n").unwrap();
        fs::write(
            root.join("target").join("build.py"),
            "def build():\n    return 1\n",
        )
        .unwrap();
        fs::write(
            root.join(".codebaseGraph").join("internal.py"),
            "def internal():\n    return 1\n",
        )
        .unwrap();
        fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
        fs::write(
            root.join(".codebaseGraph").join("config.json"),
            r#"{"materialization":{"exclude":["config_skip.py"]}}"#,
        )
        .unwrap();
        let filter = watch_filter_for(&root, &["--exclude", "cli_skip.py"]);

        let snapshot = watch_file_snapshot(&filter).unwrap();

        assert!(snapshot.contains_key("keep.py"));
        assert!(!snapshot.contains_key("ignored.py"));
        assert!(!snapshot.contains_key("config_skip.py"));
        assert!(!snapshot.contains_key("cli_skip.py"));
        assert!(!snapshot.contains_key("target/build.py"));
        assert!(!snapshot.contains_key(".codebaseGraph/internal.py"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_poll_snapshot_detects_create_modify_and_delete() {
        let root = unique_temp_dir("codebase-graph-rust-watch-poll-diff");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("modify.py"), "def value():\n    return 1\n").unwrap();
        fs::write(root.join("delete.py"), "def gone():\n    return 1\n").unwrap();
        let filter = watch_filter_for(&root, &[]);
        let previous = watch_file_snapshot(&filter).unwrap();

        fs::write(root.join("modify.py"), "def value():\n    return 100\n").unwrap();
        fs::write(root.join("create.py"), "def new():\n    return 2\n").unwrap();
        fs::remove_file(root.join("delete.py")).unwrap();
        let current = watch_file_snapshot(&filter).unwrap();
        let diff = watch_snapshot_diff(&previous, &current);

        assert_eq!(
            diff,
            BTreeSet::from([
                "create.py".to_string(),
                "delete.py".to_string(),
                "modify.py".to_string()
            ])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_poll_batch_flushes_under_sustained_churn() {
        let root = unique_temp_dir("codebase-graph-rust-watch-poll-churn");
        fs::create_dir_all(&root).unwrap();
        let filter = watch_filter_for(&root, &[]);
        let mut previous = watch_file_snapshot(&filter).unwrap();
        let writer_root = root.clone();
        let writer = std::thread::spawn(move || {
            for index in 0..20 {
                fs::write(
                    writer_root.join(format!("churn-{index}.py")),
                    format!("def churn_{index}():\n    return {index}\n"),
                )
                .unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        });

        let started = Instant::now();
        let batch = collect_poll_batch(
            &filter,
            &mut previous,
            Duration::from_millis(5),
            Duration::from_millis(100),
            Duration::from_millis(30),
        )
        .unwrap();
        writer.join().unwrap();

        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(batch.event_count > 1);
        assert!(!batch.paths.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_poll_backend_refreshes_after_create() {
        let root = unique_temp_dir("codebase-graph-rust-watch-poll-cli");
        fs::create_dir_all(&root).unwrap();
        let watch_root = root.clone();
        let handle = std::thread::spawn(move || {
            let mut output = Vec::new();
            run(
                [
                    "watch",
                    "--source-root",
                    watch_root.to_str().unwrap(),
                    "--watch-backend",
                    "poll",
                    "--poll-ms",
                    "10",
                    "--debounce-ms",
                    "10",
                    "--max-iterations",
                    "1",
                    "--no-git",
                    "--no-fts",
                    "--no-semantic-enrichment",
                ],
                &mut output,
            )
            .unwrap();
            String::from_utf8(output).unwrap()
        });
        std::thread::sleep(Duration::from_millis(30));
        fs::write(root.join("created.py"), "def created():\n    return 1\n").unwrap();
        let text = handle.join().unwrap();

        assert!(text.contains("watch event=refreshed backend=poll"));
        assert!(text.contains("changed_paths=1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_auto_backend_falls_back_to_poll_when_probe_times_out() {
        let _guard = watch_test_env_lock();
        set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "1");
        set_test_env("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE", "1");
        let root = unique_temp_dir("codebase-graph-rust-watch-auto-fallback");
        fs::create_dir_all(&root).unwrap();
        let watch_root = root.clone();
        let handle = std::thread::spawn(move || {
            let mut output = Vec::new();
            run(
                [
                    "watch",
                    "--source-root",
                    watch_root.to_str().unwrap(),
                    "--watch-backend",
                    "auto",
                    "--poll-ms",
                    "10",
                    "--debounce-ms",
                    "10",
                    "--max-iterations",
                    "1",
                    "--no-git",
                    "--no-fts",
                    "--no-semantic-enrichment",
                ],
                &mut output,
            )
            .unwrap();
            String::from_utf8(output).unwrap()
        });
        std::thread::sleep(Duration::from_millis(50));
        fs::write(root.join("created.py"), "def created():\n    return 1\n").unwrap();
        let text = handle.join().unwrap();

        assert!(text.contains("watch event=fallback backend=poll reason=probe_timeout"));
        assert!(text.contains("watch event=refreshed backend=poll"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn watch_backend_parser_accepts_native_without_fallback() {
        let options =
            WatchOptions::parse(&["--watch-backend".to_string(), "native".to_string()]).unwrap();

        assert_eq!(options.backend, WatchBackend::Native);
    }

    #[test]
    fn watch_once_runs_single_refresh_and_exits() {
        let root = unique_temp_dir("codebase-graph-rust-watch-once");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        let mut output = Vec::new();
        run(
            [
                "watch",
                "--source-root",
                root.to_str().unwrap(),
                "--once",
                "--no-git",
                "--no-fts",
                "--no-semantic-enrichment",
            ],
            &mut output,
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();

        assert!(text.contains("watch event=refreshed event_count=0 changed_paths=0"));
        assert!(root.join(".codebaseGraph").join("manifest.json").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn git_diff_plan_scopes_to_changed_paths() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir("codebase-graph-rust-git-diff");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.py"), "def a():\n    return 1\n").unwrap();
        fs::write(root.join("b.py"), "def b():\n    return 1\n").unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&root)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Test",
                "commit",
                "-m",
                "initial",
            ])
            .current_dir(&root)
            .output()
            .unwrap();
        setup_fixture_repo(&root);
        run(
            [
                "materialize",
                "--source-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut Vec::new(),
        )
        .unwrap();

        fs::write(root.join("a.py"), "def a():\n    return 2\n").unwrap();
        let mut output = Vec::new();
        run(
            [
                "plan",
                "--source-root",
                root.to_str().unwrap(),
                "--git-diff",
                "--json",
            ],
            &mut output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_json_array_contains(&value["would_rebuild"], "a.py");
        assert!(!json_array_contains(&value["would_rebuild"], "b.py"));
        assert!(!json_array_contains(&value["would_delete"], "b.py"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_materialize_reports_progress_events() {
        let root = unique_temp_dir("codebase-graph-rust-progress");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.py"), "def a():\n    return 1\n").unwrap();
        fs::write(root.join("b.py"), "def b():\n    return 1\n").unwrap();

        let mut output = Vec::new();
        run(
            [
                "materialize",
                "--source-root",
                root.to_str().unwrap(),
                "--no-git",
                "--parallel",
                "--progress",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["database_written"], true);
        assert!(value["progress_events"].as_array().unwrap().len() >= 2);
        assert_eq!(value["diff"]["added"][0], "a.py");
        assert_eq!(value["diff"]["added"][1], "b.py");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn setup_materializes_graph_and_writes_config() {
        let root = unique_temp_dir("codebase-graph-rust-setup");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        let mut output = Vec::new();
        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["database_written"], true);
        assert!(root.join(".codebaseGraph").join("config.json").exists());
        assert!(root.join(".codebaseGraph").join("manifest.json").exists());
        assert!(PathBuf::from(value["database_path"].as_str().unwrap()).exists());

        let config: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join(".codebaseGraph").join("config.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(config["schema_version"], 1);
        assert_eq!(config["mcp"]["server_name"], "codebase_graph");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mcp_install_writes_generic_client_config() {
        let root = unique_temp_dir("codebase-graph-rust-mcp-install");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut Vec::new(),
        )
        .unwrap();
        let client_config = root.join("client").join("mcp.json");
        let config_path = root.join(".codebaseGraph").join("config.json");
        let mut output = Vec::new();
        run(
            [
                "mcp",
                "install",
                "--client",
                "generic",
                "--config-path",
                config_path.to_str().unwrap(),
                "--client-config-path",
                client_config.to_str().unwrap(),
                "--json",
            ],
            &mut output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["action"], "created");
        assert_eq!(value["method"], "file_adapter");
        let server_name = value["server_name"].as_str().unwrap();
        assert!(server_name.starts_with("codebase_graph_codebase-graph-rust-mcp-install"));
        assert!(client_config.exists());
        let client_payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
        assert_eq!(client_payload["mcpServers"][server_name]["args"][0], "mcp");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mcp_install_reports_copilot_studio_metadata() {
        let root = unique_temp_dir("codebase-graph-rust-copilot-install");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut Vec::new(),
        )
        .unwrap();
        let config_path = root.join(".codebaseGraph").join("config.json");
        let mut output = Vec::new();
        run(
            [
                "mcp",
                "install",
                "--client",
                "copilot-studio",
                "--config-path",
                config_path.to_str().unwrap(),
                "--json",
            ],
            &mut output,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["action"], "reported");
        assert_eq!(value["method"], "manual_metadata");
        assert_eq!(value["payload"]["http"]["url"], "http://127.0.0.1:8765/mcp");
        assert_eq!(value["payload"]["stdio"]["type"], "stdio");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_health_reports_native_database() {
        let root = unique_temp_dir("codebase-graph-rust-health");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut Vec::new(),
        )
        .unwrap();

        let mut output = Vec::new();
        run(
            [
                "graph-health",
                "--repo-root",
                root.to_str().unwrap(),
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["database_exists"], true);
        assert_eq!(value["manifest_exists"], true);
        assert_eq!(value["graph_readable"], true);
        assert!(value["total_nodes"].as_u64().unwrap() > 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_query_reads_native_database() {
        let root = unique_temp_dir("codebase-graph-rust-query");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        setup_fixture_repo(&root);

        let mut output = Vec::new();
        run(
            [
                "graph-query",
                "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1",
                "--repo-root",
                root.to_str().unwrap(),
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            value["statement"],
            "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1"
        );
        assert_eq!(value["row_count"], 1);
        assert_eq!(value["truncated"], false);
        assert!(value["rows"][0][0].as_u64().unwrap() > 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_query_binds_json_parameters() {
        let root = unique_temp_dir("codebase-graph-rust-query-params");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

        setup_fixture_repo(&root);

        let mut output = Vec::new();
        run(
            [
                "graph-query",
                "MATCH (n) WHERE n.path = $path RETURN n.path LIMIT 1",
                "--repo-root",
                root.to_str().unwrap(),
                "--parameters",
                r#"{"path":"service.py"}"#,
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["row_count"], 1);
        assert_eq!(value["rows"][0][0], "service.py");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mcp_graph_query_binds_json_parameters() {
        let root = unique_temp_dir("codebase-graph-rust-mcp-query-params");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
        setup_fixture_repo(&root);

        let options = McpServeOptions {
            repo_root: root.clone(),
            config: None,
            db: None,
            manifest: None,
        };
        let result = mcp_call_tool_result(
            "graph_query",
            &json!({
                "statement": "MATCH (n) WHERE n.path = $path RETURN n.path LIMIT 1",
                "parameters": {"path": "service.py"},
                "output_format": "json",
                "include_structured_content": true,
            }),
            &options,
        )
        .unwrap();

        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["row_count"], 1);
        assert_eq!(result["structuredContent"]["rows"][0][0], "service.py");
        let text_payload: serde_json::Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text_payload["rows"][0][0], "service.py");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_query_reports_truncation_without_materializing_all_rows() {
        let root = unique_temp_dir("codebase-graph-rust-query-limit");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("service.py"),
            "def helper():\n    return 1\n\ndef other():\n    return helper()\n",
        )
        .unwrap();

        setup_fixture_repo(&root);

        let mut output = Vec::new();
        run(
            [
                "graph-query",
                "MATCH (n) RETURN n.id AS id",
                "--repo-root",
                root.to_str().unwrap(),
                "--limit",
                "1",
                "--json",
            ],
            &mut output,
        )
        .unwrap();

        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["row_count"], 1);
        assert_eq!(value["truncated"], true);
        assert!(value["rows"][0][0].as_str().is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn graph_query_rejects_write_like_statements() {
        let error = run(
            ["graph-query", "MATCH (n) DELETE n", "--repo-root", "."],
            &mut Vec::new(),
        )
        .unwrap_err();
        assert!(error.contains("blocked keyword: DELETE"));
    }

    #[test]
    fn mcp_stdio_serves_tools_and_tool_errors() {
        let root = unique_temp_dir("codebase-graph-rust-mcp");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("service.py"),
            "class SampleService:\n    def helper(self):\n        return 1\n",
        )
        .unwrap();
        setup_search_fixture_repo(&root);

        let requests = [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {},
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "graph_health",
                    "arguments": {"include_structured_content": true},
                },
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "graph_search",
                    "arguments": {
                        "query": "SampleService",
                        "limit": 2,
                        "output_format": "json",
                    },
                },
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "graph_query",
                    "arguments": {
                        "statement": "MATCH (n) DELETE n",
                        "include_structured_content": true,
                    },
                },
            }),
        ];
        let input = requests
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        let options = McpServeOptions {
            repo_root: root.clone(),
            config: None,
            db: None,
            manifest: None,
        };
        let mut output = Vec::new();
        serve_mcp_stdio(&options, std::io::Cursor::new(input), &mut output).unwrap();
        let responses: Vec<serde_json::Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(responses.len(), 5);
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
        let tools = responses[1]["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "graph_health"));
        assert!(tools.iter().any(|tool| tool["name"] == "graph_search"));
        assert!(tools.iter().any(|tool| tool["name"] == "graph_query"));
        assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]
            .get("output_format")
            .is_some()));
        assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]
            .get("include_structured_content")
            .is_some()));

        assert_eq!(responses[2]["result"]["isError"], false);
        assert_eq!(responses[2]["result"]["structuredContent"]["ok"], true);
        assert!(responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("health ok=true"));

        let search_text = responses[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        let search_payload: serde_json::Value = serde_json::from_str(search_text).unwrap();
        assert!(search_payload["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["label"] == "SampleService"));

        assert_eq!(responses[4]["result"]["isError"], true);
        assert_eq!(
            responses[4]["result"]["structuredContent"]["error"]["tool"],
            "graph_query"
        );
        assert!(responses[4]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("error tool=graph_query type=ValueError"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mcp_http_rejects_remote_bind_without_auth_token() {
        let error = McpHttpOptions::parse(&[
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--allow-remote".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("auth token"));

        let local_error =
            McpHttpOptions::parse(&["--host".to_string(), "0.0.0.0".to_string()]).unwrap_err();
        assert!(local_error.contains("localhost"));
    }

    #[test]
    fn mcp_http_handles_initialize_list_call_and_protocol_errors() {
        let root = unique_temp_dir("codebase-graph-rust-mcp-http");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("service.py"),
            "class SampleService:\n    def helper(self):\n        return 1\n",
        )
        .unwrap();
        setup_search_fixture_repo(&root);

        let options = test_http_options(root.clone(), None);
        let mut state = McpHttpState::default();
        let initialize = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[("mcp-protocol-version", "2025-11-25")],
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {"protocolVersion": "2025-11-25"},
                }),
            ),
        );
        assert_eq!(initialize.status, 200);
        assert_eq!(
            initialize.payload["result"]["protocolVersion"],
            "2025-11-25"
        );
        let session_id = initialize
            .headers
            .iter()
            .find(|(name, _)| name == "Mcp-Session-Id")
            .map(|(_, value)| value.clone())
            .unwrap();

        let missing_session = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[("mcp-protocol-version", "2025-11-25")],
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
            ),
        );
        assert_eq!(missing_session.status, 400);
        assert_eq!(missing_session.payload["error"]["code"], -32002);

        let listed = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[
                    ("mcp-protocol-version", "2025-11-25"),
                    ("mcp-session-id", session_id.as_str()),
                ],
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}}),
            ),
        );
        assert_eq!(listed.status, 200);
        assert!(listed.payload["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "graph_context"));

        let health = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[
                    ("mcp-protocol-version", "2025-11-25"),
                    ("mcp-session-id", session_id.as_str()),
                ],
                json!({
                    "jsonrpc": "2.0",
                    "id": 4,
                    "method": "tools/call",
                    "params": {"name": "graph_health", "arguments": {}},
                }),
            ),
        );
        assert_eq!(health.status, 200);
        assert_eq!(health.payload["result"]["isError"], false);
        assert!(health.payload["result"]["structuredContent"].is_null());
        assert!(health.payload["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("health ok=true "));

        let protocol_error = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[
                    ("mcp-protocol-version", "1900-01-01"),
                    ("mcp-session-id", session_id.as_str()),
                ],
                json!({"jsonrpc": "2.0", "id": 5, "method": "ping", "params": {}}),
            ),
        );
        assert_eq!(protocol_error.status, 400);
        assert_eq!(protocol_error.payload["error"]["code"], -32602);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mcp_http_enforces_bearer_token_when_configured() {
        let root = unique_temp_dir("codebase-graph-rust-mcp-http-auth");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
        setup_search_fixture_repo(&root);

        let options = test_http_options(root.clone(), Some("secret"));
        let mut state = McpHttpState::default();
        let missing = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[("origin", "http://127.0.0.1:8765")],
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {"protocolVersion": "2025-11-25"},
                }),
            ),
        );
        assert_eq!(missing.status, 401);

        let wrong = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[
                    ("origin", "http://127.0.0.1:8765"),
                    ("authorization", "Bearer wrong"),
                ],
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "initialize",
                    "params": {"protocolVersion": "2025-11-25"},
                }),
            ),
        );
        assert_eq!(wrong.status, 401);

        let ok = handle_mcp_http_request(
            &options,
            &mut state,
            http_json_request(
                "POST",
                "/mcp",
                &[
                    ("origin", "http://127.0.0.1:8765"),
                    ("authorization", "Bearer secret"),
                ],
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "initialize",
                    "params": {"protocolVersion": "2025-11-25"},
                }),
            ),
        );
        assert_eq!(ok.status, 200);
        assert_eq!(ok.payload["result"]["protocolVersion"], "2025-11-25");

        let _ = fs::remove_dir_all(root);
    }

    fn setup_fixture_repo(root: &Path) {
        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-fts",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut Vec::new(),
        )
        .unwrap();
    }

    fn setup_search_fixture_repo(root: &Path) {
        run(
            [
                "setup",
                "--repo-root",
                root.to_str().unwrap(),
                "--mode",
                "full",
                "--mcp-client",
                "none",
                "--no-semantic-enrichment",
                "--json",
            ],
            &mut Vec::new(),
        )
        .unwrap();
    }

    fn test_http_options(root: PathBuf, auth_token: Option<&str>) -> McpHttpOptions {
        McpHttpOptions {
            serve: McpServeOptions {
                repo_root: root,
                config: None,
                db: None,
                manifest: None,
            },
            host: "127.0.0.1".to_string(),
            port: 8765,
            endpoint_path: "/mcp".to_string(),
            allow_remote: false,
            auth_token: auth_token.map(str::to_string),
        }
    }

    fn http_json_request(
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        payload: serde_json::Value,
    ) -> HttpRequest {
        let mut header_map = BTreeMap::new();
        for (name, value) in headers {
            header_map.insert(name.to_ascii_lowercase(), value.to_string());
        }
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            headers: header_map,
            body: serde_json::to_vec(&payload).unwrap(),
            body_too_large: false,
        }
    }

    fn assert_json_array_contains(value: &serde_json::Value, expected: &str) {
        assert!(
            json_array_contains(value, expected),
            "expected {value:?} to contain {expected}"
        );
    }

    fn json_array_contains(value: &serde_json::Value, expected: &str) -> bool {
        value
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.as_str() == Some(expected))
    }

    fn watch_filter_for(root: &Path, extra_args: &[&str]) -> WatchEventFilter {
        fs::create_dir_all(root).unwrap();
        let source_root = root.canonicalize().unwrap();
        let mut args = vec![
            "--source-root".to_string(),
            source_root.to_string_lossy().to_string(),
            "--no-git".to_string(),
        ];
        args.extend(extra_args.iter().map(|arg| arg.to_string()));
        let options = MaterializeOptions::parse_with_command(&args, "watch").unwrap();
        WatchEventFilter::from_options(&source_root, &options).unwrap()
    }

    fn watch_test_event(root: &Path, kind: EventKind, paths: &[&str]) -> Event {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        Event {
            kind,
            paths: paths.iter().map(|path| root.join(path)).collect(),
            attrs: Default::default(),
        }
    }

    struct WatchTestEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for WatchTestEnvGuard {
        fn drop(&mut self) {
            env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS");
            env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE");
        }
    }

    fn watch_test_env_lock() -> WatchTestEnvGuard {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap();
        env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS");
        env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE");
        WatchTestEnvGuard { _guard: guard }
    }

    fn set_test_env(key: &str, value: &str) {
        env::set_var(key, value);
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn unique_workspace_dir(prefix: &str) -> PathBuf {
        env::current_dir().unwrap().join(format!(
            ".{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn json_path(path: &Path) -> String {
        path.to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    }
}
