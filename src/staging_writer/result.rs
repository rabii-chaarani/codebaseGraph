#[derive(Debug, Clone)]
pub(crate) struct StagingResult {
    pub(crate) copy_statements: Vec<String>,
    pub(crate) node_rows: usize,
    pub(crate) edge_rows: usize,
    pub(crate) connector_rows: usize,
    pub(crate) copy_calls: usize,
}
