use super::options::McpServeOptions;
use crate::api::{
    contracts::RepoSelector,
    refresh::{start_refresh_service, RefreshState},
};
use std::sync::Arc;

pub(in crate::cli) type McpRefreshState = RefreshState;

pub(in crate::cli) fn start_auto_refresh(options: &McpServeOptions) -> Arc<McpRefreshState> {
    start_refresh_service(RepoSelector {
        repo_root: options.repo_root.clone(),
        config_path: options.config.clone(),
        db_path: options.db.clone(),
        manifest_path: options.manifest.clone(),
    })
}
