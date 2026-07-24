use crate::adapters::cli::format::{materialize_help, plan_help, watch_help};
use crate::api::materialization::MaterializeOptions;
use std::path::PathBuf;

impl MaterializeOptions {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        Self::parse_with_command(args, "build")
    }

    pub(crate) fn parse_with_command(args: &[String], command_name: &str) -> Result<Self, String> {
        let mut options = Self {
            include_fts: true,
            semantic_enrichment: true,
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

pub(in crate::adapters::cli) fn materialize_like_help(command_name: &str) -> &'static str {
    match command_name {
        "plan" => plan_help(),
        "watch" => watch_help(),
        _ => materialize_help(),
    }
}
