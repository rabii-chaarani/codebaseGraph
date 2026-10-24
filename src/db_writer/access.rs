use crate::error::NativeError;
use crate::storage::maintenance_ownership::DatabaseUseLease;
use lbug::{Connection, Database, SystemConfig};
use std::{
    ops::Deref,
    path::{Component, Path, PathBuf},
    thread,
    time::Duration,
};

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
const DEFAULT_READ_BUFFER_POOL_BYTES: u64 = 128 * 1024 * 1024;
const DEFAULT_READ_THREADS: u64 = 2;

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

/// A native database and its physical-resource exclusion, owned together.
///
/// Connections borrow the native handle through this wrapper, so safe Rust
/// cannot release the exclusion while a connection remains in use.
///
/// ```no_run
/// use codebase_graph::db_writer::{connect_ladybug_database, open_ladybug_database};
/// let database = open_ladybug_database(std::path::Path::new("graph.lbug"), true)?;
/// let connection = connect_ladybug_database(&database)?;
/// connection.query("RETURN 1")?;
/// drop(connection);
/// drop(database);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ```compile_fail
/// use codebase_graph::db_writer::{connect_ladybug_database, open_ladybug_database};
/// let database = open_ladybug_database(std::path::Path::new("graph.lbug"), true).unwrap();
/// let connection = connect_ladybug_database(&database).unwrap();
/// drop(database);
/// connection.query("RETURN 1").unwrap();
/// ```
///
/// ```compile_fail
/// use codebase_graph::db_writer::open_ladybug_database;
/// let database = open_ladybug_database(std::path::Path::new("graph.lbug"), true).unwrap();
/// let native: lbug::Database = *database;
/// ```
#[derive(Debug)]
pub struct GuardedDatabase {
    // Rust drops fields in declaration order: close the native database before
    // releasing either family or resource exclusion, including during unwind.
    database: Database,
    _lease: DatabaseUseLease,
}

impl Deref for GuardedDatabase {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

pub fn open_ladybug_database(
    db_path: &Path,
    read_only: bool,
) -> Result<GuardedDatabase, NativeError> {
    open_ladybug_database_with_limits(
        db_path,
        read_only,
        DEFAULT_READ_BUFFER_POOL_BYTES,
        DEFAULT_READ_THREADS,
    )
}

pub fn open_ladybug_database_with_limits(
    db_path: &Path,
    read_only: bool,
    buffer_pool_bytes: u64,
    max_num_threads: u64,
) -> Result<GuardedDatabase, NativeError> {
    let config = SystemConfig::default()
        .read_only(read_only)
        .buffer_pool_size(buffer_pool_bytes)
        .max_num_threads(max_num_threads);
    open_ladybug_database_with_config(db_path, config)
}

pub(crate) fn open_ladybug_database_with_config(
    db_path: &Path,
    config: SystemConfig,
) -> Result<GuardedDatabase, NativeError> {
    // Acquire before native open, including absent-to-created databases. An
    // open error drops the lease; successful construction moves it with the DB.
    let native_path = guarded_native_path(db_path)?;
    let lease = DatabaseUseLease::acquire_shared(&native_path)?;
    #[cfg(windows)]
    ensure_windows_native_encoding(&native_path)?;
    let database = Database::new(&native_path, config).map_err(|error| {
        NativeError::Database(format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        ))
    })?;
    Ok(GuardedDatabase {
        database,
        _lease: lease,
    })
}

/// Bind the resource using the same local, absolute UTF-8 spelling sent to
/// Ladybug. Its Rust SDK converts paths through `display()`, and its pinned
/// native StorageUtils::expandPath expands a leading '~' and lexically removes
/// '..'. Reject those ambiguous forms before creating any coordination files;
/// resolving once also keeps guard and native open independent of later CWD
/// changes. This does not change persisted Direct writer/journal addresses.
pub(super) fn guarded_native_path(db_path: &Path) -> Result<PathBuf, NativeError> {
    let spelling = db_path
        .to_str()
        .ok_or_else(|| NativeError::InvalidInput("database_path_not_utf8".into()))?;
    if spelling.is_empty()
        || spelling.contains('\0')
        || spelling == ":memory:"
        || spelling.starts_with('~')
    {
        return Err(NativeError::InvalidInput(
            "database_path_requires_literal_disk_resource".into(),
        ));
    }
    if db_path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(NativeError::InvalidInput(
            "database_path_parent_traversal_unsupported".into(),
        ));
    }
    // A terminal separator or '/.' denotes a directory to the native opener.
    // Do not normalize it into a different, valid database filename.
    if spelling.ends_with('/') || spelling.ends_with("/.") || spelling == "." {
        return Err(NativeError::InvalidInput(
            "database_path_requires_file_name".into(),
        ));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if spelling.ends_with('\\') || spelling.ends_with("\\.") {
            return Err(NativeError::InvalidInput(
                "database_path_requires_file_name".into(),
            ));
        }
        if db_path.components().any(|part| match part {
            Component::Prefix(prefix) => !matches!(
                prefix.kind(),
                Prefix::Disk(_)
                    | Prefix::UNC(_, _)
                    | Prefix::VerbatimDisk(_)
                    | Prefix::VerbatimUNC(_, _)
            ),
            _ => false,
        }) || (!db_path.is_absolute() && db_path.has_root())
            || (!db_path.is_absolute()
                && matches!(db_path.components().next(), Some(Component::Prefix(_))))
        {
            return Err(NativeError::InvalidInput(
                "database_path_prefix_unsupported".into(),
            ));
        }
    }
    let absolute = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(db_path)
    };
    if absolute.to_str().is_none() || !absolute.is_absolute() {
        return Err(NativeError::InvalidInput(
            "database_path_absolute_utf8_required".into(),
        ));
    }
    #[cfg(windows)]
    {
        let native_path = normalized_windows_native_path(absolute)?;
        ensure_windows_native_encoding(&native_path)?;
        return Ok(native_path);
    }
    #[cfg(not(windows))]
    {
        // ParentDir is excluded above. Removing CurDir/repeated separators
        // agrees with native lexical normalization without erasing symlink/..
        // proof.
        Ok(absolute.components().collect())
    }
}

