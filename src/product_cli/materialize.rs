use super::{
    format::{
        block_value, materialize_help, plan_help, schema_statements_from_copy_statements,
        value_array, value_str, watch_help,
    },
    setup::GraphStatePaths,
    watch::scan_source_snapshots,
};
use crate::{
    ladybug_writer::{write_database, LadybugWriteRequest},
    protocol::{
        NativeManifest, NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse,
    },
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

pub(super) fn run_materialize<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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

pub(super) fn materialize(
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

pub(super) fn run_plan<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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
pub(super) fn build_request(
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
pub(super) struct ConfigScanRules {
    pub(super) include_patterns: Vec<String>,
    pub(super) exclude_patterns: Vec<String>,
}

pub(super) fn read_materialization_config_rules(path: &Path) -> Result<ConfigScanRules, String> {
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

pub(super) fn json_string_array(value: &serde_json::Value) -> Vec<String> {
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

pub(super) fn read_codebase_graph_ignore(source_root: &Path) -> Result<Vec<String>, String> {
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

pub(super) fn git_candidate_paths(
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

pub(super) fn git_paths(source_root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
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

pub(super) fn read_manifest(path: &Path) -> Result<NativeManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read manifest {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse manifest {}: {error}", path.display()))
}

pub(super) fn request_manifest_path(options: &MaterializeOptions) -> Option<PathBuf> {
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

pub(super) fn default_excluded_parts() -> Vec<String> {
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

pub(super) fn read_request(path: &Path) -> Result<NativeSyntaxMaterializationRequest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read native request {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse native request {}: {error}", path.display()))
}

pub(super) fn write_manifest(
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
pub(super) struct MaterializeOptions {
    pub(super) native_request: Option<PathBuf>,
    pub(super) source_root: Option<PathBuf>,
    pub(super) db: Option<PathBuf>,
    pub(super) manifest: Option<PathBuf>,
    pub(super) mode: String,
    pub(super) include_fts: bool,
    pub(super) semantic_enrichment: bool,
    pub(super) semantic_provider_mode: String,
    pub(super) use_git: bool,
    pub(super) git_diff: bool,
    pub(super) git_base: Option<String>,
    pub(super) include_patterns: Vec<String>,
    pub(super) exclude_patterns: Vec<String>,
    pub(super) parallel: bool,
    pub(super) progress: bool,
    pub(super) plan_only: bool,
    pub(super) help: bool,
    pub(super) json_output: bool,
}

impl MaterializeOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        Self::parse_with_command(args, "materialize")
    }

    pub(super) fn parse_with_command(args: &[String], command_name: &str) -> Result<Self, String> {
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

pub(super) fn materialize_like_help(command_name: &str) -> &'static str {
    match command_name {
        "plan" => plan_help(),
        "watch" => watch_help(),
        _ => materialize_help(),
    }
}
pub(super) fn materialization_payload(
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

pub(super) fn dry_run_materialization_payload(
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

pub(super) fn serialize_plan_block(payload: &serde_json::Value) -> String {
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

pub(super) fn append_plan_path_lines(
    lines: &mut Vec<String>,
    label: &str,
    paths: &[serde_json::Value],
) {
    for path in paths {
        if let Some(path) = path.as_str() {
            lines.push(format!("{label} {}", block_value(path)));
        }
    }
}
