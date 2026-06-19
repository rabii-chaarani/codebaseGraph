mod commands;
mod http;
mod options;
mod protocol;
mod stdio;
mod tools;

pub(in crate::cli) use commands::run_mcp_command;
pub(in crate::cli) use http::serve_mcp_http;
pub(in crate::cli) use options::{McpHttpOptions, McpServeOptions};
pub(in crate::cli) use protocol::McpSession;
pub(in crate::cli) use stdio::serve_mcp_stdio;

#[cfg(test)]
pub(super) use http::{handle_mcp_http_request, HttpRequest};
#[cfg(test)]
pub(super) use tools::mcp_call_tool_result;
