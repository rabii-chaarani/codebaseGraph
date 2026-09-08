use crate::{
    api::{
        context::{
            read_install_config, resolve_runtime, GraphRefreshBackend, GraphRefreshPolicy,
            RepoPaths, RepositoryIdentity,
        },
        contracts::{
            MaterializationRequest, RefreshBackend, RefreshLoopConfig, RefreshWatchConfig,
            RefreshWatchObserver, RefreshWatchSummary, RepoSelector,
        },
        lifecycle::is_retryable_refresh_failure,
        materialization::{
            default_excluded_parts, execute_candidate_materialization, read_codebase_graph_ignore,
            read_materialization_config_rules, MaterializationIntent, MaterializeOptions,
        },
        normalization::normalize_materialize_options,
    },
    materialization_worker::execute_refresh_worker_cancellable,
    profiles::ProfileSet,
    protocol::NativeSyntaxMaterializationResponse,
    source_selection::SourceSelection,
    storage::{
        layout::{DirectLayout, ManagedLayout},
        locks::{try_open_locked, LockMode, RefreshLease},
    },
};
use notify::{
    event::{AccessKind, AccessMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex, Weak,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_PENDING_PATHS: usize = 4_096;
const MAX_PENDING_PATH_BYTES: usize = 1024 * 1024;
const REFRESH_ELECTION_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug)]
pub(crate) struct RefreshServiceConfig {
    pub(crate) policy: GraphRefreshPolicy,
    pub(crate) include_fts: bool,
    pub(crate) semantic_enrichment: bool,
    pub(crate) worker_memory_mib: u64,
    pub(crate) rust_memory_mib: u64,
    pub(crate) spill_chunk_mib: u64,
    pub(crate) max_parallelism: usize,
    pub(crate) backend: GraphRefreshBackend,
    pub(crate) reconcile_interval: Duration,
    pub(crate) explicit_overrides: RefreshConfigOverrides,
}

/// Values supplied by a caller should remain stable while install-config
/// values are reloaded between supervised attempts.  The mask keeps that
/// distinction private to the refresh service without changing public API
/// request types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RefreshConfigOverrides {
    pub(crate) policy: bool,
    pub(crate) include_fts: bool,
    pub(crate) semantic_enrichment: bool,
    pub(crate) worker_memory_mib: bool,
    pub(crate) rust_memory_mib: bool,
    pub(crate) spill_chunk_mib: bool,
    pub(crate) max_parallelism: bool,
    pub(crate) backend: bool,
    pub(crate) reconcile_interval: bool,
}

impl Default for RefreshServiceConfig {
    fn default() -> Self {
        Self {
            policy: GraphRefreshPolicy::Leader,
            include_fts: true,
            semantic_enrichment: false,
            worker_memory_mib: crate::api::context::DEFAULT_WORKER_MEMORY_MIB,
            rust_memory_mib: crate::api::context::DEFAULT_RUST_MEMORY_MIB,
            spill_chunk_mib: crate::api::context::DEFAULT_SPILL_CHUNK_MIB,
            max_parallelism: crate::api::context::DEFAULT_MAX_PARALLELISM,
            backend: GraphRefreshBackend::Auto,
            reconcile_interval: Duration::from_millis(
                crate::api::context::DEFAULT_RECONCILE_INTERVAL_MS,
            ),
            explicit_overrides: RefreshConfigOverrides::default(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WatchEventFilter {
    pub(crate) source_root: PathBuf,
    pub(crate) current_dir: PathBuf,
    config_path: PathBuf,
    pub(crate) excluded_parts: BTreeSet<String>,
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) ignore_patterns: Vec<String>,
    profiles: ProfileSet,
    protected_roots: Vec<PathBuf>,
    config_signatures: [Option<String>; 2],
    ignore_signature: Option<String>,
}

impl WatchEventFilter {
    #[cfg(test)]
    pub(crate) fn from_request(
        source_root: &Path,
        request: &MaterializationRequest,
    ) -> Result<Self, String> {
        Self::from_patterns(
            source_root,
            request.repo.config_path.clone(),
            request.include_patterns.clone(),
            request.exclude_patterns.clone(),
            protected_roots(
                source_root,
                None,
                request.repo.db_path.as_deref(),
                request.repo.manifest_path.as_deref(),
            ),
        )
    }

    pub(crate) fn from_options(
        source_root: &Path,
        options: &MaterializeOptions,
    ) -> Result<Self, String> {
        Self::from_patterns(
            source_root,
            options.config.clone(),
            options.include_patterns.clone(),
            options.exclude_patterns.clone(),
            protected_roots(
                source_root,
                options.storage_root.as_deref(),
                options.db.as_deref(),
                options.manifest.as_deref(),
            ),
        )
    }

    fn from_patterns(
        source_root: &Path,
        config_path: Option<PathBuf>,
        mut include_patterns: Vec<String>,
        mut exclude_patterns: Vec<String>,
        protected_roots: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let config_path = config_path.unwrap_or_else(|| config_path_for(source_root));
        let config_rules = read_materialization_config_rules(&config_path)?;
        let config_signature = file_signature(&config_path);
        let default_config_signature =
            file_signature(&source_root.join(".codebaseGraph").join("config.json"));
        include_patterns.splice(0..0, config_rules.include_patterns);
        exclude_patterns.splice(0..0, config_rules.exclude_patterns);
        let ignore_path = source_root.join(".codebaseGraphignore");
        Ok(Self {
            source_root: source_root.to_path_buf(),
            current_dir: env::current_dir().unwrap_or_else(|_| source_root.to_path_buf()),
            config_path,
            excluded_parts: default_excluded_parts().into_iter().collect(),
            include_patterns,
            exclude_patterns,
            ignore_patterns: read_codebase_graph_ignore(source_root)?,
            profiles: ProfileSet::new(&[]),
            protected_roots,
            config_signatures: [config_signature, default_config_signature],
            ignore_signature: file_signature(&ignore_path),
        })
    }

    pub(crate) fn relevant_paths(&self, event: &Event) -> BTreeSet<String> {
        if !watch_event_refreshes(event) {
            return BTreeSet::new();
        }
        event
            .paths
            .iter()
            .filter_map(|path| self.relevant_path(path))
            .collect()
    }

    fn directory_rescan_path_count(&self, event: &Event) -> usize {
        if !matches!(
            event.kind,
            EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
        ) {
            return 0;
        }
        event
            .paths
            .iter()
            .filter(|path| self.directory_change_path(path))
            .count()
    }

    fn directory_change_path(&self, path: &Path) -> bool {
        if self.is_configuration_path(path) || self.is_protected_path(path) {
            return false;
        }
        let Some(relative) = self.relative_event_path(path) else {
            return false;
        };
        if relative.as_os_str().is_empty()
            || !self
                .source_selection()
                .should_descend(&relative.to_string_lossy())
        {
            return false;
        }
        path.is_dir() || path.extension().is_none()
    }

    pub(crate) fn relevant_path(&self, path: &Path) -> Option<String> {
        if self.is_configuration_path(path) {
            return self
                .relative_event_path(path)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"));
        }
        if self.is_protected_path(path) {
            return None;
        }
        let relative = self.relative_event_path(path)?;
        if relative.as_os_str().is_empty() {
            return None;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        if !self.source_selection().includes_file(&relative)
            || self
                .profiles
                .language_for_path(Path::new(&relative))
                .is_none()
        {
            None
        } else {
            Some(relative)
        }
    }

    fn is_configuration_path(&self, path: &Path) -> bool {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.current_dir.join(path)
        };
        let absolute = normalize_watch_path(&absolute);
        absolute == normalize_watch_path(&self.config_path)
            || absolute == normalize_watch_path(&self.source_root.join(".codebaseGraphignore"))
    }

    fn is_protected_path(&self, path: &Path) -> bool {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.current_dir.join(path)
        };
        let absolute = normalize_watch_path(&absolute);
        self.protected_roots
            .iter()
            .any(|root| absolute.starts_with(normalize_watch_path(root)))
    }

    pub(crate) fn relative_event_path(&self, path: &Path) -> Option<PathBuf> {
        let path = normalize_watch_path(path);
        let source_root = normalize_watch_path(&self.source_root);
        if let Ok(relative) = path.strip_prefix(&source_root) {
            return Some(relative.to_path_buf());
        }
        if path.is_relative() {
            let absolute = normalize_watch_path(&self.current_dir.join(&path));
            if let Ok(relative) = absolute.strip_prefix(&source_root) {
                return Some(relative.to_path_buf());
            }
            return Some(path);
        }
        None
    }

    fn source_selection(&self) -> SourceSelection<'_> {
        SourceSelection::new(
            &self.excluded_parts,
            &self.include_patterns,
            &self.exclude_patterns,
            &self.ignore_patterns,
        )
    }

    fn configuration_paths(&self) -> [PathBuf; 2] {
        [
            self.config_path.clone(),
            self.source_root.join(".codebaseGraph").join("config.json"),
        ]
    }

    fn configuration_changed(&self) -> bool {
        self.configuration_paths()
            .iter()
            .zip(self.config_signatures.iter())
            .any(|(path, signature)| file_signature(path) != *signature)
            || file_signature(&self.source_root.join(".codebaseGraphignore"))
                != self.ignore_signature
    }

    fn configuration_key(&self, path: &Path) -> Option<String> {
        let normalized = normalize_watch_path(path);
        self.configuration_paths()
            .iter()
            .map(|candidate| normalize_watch_path(candidate))
            .find(|candidate| *candidate == normalized)
            .map(|candidate| {
                if let Ok(relative) =
                    candidate.strip_prefix(normalize_watch_path(&self.source_root))
                {
                    relative.to_string_lossy().replace('\\', "/")
                } else {
                    format!("@config:{}", candidate.to_string_lossy())
                }
            })
    }
}

fn file_signature(path: &Path) -> Option<String> {
    crate::hash::sha256_file(path).ok()
}

fn protected_roots(
    source_root: &Path,
    storage_root: Option<&Path>,
    db_path: Option<&Path>,
    manifest_path: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = vec![source_root.join(".codebaseGraph")];
    roots.extend(storage_root.map(Path::to_path_buf));
    roots.extend(db_path.map(Path::to_path_buf));
    roots.extend(manifest_path.map(Path::to_path_buf));
    if storage_root.is_none() {
        if let (Some(db_path), Some(manifest_path)) = (db_path, manifest_path) {
            roots.push(DirectLayout::new(db_path, manifest_path).artifact_root_path());
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(windows)]
fn normalize_windows_verbatim_path(path: &Path) -> PathBuf {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = normalized.strip_prefix("//?/UNC/") {
        PathBuf::from(format!("//{stripped}"))
    } else if let Some(stripped) = normalized.strip_prefix("//?/") {
        PathBuf::from(stripped)
    } else {
        PathBuf::from(normalized)
    }
}

fn normalize_watch_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        normalize_windows_verbatim_path(path)
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn config_path_for(source_root: &Path) -> PathBuf {
    RepoPaths::derive(source_root).config_path
}

pub(crate) fn watch_event_refreshes(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
            | EventKind::Other
            | EventKind::Access(AccessKind::Close(AccessMode::Write))
    )
}

#[derive(Debug)]
pub(crate) enum WatchMessage {
    Event(Event),
    Error(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct WatchChangeBatch {
    pub(crate) paths: BTreeSet<String>,
    pub(crate) event_count: usize,
    pub(crate) full_rescan: bool,
    pub(crate) configuration_changed: bool,
    pub(crate) overflow_count: usize,
    pub(crate) filtered_event_count: usize,
    path_bytes: usize,
}

impl WatchChangeBatch {
    pub(crate) fn extend_paths(&mut self, paths: impl IntoIterator<Item = String>) {
        if self.full_rescan {
            return;
        }
        for path in paths {
            if self.paths.contains(&path) {
                continue;
            }
            let Some(next_bytes) = self.path_bytes.checked_add(path.len()) else {
                self.mark_overflow();
                return;
            };
            if self.paths.len() >= MAX_PENDING_PATHS || next_bytes > MAX_PENDING_PATH_BYTES {
                self.mark_overflow();
                return;
            }
            self.path_bytes = next_bytes;
            self.paths.insert(path);
        }
    }

    fn mark_overflow(&mut self) {
        self.mark_full_rescan();
        self.overflow_count = self.overflow_count.saturating_add(1);
    }

    fn mark_full_rescan(&mut self) {
        self.paths.clear();
        self.path_bytes = 0;
        self.full_rescan = true;
    }

    fn has_changes(&self) -> bool {
        self.full_rescan || !self.paths.is_empty()
    }

    fn merge(&mut self, other: &Self) {
        let was_full_rescan = self.full_rescan;
        self.event_count = self.event_count.saturating_add(other.event_count);
        self.configuration_changed |= other.configuration_changed;
        self.overflow_count = self.overflow_count.saturating_add(other.overflow_count);
        self.filtered_event_count = self
            .filtered_event_count
            .saturating_add(other.filtered_event_count);
        if was_full_rescan {
            return;
        }
        if other.full_rescan {
            self.mark_full_rescan();
            return;
        }
        self.extend_paths(other.paths.iter().cloned());
    }
}

#[derive(Debug, Default)]
pub(crate) struct WatchProbeOutcome {
    pub(crate) delivered: bool,
    pub(crate) queued: VecDeque<WatchMessage>,
    pub(crate) reason: Option<String>,
}

pub(crate) fn start_native_watcher(
    source_root: &Path,
    filter: Arc<WatchEventFilter>,
    dirty_state: Option<Weak<RefreshState>>,
) -> Result<
    (
        notify::RecommendedWatcher,
        Receiver<WatchMessage>,
        Arc<AtomicBool>,
    ),
    String,
> {
    let (tx, rx) = mpsc::sync_channel(1);
    let overflowed = Arc::new(AtomicBool::new(false));
    let callback_overflowed = Arc::clone(&overflowed);
    let callback_filter = Arc::clone(&filter);
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let message = match result {
            Ok(event) => {
                let probe_event = event.paths.iter().any(|path| {
                    watch_path_is_under_dir(
                        path,
                        &callback_filter
                            .source_root
                            .join(".codebaseGraph")
                            .join("watch-probe"),
                        &callback_filter.source_root,
                        &callback_filter.current_dir,
                    )
                });
                let relevant = event.need_rescan()
                    || probe_event
                    || (watch_event_refreshes(&event)
                        && event.paths.iter().any(|path| {
                            callback_filter.relevant_path(path).is_some()
                                || callback_filter.directory_change_path(path)
                        }));
                if !relevant {
                    return;
                }
                if !probe_event {
                    if let Some(state) = dirty_state.as_ref().and_then(Weak::upgrade) {
                        state.mark_dirty();
                    }
                }
                WatchMessage::Event(event)
            }
            Err(error) => WatchMessage::Error(error.to_string()),
        };
        if tx.try_send(message).is_err() {
            callback_overflowed.store(true, Ordering::Release);
        }
    })
    .map_err(|error| format!("failed to start filesystem watcher: {error}"))?;
    watcher
        .watch(source_root, RecursiveMode::Recursive)
        .map_err(|error| format!("failed to watch {}: {error}", source_root.display()))?;
    Ok((watcher, rx, overflowed))
}

pub(crate) fn probe_native_watcher(
    source_root: &Path,
    filter: &WatchEventFilter,
    rx: &Receiver<WatchMessage>,
) -> Result<WatchProbeOutcome, String> {
    let timeout = watch_probe_timeout();
    let probe_dir = source_root.join(".codebaseGraph").join("watch-probe");
    let probe_path = probe_dir.join(format!(
        "probe-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    if !watch_probe_skip_write() {
        fs::create_dir_all(&probe_dir)
            .map_err(|error| format!("failed to create watch probe directory: {error}"))?;
        fs::write(&probe_path, b"probe")
            .map_err(|error| format!("failed to write watch probe: {error}"))?;
    }

    let started = Instant::now();
    let mut outcome = WatchProbeOutcome::default();
    while started.elapsed() < timeout {
        let remaining = timeout.saturating_sub(started.elapsed());
        match rx.recv_timeout(remaining) {
            Ok(WatchMessage::Event(event)) => {
                outcome.delivered = true;
                if !watch_event_is_under_dir(&event, &probe_dir, source_root, &filter.current_dir) {
                    outcome.queued.push_back(WatchMessage::Event(event));
                }
            }
            Ok(WatchMessage::Error(error)) => {
                outcome.reason = Some("watcher_error".to_string());
                outcome.queued.push_back(WatchMessage::Error(error));
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("filesystem watcher stopped during health probe".to_string())
            }
        }
    }
    let _ = fs::remove_file(&probe_path);
    if !outcome.delivered && outcome.reason.is_none() {
        outcome.reason = Some("probe_timeout".to_string());
    }
    Ok(outcome)
}

fn watch_probe_timeout() -> Duration {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(750))
}

fn watch_probe_skip_write() -> bool {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE").is_ok_and(|value| value == "1")
}

fn watch_event_is_under_dir(
    event: &Event,
    directory: &Path,
    source_root: &Path,
    current_dir: &Path,
) -> bool {
    !event.paths.is_empty()
        && event
            .paths
            .iter()
            .all(|path| watch_path_is_under_dir(path, directory, source_root, current_dir))
}

fn watch_path_is_under_dir(
    path: &Path,
    directory: &Path,
    source_root: &Path,
    current_dir: &Path,
) -> bool {
    let path = normalize_watch_path(path);
    let directory = normalize_watch_path(directory);
    if path.starts_with(&directory) {
        return true;
    }
    if path.is_relative() {
        return normalize_watch_path(&current_dir.join(&path)).starts_with(&directory)
            || normalize_watch_path(&source_root.join(path)).starts_with(directory);
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WatchFileState {
    pub(crate) modified_nanos: u128,
    pub(crate) len: u64,
}

pub(crate) type WatchFileSnapshot = BTreeMap<String, WatchFileState>;

pub(crate) fn apply_watch_message(
    message: WatchMessage,
    filter: &WatchEventFilter,
    batch: &mut WatchChangeBatch,
) -> Result<(), String> {
    match message {
        WatchMessage::Event(event) => {
            let configuration_changed = event
                .paths
                .iter()
                .any(|path| filter.is_configuration_path(path));
            if event.need_rescan() {
                batch.configuration_changed |= configuration_changed;
                batch.event_count = batch.event_count.saturating_add(1);
                batch.mark_full_rescan();
                return Ok(());
            }
            let candidate_count = event.paths.len();
            let directory_rescan_count = filter.directory_rescan_path_count(&event);
            let paths = filter.relevant_paths(&event);
            batch.filtered_event_count = batch.filtered_event_count.saturating_add(
                candidate_count
                    .saturating_sub(paths.len())
                    .saturating_sub(directory_rescan_count),
            );
            let has_paths = !paths.is_empty();
            if has_paths {
                batch.extend_paths(paths);
            }
            if directory_rescan_count > 0 {
                batch.mark_full_rescan();
            }
            batch.configuration_changed |= configuration_changed;
            if configuration_changed {
                batch.mark_full_rescan();
            }
            if has_paths || directory_rescan_count > 0 {
                batch.event_count += 1;
            }
            Ok(())
        }
        WatchMessage::Error(error) => Err(format!("filesystem watcher error: {error}")),
    }
}

pub(crate) fn collect_watch_batch(
    first: WatchMessage,
    rx: &Receiver<WatchMessage>,
    overflowed: Option<&AtomicBool>,
    queued: &mut VecDeque<WatchMessage>,
    filter: &WatchEventFilter,
    debounce: Duration,
    max_wait: Duration,
) -> Result<Option<WatchChangeBatch>, String> {
    let mut batch = WatchChangeBatch::default();
    apply_watch_message(first, filter, &mut batch)?;
    if overflowed.is_some_and(|flag| flag.swap(false, Ordering::AcqRel)) {
        batch.mark_overflow();
    }
    if !batch.has_changes() {
        return Ok(None);
    }

    let started = Instant::now();
    let mut last_relevant = started;
    loop {
        let elapsed = started.elapsed();
        if elapsed >= max_wait {
            if overflowed.is_some_and(|flag| flag.swap(false, Ordering::AcqRel)) {
                batch.mark_overflow();
            }
            return Ok(Some(batch));
        }
        let quiet_elapsed = last_relevant.elapsed();
        if quiet_elapsed >= debounce {
            if overflowed.is_some_and(|flag| flag.swap(false, Ordering::AcqRel)) {
                batch.mark_overflow();
            }
            return Ok(Some(batch));
        }
        let timeout = debounce
            .saturating_sub(quiet_elapsed)
            .min(max_wait.saturating_sub(elapsed));
        let message = match queued.pop_front() {
            Some(message) => Ok(message),
            None => rx.recv_timeout(timeout),
        };
        match message {
            Ok(message) => {
                let before = batch.paths.len();
                let before_events = batch.event_count;
                apply_watch_message(message, filter, &mut batch)?;
                if batch.paths.len() != before || batch.event_count != before_events {
                    last_relevant = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if overflowed.is_some_and(|flag| flag.swap(false, Ordering::AcqRel)) {
                    batch.mark_overflow();
                }
                return Ok(Some(batch));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("filesystem watcher stopped".to_string())
            }
        }
    }
}

pub(crate) fn watch_file_snapshot(filter: &WatchEventFilter) -> Result<WatchFileSnapshot, String> {
    let mut snapshot = BTreeMap::new();
    watch_file_snapshot_inner(filter, &filter.source_root, &mut snapshot)?;
    for path in filter.configuration_paths() {
        if path.is_file() {
            insert_watch_file_state(
                &path,
                filter
                    .configuration_key(&path)
                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
                &mut snapshot,
            );
        }
    }
    Ok(snapshot)
}

fn watch_file_snapshot_inner(
    filter: &WatchEventFilter,
    directory: &Path,
    snapshot: &mut WatchFileSnapshot,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read directory {}: {error}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if path.is_dir() {
            let Some(relative) = path
                .strip_prefix(&filter.source_root)
                .ok()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
            else {
                continue;
            };
            if !relative.is_empty() && !filter.source_selection().should_descend(&relative) {
                continue;
            }
            watch_file_snapshot_inner(filter, &path, snapshot)?;
        } else if path.is_file() {
            let Some(relative_path) = filter.relevant_path(&path) else {
                continue;
            };
            insert_watch_file_state(&path, relative_path, snapshot);
        }
    }
    Ok(())
}

fn insert_watch_file_state(path: &Path, key: String, snapshot: &mut WatchFileSnapshot) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos())
        })
        .unwrap_or(0);
    snapshot.insert(
        key,
        WatchFileState {
            modified_nanos,
            len: metadata.len(),
        },
    );
}

