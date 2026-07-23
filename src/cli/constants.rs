use std::env;

pub(crate) const GRAPH_SCHEMA_JSON: &str = include_str!("../../assets/graph_schema.json");
pub(crate) const QUERY_HELPERS_JSON: &str = include_str!("../../assets/query_helpers.json");
pub(crate) const ARCHITECTURE_QUERIES_JSON: &str =
    include_str!("../../assets/architecture_queries.json");
pub(super) const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
pub(super) const MAX_HTTP_BODY_BYTES: usize = 1_000_000;

pub(super) fn server_command() -> String {
    env::var("CODEBASE_GRAPH_SERVER_COMMAND").unwrap_or_else(|_| "codebase-graph".to_string())
}
