use std::fmt;

#[derive(Debug)]
pub enum NativeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Database(String),
    InvalidInput(String),
    Unsupported(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeError::Io(error) => write!(formatter, "{error}"),
            NativeError::Json(error) => write!(formatter, "{error}"),
            NativeError::Database(message) => write!(formatter, "{message}"),
            NativeError::InvalidInput(message) => write!(formatter, "{message}"),
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
