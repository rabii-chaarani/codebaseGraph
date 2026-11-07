//! Untrusted mapping claims and their canonical identity.
mod identity;
mod input;
pub use identity::{mapping_facts_digest, MappingFactsDigest};
pub use input::*;

/// An incomplete evidence operation. No successful validation is carried here.
#[derive(Debug)]
pub struct EvidenceError {
    code: &'static str,
    message: String,
}
impl EvidenceError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl std::fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for EvidenceError {}
impl From<super::compile::CompileError> for EvidenceError {
    fn from(value: super::compile::CompileError) -> Self {
        Self::new(value.code(), value.to_string())
    }
}