pub(crate) fn watch_snapshot_diff(
    previous: &WatchFileSnapshot,
    current: &WatchFileSnapshot,
) -> BTreeSet<String> {
    let mut changed_paths = BTreeSet::new();
    for (path, state) in current {
        if previous.get(path) != Some(state) {
            changed_paths.insert(path.clone());
        }
    }
    for path in previous.keys() {
        if !current.contains_key(path) {
            changed_paths.insert(path.clone());
        }
    }
    changed_paths
}

#[cfg(test)]
pub(crate) fn collect_poll_batch(
    filter: &WatchEventFilter,
    previous_snapshot: &mut WatchFileSnapshot,
    poll_interval: Duration,
    debounce: Duration,
    max_wait: Duration,
) -> Result<WatchChangeBatch, String> {
    fn never_stop() -> bool {
        false
    }
    collect_poll_batch_until(
        filter,
        previous_snapshot,
        poll_interval,
        debounce,
        max_wait,
        None,
        &never_stop,
    )?
    .ok_or_else(|| "polling ended before a change batch was collected".to_string())
}

fn collect_poll_batch_until(
    filter: &WatchEventFilter,
    previous_snapshot: &mut WatchFileSnapshot,
    poll_interval: Duration,
    debounce: Duration,
    max_wait: Duration,
    deadline: Option<Instant>,
    should_stop: &dyn Fn() -> bool,
) -> Result<Option<WatchChangeBatch>, String> {
    loop {
        if should_stop() {
            return Ok(None);
        }
        let sleep_for = deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(poll_interval)
            .min(poll_interval);
        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        }
        if should_stop() {
            return Ok(None);
        }
        let current_snapshot = watch_file_snapshot(filter)?;
        let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
        *previous_snapshot = current_snapshot;
        if changed_paths.is_empty() {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Ok(None);
            }
            continue;
        }

        let started = Instant::now();
        let mut last_relevant = started;
        let mut batch = WatchChangeBatch {
            paths: BTreeSet::new(),
            event_count: 1,
            full_rescan: false,
            configuration_changed: false,
            overflow_count: 0,
            filtered_event_count: 0,
            path_bytes: 0,
        };
        batch.extend_paths(changed_paths);
        if batch
            .paths
            .iter()
            .any(|path| path == ".codebaseGraph/config.json" || path.starts_with("@config:"))
        {
            batch.configuration_changed = true;
            batch.mark_full_rescan();
        }
        loop {
            if should_stop() {
                return Ok(None);
            }
            let elapsed = started.elapsed();
            if elapsed >= max_wait {
                return Ok(Some(batch));
            }
            let quiet_elapsed = last_relevant.elapsed();
            if quiet_elapsed >= debounce {
                return Ok(Some(batch));
            }
            let timeout = poll_interval
                .min(debounce.saturating_sub(quiet_elapsed))
                .min(max_wait.saturating_sub(elapsed));
            thread::sleep(timeout);
            let current_snapshot = watch_file_snapshot(filter)?;
            let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
            *previous_snapshot = current_snapshot;
            if !changed_paths.is_empty() {
                let config_changed = changed_paths.iter().any(|path| {
                    path == ".codebaseGraph/config.json" || path.starts_with("@config:")
                });
                batch.extend_paths(changed_paths);
                if config_changed {
                    batch.configuration_changed = true;
                    batch.mark_full_rescan();
                }
                batch.event_count += 1;
                last_relevant = Instant::now();
            }
        }
    }
}

