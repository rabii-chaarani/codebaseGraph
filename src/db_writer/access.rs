use crate::error::NativeError;
use lbug::{Connection, Database, SystemConfig};
use std::{path::Path, thread, time::Duration};

#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    max_attempts: usize,
    initial_delay: Duration,
    max_delay: Duration,
}

impl RetryPolicy {
    pub const fn new(max_attempts: usize, initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay,
        }
    }
}

pub const READ_RETRY_POLICY: RetryPolicy =
    RetryPolicy::new(3, Duration::from_millis(40), Duration::from_millis(160));
pub const WRITE_RETRY_POLICY: RetryPolicy =
    RetryPolicy::new(8, Duration::from_millis(100), Duration::from_millis(1_000));

pub fn is_transient_database_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "could not set lock",
        "lock is held",
        "database is locked",
        "database busy",
        "resource busy",
        "couldn't replay shadow pages",
        "read-only mode",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

pub fn retry_transient_database<T>(
    policy: RetryPolicy,
    mut operation: impl FnMut() -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let max_attempts = policy.max_attempts.max(1);
    let mut delay = policy.initial_delay;
    for attempt in 1..=max_attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempt == max_attempts || !is_transient_database_error(&error.to_string()) {
                    return Err(error);
                }
                thread::sleep(delay);
                delay = delay.saturating_mul(2).min(policy.max_delay);
            }
        }
    }
    unreachable!("retry loop always returns")
}

pub fn open_ladybug_database(db_path: &Path, read_only: bool) -> Result<Database, NativeError> {
    Database::new(db_path, SystemConfig::default().read_only(read_only)).map_err(|error| {
        NativeError::Database(format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        ))
    })
}

pub fn connect_ladybug_database(database: &Database) -> Result<Connection<'_>, NativeError> {
    Connection::new(database)
        .map_err(|error| NativeError::Database(format!("failed to connect to graph: {error}")))
}
