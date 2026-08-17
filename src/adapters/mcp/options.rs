use super::http::is_local_host;
use crate::adapters::required_arg;
use crate::api::context::{
    read_selected_install_config, GraphInstallConfig, GraphRefreshBackend, GraphRefreshPolicy,
    DEFAULT_MAX_PARALLELISM, DEFAULT_RUST_MEMORY_MIB, DEFAULT_SPILL_CHUNK_MIB,
    DEFAULT_WORKER_MEMORY_MIB,
};
use crate::api::{CodebaseGraphApi, RepoSelector};
use std::{env, net::TcpListener, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct McpRuntimeSettings {
    pub(crate) refresh_policy: GraphRefreshPolicy,
    pub(crate) refresh_backend: GraphRefreshBackend,
    pub(crate) include_fts: bool,
    pub(crate) semantic_enrichment: bool,
    pub(crate) worker_memory_mib: u64,
    pub(crate) rust_memory_mib: u64,
    pub(crate) spill_chunk_mib: u64,
    pub(crate) max_parallelism: usize,
}

impl Default for McpRuntimeSettings {
    fn default() -> Self {
        Self {
            refresh_policy: GraphRefreshPolicy::Leader,
            refresh_backend: GraphRefreshBackend::Auto,
            include_fts: true,
            semantic_enrichment: false,
            worker_memory_mib: DEFAULT_WORKER_MEMORY_MIB,
            rust_memory_mib: DEFAULT_RUST_MEMORY_MIB,
            spill_chunk_mib: DEFAULT_SPILL_CHUNK_MIB,
            max_parallelism: DEFAULT_MAX_PARALLELISM,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct McpServeOptions {
    pub(in crate::adapters) repo_root: Option<PathBuf>,
    pub(in crate::adapters) config: Option<PathBuf>,
    pub(in crate::adapters) db: Option<PathBuf>,
    pub(in crate::adapters) manifest: Option<PathBuf>,
    pub(in crate::adapters) api: Option<CodebaseGraphApi>,
    pub(in crate::adapters) refresh_policy: Option<GraphRefreshPolicy>,
    pub(in crate::adapters) worker_memory_mib: Option<u64>,
    pub(in crate::adapters) rust_memory_mib: Option<u64>,
    pub(in crate::adapters) spill_chunk_mib: Option<u64>,
    pub(in crate::adapters) max_parallelism: Option<usize>,
}

impl McpServeOptions {
    pub(crate) fn parse(args: &[String], help: &str) -> Result<Self, String> {
        let mut options = Self::empty();
        let mut index = 0;
        while index < args.len() {
            if let Some(next) = options.parse_common_option(args, index)? {
                index = next;
            } else {
                return Err(format!(
                    "unknown mcp start option: {}\n\n{help}",
                    args[index]
                ));
            }
        }
        Ok(options)
    }

    fn empty() -> Self {
        Self {
            repo_root: None,
            config: None,
            db: None,
            manifest: None,
            api: None,
            refresh_policy: None,
            worker_memory_mib: None,
            rust_memory_mib: None,
            spill_chunk_mib: None,
            max_parallelism: None,
        }
    }

    fn parse_common_option(
        &mut self,
        args: &[String],
        index: usize,
    ) -> Result<Option<usize>, String> {
        let next = match args[index].as_str() {
            "--repo-root" => {
                self.repo_root = Some(PathBuf::from(required_arg(args, index, "--repo-root")?));
                index + 2
            }
            "--config" => {
                self.config = Some(PathBuf::from(required_arg(args, index, "--config")?));
                index + 2
            }
            "--db" => {
                self.db = Some(PathBuf::from(required_arg(args, index, "--db")?));
                index + 2
            }
            "--manifest" => {
                self.manifest = Some(PathBuf::from(required_arg(args, index, "--manifest")?));
                index + 2
            }
            "--refresh-policy" => {
                self.refresh_policy = Some(parse_refresh_policy(required_arg(
                    args,
                    index,
                    "--refresh-policy",
                )?)?);
                index + 2
            }
            "--worker-memory-mib" => {
                self.worker_memory_mib = Some(parse_positive_u64(
                    required_arg(args, index, "--worker-memory-mib")?,
                    "--worker-memory-mib",
                )?);
                index + 2
            }
            "--rust-memory-mib" => {
                self.rust_memory_mib = Some(parse_positive_u64(
                    required_arg(args, index, "--rust-memory-mib")?,
                    "--rust-memory-mib",
                )?);
                index + 2
            }
            "--spill-chunk-mib" => {
                self.spill_chunk_mib = Some(parse_positive_u64(
                    required_arg(args, index, "--spill-chunk-mib")?,
                    "--spill-chunk-mib",
                )?);
                index + 2
            }
            "--max-parallelism" => {
                self.max_parallelism = Some(parse_positive_usize(
                    required_arg(args, index, "--max-parallelism")?,
                    "--max-parallelism",
                )?);
                index + 2
            }
            _ => return Ok(None),
        };
        Ok(Some(next))
    }

    pub(in crate::adapters) fn repo_selector(&self) -> RepoSelector {
        RepoSelector {
            repo_root: self.repo_root.clone(),
            config_path: self.config.clone(),
            db_path: self.db.clone(),
            manifest_path: self.manifest.clone(),
        }
    }

    pub(crate) fn runtime_settings(&self) -> Result<McpRuntimeSettings, String> {
        let mut settings = read_selected_install_config(&self.repo_selector())?
            .map(settings_from_install_config)
            .unwrap_or_default();
        if let Some(value) = self.refresh_policy {
            settings.refresh_policy = value;
        }
        if let Some(value) = self.worker_memory_mib {
            settings.worker_memory_mib = value;
        }
        if let Some(value) = self.rust_memory_mib {
            settings.rust_memory_mib = value;
        }
        if let Some(value) = self.spill_chunk_mib {
            settings.spill_chunk_mib = value;
        }
        if let Some(value) = self.max_parallelism {
            settings.max_parallelism = value;
        }
        validate_runtime_settings(settings)
    }
}

fn settings_from_install_config(config: GraphInstallConfig) -> McpRuntimeSettings {
    McpRuntimeSettings {
        refresh_policy: config.refresh.policy,
        refresh_backend: config.refresh.backend,
        include_fts: config.materialization.include_fts,
        semantic_enrichment: false,
        worker_memory_mib: config.materialization.worker_memory_mib,
        rust_memory_mib: config.materialization.rust_memory_mib,
        spill_chunk_mib: config.materialization.spill_chunk_mib,
        max_parallelism: config.materialization.max_parallelism,
    }
}

fn validate_runtime_settings(settings: McpRuntimeSettings) -> Result<McpRuntimeSettings, String> {
    if settings.worker_memory_mib == 0
        || settings.rust_memory_mib == 0
        || settings.spill_chunk_mib == 0
        || settings.max_parallelism == 0
    {
        return Err(
            "MCP materialization memory limits and max parallelism must be positive".into(),
        );
    }
    if settings.rust_memory_mib > settings.worker_memory_mib {
        return Err("rust_memory_mib must not exceed worker_memory_mib".into());
    }
    if settings.spill_chunk_mib > settings.rust_memory_mib {
        return Err("spill_chunk_mib must not exceed rust_memory_mib".into());
    }
    Ok(settings)
}

fn parse_refresh_policy(value: &str) -> Result<GraphRefreshPolicy, String> {
    match value {
        "off" => Ok(GraphRefreshPolicy::Off),
        "leader" => Ok(GraphRefreshPolicy::Leader),
        _ => Err("--refresh-policy must be off or leader".to_string()),
    }
}

fn parse_positive_u64(value: &str, option: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{option} must be a positive integer"))
}

fn parse_positive_usize(value: &str, option: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{option} must be a positive integer"))
}

#[derive(Clone, Debug)]
pub(crate) struct McpHttpOptions {
    pub(in crate::adapters) serve: McpServeOptions,
    pub(in crate::adapters) host: String,
    pub(in crate::adapters) port: u16,
    pub(in crate::adapters) endpoint_path: String,
    pub(in crate::adapters) allow_remote: bool,
    pub(in crate::adapters) auth_token: Option<String>,
}

impl McpHttpOptions {
    pub(crate) fn parse(args: &[String], help: &str) -> Result<Self, String> {
        let mut options = Self {
            serve: McpServeOptions::empty(),
            host: "127.0.0.1".to_string(),
            port: 8765,
            endpoint_path: "/mcp".to_string(),
            allow_remote: false,
            auth_token: None,
        };
        let mut index = 0;
        while index < args.len() {
            if let Some(next) = options.serve.parse_common_option(args, index)? {
                index = next;
                continue;
            }
            match args[index].as_str() {
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
                    return Err(format!("unknown mcp http option: {other}\n\n{help}"));
                }
            }
        }
        options.validate()?;
        Ok(options)
    }

    pub(in crate::adapters) fn validate(&self) -> Result<(), String> {
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

    pub(in crate::adapters) fn bind_listener(&self) -> Result<TcpListener, String> {
        self.validate()?;
        TcpListener::bind((self.host.as_str(), self.port))
            .map_err(|error| format!("failed to bind MCP HTTP server: {error}"))
    }
}