#[cfg(windows)]
fn normalized_windows_native_path(absolute: PathBuf) -> Result<PathBuf, NativeError> {
    use std::path::Prefix;
    let Some(Component::Prefix(prefix)) = absolute.components().next() else {
        return Err(NativeError::InvalidInput(
            "database_path_prefix_unsupported".into(),
        ));
    };
    let spelling = absolute
        .to_str()
        .ok_or_else(|| NativeError::InvalidInput("database_path_absolute_utf8_required".into()))?;
    match prefix.kind() {
        Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _) => {
            // fs::canonicalize returns these ordinary Windows filesystem
            // namespaces. Preserve them exactly; removing the prefix changes
            // length/parsing semantics. Reject tails native lexical cleanup
            // would reinterpret, including '/' (literal in verbatim paths).
            let tail = &spelling[prefix.as_os_str().len()..];
            if spelling.contains('/')
                || !tail.starts_with('\\')
                || tail[1..]
                    .split('\\')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(NativeError::InvalidInput(
                    "database_path_verbatim_normalization_unsupported".into(),
                ));
            }
            Ok(absolute)
        }
        Prefix::Disk(_) | Prefix::UNC(_, _) => {
            // Ordinary Win32 paths normalize these components differently from
            // extended namespaces; do not rely on wide/narrow API equivalence.
            if absolute.components().any(|part| matches!(part,
                Component::Normal(name) if name.to_str().map_or(true, |name| name.ends_with('.') || name.ends_with(' '))))
            {
                return Err(NativeError::InvalidInput("database_path_component_normalization_unsupported".into()));
            }
            Ok(absolute.components().collect())
        }
        _ => Err(NativeError::InvalidInput(
            "database_path_prefix_unsupported".into(),
        )),
    }
}

// CP_UTF8 is 65001. This byte-policy helper is tested without changing the
// process-global ANSI/OEM FileAPI mode. The whole absolute path participates.
#[cfg(any(windows, test))]
pub(super) fn windows_native_encoding_matches(spelling: &str, effective_code_page: u32) -> bool {
    spelling.is_ascii() || effective_code_page == 65001
}

#[cfg(windows)]
pub(super) fn windows_effective_file_api_code_page() -> u32 {
    use windows_sys::Win32::{
        Globalization::{GetACP, GetOEMCP},
        Storage::FileSystem::AreFileApisANSI,
    };
    // lbug 0.19's LocalFileSystem calls CreateFileA, whose process FileAPI
    // ANSI/OEM mode determines narrow filename decoding. Console code pages
    // and Rust's wide filesystem calls do not establish this equivalence.
    unsafe {
        if AreFileApisANSI() != 0 {
            GetACP()
        } else {
            GetOEMCP()
        }
    }
}

#[cfg(windows)]
fn ensure_windows_native_encoding(path: &Path) -> Result<(), NativeError> {
    let spelling = path
        .to_str()
        .ok_or_else(|| NativeError::InvalidInput("database_path_not_utf8".into()))?;
    if !windows_native_encoding_matches(spelling, windows_effective_file_api_code_page()) {
        return Err(NativeError::InvalidInput(
            "database_path_windows_native_encoding_unsupported".into(),
        ));
    }
    Ok(())
}

pub fn connect_ladybug_database(database: &Database) -> Result<Connection<'_>, NativeError> {
    Connection::new(database)
        .map_err(|error| NativeError::Database(format!("failed to connect to graph: {error}")))
}