pub(crate) fn run_refresh_watch(
    request: &MaterializationRequest,
    config: RefreshWatchConfig,
    observer: &mut impl RefreshWatchObserver,
) -> Result<(), String> {
    let runtime = resolve_refresh_runtime(&request.repo)?;
    runtime.require_graph_write()?;
    let _refresh_lease = if config.once {
        None
    } else {
        try_open_locked(refresh_lock_path(&runtime), LockMode::Exclusive)
            .map_err(|error| format!("failed to acquire refresh ownership: {error}"))?
            .ok_or_else(|| {
                "another process already owns repository refresh monitoring".to_string()
            })?
            .into()
    };
    let mut materialize_options = MaterializeOptions::from_request(request, &runtime, false);
    normalize_materialize_options(&mut materialize_options);
    materialize_options.intent = MaterializationIntent::Refresh;
    let execution = RefreshExecutionPlan::new(request.repo.clone(), materialize_options.clone())?;

    if config.once {
        let response = execution.execute(Vec::new())?;
        return observer.on_success(None, &refresh_watch_summary(&response), 0, 0);
    }

    let filter = Arc::new(WatchEventFilter::from_options(
        &runtime.repo_root,
        &materialize_options,
    )?);
    match config.backend {
        RefreshBackend::Poll => run_poll_watch(config.loop_config, &filter, |batch| {
            refresh_watch_batch(observer, "poll", &execution, batch)
        }),
        RefreshBackend::Native => {
            let (watcher, rx, overflowed) =
                start_native_watcher(&runtime.repo_root, Arc::clone(&filter), None)?;
            run_native_watch(
                config.loop_config,
                &filter,
                watcher,
                rx,
                overflowed,
                VecDeque::new(),
                |batch| refresh_watch_batch(observer, "native", &execution, batch),
            )
        }
        RefreshBackend::Auto => {
            match start_native_watcher(&runtime.repo_root, Arc::clone(&filter), None) {
                Ok((watcher, rx, overflowed)) => {
                    let probe = probe_native_watcher(&runtime.repo_root, &filter, &rx)?;
                    if probe.delivered {
                        run_native_watch(
                            config.loop_config,
                            &filter,
                            watcher,
                            rx,
                            overflowed,
                            probe.queued,
                            |batch| refresh_watch_batch(observer, "native", &execution, batch),
                        )
                    } else {
                        drop(watcher);
                        observer.on_fallback(
                            "poll",
                            probe.reason.as_deref().unwrap_or("probe_failed"),
                        )?;
                        run_poll_watch(config.loop_config, &filter, |batch| {
                            refresh_watch_batch(observer, "poll", &execution, batch)
                        })
                    }
                }
                Err(_) => {
                    observer.on_fallback("poll", "watcher_start_failed")?;
                    run_poll_watch(config.loop_config, &filter, |batch| {
                        refresh_watch_batch(observer, "poll", &execution, batch)
                    })
                }
            }
        }
    }
}

fn resolve_refresh_runtime(
    selector: &RepoSelector,
) -> Result<crate::api::context::RepoRuntime, String> {
    let mut runtime = resolve_runtime(selector)?;
    runtime.release_read_leases();
    Ok(runtime)
}

fn refresh_watch_batch(
    observer: &mut impl RefreshWatchObserver,
    backend: &str,
    execution: &RefreshExecutionPlan,
    batch: &WatchChangeBatch,
) -> Result<bool, String> {
    let mut bound_observer = BoundRefreshWatchObserver { observer, backend };
    execute_refresh_with_policy(
        &mut bound_observer,
        batch.event_count,
        &batch.paths,
        batch.full_rescan,
        RefreshRetryPolicy::default(),
        |candidate_paths| execution.execute(candidate_paths),
    )
}

struct BoundRefreshWatchObserver<'a, O> {
    observer: &'a mut O,
    backend: &'a str,
}

impl<O: RefreshWatchObserver> RefreshObserver for BoundRefreshWatchObserver<'_, O> {
    fn on_success(
        &mut self,
        response: &NativeSyntaxMaterializationResponse,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        self.observer.on_success(
            Some(self.backend),
            &refresh_watch_summary(response),
            event_count,
            changed_paths,
        )
    }

    fn on_error(
        &mut self,
        error: &str,
        retrying: bool,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        self.observer
            .on_error(self.backend, error, retrying, event_count, changed_paths)
    }
}

fn refresh_watch_summary(response: &NativeSyntaxMaterializationResponse) -> RefreshWatchSummary {
    RefreshWatchSummary {
        rebuilt: response.diff.rebuild_paths().len(),
        deleted: response.diff.deleted.len(),
        skipped: response.skipped,
        database_written: response.database_written,
    }
}

pub(crate) fn run_poll_watch(
    config: RefreshLoopConfig,
    filter: &WatchEventFilter,
    mut refresh: impl FnMut(&WatchChangeBatch) -> Result<bool, String>,
) -> Result<(), String> {
    fn no_prepare(_: &WatchChangeBatch) {}
    fn default_retry_delay() -> Duration {
        Duration::from_secs(1)
    }
    let hooks = WatchLoopHooks {
        should_stop: &|| false,
        restart_on_config: false,
        before_refresh: &no_prepare,
        retry_delay: &default_retry_delay,
    };
    run_poll_watch_until(
        config,
        filter,
        Some(default_reconcile_interval()),
        hooks,
        &mut refresh,
    )
}

struct WatchLoopHooks<'a> {
    should_stop: &'a dyn Fn() -> bool,
    restart_on_config: bool,
    before_refresh: &'a dyn Fn(&WatchChangeBatch),
    retry_delay: &'a dyn Fn() -> Duration,
}

fn run_poll_watch_until(
    config: RefreshLoopConfig,
    filter: &WatchEventFilter,
    reconcile_interval: Option<Duration>,
    hooks: WatchLoopHooks<'_>,
    refresh: &mut impl FnMut(&WatchChangeBatch) -> Result<bool, String>,
) -> Result<(), String> {
    let mut previous_snapshot = watch_file_snapshot(filter)?;
    let mut refreshes = 0_usize;
    let mut next_reconciliation = reconcile_interval.map(|interval| Instant::now() + interval);
    let mut next_retry = None;
    let mut pending_failed: Option<WatchChangeBatch> = None;
    loop {
        if (hooks.should_stop)() {
            return Ok(());
        }
        let wake_deadline = if pending_failed.is_some() {
            next_retry.or(next_reconciliation)
        } else {
            next_reconciliation
        };
        let batch = collect_poll_batch_until(
            filter,
            &mut previous_snapshot,
            config.poll_interval,
            config.debounce,
            config.max_wait,
            wake_deadline,
            hooks.should_stop,
        )?;
        if batch.is_none() && (hooks.should_stop)() {
            return Ok(());
        }
        let configuration_changed = filter.configuration_changed();
        let mut batch = match batch {
            Some(batch) => batch,
            None => WatchChangeBatch {
                full_rescan: true,
                event_count: 0,
                ..WatchChangeBatch::default()
            },
        };
        if configuration_changed {
            batch.configuration_changed = true;
            batch.mark_full_rescan();
        }
        let had_pending_failure = pending_failed.is_some();
        if let Some(previous) = pending_failed.take() {
            let mut merged = previous;
            merged.merge(&batch);
            batch = merged;
        }
        if batch.event_count > 0 || !had_pending_failure {
            (hooks.before_refresh)(&batch);
        }
        if hooks.restart_on_config && batch.configuration_changed {
            return Err("refresh configuration changed; rebuilding watcher".to_string());
        }
        let retry_due = next_retry.is_some_and(|deadline| Instant::now() >= deadline);
        let reconciliation_due =
            next_reconciliation.is_some_and(|deadline| Instant::now() >= deadline);
        if retry_due || reconciliation_due {
            batch.mark_full_rescan();
        }
        if had_pending_failure && !retry_due {
            pending_failed = Some(batch);
            continue;
        }
        if !refresh(&batch)? {
            let retry_delay = (hooks.retry_delay)();
            pending_failed = Some(batch);
            next_retry = Some(Instant::now() + retry_delay);
            continue;
        }
        if batch.full_rescan {
            next_reconciliation = reconcile_interval.map(|interval| Instant::now() + interval);
        }
        next_retry = None;
        refreshes += 1;
        if config.max_iterations.is_some_and(|max| refreshes >= max) {
            return Ok(());
        }
    }
}

pub(crate) fn run_native_watch(
    config: RefreshLoopConfig,
    filter: &WatchEventFilter,
    watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    overflowed: Arc<AtomicBool>,
    queued: VecDeque<WatchMessage>,
    mut refresh: impl FnMut(&WatchChangeBatch) -> Result<bool, String>,
) -> Result<(), String> {
    fn no_prepare(_: &WatchChangeBatch) {}
    fn default_retry_delay() -> Duration {
        Duration::from_secs(1)
    }
    let hooks = WatchLoopHooks {
        should_stop: &|| false,
        restart_on_config: false,
        before_refresh: &no_prepare,
        retry_delay: &default_retry_delay,
    };
    run_native_watch_until(
        config,
        filter,
        NativeWatchResources {
            _watcher: watcher,
            rx,
            overflowed,
            queued,
        },
        Some(default_reconcile_interval()),
        hooks,
        &mut refresh,
    )
}

struct NativeWatchResources {
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    overflowed: Arc<AtomicBool>,
    queued: VecDeque<WatchMessage>,
}

fn run_native_watch_until(
    config: RefreshLoopConfig,
    filter: &WatchEventFilter,
    mut resources: NativeWatchResources,
    reconcile_interval: Option<Duration>,
    hooks: WatchLoopHooks<'_>,
    refresh: &mut impl FnMut(&WatchChangeBatch) -> Result<bool, String>,
) -> Result<(), String> {
    let mut refreshes = 0_usize;
    let mut next_reconciliation = reconcile_interval.map(|interval| Instant::now() + interval);
    let mut next_retry = None;
    let mut pending_failed: Option<WatchChangeBatch> = None;
    loop {
        if (hooks.should_stop)() {
            return Ok(());
        }
        let first = match resources.queued.pop_front() {
            Some(message) => message,
            None => {
                let wake_deadline = if pending_failed.is_some() {
                    next_retry.or(next_reconciliation)
                } else {
                    next_reconciliation
                };
                let timeout = wake_deadline
                    .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                    .unwrap_or_else(|| Duration::from_millis(250))
                    .min(Duration::from_millis(250));
                match resources.rx.recv_timeout(timeout) {
                    Ok(message) => message,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if (hooks.should_stop)() {
                            return Ok(());
                        }
                        let Some(deadline) = wake_deadline else {
                            continue;
                        };
                        if Instant::now() < deadline {
                            continue;
                        }
                        WatchMessage::Event(Event::new(EventKind::Other))
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("filesystem watcher stopped".to_string())
                    }
                }
            }
        };
        let mut batch = match collect_watch_batch(
            first,
            &resources.rx,
            Some(&resources.overflowed),
            &mut resources.queued,
            filter,
            config.debounce,
            config.max_wait,
        )? {
            Some(batch) => batch,
            None => {
                let wake_deadline = if pending_failed.is_some() {
                    next_retry.or(next_reconciliation)
                } else {
                    next_reconciliation
                };
                if wake_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    WatchChangeBatch {
                        full_rescan: true,
                        event_count: 0,
                        ..WatchChangeBatch::default()
                    }
                } else {
                    continue;
                }
            }
        };
        if filter.configuration_changed() {
            batch.configuration_changed = true;
            batch.mark_full_rescan();
        }
        let had_pending_failure = pending_failed.is_some();
        if let Some(previous) = pending_failed.take() {
            let mut merged = previous;
            merged.merge(&batch);
            batch = merged;
        }
        if batch.event_count > 0 || !had_pending_failure {
            (hooks.before_refresh)(&batch);
        }
        if hooks.restart_on_config && batch.configuration_changed {
            return Err("refresh configuration changed; rebuilding watcher".to_string());
        }
        let retry_due = next_retry.is_some_and(|deadline| Instant::now() >= deadline);
        let reconciliation_due =
            next_reconciliation.is_some_and(|deadline| Instant::now() >= deadline);
        if retry_due || reconciliation_due {
            batch.mark_full_rescan();
        }
        if had_pending_failure && !retry_due {
            pending_failed = Some(batch);
            continue;
        }
        if !refresh(&batch)? {
            let retry_delay = (hooks.retry_delay)();
            pending_failed = Some(batch);
            next_retry = Some(Instant::now() + retry_delay);
            continue;
        }
        if batch.full_rescan {
            next_reconciliation = reconcile_interval.map(|interval| Instant::now() + interval);
        }
        next_retry = None;
        refreshes += 1;
        if config.max_iterations.is_some_and(|max| refreshes >= max) {
            return Ok(());
        }
    }
}

