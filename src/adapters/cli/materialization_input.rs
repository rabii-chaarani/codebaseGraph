use crate::adapters::cli::format::{materialize_help, plan_help, watch_help};
use crate::api::{MaterializationRequest, OutputFormat, RepoSelector};
use std::path::PathBuf;

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
    pub(crate) worker_memory_mib: Option<u64>,
    pub(crate) rust_memory_mib: Option<u64>,
    pub(crate) spill_chunk_mib: Option<u64>,
    pub(crate) max_parallelism: Option<usize>,
    pub(crate) progress: bool,
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
            worker_memory_mib: None,
            rust_memory_mib: None,
            spill_chunk_mib: None,
            max_parallelism: None,
            progress: false,
            help: false,
            json_output: false,
        }
    }
}

impl MaterializeOptions {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        Self::parse_with_command(args, "build")
    }

    pub(crate) fn parse_with_command(args: &[String], command_name: &str) -> Result<Self, String> {
        let mut options = Self {
            include_fts: true,
            semantic_enrichment: false,
            use_git: true,
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
                        .ok_or_else(|| "--mode requires a value".to_string())?;
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
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--semantic-provider-mode requires a value".to_string())?;
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
                "--worker-memory-mib" => {
                    options.worker_memory_mib = Some(parse_positive_u64(args, index)?);
                    index += 2;
                }
                "--rust-memory-mib" => {
                    options.rust_memory_mib = Some(parse_positive_u64(args, index)?);
                    index += 2;
                }
                "--spill-chunk-mib" => {
                    options.spill_chunk_mib = Some(parse_positive_u64(args, index)?);
                    index += 2;
                }
                "--max-parallelism" => {
                    let value = parse_positive_u64(args, index)?;
                    options.max_parallelism = Some(usize::try_from(value).map_err(|_| {
                        format!("{} exceeds this platform's usize range", args[index])
                    })?);
                    index += 2;
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

fn parse_positive_u64(args: &[String], option_index: usize) -> Result<u64, String> {
    let option = &args[option_index];
    let value = args
        .get(option_index + 1)
        .ok_or_else(|| format!("{option} requires a positive integer"))?
        .parse::<u64>()
        .map_err(|_| format!("{option} requires a positive integer"))?;
    if value == 0 {
        return Err(format!("{option} requires a positive integer"));
    }
    Ok(value)
}

pub(in crate::adapters::cli) fn materialize_like_help(command_name: &str) -> &'static str {
    match command_name {
        "plan" => plan_help(),
        "watch" => watch_help(),
        _ => materialize_help(),
    }
}

pub(in crate::adapters::cli) fn materialize_request(
    options: &MaterializeOptions,
    output_format: OutputFormat,
) -> MaterializationRequest {
    MaterializationRequest {
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
        semantic_enrichment: false,
        semantic_provider_mode: options.semantic_provider_mode.clone(),
        use_git: options.use_git,
        git_diff: options.git_diff,
        git_base: options.git_base.clone(),
        include_patterns: options.include_patterns.clone(),
        exclude_patterns: options.exclude_patterns.clone(),
        candidate_paths: options.candidate_paths.clone(),
        parallel: options.parallel,
        worker_memory_mib: options.worker_memory_mib,
        rust_memory_mib: options.rust_memory_mib,
        spill_chunk_mib: options.spill_chunk_mib,
        max_parallelism: options.max_parallelism,
        progress: options.progress,
        output_format,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_request_maps_command_options_to_public_contract() {
        let options = MaterializeOptions::parse(&[
            "--repo-root".to_string(),
            "/tmp/project".to_string(),
            "--db".to_string(),
            "/tmp/project/graph.db".to_string(),
            "--mode".to_string(),
            "incremental".to_string(),
            "--no-fts".to_string(),
            "--git-diff".to_string(),
            "--include".to_string(),
            "src/**/*.rs".to_string(),
            "--single-thread".to_string(),
            "--worker-memory-mib".to_string(),
            "640".to_string(),
            "--rust-memory-mib".to_string(),
            "320".to_string(),
            "--spill-chunk-mib".to_string(),
            "16".to_string(),
            "--max-parallelism".to_string(),
            "3".to_string(),
        ])
        .expect("command options should parse");

        let request = materialize_request(&options, OutputFormat::Block);

        assert_eq!(
            request.repo.repo_root.as_deref(),
            Some(std::path::Path::new("/tmp/project"))
        );
        assert_eq!(
            request.repo.db_path.as_deref(),
            Some(std::path::Path::new("/tmp/project/graph.db"))
        );
        assert_eq!(request.mode, "incremental");
        assert!(!request.include_fts);
        assert!(request.git_diff);
        assert_eq!(request.include_patterns, ["src/**/*.rs"]);
        assert!(!request.parallel);
        assert_eq!(request.worker_memory_mib, Some(640));
        assert_eq!(request.rust_memory_mib, Some(320));
        assert_eq!(request.spill_chunk_mib, Some(16));
        assert_eq!(request.max_parallelism, Some(3));
        assert_eq!(request.output_format, OutputFormat::Block);
    }

    #[test]
    fn materialize_memory_overrides_reject_zero_and_non_numeric_values() {
        for args in [
            vec!["--worker-memory-mib".to_string(), "0".to_string()],
            vec!["--max-parallelism".to_string(), "many".to_string()],
        ] {
            let error = MaterializeOptions::parse(&args).unwrap_err();
            assert!(error.contains("requires a positive integer"));
        }
    }
}
