use super::McpSession;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(in crate::adapters) struct McpHttpState {
    pub(in crate::adapters) sessions: BTreeMap<String, McpSession>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::adapters) enum SessionIdGenerationError {
    Entropy,
    Collision,
}

impl McpHttpState {
    pub(in crate::adapters) fn next_session_id_with<F, E>(
        &self,
        fill_random: F,
    ) -> Result<String, SessionIdGenerationError>
    where
        F: FnOnce(&mut [u8]) -> Result<(), E>,
    {
        let mut random = [0u8; 32];
        fill_random(&mut random).map_err(|_| SessionIdGenerationError::Entropy)?;

        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut suffix = String::with_capacity(random.len() * 2);
        for byte in random {
            suffix.push(HEX[(byte >> 4) as usize] as char);
            suffix.push(HEX[(byte & 0x0f) as usize] as char);
        }

        let session_id = format!("native-http-session-{suffix}");
        if self.sessions.contains_key(&session_id) {
            return Err(SessionIdGenerationError::Collision);
        }
        Ok(session_id)
    }

    pub(in crate::adapters) fn snapshot_session(&self, session_id: &str) -> Option<Self> {
        self.sessions.get(session_id).cloned().map(|session| Self {
            sessions: BTreeMap::from([(session_id.to_string(), session)]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_session_ids_are_random_ascii_and_unique_across_state_instances() {
        let state = McpHttpState::default();
        let first = state
            .next_session_id_with(|bytes| getrandom::fill(bytes).map_err(|_| ()))
            .expect("secure session ID");
        let second = state
            .next_session_id_with(|bytes| getrandom::fill(bytes).map_err(|_| ()))
            .expect("second secure session ID");
        let other_state = McpHttpState::default();
        let after_restart = other_state
            .next_session_id_with(|bytes| getrandom::fill(bytes).map_err(|_| ()))
            .expect("secure ID from fresh state");

        for id in [&first, &second, &after_restart] {
            assert!(id.starts_with("native-http-session-"));
            assert_eq!(id.len(), "native-http-session-".len() + 64);
            assert!(id.bytes().all(|byte| (0x21..=0x7e).contains(&byte)));
        }
        assert_ne!(first, second);
        assert_ne!(first, after_restart);
        assert_ne!(second, after_restart);
    }

    #[test]
    fn session_snapshot_copies_only_the_requested_session() {
        let mut state = McpHttpState::default();
        state
            .sessions
            .insert("first".to_string(), McpSession::default());
        state
            .sessions
            .insert("second".to_string(), McpSession::default());

        let snapshot = state.snapshot_session("first").expect("known session");

        assert_eq!(snapshot.sessions.len(), 1);
        assert!(snapshot.sessions.contains_key("first"));
        assert!(!snapshot.sessions.contains_key("second"));
    }

    #[test]
    fn entropy_failure_does_not_change_sessions() {
        let mut state = McpHttpState::default();
        state
            .sessions
            .insert("existing".to_string(), McpSession::default());
        let previous = state.sessions.clone();

        let result = state.next_session_id_with(|_| Err::<(), _>(()));

        assert_eq!(result, Err(SessionIdGenerationError::Entropy));
        assert_eq!(state.sessions.len(), previous.len());
        assert!(state.sessions.contains_key("existing"));
    }

    #[test]
    fn generated_session_id_collision_does_not_change_sessions() {
        let mut state = McpHttpState::default();
        let collision_id = format!("native-http-session-{}", "ab".repeat(32));
        let existing = McpSession {
            protocol_version: Some("2025-11-25".to_string()),
            ..McpSession::default()
        };
        state.sessions.insert(collision_id.clone(), existing);

        let result = state.next_session_id_with(|bytes| {
            bytes.fill(0xab);
            Ok::<(), ()>(())
        });

        assert_eq!(result, Err(SessionIdGenerationError::Collision));
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(
            state.sessions[&collision_id].protocol_version.as_deref(),
            Some("2025-11-25")
        );
    }
}