pub(crate) fn execute_refresh_operation(
    options: &MaterializeOptions,
    paths: Vec<String>,
) -> Result<NativeSyntaxMaterializationResponse, String> {
    let (_request, response) = execute_candidate_materialization(options, paths)?;
    Ok(response)
}

#[derive(Clone, Debug)]
struct RefreshExecutionPlan {
    selector: RepoSelector,
    identity: RepositoryIdentity,
    base_options: MaterializeOptions,
}

impl RefreshExecutionPlan {
    fn new(selector: RepoSelector, base_options: MaterializeOptions) -> Result<Self, String> {
        let identity = RepositoryIdentity::capture(&selector)?;
        Ok(Self {
            selector,
            identity,
            base_options,
        })
    }

    fn resolve_options(&self) -> Result<MaterializeOptions, String> {
        self.identity.validate()?;
        let runtime = resolve_runtime(&self.selector)?;
        let current = RepositoryIdentity::capture(&self.selector)?;
        if current != self.identity {
            return Err(
                "repository identity changed while refresh was running; restart required"
                    .to_string(),
            );
        }
        runtime.require_graph_write()?;
        let mut options = self.base_options.clone();
        options.source_root = Some(runtime.repo_root);
        options.config = runtime.config_path;
        options.db = Some(runtime.db_path);
        options.manifest = Some(runtime.manifest_path);
        options.storage_root = runtime.storage_root;
        Ok(options)
    }

    fn execute(
        &self,
        candidate_paths: Vec<String>,
    ) -> Result<NativeSyntaxMaterializationResponse, String> {
        let options = self.resolve_options()?;
        execute_refresh_operation(&options, candidate_paths)
    }

    fn execute_isolated(
        &self,
        candidate_paths: Vec<String>,
        state: &Arc<RefreshState>,
    ) -> Result<NativeSyntaxMaterializationResponse, String> {
        let options = self.resolve_options()?;
        execute_refresh_worker_cancellable(
            &options,
            candidate_paths,
            |pid| state.set_worker_pid(pid),
            || refresh_task_should_stop(state),
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct RefreshServiceContext<'a> {
    state: &'a Arc<RefreshState>,
    execution: &'a RefreshExecutionPlan,
}

impl<'a> RefreshServiceContext<'a> {
    fn new(state: &'a Arc<RefreshState>, execution: &'a RefreshExecutionPlan) -> Self {
        Self { state, execution }
    }

    fn retry_delay(&self) -> Duration {
        let now = unix_ms();
        self.state
            .snapshot()
            .next_retry_unix_ms
            .and_then(|deadline| deadline.checked_sub(now))
            .map(|millis| Duration::from_millis(millis.min(u128::from(u64::MAX)) as u64))
            .filter(|delay| !delay.is_zero())
            .unwrap_or_else(|| Duration::from_secs(1))
    }

    fn refresh_batch(&self, backend: &'a str, batch: &WatchChangeBatch) -> Result<bool, String> {
        let mut observer = StateRefreshObserver::new(
            self.state,
            backend,
            batch.overflow_count,
            batch.filtered_event_count,
        );
        let result = execute_refresh_with_policy_limit(
            &mut observer,
            batch.event_count,
            &batch.paths,
            batch.full_rescan,
            RefreshRetryPolicy::default(),
            Some(4),
            |candidate_paths| self.execution.execute_isolated(candidate_paths, self.state),
        )?;
        if refresh_task_should_stop(self.state) {
            self.state.mark_stopped();
            return Ok(false);
        }
        if !result {
            match self.state.snapshot().state.as_str() {
                "retrying" => self.state.mark_retrying(unix_ms().saturating_add(1_000)),
                "blocked" if self.state.snapshot().next_retry_unix_ms.is_none() => self
                    .state
                    .mark_blocked_until(unix_ms().saturating_add(5_000)),
                _ => {}
            }
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug)]
struct RefreshWatchRuntime<'a> {
    service: RefreshServiceContext<'a>,
    config: RefreshLoopConfig,
    filter: &'a WatchEventFilter,
    reconcile_interval: Duration,
}

impl<'a> RefreshWatchRuntime<'a> {
    fn new(
        service: RefreshServiceContext<'a>,
        config: RefreshLoopConfig,
        filter: &'a WatchEventFilter,
        reconcile_interval: Duration,
    ) -> Self {
        Self {
            service,
            config,
            filter,
            reconcile_interval,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RefreshRetryPolicy {
    pub(crate) initial_delay: Duration,
    pub(crate) max_delay: Duration,
}

impl Default for RefreshRetryPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(1_000),
        }
    }
}

pub(crate) trait RefreshObserver {
    fn before_attempt(&mut self, _event_count: usize, _changed_paths: usize) -> Result<(), String> {
        Ok(())
    }

    fn on_success(
        &mut self,
        response: &NativeSyntaxMaterializationResponse,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String>;

    fn on_error(
        &mut self,
        error: &str,
        retrying: bool,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String>;
}

pub(crate) fn execute_refresh_with_policy(
    observer: &mut impl RefreshObserver,
    event_count: usize,
    paths: &BTreeSet<String>,
    full_rescan: bool,
    policy: RefreshRetryPolicy,
    refresh: impl FnMut(Vec<String>) -> Result<NativeSyntaxMaterializationResponse, String>,
) -> Result<bool, String> {
    execute_refresh_with_policy_limit(
        observer,
        event_count,
        paths,
        full_rescan,
        policy,
        None,
        refresh,
    )
}

fn execute_refresh_with_policy_limit(
    observer: &mut impl RefreshObserver,
    event_count: usize,
    paths: &BTreeSet<String>,
    full_rescan: bool,
    policy: RefreshRetryPolicy,
    max_attempts: Option<usize>,
    mut refresh: impl FnMut(Vec<String>) -> Result<NativeSyntaxMaterializationResponse, String>,
) -> Result<bool, String> {
    let changed_paths = paths.len();
    if changed_paths == 0 && !full_rescan {
        return Ok(true);
    }

    let candidate_paths = if full_rescan {
        Vec::new()
    } else {
        paths.iter().cloned().collect::<Vec<_>>()
    };
    let mut delay = policy.initial_delay;
    let mut attempts = 0_usize;
    loop {
        attempts = attempts.saturating_add(1);
        observer.before_attempt(event_count, changed_paths)?;
        match refresh(candidate_paths.clone()) {
            Ok(response) => {
                observer.on_success(&response, event_count, changed_paths)?;
                return Ok(true);
            }
            Err(error) => {
                let retrying = is_retryable_refresh_failure(&error);
                observer.on_error(&error, retrying, event_count, changed_paths)?;
                if !retrying || max_attempts.is_some_and(|max| attempts >= max.max(1)) {
                    return Ok(false);
                }
                thread::sleep(delay);
                delay = delay.saturating_mul(2).min(policy.max_delay);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RefreshStatusMetrics<'a> {
    backend: &'a str,
    event_count: usize,
    changed_paths: usize,
    rebuilt: usize,
    deleted: usize,
    database_written: bool,
    overflow_count: usize,
    filtered_event_count: usize,
    phase_high_water_marks: &'a BTreeMap<String, u64>,
    spill_bytes: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct RefreshStatus {
    pub(crate) enabled: bool,
    /// Whether the refresh task is still running and able to make progress.
    ///
    /// This is deliberately separate from `enabled`: a configured service can
    /// remain enabled after its detached task has stopped.
    pub(crate) task_alive: bool,
    /// Supervised task state.  The values are part of the structured status
    /// contract: starting, running, retrying, blocked, stopped, standby, and
    /// disabled.
    pub(crate) state: String,
    pub(crate) role: String,
    pub(crate) leader_pid: Option<u32>,
    pub(crate) worker_pid: Option<u32>,
    pub(crate) backend: String,
    /// The source root selected by the refresh runtime, when resolution has
    /// completed.  This is kept separate from the storage paths so clients
    /// can diagnose split repository identity.
    pub(crate) effective_root: Option<PathBuf>,
    pub(crate) refreshing: bool,
    pub(crate) pending: bool,
    pub(crate) last_refresh_unix_ms: Option<u128>,
    pub(crate) last_successful_reconciliation_unix_ms: Option<u128>,
    pub(crate) oldest_pending_unix_ms: Option<u128>,
    pub(crate) next_retry_unix_ms: Option<u128>,
    pub(crate) reconcile_interval_ms: u64,
    /// Monotonic input epoch and the latest epoch acknowledged by a
    /// successful reconciliation.  A success may acknowledge only the epoch
    /// captured when its attempt began, preserving edits received while it
    /// was running.
    pub(crate) dirty_epoch: u64,
    pub(crate) reconciled_epoch: u64,
    pub(crate) last_error: Option<String>,
    pub(crate) last_error_count: usize,
    pub(crate) last_retry_unix_ms: Option<u128>,
    pub(crate) last_event_count: usize,
    pub(crate) last_changed_paths: usize,
    pub(crate) last_rebuilt: usize,
    pub(crate) last_deleted: usize,
    pub(crate) last_database_written: bool,
    pub(crate) coalesced_event_count: usize,
    pub(crate) filtered_event_count: usize,
    pub(crate) overflow_count: usize,
    pub(crate) deduplicated_refresh_count: usize,
    pub(crate) last_noop_reason: Option<String>,
    pub(crate) worker_memory_mib: u64,
    pub(crate) rust_memory_mib: u64,
    pub(crate) spill_chunk_mib: u64,
    pub(crate) max_parallelism: usize,
    pub(crate) phase_high_water_marks: BTreeMap<String, u64>,
    pub(crate) spill_bytes: u64,
    /// Epoch captured by the currently running reconciliation.  This is an
    /// implementation detail and is intentionally omitted from JSON status.
    refreshing_epoch: u64,
    blocked_retry_count: u32,
}

impl Default for RefreshStatus {
    fn default() -> Self {
        Self {
            enabled: true,
            task_alive: true,
            state: "starting".to_string(),
            role: "starting".to_string(),
            leader_pid: None,
            worker_pid: None,
            backend: "starting".to_string(),
            effective_root: None,
            refreshing: false,
            pending: false,
            last_refresh_unix_ms: None,
            last_successful_reconciliation_unix_ms: None,
            oldest_pending_unix_ms: None,
            next_retry_unix_ms: None,
            reconcile_interval_ms: crate::api::context::DEFAULT_RECONCILE_INTERVAL_MS,
            dirty_epoch: 0,
            reconciled_epoch: 0,
            last_error: None,
            last_error_count: 0,
            last_retry_unix_ms: None,
            last_event_count: 0,
            last_changed_paths: 0,
            last_rebuilt: 0,
            last_deleted: 0,
            last_database_written: false,
            coalesced_event_count: 0,
            filtered_event_count: 0,
            overflow_count: 0,
            deduplicated_refresh_count: 0,
            last_noop_reason: None,
            worker_memory_mib: crate::api::context::DEFAULT_WORKER_MEMORY_MIB,
            rust_memory_mib: crate::api::context::DEFAULT_RUST_MEMORY_MIB,
            spill_chunk_mib: crate::api::context::DEFAULT_SPILL_CHUNK_MIB,
            max_parallelism: crate::api::context::DEFAULT_MAX_PARALLELISM,
            phase_high_water_marks: BTreeMap::new(),
            spill_bytes: 0,
            refreshing_epoch: 0,
            blocked_retry_count: 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RefreshState {
    status: Mutex<RefreshStatus>,
}

impl RefreshState {
    pub(crate) fn with_config(config: RefreshServiceConfig) -> Self {
        let status = RefreshStatus {
            worker_memory_mib: config.worker_memory_mib,
            rust_memory_mib: config.rust_memory_mib,
            spill_chunk_mib: config.spill_chunk_mib,
            max_parallelism: config.max_parallelism,
            reconcile_interval_ms: config
                .reconcile_interval
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            ..RefreshStatus::default()
        };
        Self {
            status: Mutex::new(status),
        }
    }

    pub(crate) fn snapshot(&self) -> RefreshStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| RefreshStatus {
                enabled: false,
                task_alive: false,
                state: "stopped".to_string(),
                role: "failed".to_string(),
                leader_pid: None,
                worker_pid: None,
                backend: "failed".to_string(),
                effective_root: None,
                refreshing: false,
                pending: true,
                last_refresh_unix_ms: None,
                last_successful_reconciliation_unix_ms: None,
                oldest_pending_unix_ms: Some(unix_ms()),
                next_retry_unix_ms: None,
                reconcile_interval_ms: 0,
                dirty_epoch: 0,
                reconciled_epoch: 0,
                last_error: Some("refresh status lock poisoned".to_string()),
                last_error_count: 1,
                last_retry_unix_ms: None,
                last_event_count: 0,
                last_changed_paths: 0,
                last_rebuilt: 0,
                last_deleted: 0,
                last_database_written: false,
                coalesced_event_count: 0,
                filtered_event_count: 0,
                overflow_count: 0,
                deduplicated_refresh_count: 0,
                last_noop_reason: None,
                worker_memory_mib: 0,
                rust_memory_mib: 0,
                spill_chunk_mib: 0,
                max_parallelism: 0,
                phase_high_water_marks: BTreeMap::new(),
                spill_bytes: 0,
                refreshing_epoch: 0,
                blocked_retry_count: 0,
            })
    }

    pub(crate) fn as_json(&self) -> serde_json::Value {
        let status = self.snapshot();
        json!({
            "enabled": status.enabled,
            "task_alive": status.task_alive,
            "state": status.state,
            "role": status.role,
            "leader_pid": status.leader_pid,
            "worker_pid": status.worker_pid,
            "backend": status.backend,
            "effective_root": status.effective_root,
            "refreshing": status.refreshing,
            "pending": status.pending,
            "last_refresh_unix_ms": status.last_refresh_unix_ms,
            "last_successful_reconciliation_unix_ms": status.last_successful_reconciliation_unix_ms,
            "oldest_pending_unix_ms": status.oldest_pending_unix_ms,
            "next_retry_unix_ms": status.next_retry_unix_ms,
            "reconcile_interval_ms": status.reconcile_interval_ms,
            "dirty_epoch": status.dirty_epoch,
            "reconciled_epoch": status.reconciled_epoch,
            "last_error": status.last_error,
            "last_error_count": status.last_error_count,
            "last_retry_unix_ms": status.last_retry_unix_ms,
            "last_event_count": status.last_event_count,
            "last_changed_paths": status.last_changed_paths,
            "last_rebuilt": status.last_rebuilt,
            "last_deleted": status.last_deleted,
            "last_database_written": status.last_database_written,
            "coalesced_event_count": status.coalesced_event_count,
            "filtered_event_count": status.filtered_event_count,
            "overflow_count": status.overflow_count,
            "deduplicated_refresh_count": status.deduplicated_refresh_count,
            "last_noop_reason": status.last_noop_reason,
            "memory_limits": {
                "worker_memory_mib": status.worker_memory_mib,
                "rust_memory_mib": status.rust_memory_mib,
                "spill_chunk_mib": status.spill_chunk_mib,
                "max_parallelism": status.max_parallelism,
            },
            "phase_high_water_marks": status.phase_high_water_marks,
            "spill_bytes": status.spill_bytes,
        })
    }

    pub(crate) fn mark_leader(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = true;
            status.state = "running".to_string();
            status.role = "leader".to_string();
            status.leader_pid = Some(std::process::id());
            status.enabled = true;
        }
    }

    pub(crate) fn mark_standby(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = true;
            status.state = "standby".to_string();
            status.role = "standby".to_string();
            status.leader_pid = None;
            status.worker_pid = None;
            status.backend = "standby".to_string();
            status.refreshing = false;
            status.enabled = true;
        }
    }

    /// Record the source root selected by the refresh runtime.
    pub(crate) fn set_effective_root(&self, root: PathBuf) {
        if let Ok(mut status) = self.status.lock() {
            status.effective_root = Some(root);
        }
    }

    pub(crate) fn set_reconcile_interval(&self, interval: Duration) {
        if let Ok(mut status) = self.status.lock() {
            status.reconcile_interval_ms = interval.as_millis().min(u128::from(u64::MAX)) as u64;
        }
    }

    pub(crate) fn set_materialization_limits(&self, config: RefreshServiceConfig) {
        if let Ok(mut status) = self.status.lock() {
            status.worker_memory_mib = config.worker_memory_mib;
            status.rust_memory_mib = config.rust_memory_mib;
            status.spill_chunk_mib = config.spill_chunk_mib;
            status.max_parallelism = config.max_parallelism;
        }
    }

    pub(crate) fn set_backend(&self, backend: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = true;
            if status.state != "standby" {
                status.state = "running".to_string();
            }
            status.backend = backend.to_string();
            status.enabled = true;
            status.last_error = None;
            status.next_retry_unix_ms = None;
        }
    }

    pub(crate) fn set_error(&self, backend: &str, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = true;
            if backend == "failed" {
                // The detached service calls this form only after its task
                // has actually terminated.  Fallback errors use another
                // backend (currently `poll`) and must keep reporting the
                // still-live owner while the fallback loop takes over.
                status.task_alive = false;
                status.state = "stopped".to_string();
                status.role = "stopped".to_string();
                status.leader_pid = None;
                status.worker_pid = None;
                status.refreshing = false;
            } else {
                status.task_alive = true;
                if status.state != "standby" {
                    status.state = "running".to_string();
                }
            }
            status.next_retry_unix_ms = None;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
        }
    }

    pub(crate) fn disable(&self, backend: &str, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = false;
            status.task_alive = false;
            status.state = "disabled".to_string();
            status.role = "disabled".to_string();
            status.leader_pid = None;
            status.worker_pid = None;
            status.refreshing = false;
            status.pending = false;
            status.next_retry_unix_ms = None;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
        }
    }

    pub(crate) fn mark_policy_disabled(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.enabled = false;
            status.task_alive = true;
            status.state = "disabled".to_string();
            status.role = "disabled".to_string();
            status.leader_pid = None;
            status.worker_pid = None;
            status.refreshing = false;
            status.next_retry_unix_ms = None;
        }
    }

    pub(crate) fn mark_policy_disabled_until(&self, next_retry_unix_ms: u128) {
        self.mark_policy_disabled();
        if let Ok(mut status) = self.status.lock() {
            status.next_retry_unix_ms = Some(next_retry_unix_ms);
        }
    }

    pub(crate) fn mark_pending(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.pending = true;
            if status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(unix_ms());
            }
        }
    }

    /// Mark a new source change epoch.  The returned epoch can be captured by
    /// an in-flight reconciliation and acknowledged after it completes.
    pub(crate) fn mark_dirty(&self) -> u64 {
        self.mark_dirty_at(unix_ms())
    }

    fn mark_dirty_at(&self, timestamp_unix_ms: u128) -> u64 {
        if let Ok(mut status) = self.status.lock() {
            status.dirty_epoch = status.dirty_epoch.saturating_add(1);
            status.pending = true;
            if status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(timestamp_unix_ms);
            }
            status.dirty_epoch
        } else {
            0
        }
    }

