use super::options::McpServeOptions;
use crate::api::context::GraphRefreshPolicy;
use crate::api::{CoordinatorCodebaseGraphApi, RefreshServiceConfig};

pub(in crate::adapters) fn start_configured_api(
    options: &McpServeOptions,
) -> Result<CoordinatorCodebaseGraphApi, String> {
    let settings = options.runtime_settings()?;
    let refresh = match settings.refresh_policy {
        GraphRefreshPolicy::Off => None,
        GraphRefreshPolicy::Leader => Some(RefreshServiceConfig {
            include_fts: settings.include_fts,
            semantic_enrichment: settings.semantic_enrichment,
            worker_memory_mib: settings.worker_memory_mib,
            rust_memory_mib: settings.rust_memory_mib,
            spill_chunk_mib: settings.spill_chunk_mib,
            max_parallelism: settings.max_parallelism,
        }),
    };
    CoordinatorCodebaseGraphApi::connect(options.repo_selector(), refresh)
}
