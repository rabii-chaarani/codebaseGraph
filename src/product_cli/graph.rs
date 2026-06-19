use super::*;

pub(super) fn run_graph_health<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

pub(super) fn run_graph_schema<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = MetadataOutputOptions::parse(args, "graph-schema")?;
    if options.help {
        writeln!(stdout, "{}", graph_schema_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let payload = metadata_payload(GRAPH_SCHEMA_JSON)?;
    write_metadata_output(stdout, &payload, &options, serialize_schema_block)
}

pub(super) fn run_graph_query_helpers<W: Write>(
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

pub(super) fn run_graph_architecture_queries<W: Write>(
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

pub(super) fn run_graph_search<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

pub(super) fn run_graph_context<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

pub(super) fn run_graph_query<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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
#[derive(Debug)]
pub(super) struct HealthOptions {
    pub(super) repo_root: PathBuf,
    pub(super) config: Option<PathBuf>,
    pub(super) db: Option<PathBuf>,
    pub(super) manifest: Option<PathBuf>,
    pub(super) help: bool,
    pub(super) json: bool,
}

impl HealthOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
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
pub(super) struct GraphQueryOptions {
    pub(super) statement: String,
    pub(super) parameters: serde_json::Map<String, serde_json::Value>,
    pub(super) limit: usize,
    pub(super) repo_root: PathBuf,
    pub(super) config: Option<PathBuf>,
    pub(super) db: Option<PathBuf>,
    pub(super) manifest: Option<PathBuf>,
    pub(super) help: bool,
    pub(super) json: bool,
}

impl GraphQueryOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
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
pub(super) struct MetadataOutputOptions {
    pub(super) format: String,
    pub(super) pretty: bool,
    pub(super) help: bool,
}

impl MetadataOutputOptions {
    pub(super) fn parse(args: &[String], command_name: &str) -> Result<Self, String> {
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
pub(super) struct ArchitectureQueryOptions {
    pub(super) output: MetadataOutputOptions,
    pub(super) group: Option<String>,
}

impl ArchitectureQueryOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
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
pub(super) struct GraphSearchOptions {
    pub(super) query: String,
    pub(super) limit: usize,
    pub(super) profile: String,
    pub(super) budget: usize,
    pub(super) context_limit: usize,
    pub(super) max_depth: Option<usize>,
    pub(super) detail: String,
    pub(super) repo_root: PathBuf,
    pub(super) config: Option<PathBuf>,
    pub(super) db: Option<PathBuf>,
    pub(super) manifest: Option<PathBuf>,
    pub(super) output: MetadataOutputOptions,
}

impl GraphSearchOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
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
pub(super) struct GraphContextOptions {
    pub(super) search: GraphSearchOptions,
    pub(super) node_id: Option<String>,
    pub(super) node_type: Option<String>,
}

impl GraphContextOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
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
#[derive(Debug)]
pub(super) struct HealthRuntime {
    pub(super) repo_root: PathBuf,
    pub(super) db_path: PathBuf,
    pub(super) manifest_path: PathBuf,
}

pub(super) fn resolve_health_runtime(options: &HealthOptions) -> Result<HealthRuntime, String> {
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
pub(super) fn count_graph_nodes(db_path: &Path) -> Result<u64, String> {
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

pub(super) fn value_to_u64(value: &Value) -> Option<u64> {
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

pub(super) fn execute_graph_search(
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

pub(super) fn execute_graph_context(
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

pub(super) fn query_relation_neighbors(
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

pub(super) fn relation_type<'a>(
    schema: &'a serde_json::Value,
    relation: &str,
) -> Option<&'a serde_json::Value> {
    value_array(schema, "relation_types")
        .iter()
        .find(|value| value_str(value, "name") == relation)
}

pub(super) fn neighbor_statement(
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

pub(super) fn search_fts_index(
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

pub(super) fn is_missing_search_target_error(error: &str) -> bool {
    error.contains("does not exist")
        || error.contains("doesn't have an index")
        || error.contains("Index not found")
}

#[derive(Debug, Clone)]
pub(super) struct SearchHitRow {
    pub(super) id: String,
    pub(super) node_type: String,
    pub(super) label: String,
    pub(super) qualified_name: String,
    pub(super) path: String,
    pub(super) line_start: Option<i64>,
    pub(super) line_end: Option<i64>,
    pub(super) summary: String,
    pub(super) score: f64,
    pub(super) rank_score: f64,
    pub(super) index_order: usize,
}

impl SearchHitRow {
    pub(super) fn into_json(self, options: &GraphSearchOptions) -> serde_json::Value {
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

pub(super) fn rank_search_hits(hits: &mut [SearchHitRow], query: &str) {
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

pub(super) fn lexical_score(query: &str, hit: &SearchHitRow) -> f64 {
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

pub(super) fn entity_priority_score(node_type: &str) -> f64 {
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

pub(super) fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

pub(super) fn span_json(line_start: Option<i64>, line_end: Option<i64>) -> serde_json::Value {
    let mut span = serde_json::Map::new();
    if let Some(line_start) = line_start {
        span.insert("line_start".to_string(), json!(line_start));
    }
    if let Some(line_end) = line_end {
        span.insert("line_end".to_string(), json!(line_end));
    }
    serde_json::Value::Object(span)
}

pub(super) fn cypher_single_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

pub(super) fn cypher_identifier(value: &str) -> String {
    value.replace('`', "``")
}

pub(super) fn value_to_string(value: Option<&Value>) -> String {
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

pub(super) fn value_to_i64(value: Option<&Value>) -> Option<i64> {
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

pub(super) fn value_to_f64(value: Option<&Value>) -> f64 {
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

pub(super) fn validate_read_only_statement(statement: &str) -> Result<(), String> {
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

pub(super) fn contains_keyword(statement: &str, keyword: &str) -> bool {
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

pub(super) fn is_keyword_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

pub(super) fn execute_read_only_query(
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

pub(super) fn lbug_query_parameters(
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

pub(super) fn json_parameter_to_lbug_value(value: &serde_json::Value) -> Result<Value, String> {
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

pub(super) fn json_safe_value(value: Value) -> serde_json::Value {
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
