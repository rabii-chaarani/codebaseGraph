use super::manifest::read_manifest;
use crate::cli::format::{materialize_help, plan_help, watch_help};
use crate::cli::setup::GraphStatePaths;
use crate::protocol::NativeSyntaxMaterializationRequest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(in crate::cli) fn build_request(
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
pub(in crate::cli) struct ConfigScanRules {
    pub(in crate::cli) include_patterns: Vec<String>,
    pub(in crate::cli) exclude_patterns: Vec<String>,
}

pub(in crate::cli) fn read_materialization_config_rules(
    path: &Path,
) -> Result<ConfigScanRules, String> {
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

pub(in crate::cli) fn json_string_array(value: &serde_json::Value) -> Vec<String> {
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

pub(in crate::cli) fn read_codebase_graph_ignore(
    source_root: &Path,
) -> Result<Vec<String>, String> {
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

pub(in crate::cli) fn git_candidate_paths(
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

pub(in crate::cli) fn git_paths(source_root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
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

pub(in crate::cli) fn default_excluded_parts() -> Vec<String> {
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
        "dist",
        "node_modules",
        "target",
        "venv",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[derive(Debug, Default)]
pub(in crate::cli) struct MaterializeOptions {
    pub(in crate::cli) native_request: Option<PathBuf>,
    pub(in crate::cli) source_root: Option<PathBuf>,
    pub(in crate::cli) db: Option<PathBuf>,
    pub(in crate::cli) manifest: Option<PathBuf>,
    pub(in crate::cli) mode: String,
    pub(in crate::cli) include_fts: bool,
    pub(in crate::cli) semantic_enrichment: bool,
    pub(in crate::cli) semantic_provider_mode: String,
    pub(in crate::cli) use_git: bool,
    pub(in crate::cli) git_diff: bool,
    pub(in crate::cli) git_base: Option<String>,
    pub(in crate::cli) include_patterns: Vec<String>,
    pub(in crate::cli) exclude_patterns: Vec<String>,
    pub(in crate::cli) parallel: bool,
    pub(in crate::cli) progress: bool,
    pub(in crate::cli) plan_only: bool,
    pub(in crate::cli) help: bool,
    pub(in crate::cli) json_output: bool,
}

impl MaterializeOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
        Self::parse_with_command(args, "materialize")
    }

    pub(in crate::cli) fn parse_with_command(
        args: &[String],
        command_name: &str,
    ) -> Result<Self, String> {
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

pub(in crate::cli) fn materialize_like_help(command_name: &str) -> &'static str {
    match command_name {
        "plan" => plan_help(),
        "watch" => watch_help(),
        _ => materialize_help(),
    }
}
