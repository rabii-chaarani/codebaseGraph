use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBudgetExceeded {
    pub phase: String,
    pub limit_bytes: u64,
    pub accounted_bytes: u64,
    pub observed_rss_bytes: u64,
}

impl MemoryBudgetExceeded {
    pub(crate) fn new(
        phase: impl Into<String>,
        limit_bytes: u64,
        accounted_bytes: u64,
        observed_rss_bytes: u64,
    ) -> Self {
        Self {
            phase: phase.into(),
            limit_bytes,
            accounted_bytes,
            observed_rss_bytes,
        }
    }
}

#[derive(Debug)]
pub enum NativeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Database(String),
    InvalidInput(String),
    MemoryBudgetExceeded(MemoryBudgetExceeded),
    Unsupported(String),
}

pub type MaterializationError = NativeError;

impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeError::Io(error) => write!(formatter, "{error}"),
            NativeError::Json(error) => write!(formatter, "{error}"),
            NativeError::Database(message) => write!(formatter, "{message}"),
            NativeError::InvalidInput(message) => write!(formatter, "{message}"),
            NativeError::MemoryBudgetExceeded(error) => write!(
                formatter,
                "{}",
                serde_json::json!({
                    "error": "memory_budget_exceeded",
                    "phase": error.phase,
                    "limit_bytes": error.limit_bytes,
                    "accounted_bytes": error.accounted_bytes,
                    "observed_rss_bytes": error.observed_rss_bytes,
                })
            ),
            NativeError::Unsupported(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<std::io::Error> for NativeError {
    fn from(error: std::io::Error) -> Self {
        NativeError::Io(error)
    }
}

impl From<serde_json::Error> for NativeError {
    fn from(error: serde_json::Error) -> Self {
        NativeError::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_budget_failure_has_a_structured_machine_readable_message() {
        let error =
            NativeError::MemoryBudgetExceeded(MemoryBudgetExceeded::new("parse", 1024, 2048, 512));
        let value: serde_json::Value = serde_json::from_str(&error.to_string()).unwrap();

        assert_eq!(value["error"], "memory_budget_exceeded");
        assert_eq!(value["phase"], "parse");
        assert_eq!(value["limit_bytes"], 1024);
        assert_eq!(value["accounted_bytes"], 2048);
        assert_eq!(value["observed_rss_bytes"], 512);
    }
}
