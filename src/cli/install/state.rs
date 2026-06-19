use crate::cli::mcp::McpSession;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(in crate::cli) struct McpHttpState {
    pub(in crate::cli) sessions: BTreeMap<String, McpSession>,
    pub(in crate::cli) next_session: u64,
}

impl McpHttpState {
    pub(in crate::cli) fn next_session_id(&mut self) -> String {
        self.next_session += 1;
        format!("native-http-session-{}", self.next_session)
    }
}
