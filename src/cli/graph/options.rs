use crate::cli::{
    format::{graph_health_help, graph_query_help, graph_search_help, metadata_help},
    util::{parse_usize_arg, required_arg},
};
use std::path::PathBuf;

#[derive(Debug)]
pub(in crate::cli) struct HealthOptions {
    pub(in crate::cli) repo_root: PathBuf,
    pub(in crate::cli) config: Option<PathBuf>,
    pub(in crate::cli) db: Option<PathBuf>,
    pub(in crate::cli) manifest: Option<PathBuf>,
    pub(in crate::cli) help: bool,
    pub(in crate::cli) json: bool,
}

impl HealthOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
                        "unknown check-health option: {other}\n\n{}",
                        graph_health_help()
                    ));
                }
            }
        }
        Ok(options)
    }
}

#[derive(Debug)]
pub(in crate::cli) struct GraphQueryOptions {
    pub(in crate::cli) statement: String,
    pub(in crate::cli) parameters: serde_json::Map<String, serde_json::Value>,
    pub(in crate::cli) limit: usize,
    pub(in crate::cli) repo_root: PathBuf,
    pub(in crate::cli) config: Option<PathBuf>,
    pub(in crate::cli) db: Option<PathBuf>,
    pub(in crate::cli) manifest: Option<PathBuf>,
    pub(in crate::cli) help: bool,
    pub(in crate::cli) json: bool,
}

impl GraphQueryOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
pub(in crate::cli) struct MetadataOutputOptions {
    pub(in crate::cli) format: String,
    pub(in crate::cli) pretty: bool,
    pub(in crate::cli) help: bool,
}

impl MetadataOutputOptions {
    pub(in crate::cli) fn parse(args: &[String], command_name: &str) -> Result<Self, String> {
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
pub(in crate::cli) struct ArchitectureQueryOptions {
    pub(in crate::cli) output: MetadataOutputOptions,
    pub(in crate::cli) group: Option<String>,
}

impl ArchitectureQueryOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
            output: MetadataOutputOptions::parse(&metadata_args, "codebase-architecture-queries")?,
            group,
        })
    }
}

#[derive(Debug)]
pub(in crate::cli) struct GraphSearchOptions {
    pub(in crate::cli) query: String,
    pub(in crate::cli) limit: usize,
    pub(in crate::cli) profile: String,
    pub(in crate::cli) budget: usize,
    pub(in crate::cli) context_limit: usize,
    pub(in crate::cli) max_depth: Option<usize>,
    pub(in crate::cli) detail: String,
    pub(in crate::cli) repo_root: PathBuf,
    pub(in crate::cli) config: Option<PathBuf>,
    pub(in crate::cli) db: Option<PathBuf>,
    pub(in crate::cli) manifest: Option<PathBuf>,
    pub(in crate::cli) output: MetadataOutputOptions,
}

impl GraphSearchOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
                        "unknown codebase-search option: {other}\n\n{}",
                        graph_search_help()
                    ));
                }
                value => {
                    if query.is_some() {
                        return Err("codebase-search accepts exactly one query".to_string());
                    }
                    query = Some(value.to_string());
                    index += 1;
                }
            }
        }
        let output = MetadataOutputOptions::parse(&output_args, "codebase-search")?;
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
pub(in crate::cli) struct GraphContextOptions {
    pub(in crate::cli) search: GraphSearchOptions,
    pub(in crate::cli) node_id: Option<String>,
    pub(in crate::cli) node_type: Option<String>,
}

impl GraphContextOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
                "codebase-context explicit lookup requires both --node-id and --node-type"
                    .to_string(),
            );
        }
        Ok(Self {
            search,
            node_id,
            node_type,
        })
    }
}
