use crate::api::{
    context::{resolve_repository_root, RepoPaths},
    contracts::{MaterializationRequest, OutputFormat, RepoSelector},
};
use crate::protocol::{
    ManifestDiff, ManifestEntry, NativeManifest, NativeSyntaxMaterializationRequest,
    NativeSyntaxMaterializationResponse,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Clone, Debug)]
pub(crate) struct MaterializeOptions {
    pub(crate) native_request: Option<PathBuf>,
    pub(crate) source_root: Option<PathBuf>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) db: Option<PathBuf>,
    pub(crate) manifest: Option<PathBuf>,
    pub(crate) mode: String,
    pub(crate) include_fts: bool,
    pub(crate) semantic_enrichment: bool,
    pub(crate) semantic_provider_mode: String,
    pub(crate) use_git: bool,
    pub(crate) git_diff: bool,
    pub(crate) git_base: Option<String>,
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) candidate_paths: Vec<String>,
    pub(crate) parallel: bool,
    pub(crate) progress: bool,
    pub(crate) plan_only: bool,
    pub(crate) help: bool,
    pub(crate) json_output: bool,
}

impl Default for MaterializeOptions {
    fn default() -> Self {
        Self {
            native_request: None,
            source_root: None,
            config: None,
            db: None,
            manifest: None,
            mode: String::new(),
            include_fts: false,
            semantic_enrichment: false,
            semantic_provider_mode: String::new(),
            use_git: false,
            git_diff: false,
            git_base: None,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            parallel: true,
            progress: false,
            plan_only: false,
            help: false,
            json_output: false,
        }
    }
}

#[derive(Default)]
pub(crate) struct ConfigScanRules {
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
}

pub(crate) fn materialization_request(
    options: &MaterializeOptions,
    output_format: OutputFormat,
) -> Result<MaterializationRequest, String> {
    Ok(MaterializationRequest {
        repo: RepoSelector {
            repo_root: options.source_root.clone(),
            config_path: options.config.clone(),
            db_path: options.db.clone(),
            manifest_path: options.manifest.clone(),
        },
        native_request_path: options.native_request.clone(),
        source_root: options
            .source_root
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        mode: options.mode.clone(),
        include_fts: options.include_fts,
        semantic_enrichment: options.semantic_enrichment,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: options.use_git,
        git_diff: options.git_diff,
        git_base: options.git_base.clone(),
        include_patterns: options.include_patterns.clone(),
        exclude_patterns: options.exclude_patterns.clone(),
        candidate_paths: options.candidate_paths.clone(),
        parallel: options.parallel,
        progress: options.progress,
        output_format,
    })
}

pub(crate) fn execute_materialization(
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
    let manifest_path = request_manifest_path(options);
    execute_materialization_request_with_manifest(manifest_path.as_deref(), request)
}

pub(crate) fn execute_candidate_materialization(
    options: &MaterializeOptions,
    candidate_paths: Vec<String>,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let mut request = build_request(options)?;
    request.candidate_paths = candidate_paths;
    request.atomic_rebuild = false;
    let manifest_path = request_manifest_path(options);
    execute_materialization_request_with_manifest(manifest_path.as_deref(), request)
}

pub(crate) fn execute_materialization_request(
    options: &MaterializeOptions,
    request: NativeSyntaxMaterializationRequest,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let manifest_path = request_manifest_path(options);
    execute_materialization_request_with_manifest(manifest_path.as_deref(), request)
}

pub(crate) fn execute_materialization_request_with_manifest(
    manifest_path: Option<&Path>,
    request: NativeSyntaxMaterializationRequest,
) -> Result<
    (
        NativeSyntaxMaterializationRequest,
        NativeSyntaxMaterializationResponse,
    ),
    String,
> {
    let started = Instant::now();
    let final_request = request;
    let mut response = crate::execute_materialization_pipeline(&final_request)
        .map_err(|error| error.to_string())?;
    response.phase_timings.insert(
        "native_cli_seconds".to_string(),
        started.elapsed().as_secs_f64(),
    );

    if let Some(path) = manifest_path {
        write_manifest(
            path,
            &final_request,
            &response.rebuilt_entries,
            &response.diff,
        )?;
    }

    Ok((final_request, response))
}

