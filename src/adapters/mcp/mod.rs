mod block;
mod http;
mod options;
mod protocol;
mod refresh;
mod state;
mod stdio;
mod tools;

pub(in crate::adapters) use http::serve_mcp_http;
pub(in crate::adapters) use options::{McpHttpOptions, McpServeOptions};
pub(in crate::adapters) use protocol::{McpSession, LATEST_PROTOCOL_VERSION};
pub(in crate::adapters) use stdio::serve_mcp_stdio;

#[cfg(test)]
pub(super) use http::{handle_mcp_http_request, HttpRequest};
#[cfg(test)]
pub(super) use state::McpHttpState;
#[cfg(test)]
pub(super) use tools::mcp_call_tool_result;