    /// Acknowledge work through `epoch` after a successful reconciliation.
    /// A newer dirty epoch remains pending and keeps its original age.
    #[allow(dead_code)]
    pub(crate) fn acknowledge_reconciliation(&self, epoch: u64) {
        if let Ok(mut status) = self.status.lock() {
            Self::acknowledge_reconciliation_locked(&mut status, epoch, unix_ms());
        }
    }

    fn acknowledge_reconciliation_locked(
        status: &mut RefreshStatus,
        epoch: u64,
        successful_at_unix_ms: u128,
    ) {
        let acknowledged = epoch.min(status.dirty_epoch);
        status.reconciled_epoch = status.reconciled_epoch.max(acknowledged);
        status.last_successful_reconciliation_unix_ms = Some(successful_at_unix_ms);
        if status.reconciled_epoch >= status.dirty_epoch {
            status.pending = false;
            status.oldest_pending_unix_ms = None;
        } else {
            status.pending = true;
        }
    }

    /// Record a retry schedule supplied by a supervising loop.
    pub(crate) fn mark_retrying(&self, next_retry_unix_ms: u128) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = true;
            status.state = "retrying".to_string();
            status.refreshing = false;
            status.pending = true;
            if status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(unix_ms());
            }
            status.next_retry_unix_ms = Some(next_retry_unix_ms);
        }
    }

    /// Mark a live task as blocked on a non-retryable error while retaining
    /// unsatisfied work for later recovery.
    pub(crate) fn mark_blocked(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = true;
            status.state = "blocked".to_string();
            status.role = "blocked".to_string();
            status.leader_pid = None;
            status.worker_pid = None;
            status.refreshing = false;
            if status.pending && status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(unix_ms());
            }
            status.next_retry_unix_ms = None;
        }
    }

    pub(crate) fn mark_blocked_until(&self, next_retry_unix_ms: u128) {
        self.mark_blocked();
        if let Ok(mut status) = self.status.lock() {
            status.next_retry_unix_ms = Some(next_retry_unix_ms);
        }
    }

    /// Mark the refresh task as terminal.  Terminal tasks must not retain
    /// leadership or worker identity in status.
    pub(crate) fn mark_stopped(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = false;
            status.state = "stopped".to_string();
            status.role = "stopped".to_string();
            status.leader_pid = None;
            status.worker_pid = None;
            status.refreshing = false;
            if status.pending && status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(unix_ms());
            }
            status.next_retry_unix_ms = None;
        }
    }

    pub(crate) fn set_worker_pid(&self, pid: Option<u32>) {
        if let Ok(mut status) = self.status.lock() {
            status.worker_pid = pid;
        }
    }

    pub(crate) fn mark_refreshing(&self, backend: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.task_alive = true;
            status.state = "running".to_string();
            status.refreshing_epoch = status.dirty_epoch;
            status.backend = backend.to_string();
            status.refreshing = true;
            status.pending = true;
            if status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(unix_ms());
            }
            status.next_retry_unix_ms = None;
            status.last_error = None;
        }
    }

    pub(crate) fn mark_refresh_error(
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
            status.pending = true;
            if status.oldest_pending_unix_ms.is_none() {
                status.oldest_pending_unix_ms = Some(unix_ms());
            }
            if retrying {
                status.task_alive = true;
                status.state = "retrying".to_string();
                status.next_retry_unix_ms = None;
            } else {
                status.task_alive = true;
                status.state = "blocked".to_string();
                status.role = "blocked".to_string();
                status.leader_pid = None;
                status.worker_pid = None;
                status.blocked_retry_count = status.blocked_retry_count.saturating_add(1);
                let shift = status.blocked_retry_count.saturating_sub(1).min(4);
                let delay_seconds = 5_u64.saturating_mul(1_u64 << shift);
                status.next_retry_unix_ms =
                    Some(unix_ms().saturating_add(u128::from(delay_seconds.min(60)) * 1_000));
            }
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
            status.last_retry_unix_ms = retrying.then_some(unix_ms());
            if retrying {
                status.next_retry_unix_ms = None;
            }
            status.last_event_count = event_count;
            status.last_changed_paths = changed_paths;
        }
    }

    pub(crate) fn mark_refreshed(&self, metrics: RefreshStatusMetrics<'_>) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = metrics.backend.to_string();
            status.task_alive = true;
            status.state = "running".to_string();
            if status.role == "blocked" {
                status.role = "leader".to_string();
                status.leader_pid = Some(std::process::id());
            }
            status.refreshing = false;
            status.last_refresh_unix_ms = Some(unix_ms());
            status.next_retry_unix_ms = None;
            status.last_error = None;
            status.last_error_count = 0;
            status.last_retry_unix_ms = None;
            status.blocked_retry_count = 0;
            status.last_event_count = metrics.event_count;
            status.last_changed_paths = metrics.changed_paths;
            status.last_rebuilt = metrics.rebuilt;
            status.last_deleted = metrics.deleted;
            status.last_database_written = metrics.database_written;
            status.coalesced_event_count = status
                .coalesced_event_count
                .saturating_add(metrics.event_count.saturating_sub(1));
            status.overflow_count = status.overflow_count.saturating_add(metrics.overflow_count);
            status.filtered_event_count = status
                .filtered_event_count
                .saturating_add(metrics.filtered_event_count);
            let acknowledged_epoch = status.refreshing_epoch.min(status.dirty_epoch);
            let successful_at = status.last_refresh_unix_ms.unwrap_or_else(unix_ms);
            Self::acknowledge_reconciliation_locked(&mut status, acknowledged_epoch, successful_at);
            if metrics.database_written {
                status.last_noop_reason = None;
                status.phase_high_water_marks = metrics.phase_high_water_marks.clone();
                status.spill_bytes = metrics.spill_bytes;
            } else {
                status.deduplicated_refresh_count =
                    status.deduplicated_refresh_count.saturating_add(1);
                status.last_noop_reason = Some("active_generation_current".to_string());
            }
        }
    }
}

pub(crate) fn start_refresh_service(
    selector: RepoSelector,
    config: RefreshServiceConfig,
) -> Arc<RefreshState> {
    let state = Arc::new(RefreshState::with_config(config));
    let thread_state = Arc::clone(&state);
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_refresh_service(selector, config, &thread_state)
        }));
        match result {
            Ok(Err(error)) => {
                thread_state.set_error("failed", error.clone());
                eprintln!(
                    "{}",
                    json!({"event": "repository.refresh_error", "message": error})
                );
            }
            Ok(Ok(())) if refresh_task_should_stop(&thread_state) => thread_state.mark_stopped(),
            Err(_) => thread_state.set_error(
                "failed",
                "refresh supervisor panicked; retry requires service restart".to_string(),
            ),
            _ => {}
        }
    });
    state
}

