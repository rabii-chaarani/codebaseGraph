use super::options::McpServeOptions;
use crate::api::context::GraphRefreshPolicy;
use crate::api::{CodebaseGraphApi, RefreshServiceConfig};

pub(in crate::adapters) fn start_configured_api(
    options: &McpServeOptions,
) -> Result<CodebaseGraphApi, String> {
    let settings = options.runtime_settings()?;
    match settings.refresh_policy {
        GraphRefreshPolicy::Off => Ok(CodebaseGraphApi::new()),
        GraphRefreshPolicy::Leader => Ok(CodebaseGraphApi::with_auto_refresh(
            options.repo_selector(),
            RefreshServiceConfig {
                include_fts: settings.include_fts,
                semantic_enrichment: settings.semantic_enrichment,
                worker_memory_mib: settings.worker_memory_mib,
                rust_memory_mib: settings.rust_memory_mib,
                spill_chunk_mib: settings.spill_chunk_mib,
                max_parallelism: settings.max_parallelism,
            },
        )),
    }
}
