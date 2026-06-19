use crate::cli::{
    build::MaterializeOptions, format::setup_help, install::supported_install_clients,
};
use std::path::PathBuf;

#[derive(Debug)]
pub(in crate::cli) struct WatchOptions {
    pub(in crate::cli) materialize: MaterializeOptions,
    pub(in crate::cli) backend: WatchBackend,
    pub(in crate::cli) poll_ms: u64,
    pub(in crate::cli) debounce_ms: u64,
    pub(in crate::cli) max_iterations: Option<usize>,
    pub(in crate::cli) once: bool,
    pub(in crate::cli) help: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::cli) enum WatchBackend {
    Auto,
    Native,
    Poll,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::cli) struct WatchLoopConfig {
    pub(in crate::cli) poll_ms: u64,
    pub(in crate::cli) debounce_ms: u64,
    pub(in crate::cli) max_iterations: Option<usize>,
}

impl WatchBackend {
    pub(in crate::cli) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "native" => Ok(Self::Native),
            "poll" => Ok(Self::Poll),
            _ => Err("--watch-backend must be auto, native, or poll".to_string()),
        }
    }
}

impl WatchOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
pub(in crate::cli) struct SetupOptions {
    pub(in crate::cli) repo_root: PathBuf,
    pub(in crate::cli) mode: String,
    pub(in crate::cli) include_fts: bool,
    pub(in crate::cli) semantic_enrichment: bool,
    pub(in crate::cli) semantic_provider_mode: String,
    pub(in crate::cli) mcp_client: String,
    pub(in crate::cli) mcp_config_path: Option<PathBuf>,
    pub(in crate::cli) skip_mcp_config: bool,
    pub(in crate::cli) dry_run: bool,
    pub(in crate::cli) instructions_target: String,
    pub(in crate::cli) help: bool,
}

impl SetupOptions {
    pub(in crate::cli) fn parse(args: &[String]) -> Result<Self, String> {
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
                    return Err(format!(
                        "unknown install option: {other}\n\n{}",
                        setup_help()
                    ));
                }
            }
        }
        Ok(options)
    }
}
