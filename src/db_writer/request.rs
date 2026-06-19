#[derive(Debug, Clone)]
pub struct LadybugWriteRequest {
    pub db_path: String,
    pub include_fts: bool,
    pub schema_statements: Vec<String>,
    pub replace_database: bool,
    pub delete_statements: Vec<String>,
    pub copy_statements: Vec<String>,
}
