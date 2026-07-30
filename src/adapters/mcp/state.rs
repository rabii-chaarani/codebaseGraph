use super::McpSession;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(in crate::adapters) struct McpHttpState {
    pub(in crate::adapters) sessions: BTreeMap<String, McpSession>,
    pub(in crate::adapters) next_session: u64,
}

impl McpHttpState {
    pub(in crate::adapters) fn next_session_id(&mut self) -> String {
        self.next_session += 1;
        format!("native-http-session-{}", self.next_session)
    }
}
