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
}
