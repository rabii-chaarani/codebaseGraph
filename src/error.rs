use std::fmt;

#[derive(Debug)]
pub enum NativeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Database(String),
    InvalidInput(String),
    SourceChanged { path: String },
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
            NativeError::SourceChanged { path } => {
                let payload = serde_json::json!({
                    "error": "source_changed",
                    "path": path,
                });
                write!(formatter, "{payload}")
            }
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
    fn source_change_failure_has_a_stable_machine_readable_message() {
        let error = NativeError::SourceChanged {
            path: "src/lib.rs".to_string(),
        };
        let value: serde_json::Value = serde_json::from_str(&error.to_string()).unwrap();

        assert_eq!(value["error"], "source_changed");
        assert_eq!(value["path"], "src/lib.rs");
    }
}
