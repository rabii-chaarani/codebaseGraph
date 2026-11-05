#[derive(Debug, Clone)]
pub(crate) struct StagingResult {
    pub(crate) copy_statements: Vec<String>,
    pub(crate) node_rows: usize,
    pub(crate) edge_rows: usize,
    pub(crate) connector_rows: usize,
    pub(crate) copy_calls: usize,
    pub(crate) unique_node_count: usize,
    pub(crate) unique_edge_count: usize,
    pub(crate) spill_bytes: u64,
    pub(crate) high_water_bytes: usize,
    // Internal lifetime ownership only; this result is consumed after actual
    // native COPY closes, before constructing the public operation response.
    pub(super) _legacy_staging: Option<
        std::sync::Arc<crate::storage::maintenance_legacy_guard::LegacyStagingProducerGuard>,
    >,
}