fn run_refresh_service(
    selector: RepoSelector,
    config: RefreshServiceConfig,
    state: &Arc<RefreshState>,
) -> Result<(), String> {
    let mut identity = None;
    let mut retry_delay = Duration::from_millis(250);
    let mut blocked_delay = Duration::from_secs(5);
    loop {
        if refresh_task_should_stop(state) {
            return Ok(());
        }
        if identity.is_none() {
            match RepositoryIdentity::capture(&selector) {
                Ok(captured) => identity = Some(captured),
                Err(error) => {
                    state.set_error("identity", error);
                    let delay = blocked_delay;
                    state.mark_blocked_until(unix_ms().saturating_add(delay.as_millis()));
                    if supervisor_wait(state, delay)? {
                        return Ok(());
                    }
                    blocked_delay = blocked_delay.saturating_mul(2).min(Duration::from_secs(60));
                    continue;
                }
            }
        }
        if let Err(error) = identity.as_ref().expect("identity captured").validate() {
            state.set_error("identity", error);
            let delay = blocked_delay;
            state.mark_blocked_until(unix_ms().saturating_add(delay.as_millis()));
            if supervisor_wait(state, delay)? {
                return Ok(());
            }
            blocked_delay = blocked_delay.saturating_mul(2).min(Duration::from_secs(60));
            continue;
        }
        let runtime = match resolve_refresh_runtime(&selector) {
            Ok(runtime) => runtime,
            Err(error) => {
                state.set_error("config", error);
                let delay = blocked_delay;
                state.mark_blocked_until(unix_ms().saturating_add(delay.as_millis()));
                if supervisor_wait(state, delay)? {
                    return Ok(());
                }
                blocked_delay = blocked_delay.saturating_mul(2).min(Duration::from_secs(60));
                continue;
            }
        };
        state.set_effective_root(runtime.repo_root.clone());
        let effective_config = match resolve_service_config(&runtime, config) {
            Ok(config) => config,
            Err(error) => {
                state.set_error("config", error);
                let delay = blocked_delay;
                state.mark_blocked_until(unix_ms().saturating_add(delay.as_millis()));
                if supervisor_wait(state, delay)? {
                    return Ok(());
                }
                blocked_delay = blocked_delay.saturating_mul(2).min(Duration::from_secs(60));
                continue;
            }
        };
        state.set_reconcile_interval(effective_config.reconcile_interval);
        state.set_materialization_limits(effective_config);
        if effective_config.policy == GraphRefreshPolicy::Off {
            let delay = Duration::from_secs(5);
            state.mark_policy_disabled_until(unix_ms().saturating_add(delay.as_millis()));
            if supervisor_wait(state, delay)? {
                return Ok(());
            }
            continue;
        }
        if let Err(error) = runtime.require_graph_write() {
            state.disable("disabled", error);
            return Ok(());
        }
        let lock_path = refresh_lock_path(&runtime);
        let lease_result = try_open_locked(&lock_path, LockMode::Exclusive).map_err(|error| {
            format!(
                "failed to acquire refresh ownership {}: {error}",
                lock_path.display()
            )
        });
        match lease_result {
            Err(error) => {
                state.set_error("lock", error);
                let delay = blocked_delay;
                state.mark_blocked_until(unix_ms().saturating_add(delay.as_millis()));
                if supervisor_wait(state, delay)? {
                    return Ok(());
                }
                blocked_delay = blocked_delay.saturating_mul(2).min(Duration::from_secs(60));
            }
            Ok(Some(lease)) => {
                state.mark_leader();
                match run_refresh_leader(selector.clone(), state, runtime, lease, effective_config)
                {
                    Ok(()) => {
                        retry_delay = Duration::from_millis(250);
                        blocked_delay = Duration::from_secs(5);
                        if refresh_task_should_stop(state) {
                            return Ok(());
                        }
                    }
                    Err(error) => {
                        let retryable = is_retryable_refresh_failure(&error)
                            || error.contains("refresh configuration changed");
                        if retryable {
                            let delay = retry_delay;
                            state.mark_retrying(unix_ms().saturating_add(delay.as_millis()));
                            if supervisor_wait(state, delay)? {
                                return Ok(());
                            }
                            retry_delay = retry_delay.saturating_mul(2).min(Duration::from_secs(5));
                        } else {
                            state.set_error("blocked", error);
                            let delay = blocked_delay;
                            state.mark_blocked_until(unix_ms().saturating_add(delay.as_millis()));
                            if supervisor_wait(state, delay)? {
                                return Ok(());
                            }
                            blocked_delay =
                                blocked_delay.saturating_mul(2).min(Duration::from_secs(60));
                        }
                    }
                }
            }
            Ok(None) => {
                state.mark_standby();
                if supervisor_wait(state, refresh_election_delay())? {
                    return Ok(());
                }
            }
        }
    }
}

fn supervisor_wait(state: &Arc<RefreshState>, delay: Duration) -> Result<bool, String> {
    let started = Instant::now();
    while started.elapsed() < delay {
        if refresh_task_should_stop(state) {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(50).min(delay.saturating_sub(started.elapsed())));
    }
    Ok(false)
}

fn refresh_lock_path(runtime: &crate::api::context::RepoRuntime) -> PathBuf {
    match runtime.storage_root.as_ref() {
        Some(storage_root) => ManagedLayout::new(storage_root).refresh_lock_path(),
        None => DirectLayout::new(&runtime.db_path, &runtime.manifest_path).refresh_lock_path(),
    }
}

fn refresh_election_delay() -> Duration {
    REFRESH_ELECTION_INTERVAL
        .saturating_add(Duration::from_millis(u64::from(std::process::id() % 251)))
}

fn default_reconcile_interval() -> Duration {
    Duration::from_millis(crate::api::context::DEFAULT_RECONCILE_INTERVAL_MS)
}

/// The coordinator owns the externally visible `Arc<RefreshState>`, while the
/// service thread owns one temporary strong reference.  Once the coordinator
/// owner is dropped only that thread reference remains, so bounded watcher
/// waits can terminate without retaining the task forever.
fn refresh_task_should_stop(state: &Arc<RefreshState>) -> bool {
    Arc::strong_count(state) == 1
}

fn run_refresh_leader(
    selector: RepoSelector,
    state: &Arc<RefreshState>,
    runtime: crate::api::context::RepoRuntime,
    _lease: RefreshLease,
    config: RefreshServiceConfig,
) -> Result<(), String> {
    let config = resolve_service_config(&runtime, config)?;
    state.set_reconcile_interval(config.reconcile_interval);
    state.set_materialization_limits(config);
    let materialize_options = MaterializeOptions {
        source_root: Some(runtime.repo_root.clone()),
        config: runtime.config_path.clone(),
        db: Some(runtime.db_path.clone()),
        manifest: Some(runtime.manifest_path.clone()),
        storage_root: runtime.storage_root.clone(),
        mode: "changed".to_string(),
        include_fts: config.include_fts,
        semantic_enrichment: config.semantic_enrichment,
        semantic_provider_mode: "local_only".to_string(),
        use_git: false,
        worker_memory_mib: Some(config.worker_memory_mib),
        rust_memory_mib: Some(config.rust_memory_mib),
        spill_chunk_mib: Some(config.spill_chunk_mib),
        max_parallelism: Some(config.max_parallelism),
        intent: MaterializationIntent::Refresh,
        ..MaterializeOptions::default()
    };
    let execution = RefreshExecutionPlan::new(selector, materialize_options.clone())?;
    let filter = Arc::new(WatchEventFilter::from_options(
        &runtime.repo_root,
        &materialize_options,
    )?);
    let loop_config = RefreshLoopConfig {
        poll_interval: Duration::from_millis(500),
        debounce: Duration::from_millis(250),
        max_wait: Duration::from_millis(1_000),
        max_iterations: None,
    };
    let service = RefreshServiceContext::new(state, &execution);
    let startup_batch = WatchChangeBatch {
        full_rescan: true,
        ..Default::default()
    };
    let startup = |backend: &str| -> Result<(), String> {
        state.mark_dirty();
        if service.refresh_batch(backend, &startup_batch)? {
            Ok(())
        } else {
            Err(state
                .snapshot()
                .last_error
                .unwrap_or_else(|| "startup repository reconciliation failed".to_string()))
        }
    };
    let dirty_state = Some(Arc::downgrade(state));

    match config.backend {
        GraphRefreshBackend::Poll => {
            state.set_backend("poll");
            startup("poll")?;
            run_service_poll_loop(
                state,
                loop_config,
                &execution,
                &filter,
                config.reconcile_interval,
            )
        }
        GraphRefreshBackend::Native | GraphRefreshBackend::Auto => {
            let native = start_native_watcher(&runtime.repo_root, Arc::clone(&filter), dirty_state);
            let (watcher, rx, overflowed) = match native {
                Ok(parts) => parts,
                Err(error) if config.backend == GraphRefreshBackend::Auto => {
                    state.set_error("poll", error);
                    startup("poll")?;
                    return run_service_poll_loop(
                        state,
                        loop_config,
                        &execution,
                        &filter,
                        config.reconcile_interval,
                    );
                }
                Err(error) => return Err(error),
            };
            let probe = probe_native_watcher(&runtime.repo_root, &filter, &rx)?;
            if !probe.delivered && config.backend == GraphRefreshBackend::Auto {
                drop(watcher);
                state.set_error(
                    "poll",
                    probe
                        .reason
                        .unwrap_or_else(|| "native probe failed".to_string()),
                );
                startup("poll")?;
                return run_service_poll_loop(
                    state,
                    loop_config,
                    &execution,
                    &filter,
                    config.reconcile_interval,
                );
            }
            state.set_backend("native");
            startup("native")?;
            let watch_runtime =
                RefreshWatchRuntime::new(service, loop_config, &filter, config.reconcile_interval);
            match run_service_native_loop(watch_runtime, watcher, rx, overflowed, probe.queued) {
                Ok(()) => Ok(()),
                Err(error)
                    if config.backend == GraphRefreshBackend::Auto
                        && !error.contains("configuration changed") =>
                {
                    state.set_error("poll", error);
                    run_service_poll_loop(
                        state,
                        loop_config,
                        &execution,
                        &filter,
                        config.reconcile_interval,
                    )
                }
                Err(error) => Err(error),
            }
        }
    }
}

fn resolve_service_config(
    runtime: &crate::api::context::RepoRuntime,
    mut configured: RefreshServiceConfig,
) -> Result<RefreshServiceConfig, String> {
    let Some(config_path) = runtime.config_path.as_deref() else {
        return Ok(configured);
    };
    if !config_path.exists() {
        return Ok(configured);
    }
    let install = read_install_config(config_path)?;
    let overrides = configured.explicit_overrides;
    if !overrides.policy {
        configured.policy = install.refresh.policy;
    }
    if !overrides.include_fts {
        configured.include_fts = install.materialization.include_fts;
    }
    if !overrides.semantic_enrichment {
        configured.semantic_enrichment = install.materialization.semantic_enrichment;
    }
    if !overrides.worker_memory_mib {
        configured.worker_memory_mib = install.materialization.worker_memory_mib;
    }
    if !overrides.rust_memory_mib {
        configured.rust_memory_mib = install.materialization.rust_memory_mib;
    }
    if !overrides.spill_chunk_mib {
        configured.spill_chunk_mib = install.materialization.spill_chunk_mib;
    }
    if !overrides.max_parallelism {
        configured.max_parallelism = install.materialization.max_parallelism;
    }
    if !overrides.backend {
        configured.backend = install.refresh.backend;
    }
    if !overrides.reconcile_interval {
        configured.reconcile_interval =
            Duration::from_millis(install.refresh.reconcile_interval_ms);
    }
    Ok(configured)
}

fn run_service_native_loop(
    runtime: RefreshWatchRuntime<'_>,
    watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    overflowed: Arc<AtomicBool>,
    queued: VecDeque<WatchMessage>,
) -> Result<(), String> {
    run_native_watch_until(
        runtime.config,
        runtime.filter,
        NativeWatchResources {
            _watcher: watcher,
            rx,
            overflowed,
            queued,
        },
        Some(runtime.reconcile_interval),
        WatchLoopHooks {
            should_stop: &|| refresh_task_should_stop(runtime.service.state),
            restart_on_config: true,
            before_refresh: &|batch: &WatchChangeBatch| {
                if batch.event_count == 0 && batch.has_changes() {
                    runtime.service.state.mark_dirty();
                }
            },
            retry_delay: &|| runtime.service.retry_delay(),
        },
        &mut |batch| runtime.service.refresh_batch("native", batch),
    )
}

fn run_service_poll_loop(
    state: &Arc<RefreshState>,
    config: RefreshLoopConfig,
    execution: &RefreshExecutionPlan,
    filter: &WatchEventFilter,
    reconcile_interval: Duration,
) -> Result<(), String> {
    state.set_backend("poll");
    let service = RefreshServiceContext::new(state, execution);
    let runtime = RefreshWatchRuntime::new(service, config, filter, reconcile_interval);
    run_poll_watch_until(
        runtime.config,
        runtime.filter,
        Some(runtime.reconcile_interval),
        WatchLoopHooks {
            should_stop: &|| refresh_task_should_stop(state),
            restart_on_config: true,
            before_refresh: &|batch: &WatchChangeBatch| {
                if batch.has_changes() {
                    state.mark_dirty();
                }
            },
            retry_delay: &|| runtime.service.retry_delay(),
        },
        &mut |batch| runtime.service.refresh_batch("poll", batch),
    )
}

