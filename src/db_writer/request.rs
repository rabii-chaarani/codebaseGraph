#[derive(Debug, Clone)]
pub struct LadybugWriteRequest {
    pub db_path: String,
    pub worker_memory_bytes: u64,
    pub buffer_pool_bytes: u64,
    pub max_num_threads: u64,
    pub defer_hash_indexes: bool,
    pub include_fts: bool,
    pub schema_statements: Vec<String>,
    pub copy_statements: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LadybugWriteMetrics {
    pub high_water_bytes: u64,
}
