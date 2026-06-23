use crate::error::NativeError;
use lbug::{Connection, Database, SystemConfig};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

const WRITE_INTENT_FILE: &str = "graph-write-intent.lock";
const WRITE_INTENT_STALE_AFTER: Duration = Duration::from_secs(60);

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

pub struct WriteIntentGuard {
    path: PathBuf,
}

impl Drop for WriteIntentGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

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
    if read_only {
        wait_for_write_intent(db_path, READ_RETRY_POLICY)?;
    }
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

pub fn acquire_write_intent(db_path: &Path) -> Result<WriteIntentGuard, NativeError> {
    let path = write_intent_path(db_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    retry_transient_database(WRITE_RETRY_POLICY, || {
        remove_stale_write_intent(&path)?;
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "pid={} unix_ms={}", std::process::id(), unix_ms())?;
                Ok(WriteIntentGuard { path: path.clone() })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(NativeError::Database(format!(
                    "graph database busy: write intent exists at {}",
                    path.display()
                )))
            }
            Err(error) => Err(NativeError::Io(error)),
        }
    })
}

fn wait_for_write_intent(db_path: &Path, policy: RetryPolicy) -> Result<(), NativeError> {
    let path = write_intent_path(db_path);
    retry_transient_database(policy, || {
        remove_stale_write_intent(&path)?;
        if path.exists() {
            Err(NativeError::Database(format!(
                "graph database busy: refresh/write in progress at {}",
                path.display()
            )))
        } else {
            Ok(())
        }
    })
}

fn remove_stale_write_intent(path: &Path) -> Result<(), NativeError> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    let Ok(modified) = metadata.modified() else {
        return Ok(());
    };
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return Ok(());
    };
    if age >= WRITE_INTENT_STALE_AFTER {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn write_intent_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(WRITE_INTENT_FILE)
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
