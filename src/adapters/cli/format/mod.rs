mod help;

pub(crate) use help::mcp_help;
pub(in crate::adapters::cli) use help::{
    graph_architecture_queries_help, graph_context_help, graph_health_help, graph_query_help,
    graph_query_helpers_help, graph_schema_help, graph_search_help, materialize_help,
    mcp_install_help, metadata_help, plan_help, reinstall_help, setup_help, top_level_help,
    uninstall_help, watch_help,
};
