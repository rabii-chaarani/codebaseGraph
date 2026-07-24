use super::options::McpServeOptions;
use crate::api::refresh::{start_refresh_service, RefreshState};
use std::sync::Arc;

pub(in crate::adapters) type McpRefreshState = RefreshState;

pub(in crate::adapters) fn start_auto_refresh(options: &McpServeOptions) -> Arc<McpRefreshState> {
    start_refresh_service(options.repo_selector())
}
