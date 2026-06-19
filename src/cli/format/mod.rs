mod blocks;
mod help;
mod metadata;
mod schema;

pub(in crate::cli) use blocks::{
    block_value, serialize_architecture_queries_block, serialize_context_block,
    serialize_error_block, serialize_health_block, serialize_query_block,
    serialize_query_helpers_block, serialize_schema_block, serialize_search_block, value_array,
    value_str,
};
pub(in crate::cli) use help::{
    graph_architecture_queries_help, graph_context_help, graph_health_help, graph_query_help,
    graph_query_helpers_help, graph_schema_help, graph_search_help, materialize_help, mcp_help,
    mcp_install_help, metadata_help, plan_help, setup_help, top_level_help, watch_help,
};
pub(in crate::cli) use metadata::{
    filter_architecture_group, metadata_payload, write_metadata_output,
};
pub(in crate::cli) use schema::schema_statements_from_copy_statements;