pub(crate) fn plan_materialization_payload(
    response: &NativeSyntaxMaterializationResponse,
    mode: &str,
    manifest_path: &Path,
) -> serde_json::Value {
    let rebuilt_paths = response.diff.rebuild_paths();
    let skipped_paths = response
        .snapshots
        .iter()
        .filter(|(_, snapshot)| snapshot.language.is_none())
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let ignored_paths = response
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.strip_prefix("Ignored file: "))
        .map(str::to_string)
        .collect::<Vec<_>>();
    serde_json::json!({
        "mode": mode,
        "scanned": response.snapshots.len(),
        "rebuilt": rebuilt_paths.len(),
        "skipped": skipped_paths.len(),
        "ignored": ignored_paths.len(),
        "deleted": response.diff.deleted.len(),
        "diagnostics": response.diagnostics,
        "manifest_path": manifest_path,
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

pub(crate) fn build_request(
    options: &MaterializeOptions,
) -> Result<NativeSyntaxMaterializationRequest, String> {
    let source_root = resolved_source_root(options)?;
    let paths = RepoPaths::derive(&source_root);
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
    let config_path = options
        .config
        .clone()
        .unwrap_or_else(|| paths.config_path.clone());
    let config_rules = read_materialization_config_rules(&config_path)?;
    let mut include_patterns = config_rules.include_patterns;
    include_patterns.extend(options.include_patterns.clone());
    let mut exclude_patterns = config_rules.exclude_patterns;
    exclude_patterns.extend(options.exclude_patterns.clone());
    let ignore_patterns = read_codebase_graph_ignore(&source_root)?;
    let candidate_paths = if options.candidate_paths.is_empty() {
        git_candidate_paths(&source_root, options)?
    } else {
        normalized_candidate_paths(&options.candidate_paths)
    };
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

pub(crate) fn read_materialization_config_rules(path: &Path) -> Result<ConfigScanRules, String> {
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

pub(crate) fn json_string_array(value: &serde_json::Value) -> Vec<String> {
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

pub(crate) fn read_codebase_graph_ignore(source_root: &Path) -> Result<Vec<String>, String> {
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

pub(crate) fn git_candidate_paths(
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

pub(crate) fn git_paths(source_root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
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

pub(crate) fn normalized_candidate_paths(paths: &[String]) -> Vec<String> {
    let mut paths = paths
        .iter()
        .map(|path| path.trim().trim_start_matches("./").replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

pub(crate) fn default_excluded_parts() -> Vec<String> {
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

pub(crate) fn request_manifest_path(options: &MaterializeOptions) -> Option<PathBuf> {
    if options.native_request.is_some() {
        return options.manifest.clone();
    }
    let source_root = resolved_source_root(options).unwrap_or_else(|_| PathBuf::from("."));
    Some(
        options
            .manifest
            .clone()
            .unwrap_or_else(|| RepoPaths::derive(&source_root).manifest_path),
    )
}

pub(crate) fn read_manifest(path: &Path) -> Result<NativeManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read manifest {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse manifest {}: {error}", path.display()))
}

pub(crate) fn read_request(path: &Path) -> Result<NativeSyntaxMaterializationRequest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read native request {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse native request {}: {error}", path.display()))
}

pub(crate) fn write_manifest(
    path: &Path,
    request: &NativeSyntaxMaterializationRequest,
    rebuilt_entries: &BTreeMap<String, ManifestEntry>,
    diff: &ManifestDiff,
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

fn resolved_source_root(options: &MaterializeOptions) -> Result<PathBuf, String> {
    resolve_repository_root(options.source_root.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn public_materialization_request_preserves_transport_fields_without_preparation() {
        let root = unique_temp_dir("codebase-graph-api-materialization-request");
        fs::create_dir_all(&root).expect("temp root should exist");
        let request_path = root.join("request.json");

        let request = materialization_request(
            &MaterializeOptions {
                native_request: Some(request_path.clone()),
                source_root: Some(root.clone()),
                db: Some(root.join("graph.ldb")),
                manifest: Some(root.join("manifest.json")),
                mode: "changed".to_string(),
                include_fts: true,
                semantic_enrichment: false,
                semantic_provider_mode: "local_only".to_string(),
                use_git: true,
                git_diff: true,
                git_base: Some("origin/main".to_string()),
                include_patterns: vec!["src/**/*.rs".to_string()],
                exclude_patterns: vec!["target/**".to_string()],
                candidate_paths: vec!["src/lib.rs".to_string()],
                parallel: false,
                progress: true,
                ..MaterializeOptions::default()
            },
            OutputFormat::Typed,
        )
        .expect("request should be built");

        assert_eq!(request.native_request_path, Some(request_path));
        assert_eq!(request.repo.repo_root, Some(root.clone()));
        assert_eq!(request.repo.db_path, Some(root.join("graph.ldb")));
        assert_eq!(request.include_patterns, vec!["src/**/*.rs"]);
        assert_eq!(request.exclude_patterns, vec!["target/**"]);
        assert_eq!(request.candidate_paths, vec!["src/lib.rs"]);
        assert_eq!(request.git_base.as_deref(), Some("origin/main"));
        assert_eq!(request.output_format, OutputFormat::Typed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_request_reads_manifest_and_merges_config_rules() {
        let root = unique_temp_dir("codebase-graph-api-build-request");
        let state_dir = root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).expect("state directory should exist");
        fs::write(
            state_dir.join("config.json"),
            serde_json::to_vec(&json!({
                "materialization": {
                    "include": ["src/**/*.rs"],
                    "exclude": ["target/**"]
                }
            }))
            .expect("config should serialize"),
        )
        .expect("config should be written");
        fs::write(
            state_dir.join("manifest.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "ontology": "code_ontology_v1",
                "parser_version": "native-test",
                "files": {}
            }))
            .expect("manifest should serialize"),
        )
        .expect("manifest should be written");
        fs::write(root.join(".codebaseGraphignore"), "build/\n# note\n").expect("ignore file");

        let request = build_request(&MaterializeOptions {
            source_root: Some(root.clone()),
            mode: "full".to_string(),
            include_fts: true,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            use_git: false,
            include_patterns: vec!["tests/**/*.rs".to_string()],
            exclude_patterns: vec!["dist/**".to_string()],
            parallel: true,
            ..MaterializeOptions::default()
        })
        .expect("request should build");

        assert_eq!(
            request.repository_label,
            root.file_name()
                .and_then(|value| value.to_str())
                .expect("temp root should have a file name")
        );
        assert!(request.previous_manifest.is_some());
        assert_eq!(
            request.include_patterns,
            vec!["src/**/*.rs".to_string(), "tests/**/*.rs".to_string()]
        );
        assert_eq!(
            request.exclude_patterns,
            vec!["target/**".to_string(), "dist/**".to_string()]
        );
        assert_eq!(request.ignore_patterns, vec!["build/".to_string()]);
        assert!(request.candidate_paths.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn request_manifest_path_uses_override_for_native_requests_only_when_present() {
        let root = unique_temp_dir("codebase-graph-api-manifest-path");
        fs::create_dir_all(&root).expect("temp root should exist");
        let override_path = root.join("override-manifest.json");

        let native = request_manifest_path(&MaterializeOptions {
            native_request: Some(root.join("request.json")),
            manifest: Some(override_path.clone()),
            ..MaterializeOptions::default()
        });
        assert_eq!(native, Some(override_path.clone()));

        let direct = request_manifest_path(&MaterializeOptions {
            source_root: Some(root.clone()),
            ..MaterializeOptions::default()
        });
        assert_eq!(
            direct,
            Some(
                root.canonicalize()
                    .expect("temp root should canonicalize")
                    .join(".codebaseGraph")
                    .join("manifest.json")
            )
        );
        let _ = fs::remove_dir_all(root);
    }
}
