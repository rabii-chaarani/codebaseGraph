use serde::{Deserialize, Serialize};

/// Stable severity used across validation, builds, and transports.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    #[default]
    Info,
}

/// A safe, source-relative diagnostic suitable for public responses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub source_path: String,
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    pub fn error(
        code: impl Into<String>,
        source_path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: code.into(),
            source_path: source_path.into(),
            line,
            message: message.into(),
        }
    }

    pub fn warning(
        code: impl Into<String>,
        source_path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: code.into(),
            source_path: source_path.into(),
            line,
            message: message.into(),
        }
    }
}
