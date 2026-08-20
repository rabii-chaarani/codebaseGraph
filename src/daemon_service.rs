//! Process-lifetime service boundary for the repository-scoped MCP daemon.
//!
//! Lifecycle and command adapters depend on this neutral boundary instead of
//! importing each other. The HTTP implementation remains an MCP transport
//! detail behind these re-exports.

pub(crate) use crate::adapters::mcp::{
    repository_fingerprint, serve_mcp_daemon, service_id, stable_daemon_port, start_mcp_daemon,
    status_mcp_daemon, stop_mcp_daemon, verify_daemon_endpoint, McpDaemonOptions, McpDaemonSpec,
    DAEMON_TRANSPORT_VERSION,
};
