use super::http::is_local_host;
use super::refresh::McpRefreshState;
use crate::adapters::required_arg;
use crate::api::RepoSelector;
use std::{env, net::TcpListener, path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub(in crate::adapters) struct McpServeOptions {
    pub(in crate::adapters) repo_root: Option<PathBuf>,
    pub(in crate::adapters) config: Option<PathBuf>,
    pub(in crate::adapters) db: Option<PathBuf>,
    pub(in crate::adapters) manifest: Option<PathBuf>,
    pub(in crate::adapters) refresh: Option<Arc<McpRefreshState>>,
}

impl McpServeOptions {
    pub(in crate::adapters) fn parse(args: &[String], help: &str) -> Result<Self, String> {
        let mut options = Self {
            repo_root: None,
            config: None,
            db: None,
            manifest: None,
            refresh: None,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo-root" => {
                    options.repo_root =
                        Some(PathBuf::from(required_arg(args, index, "--repo-root")?));
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
                    return Err(format!("unknown mcp start option: {other}\n\n{help}"));
                }
            }
        }
        Ok(options)
    }

    pub(in crate::adapters) fn repo_selector(&self) -> RepoSelector {
        RepoSelector {
            repo_root: self.repo_root.clone(),
            config_path: self.config.clone(),
            db_path: self.db.clone(),
            manifest_path: self.manifest.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::adapters) struct McpHttpOptions {
    pub(in crate::adapters) serve: McpServeOptions,
    pub(in crate::adapters) host: String,
    pub(in crate::adapters) port: u16,
    pub(in crate::adapters) endpoint_path: String,
    pub(in crate::adapters) allow_remote: bool,
    pub(in crate::adapters) auth_token: Option<String>,
}

impl McpHttpOptions {
    pub(in crate::adapters) fn parse(args: &[String], help: &str) -> Result<Self, String> {
        let mut options = Self {
            serve: McpServeOptions {
                repo_root: None,
                config: None,
                db: None,
                manifest: None,
                refresh: None,
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
                        Some(PathBuf::from(required_arg(args, index, "--repo-root")?));
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
