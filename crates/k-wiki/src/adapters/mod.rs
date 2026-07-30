pub mod cli;
pub mod http;
pub mod mcp;

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct TransportPayload {
    pub text: String,
    pub structured: Value,
}

impl TransportPayload {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            structured: Value::Null,
        }
    }

    pub fn structured(text: impl Into<String>, structured: Value) -> Self {
        Self {
            text: text.into(),
            structured,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransportError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
    pub retryable: bool,
}

impl TransportError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            retryable: false,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}