struct StateRefreshObserver<'a> {
    state: &'a Arc<RefreshState>,
    backend: &'a str,
    overflow_count: usize,
    filtered_event_count: usize,
}

impl<'a> StateRefreshObserver<'a> {
    fn new(
        state: &'a Arc<RefreshState>,
        backend: &'a str,
        overflow_count: usize,
        filtered_event_count: usize,
    ) -> Self {
        Self {
            state,
            backend,
            overflow_count,
            filtered_event_count,
        }
    }
}

impl RefreshObserver for StateRefreshObserver<'_> {
    fn before_attempt(&mut self, _event_count: usize, _changed_paths: usize) -> Result<(), String> {
        self.state.mark_pending();
        self.state.mark_refreshing(self.backend);
        Ok(())
    }

    fn on_success(
        &mut self,
        response: &NativeSyntaxMaterializationResponse,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        self.state.mark_refreshed(RefreshStatusMetrics {
            backend: self.backend,
            event_count,
            changed_paths,
            rebuilt: response.diff.rebuild_paths().len(),
            deleted: response.diff.deleted.len(),
            database_written: response.database_written,
            overflow_count: self.overflow_count,
            filtered_event_count: self.filtered_event_count,
            phase_high_water_marks: &response.phase_high_water_marks,
            spill_bytes: response.spill_bytes,
        });
        Ok(())
    }

    fn on_error(
        &mut self,
        error: &str,
        retrying: bool,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        self.state.mark_refresh_error(
            self.backend,
            event_count,
            changed_paths,
            error.to_string(),
            retrying,
        );
        Ok(())
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
    use crate::storage::layout::DirectLayout;
    use crate::storage::locks::{try_open_locked, LockMode};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    fn skipped_response() -> NativeSyntaxMaterializationResponse {
        NativeSyntaxMaterializationResponse::skipped(
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
        )
    }

    #[test]
    fn refresh_status_reports_configured_limits_and_worker_placeholders() {
        let state = RefreshState::with_config(RefreshServiceConfig {
            policy: GraphRefreshPolicy::Leader,
            include_fts: false,
            semantic_enrichment: false,
            worker_memory_mib: 640,
            rust_memory_mib: 320,
            spill_chunk_mib: 16,
            max_parallelism: 1,
            backend: GraphRefreshBackend::Auto,
            reconcile_interval: Duration::from_secs(30),
            explicit_overrides: RefreshConfigOverrides::default(),
        });

        let status = state.as_json();
        assert_eq!(status["worker_pid"], serde_json::Value::Null);
        assert_eq!(status["memory_limits"]["worker_memory_mib"], 640);
        assert_eq!(status["memory_limits"]["rust_memory_mib"], 320);
        assert_eq!(status["memory_limits"]["spill_chunk_mib"], 16);
        assert_eq!(status["memory_limits"]["max_parallelism"], 1);
        assert_eq!(status["phase_high_water_marks"], serde_json::json!({}));
        assert_eq!(status["spill_bytes"], 0);
        assert_eq!(status["task_alive"], true);
        assert_eq!(status["state"], "starting");
        assert_eq!(status["effective_root"], serde_json::Value::Null);
        assert_eq!(
            status["last_successful_reconciliation_unix_ms"],
            serde_json::Value::Null
        );
        assert_eq!(status["oldest_pending_unix_ms"], serde_json::Value::Null);
        assert_eq!(status["next_retry_unix_ms"], serde_json::Value::Null);
        assert_eq!(status["reconcile_interval_ms"], 30_000);
        assert_eq!(status["dirty_epoch"], 0);
        assert_eq!(status["reconciled_epoch"], 0);
    }

    #[test]
    fn refresh_status_acknowledges_only_the_epoch_captured_by_a_refresh() {
        let state = RefreshState::with_config(RefreshServiceConfig::default());
        let first_epoch = state.mark_dirty_at(100);
        state.mark_refreshing("test");
        let second_epoch = state.mark_dirty_at(200);
        assert_eq!((first_epoch, second_epoch), (1, 2));

        let response = skipped_response();
        state.mark_refreshed(RefreshStatusMetrics {
            backend: "test",
            event_count: 1,
            changed_paths: 1,
            rebuilt: 0,
            deleted: 0,
            database_written: response.database_written,
            overflow_count: 0,
            filtered_event_count: 0,
            phase_high_water_marks: &response.phase_high_water_marks,
            spill_bytes: response.spill_bytes,
        });

        let status = state.snapshot();
        assert_eq!(status.dirty_epoch, 2);
        assert_eq!(status.reconciled_epoch, 1);
        assert!(status.pending);
        assert_eq!(status.oldest_pending_unix_ms, Some(100));
        assert!(status.last_successful_reconciliation_unix_ms.is_some());

        state.acknowledge_reconciliation(2);
        let status = state.snapshot();
        assert_eq!(status.reconciled_epoch, 2);
        assert!(!status.pending);
        assert_eq!(status.oldest_pending_unix_ms, None);
    }

    #[test]
    fn terminal_refresh_status_drops_leadership_and_keeps_work_pending() {
        let state = RefreshState::with_config(RefreshServiceConfig::default());
        state.mark_leader();
        state.mark_dirty_at(123);
        state.set_error("failed", "refresh task exited".to_string());

        let status = state.snapshot();
        assert!(!status.task_alive);
        assert_eq!(status.state, "stopped");
        assert_eq!(status.role, "stopped");
        assert_eq!(status.leader_pid, None);
        assert_eq!(status.worker_pid, None);
        assert!(status.pending);
        assert_eq!(status.oldest_pending_unix_ms, Some(123));
    }

    #[test]
    fn watch_filter_never_admits_configured_storage_root() {
        let root = unique_temp_dir("codebase-graph-rust-watch-filter-storage");
        let storage_root = root.join("graph-storage");
        fs::create_dir_all(&storage_root).unwrap();
        let options = MaterializeOptions {
            source_root: Some(root.clone()),
            storage_root: Some(storage_root.clone()),
            include_patterns: vec!["graph-storage/*".to_string()],
            ..MaterializeOptions::default()
        };
        let filter = WatchEventFilter::from_options(&root, &options).unwrap();

        assert_eq!(
            filter.relevant_path(&storage_root.join("generated.rs")),
            None
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn poll_snapshot_keeps_default_config_trigger_outside_protected_source_selection() {
        let root = unique_temp_dir("codebase-graph-refresh-config-trigger");
        let state_dir = root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join("config.json"), "{}\n").unwrap();
        fs::write(root.join("main.py"), "print('ok')\n").unwrap();
        let filter = WatchEventFilter::from_options(
            &root,
            &MaterializeOptions {
                source_root: Some(root.clone()),
                ..MaterializeOptions::default()
            },
        )
        .unwrap();

        let snapshot = watch_file_snapshot(&filter).unwrap();
        assert!(snapshot.contains_key(".codebaseGraph/config.json"));
        assert!(snapshot.contains_key("main.py"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pathless_rescan_event_forces_a_full_refresh_batch() {
        let root = unique_temp_dir("codebase-graph-refresh-pathless-rescan");
        fs::create_dir_all(&root).unwrap();
        let filter = WatchEventFilter::from_options(
            &root,
            &MaterializeOptions {
                source_root: Some(root.clone()),
                ..MaterializeOptions::default()
            },
        )
        .unwrap();
        let mut batch = WatchChangeBatch::default();
        let event = Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan);
        apply_watch_message(WatchMessage::Event(event), &filter, &mut batch).unwrap();
        assert!(batch.full_rescan);
        assert_eq!(batch.event_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configuration_change_restarts_before_a_failed_refresh_attempt() {
        let root = unique_temp_dir("codebase-graph-refresh-config-restart");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.py"), "print('ok')\n").unwrap();
        let options = MaterializeOptions {
            source_root: Some(root.clone()),
            ..MaterializeOptions::default()
        };
        let filter = WatchEventFilter::from_options(&root, &options).unwrap();
        let config_path = root.join(".codebaseGraph").join("config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "{}\n").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in_refresh = Arc::clone(&calls);
        let mut refresh = move |_batch: &WatchChangeBatch| {
            calls_in_refresh.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        };
        let before = |_batch: &WatchChangeBatch| {};
        let retry_delay = || Duration::from_secs(1);
        let hooks = WatchLoopHooks {
            should_stop: &|| false,
            restart_on_config: true,
            before_refresh: &before,
            retry_delay: &retry_delay,
        };
        let result = run_poll_watch_until(
            RefreshLoopConfig {
                poll_interval: Duration::from_millis(1),
                debounce: Duration::from_millis(0),
                max_wait: Duration::from_millis(2),
                max_iterations: Some(1),
            },
            &filter,
            Some(Duration::from_millis(1)),
            hooks,
            &mut refresh,
        );
        assert!(result
            .unwrap_err()
            .contains("configuration changed; rebuilding watcher"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configuration_fingerprint_detects_same_size_replacement() {
        let root = unique_temp_dir("codebase-graph-refresh-config-fingerprint");
        let state_dir = root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).unwrap();
        let config_path = state_dir.join("config.json");
        fs::write(&config_path, "{\"include\":[\"src/**\"]}\n").unwrap();
        let filter = WatchEventFilter::from_options(
            &root,
            &MaterializeOptions {
                source_root: Some(root.clone()),
                ..MaterializeOptions::default()
            },
        )
        .unwrap();
        assert!(!filter.configuration_changed());
        let original_metadata = fs::metadata(&config_path).unwrap();
        let original_modified = original_metadata.modified().unwrap();
        let original_len = original_metadata.len();
        fs::write(&config_path, "{\"include\":[\"lib/**\"]}\n").unwrap();
        fs::File::options()
            .write(true)
            .open(&config_path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(original_modified))
            .unwrap();
        let replaced_metadata = fs::metadata(&config_path).unwrap();
        assert_eq!(replaced_metadata.len(), original_len);
        assert_eq!(replaced_metadata.modified().unwrap(), original_modified);
        assert!(filter.configuration_changed());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn blocked_retry_deadline_grows_and_is_reported() {
        let state = RefreshState::with_config(RefreshServiceConfig::default());
        state.mark_refresh_error("poll", 1, 1, "permission denied".to_string(), false);
        let first = state
            .snapshot()
            .next_retry_unix_ms
            .expect("blocked work should schedule a retry");
        state.mark_refresh_error("poll", 1, 1, "permission denied".to_string(), false);
        let second = state
            .snapshot()
            .next_retry_unix_ms
            .expect("repeated blocked work should retain a retry");
        assert!(second >= first.saturating_add(4_000));
    }

    #[test]
    fn scheduler_waits_for_the_announced_blocked_retry_deadline() {
        let root = unique_temp_dir("codebase-graph-refresh-blocked-schedule");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("main.py");
        fs::write(&source, "print(0)\n").unwrap();
        let filter = WatchEventFilter::from_options(
            &root,
            &MaterializeOptions {
                source_root: Some(root.clone()),
                ..MaterializeOptions::default()
            },
        )
        .unwrap();
        let writer = thread::spawn({
            let source = source.clone();
            move || {
                thread::sleep(Duration::from_millis(2));
                fs::write(source, "print(1)\n").unwrap();
            }
        });
        let state = Arc::new(RefreshState::with_config(RefreshServiceConfig::default()));
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let observed_attempts = Arc::clone(&attempts);
        let state_for_refresh = Arc::clone(&state);
        let retry_delay = || {
            state
                .snapshot()
                .next_retry_unix_ms
                .and_then(|deadline| deadline.checked_sub(unix_ms()))
                .map(|millis| Duration::from_millis(millis as u64))
                .unwrap_or_else(|| Duration::from_secs(1))
        };
        let mut refresh = move |_batch: &WatchChangeBatch| {
            let mut attempts = observed_attempts.lock().unwrap();
            attempts.push(Instant::now());
            if attempts.len() == 1 {
                state_for_refresh.mark_blocked_until(unix_ms().saturating_add(20));
                Ok(false)
            } else {
                Ok(true)
            }
        };
        let before = |_batch: &WatchChangeBatch| {};
        let hooks = WatchLoopHooks {
            should_stop: &|| false,
            restart_on_config: false,
            before_refresh: &before,
            retry_delay: &retry_delay,
        };
        run_poll_watch_until(
            RefreshLoopConfig {
                poll_interval: Duration::from_millis(1),
                debounce: Duration::from_millis(0),
                max_wait: Duration::from_millis(2),
                max_iterations: Some(1),
            },
            &filter,
            Some(Duration::from_secs(60)),
            hooks,
            &mut refresh,
        )
        .unwrap();
        writer.join().unwrap();
        let attempts = attempts.lock().unwrap();
        assert_eq!(attempts.len(), 2);
        assert!(attempts[1].duration_since(attempts[0]) >= Duration::from_millis(15));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sustained_path_events_still_force_a_due_full_reconciliation() {
        let root = unique_temp_dir("codebase-graph-refresh-periodic-full");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("main.py");
        fs::write(&source, "print(0)\n").unwrap();
        let filter = WatchEventFilter::from_options(
            &root,
            &MaterializeOptions {
                source_root: Some(root.clone()),
                ..MaterializeOptions::default()
            },
        )
        .unwrap();
        let writer = thread::spawn({
            let source = source.clone();
            move || {
                for index in 1..=40 {
                    let _ = fs::write(&source, format!("print({index})\n"));
                    thread::sleep(Duration::from_millis(1));
                }
            }
        });
        let full_passes = Arc::new(AtomicUsize::new(0));
        let observed_full_passes = Arc::clone(&full_passes);
        let before = |_batch: &WatchChangeBatch| {};
        let retry_delay = || Duration::from_secs(1);
        let hooks = WatchLoopHooks {
            should_stop: &|| false,
            restart_on_config: false,
            before_refresh: &before,
            retry_delay: &retry_delay,
        };
        let mut refresh = move |batch: &WatchChangeBatch| {
            if batch.full_rescan {
                observed_full_passes.fetch_add(1, Ordering::SeqCst);
            }
            Ok(true)
        };
        let result = run_poll_watch_until(
            RefreshLoopConfig {
                poll_interval: Duration::from_millis(1),
                debounce: Duration::from_millis(0),
                max_wait: Duration::from_millis(2),
                max_iterations: Some(2),
            },
            &filter,
            Some(Duration::from_millis(2)),
            hooks,
            &mut refresh,
        );
        writer.join().unwrap();
        result.unwrap();
        assert!(full_passes.load(Ordering::SeqCst) >= 1);
        let _ = fs::remove_dir_all(root);
    }

    struct RecordingObserver {
        retries: Vec<(bool, String, usize, usize)>,
        successes: Vec<(usize, usize, usize)>,
    }

    impl RecordingObserver {
        fn new() -> Self {
            Self {
                retries: Vec::new(),
                successes: Vec::new(),
            }
        }
    }

    impl RefreshObserver for RecordingObserver {
        fn on_success(
            &mut self,
            response: &NativeSyntaxMaterializationResponse,
            event_count: usize,
            changed_paths: usize,
        ) -> Result<(), String> {
            self.successes.push((
                event_count,
                changed_paths,
                response.diff.rebuild_paths().len(),
            ));
            Ok(())
        }

        fn on_error(
            &mut self,
            error: &str,
            retrying: bool,
            event_count: usize,
            changed_paths: usize,
        ) -> Result<(), String> {
            self.retries
                .push((retrying, error.to_string(), event_count, changed_paths));
            Ok(())
        }
    }

    #[test]
    fn refresh_retry_policy_retries_transient_errors_before_success() {
        let attempts = AtomicUsize::new(0);
        let mut observer = RecordingObserver::new();
        let refreshed = execute_refresh_with_policy(
            &mut observer,
            2,
            &BTreeSet::from(["src/lib.rs".to_string()]),
            false,
            RefreshRetryPolicy {
                initial_delay: Duration::from_millis(0),
                max_delay: Duration::from_millis(0),
            },
            |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("IO exception: Could not set lock on file".to_string())
                } else {
                    Ok(skipped_response())
                }
            },
        )
        .unwrap();

        assert!(refreshed);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(observer.retries.len(), 1);
        assert!(observer.retries[0].0);
        assert_eq!(observer.successes, vec![(2, 1, 0)]);
    }

    #[test]
    fn refresh_retry_policy_stops_on_non_transient_errors() {
        let mut observer = RecordingObserver::new();
        let refreshed = execute_refresh_with_policy(
            &mut observer,
            1,
            &BTreeSet::from(["src/lib.rs".to_string()]),
            false,
            RefreshRetryPolicy {
                initial_delay: Duration::from_millis(0),
                max_delay: Duration::from_millis(0),
            },
            |_| Err("parser exploded".to_string()),
        )
        .unwrap();

        assert!(!refreshed);
        assert_eq!(
            observer.retries,
            vec![(false, "parser exploded".to_string(), 1, 1)]
        );
        assert!(observer.successes.is_empty());
    }

    #[test]
    fn refresh_execution_plan_reresolves_managed_v2_active_generation() {
        let root = unique_temp_dir("codebase-graph-refresh-managed-reresolve");
        let state = root.join(".codebaseGraph");
        let storage = state.join("storage");
        let generation_one = storage.join("generations").join("gen-one");
        let generation_two = storage.join("generations").join("gen-two");
        fs::create_dir_all(&generation_one).unwrap();
        fs::create_dir_all(&generation_two).unwrap();
        fs::write(generation_one.join("READY"), "ready\n").unwrap();
        fs::write(generation_two.join("READY"), "ready\n").unwrap();
        fs::write(generation_one.join("graph.ldb"), b"db-one").unwrap();
        fs::write(generation_two.join("graph.ldb"), b"db-two").unwrap();
        fs::write(generation_one.join("manifest.json"), "{}\n").unwrap();
        fs::write(generation_two.join("manifest.json"), "{}\n").unwrap();
        fs::write(
            generation_one.join("metadata.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "one",
                "created_at_ms": 0,
                "published_at_ms": 0,
                "logical_size_bytes": 0,
                "physical_size_bytes": 0,
                "node_count": 0,
                "edge_count": 0
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            generation_two.join("metadata.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "two",
                "created_at_ms": 0,
                "published_at_ms": 0,
                "logical_size_bytes": 0,
                "physical_size_bytes": 0,
                "node_count": 0,
                "edge_count": 0
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "repo_root": root,
                "storage_root": storage,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            storage.join("active.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "one",
                "activated_at_ms": 0,
            }))
            .unwrap(),
        )
        .unwrap();

        let selector = RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        };
        let runtime = resolve_runtime(&selector).unwrap();
        let plan = RefreshExecutionPlan::new(
            selector,
            MaterializeOptions {
                source_root: Some(runtime.repo_root.clone()),
                config: runtime.config_path.clone(),
                db: Some(runtime.db_path.clone()),
                manifest: Some(runtime.manifest_path.clone()),
                mode: "changed".to_string(),
                ..MaterializeOptions::default()
            },
        )
        .unwrap();

        let first = plan.resolve_options().unwrap();
        assert_eq!(
            first
                .db
                .as_deref()
                .map(fs::canonicalize)
                .transpose()
                .unwrap(),
            fs::canonicalize(generation_one.join("graph.ldb")).ok()
        );
        assert_eq!(
            first
                .manifest
                .as_deref()
                .map(fs::canonicalize)
                .transpose()
                .unwrap(),
            fs::canonicalize(generation_one.join("manifest.json")).ok()
        );

        fs::write(
            storage.join("active.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "two",
                "activated_at_ms": 1,
            }))
            .unwrap(),
        )
        .unwrap();

        let second = plan.resolve_options().unwrap();
        assert_eq!(
            second
                .db
                .as_deref()
                .map(fs::canonicalize)
                .transpose()
                .unwrap(),
            fs::canonicalize(generation_two.join("graph.ldb")).ok()
        );
        assert_eq!(
            second
                .manifest
                .as_deref()
                .map(fs::canonicalize)
                .transpose()
                .unwrap(),
            fs::canonicalize(generation_two.join("manifest.json")).ok()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_service_disables_legacy_v1_auto_refresh_with_remediation() {
        let root = unique_temp_dir("codebase-graph-refresh-legacy-disabled");
        let state_dir = root.join(".codebaseGraph");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(
            state_dir.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "repo_root": root,
            }))
            .unwrap(),
        )
        .unwrap();

        let state = start_refresh_service(
            RepoSelector {
                repo_root: Some(root.clone()),
                config_path: None,
                db_path: None,
                manifest_path: None,
            },
            RefreshServiceConfig::default(),
        );

        let mut snapshot = state.snapshot();
        for _ in 0..50 {
            if !snapshot.enabled {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            snapshot = state.snapshot();
        }

        assert!(!snapshot.enabled);
        assert_eq!(snapshot.backend, "disabled");
        assert!(snapshot.last_error.as_deref().is_some_and(|error| error
            .contains("legacy installed graph storage requires reinstall before writes")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn native_refresh_startup_reports_native_after_reconciliation() {
        let root = unique_temp_dir("codebase-graph-refresh-native-startup");
        let state_dir = root.join(".codebaseGraph");
        let storage_root = state_dir.join("storage");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn native_startup_regression() -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            state_dir.join("config.json"),
            serde_json::to_vec(&json!({
                "schema_version": 3,
                "repo_root": root,
                "storage_root": storage_root,
                "refresh": {"reconcile_interval_ms": 60_000},
            }))
            .unwrap(),
        )
        .unwrap();

        let selector = RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        };
        let state = start_refresh_service(
            selector.clone(),
            RefreshServiceConfig {
                backend: GraphRefreshBackend::Native,
                reconcile_interval: Duration::from_secs(60),
                explicit_overrides: RefreshConfigOverrides {
                    backend: true,
                    reconcile_interval: true,
                    ..RefreshConfigOverrides::default()
                },
                ..RefreshServiceConfig::default()
            },
        );

        let startup_deadline = Instant::now() + Duration::from_secs(20);
        let snapshot = loop {
            let snapshot = state.snapshot();
            if snapshot.last_successful_reconciliation_unix_ms.is_some()
                || snapshot.state == "blocked"
                || snapshot.state == "stopped"
            {
                break snapshot;
            }
            assert!(
                Instant::now() < startup_deadline,
                "native refresh startup did not complete: {:?}",
                snapshot
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(snapshot.backend, "native", "{snapshot:?}");
        assert_eq!(snapshot.state, "running", "{snapshot:?}");
        assert!(
            snapshot.last_successful_reconciliation_unix_ms.is_some(),
            "native startup did not record a successful reconciliation: {snapshot:?}"
        );
        assert_eq!(snapshot.reconcile_interval_ms, 60_000);
        assert!(snapshot.last_error.is_none(), "{snapshot:?}");

        let runtime = resolve_refresh_runtime(&selector).unwrap();
        let lock_path = refresh_lock_path(&runtime);
        drop(runtime);
        drop(state);

        let release_deadline = Instant::now() + Duration::from_secs(5);
        let released = loop {
            match try_open_locked(&lock_path, LockMode::Exclusive) {
                Ok(Some(lease)) => {
                    drop(lease);
                    break true;
                }
                Ok(None) if Instant::now() < release_deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => break false,
                Err(error) => panic!("failed to inspect refresh lease: {error}"),
            }
        };
        let cleanup = fs::remove_dir_all(&root);
        assert!(released, "refresh service did not release its lease");
        assert!(cleanup.is_ok(), "refresh test cleanup failed: {cleanup:?}");
    }

    #[test]
    fn refresh_runtime_releases_startup_read_lease_before_entering_a_watch_loop() {
        let root = unique_temp_dir("codebase-graph-refresh-release-lease");
        let state = root.join(".codebaseGraph");
        let storage = state.join("storage");
        let generation_one = storage.join("generations").join("gen-one");
        fs::create_dir_all(&generation_one).unwrap();
        fs::write(generation_one.join("READY"), "ready\n").unwrap();
        fs::write(generation_one.join("graph.ldb"), b"db").unwrap();
        fs::write(generation_one.join("manifest.json"), "{}\n").unwrap();
        fs::write(generation_one.join("lease.lock"), b"").unwrap();
        fs::write(
            generation_one.join("metadata.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "one",
                "created_at_ms": 0,
                "published_at_ms": 0,
                "logical_size_bytes": 0,
                "physical_size_bytes": 0,
                "node_count": 0,
                "edge_count": 0
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "repo_root": root,
                "storage_root": storage,
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            storage.join("active.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "generation_id": "one",
                "activated_at_ms": 0,
            }))
            .unwrap(),
        )
        .unwrap();

        let runtime = resolve_refresh_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: None,
            manifest_path: None,
        })
        .unwrap();
        assert_eq!(runtime.active_generation.as_deref(), Some("one"));
        let exclusive = try_open_locked(generation_one.join("lease.lock"), LockMode::Exclusive)
            .unwrap()
            .expect("refresh runtime must not retain a generation read lease");
        drop(exclusive);
        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_runtime_releases_direct_read_lease_before_entering_a_watch_loop() {
        let root = unique_temp_dir("codebase-graph-refresh-release-direct-lease");
        fs::create_dir_all(&root).unwrap();
        let db_path = root.join("graph.ldb");
        let manifest_path = root.join("manifest.json");
        fs::write(&db_path, b"db").unwrap();
        fs::write(&manifest_path, "{}\n").unwrap();

        let runtime = resolve_refresh_runtime(&RepoSelector {
            repo_root: Some(root.clone()),
            config_path: None,
            db_path: Some(db_path.clone()),
            manifest_path: Some(manifest_path.clone()),
        })
        .unwrap();
        assert_eq!(runtime.storage_format(), "direct");
        let lock_path = DirectLayout::new(db_path, manifest_path).writer_lock_path();
        let exclusive = try_open_locked(lock_path, LockMode::Exclusive)
            .unwrap()
            .expect("refresh runtime must not retain a direct read lease");
        drop(exclusive);
        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }
}
