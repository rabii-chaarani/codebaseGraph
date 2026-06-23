use super::options::McpServeOptions;
use crate::cli::{
    build::{materialize_candidate_paths, MaterializeOptions},
    graph::resolve_health_runtime,
    watch::{
        collect_poll_batch, collect_watch_batch, probe_native_watcher, start_native_watcher,
        watch_file_snapshot, WatchEventFilter, WatchLoopConfig, WatchMessage,
    },
};
use crate::db_writer::is_transient_database_error;
use crate::protocol::NativeSyntaxMaterializationResponse;
use serde_json::json;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::{mpsc::Receiver, Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DEFAULT_POLL_MS: u64 = 500;
const DEFAULT_DEBOUNCE_MS: u64 = 250;
const REFRESH_INITIAL_RETRY_MS: u64 = 100;
const REFRESH_MAX_RETRY_MS: u64 = 1_000;

#[derive(Debug)]
pub(in crate::cli) struct McpRefreshState {
    status: Mutex<McpRefreshStatus>,
    graph_lock: RwLock<()>,
}

#[derive(Clone, Debug)]
pub(in crate::cli) struct McpRefreshStatus {
    pub(in crate::cli) enabled: bool,
    pub(in crate::cli) backend: String,
    pub(in crate::cli) refreshing: bool,
    pub(in crate::cli) pending: bool,
    pub(in crate::cli) last_refresh_unix_ms: Option<u128>,
    pub(in crate::cli) last_error: Option<String>,
    pub(in crate::cli) last_error_count: usize,
    pub(in crate::cli) last_retry_unix_ms: Option<u128>,
    pub(in crate::cli) last_event_count: usize,
    pub(in crate::cli) last_changed_paths: usize,
    pub(in crate::cli) last_rebuilt: usize,
    pub(in crate::cli) last_deleted: usize,
    pub(in crate::cli) last_database_written: bool,
}

impl Default for McpRefreshStatus {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: "starting".to_string(),
            refreshing: false,
            pending: false,
            last_refresh_unix_ms: None,
            last_error: None,
            last_error_count: 0,
            last_retry_unix_ms: None,
            last_event_count: 0,
            last_changed_paths: 0,
            last_rebuilt: 0,
            last_deleted: 0,
            last_database_written: false,
        }
    }
}

impl McpRefreshState {
    pub(in crate::cli) fn new() -> Self {
        Self {
            status: Mutex::new(McpRefreshStatus::default()),
            graph_lock: RwLock::new(()),
        }
    }

    pub(in crate::cli) fn snapshot(&self) -> McpRefreshStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| McpRefreshStatus {
                enabled: false,
                backend: "failed".to_string(),
                refreshing: false,
                pending: false,
                last_refresh_unix_ms: None,
                last_error: Some("refresh status lock poisoned".to_string()),
                last_error_count: 1,
                last_retry_unix_ms: None,
                last_event_count: 0,
                last_changed_paths: 0,
                last_rebuilt: 0,
                last_deleted: 0,
                last_database_written: false,
            })
    }

    pub(in crate::cli) fn as_json(&self) -> serde_json::Value {
        let status = self.snapshot();
        json!({
            "enabled": status.enabled,
            "backend": status.backend,
            "refreshing": status.refreshing,
            "pending": status.pending,
            "last_refresh_unix_ms": status.last_refresh_unix_ms,
            "last_error": status.last_error,
            "last_error_count": status.last_error_count,
            "last_retry_unix_ms": status.last_retry_unix_ms,
            "last_event_count": status.last_event_count,
            "last_changed_paths": status.last_changed_paths,
            "last_rebuilt": status.last_rebuilt,
            "last_deleted": status.last_deleted,
            "last_database_written": status.last_database_written,
        })
    }

    pub(in crate::cli) fn read_guard(&self) -> Result<RwLockReadGuard<'_, ()>, String> {
        self.graph_lock
            .read()
            .map_err(|_| "refresh graph read lock poisoned".to_string())
    }

    fn write_guard(&self) -> Result<RwLockWriteGuard<'_, ()>, String> {
        self.graph_lock
            .write()
            .map_err(|_| "refresh graph write lock poisoned".to_string())
    }

    fn set_backend(&self, backend: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = true;
            status.last_error = None;
        }
    }

    fn set_error(&self, backend: &str, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = true;
            status.refreshing = false;
            status.pending = false;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
        }
    }

    fn mark_pending(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.pending = true;
        }
    }

    fn mark_refreshing(&self, backend: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.refreshing = true;
            status.pending = false;
            status.last_error = None;
        }
    }

    fn mark_refresh_error(
        &self,
        backend: &str,
        event_count: usize,
        changed_paths: usize,
        error: String,
        retrying: bool,
    ) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.refreshing = false;
            status.pending = retrying;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
            status.last_retry_unix_ms = retrying.then_some(unix_ms());
            status.last_event_count = event_count;
            status.last_changed_paths = changed_paths;
        }
    }

    fn mark_refreshed(
        &self,
        backend: &str,
        event_count: usize,
        changed_paths: usize,
        rebuilt: usize,
        deleted: usize,
        database_written: bool,
    ) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.refreshing = false;
            status.pending = false;
            status.last_refresh_unix_ms = Some(unix_ms());
            status.last_error = None;
            status.last_error_count = 0;
            status.last_retry_unix_ms = None;
            status.last_event_count = event_count;
            status.last_changed_paths = changed_paths;
            status.last_rebuilt = rebuilt;
            status.last_deleted = deleted;
            status.last_database_written = database_written;
        }
    }
}

