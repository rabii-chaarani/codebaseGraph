//! Private API execution budget; never part of the public operation arguments.
use crate::api::ApiError;
use std::time::{Duration, Instant};

pub(crate) const MAX_CONNECTIONS: usize = 32;
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(10);
pub(crate) const HOOK_TIMEOUT_HEADER: &str = "x-codebasegraph-timeout-ms";
pub(crate) const MAX_HOOK_TIMEOUT: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExecutionContext {
    pub(crate) deadline: Option<Instant>,
}

impl ExecutionContext {
    pub(crate) fn with_timeout(started: Instant, timeout: Duration) -> Self {
        Self {
            deadline: Some(started.checked_add(timeout).unwrap_or(started)),
        }
    }

    pub(crate) fn remaining(self) -> Result<Option<Duration>, ApiError> {
        match self.deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    Err(Self::expired_error())
                } else {
                    Ok(Some(remaining))
                }
            }
            None => Ok(None),
        }
    }

    pub(crate) fn expired_error() -> ApiError {
        ApiError::new("deadline_exceeded", "request execution deadline expired")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_budget_does_not_restart() {
        let context = ExecutionContext::with_timeout(
            Instant::now() - Duration::from_secs(1),
            Duration::from_millis(900),
        );
        assert_eq!(context.remaining().unwrap_err().code, "deadline_exceeded");
        assert!(ExecutionContext::default().remaining().unwrap().is_none());
    }
}