pub(in crate::cli) fn start_auto_refresh(options: &McpServeOptions) -> Arc<McpRefreshState> {
    let state = Arc::new(McpRefreshState::new());
    let mut refresh_options = options.clone();
    refresh_options.refresh = None;
    let thread_state = Arc::clone(&state);
    thread::spawn(move || {
        if let Err(error) = run_auto_refresh(refresh_options, &thread_state) {
            thread_state.set_error("failed", error.clone());
            eprintln!(
                "{}",
                json!({"event": "mcp.auto_refresh_error", "message": error})
            );
        }
    });
    state
}

fn run_auto_refresh(options: McpServeOptions, state: &Arc<McpRefreshState>) -> Result<(), String> {
    let runtime = resolve_health_runtime(&options.health_options())?;
    let materialize_options = MaterializeOptions {
        source_root: Some(runtime.repo_root.clone()),
        db: Some(runtime.db_path.clone()),
        manifest: Some(runtime.manifest_path.clone()),
        mode: "changed".to_string(),
        include_fts: true,
        semantic_enrichment: true,
        semantic_provider_mode: "local_only".to_string(),
        use_git: false,
        ..MaterializeOptions::default()
    };
    let filter = WatchEventFilter::from_options(&runtime.repo_root, &materialize_options)?;
    let loop_config = WatchLoopConfig {
        poll_ms: DEFAULT_POLL_MS,
        debounce_ms: DEFAULT_DEBOUNCE_MS,
        max_iterations: None,
    };

    match start_native_watcher(&runtime.repo_root) {
        Ok((watcher, rx)) => {
            let probe = probe_native_watcher(&runtime.repo_root, &filter, &rx)?;
            if probe.delivered {
                state.set_backend("native");
                match run_native_refresh_loop(
                    state,
                    loop_config,
                    materialize_options.clone(),
                    filter,
                    watcher,
                    rx,
                    probe.queued,
                ) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        state.set_error("poll", error);
                        let filter = WatchEventFilter::from_options(
                            &runtime.repo_root,
                            &materialize_options,
                        )?;
                        run_poll_refresh_loop(state, loop_config, materialize_options, filter)
                    }
                }
            } else {
                drop(watcher);
                state.set_error(
                    "poll",
                    probe
                        .reason
                        .unwrap_or_else(|| "native probe failed".to_string()),
                );
                run_poll_refresh_loop(state, loop_config, materialize_options, filter)
            }
        }
        Err(error) => {
            state.set_error("poll", error);
            run_poll_refresh_loop(state, loop_config, materialize_options, filter)
        }
    }
}

fn run_native_refresh_loop(
    state: &Arc<McpRefreshState>,
    loop_config: WatchLoopConfig,
    materialize_options: MaterializeOptions,
    filter: WatchEventFilter,
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    mut queued: VecDeque<WatchMessage>,
) -> Result<(), String> {
    loop {
        let first = match queued.pop_front() {
            Some(message) => message,
            None => rx
                .recv()
                .map_err(|error| format!("filesystem watcher stopped: {error}"))?,
        };
        let Some(batch) = collect_watch_batch(
            first,
            &rx,
            &mut queued,
            &filter,
            Duration::from_millis(loop_config.debounce_ms),
            Duration::from_millis(loop_config.debounce_ms.saturating_mul(4).max(1)),
        )?
        else {
            continue;
        };
        refresh_batch(
            state,
            "native",
            &materialize_options,
            batch.event_count,
            batch.paths,
        )?;
    }
}

fn run_poll_refresh_loop(
    state: &Arc<McpRefreshState>,
    loop_config: WatchLoopConfig,
    materialize_options: MaterializeOptions,
    filter: WatchEventFilter,
) -> Result<(), String> {
    state.set_backend("poll");
    let mut previous_snapshot = watch_file_snapshot(&filter)?;
    loop {
        let batch = collect_poll_batch(
            &filter,
            &mut previous_snapshot,
            Duration::from_millis(loop_config.poll_ms),
            Duration::from_millis(loop_config.debounce_ms),
            Duration::from_millis(loop_config.debounce_ms.saturating_mul(4).max(1)),
        )?;
        refresh_batch(
            state,
            "poll",
            &materialize_options,
            batch.event_count,
            batch.paths,
        )?;
    }
}

fn refresh_batch(
    state: &Arc<McpRefreshState>,
    backend: &str,
    materialize_options: &MaterializeOptions,
    event_count: usize,
    paths: BTreeSet<String>,
) -> Result<(), String> {
    let changed_paths = paths.len();
    if changed_paths == 0 {
        return Ok(());
    }
    refresh_batch_with(state, backend, event_count, paths, |candidate_paths| {
        materialize_candidate_paths(materialize_options, candidate_paths)
            .map(|(_, response)| response)
    })
}

fn refresh_batch_with(
    state: &Arc<McpRefreshState>,
    backend: &str,
    event_count: usize,
    paths: BTreeSet<String>,
    mut refresh: impl FnMut(Vec<String>) -> Result<NativeSyntaxMaterializationResponse, String>,
) -> Result<(), String> {
    let changed_paths = paths.len();
    if changed_paths == 0 {
        return Ok(());
    }
    let candidate_paths = paths.into_iter().collect::<Vec<_>>();
    let mut retry_delay = Duration::from_millis(REFRESH_INITIAL_RETRY_MS);
    loop {
        state.mark_pending();
        let result = {
            let _guard = state.write_guard()?;
            state.mark_refreshing(backend);
            refresh(candidate_paths.clone())
        };
        match result {
            Ok(response) => {
                state.mark_refreshed(
                    backend,
                    event_count,
                    changed_paths,
                    response.diff.rebuild_paths().len(),
                    response.diff.deleted.len(),
                    response.database_written,
                );
                return Ok(());
            }
            Err(error) => {
                let retrying = is_transient_database_error(&error);
                state.mark_refresh_error(backend, event_count, changed_paths, error, retrying);
                if !retrying {
                    return Ok(());
                }
                thread::sleep(retry_delay);
                retry_delay = retry_delay
                    .saturating_mul(2)
                    .min(Duration::from_millis(REFRESH_MAX_RETRY_MS));
            }
        }
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ManifestDiff;
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn refresh_batch_retries_transient_errors_without_failing_state() {
        let state = Arc::new(McpRefreshState::new());
        let attempts = AtomicUsize::new(0);
        refresh_batch_with(
            &state,
            "poll",
            1,
            BTreeSet::from(["src/lib.rs".to_string()]),
            |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("IO exception: Could not set lock on file".to_string())
                } else {
                    Ok(NativeSyntaxMaterializationResponse::skipped(
                        BTreeMap::new(),
                        ManifestDiff {
                            added: Vec::new(),
                            modified: Vec::new(),
                            unchanged: Vec::new(),
                            deleted: Vec::new(),
                            force_rebuild: false,
                        },
                        Vec::new(),
                        Vec::new(),
                        BTreeMap::new(),
                    ))
                }
            },
        )
        .unwrap();

        let status = state.snapshot();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(status.backend, "poll");
        assert!(!status.refreshing);
        assert!(!status.pending);
        assert!(status.last_error.is_none());
        assert_eq!(status.last_error_count, 0);
        assert!(status.last_refresh_unix_ms.is_some());
    }

    #[test]
    fn refresh_batch_records_non_transient_errors_and_keeps_loop_alive() {
        let state = Arc::new(McpRefreshState::new());
        refresh_batch_with(
            &state,
            "native",
            1,
            BTreeSet::from(["src/lib.rs".to_string()]),
            |_| Err("parser exploded".to_string()),
        )
        .unwrap();

        let status = state.snapshot();
        assert_eq!(status.backend, "native");
        assert!(!status.refreshing);
        assert!(!status.pending);
        assert_eq!(status.last_error.as_deref(), Some("parser exploded"));
        assert_eq!(status.last_error_count, 1);
        assert!(status.last_refresh_unix_ms.is_none());
    }
}
