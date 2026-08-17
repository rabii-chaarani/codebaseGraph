use crate::{
    api::{
        context::{resolve_runtime, RepoPaths},
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
    profiles::ProfileSet,
    protocol::NativeSyntaxMaterializationResponse,
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
        Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_PENDING_PATHS: usize = 4_096;
const MAX_PENDING_PATH_BYTES: usize = 1024 * 1024;
const REFRESH_ELECTION_INTERVAL: Duration = Duration::from_secs(1);

const GENERATED_PARTS: &[&str] = &[".astro", ".kwiki", ".scryer"];

#[derive(Clone, Copy, Debug)]
pub(crate) struct RefreshServiceConfig {
    pub(crate) include_fts: bool,
    pub(crate) semantic_enrichment: bool,
    pub(crate) worker_memory_mib: u64,
    pub(crate) rust_memory_mib: u64,
    pub(crate) spill_chunk_mib: u64,
    pub(crate) max_parallelism: usize,
}

impl Default for RefreshServiceConfig {
    fn default() -> Self {
        Self {
            include_fts: true,
            semantic_enrichment: true,
            worker_memory_mib: crate::api::context::DEFAULT_WORKER_MEMORY_MIB,
            rust_memory_mib: crate::api::context::DEFAULT_RUST_MEMORY_MIB,
            spill_chunk_mib: crate::api::context::DEFAULT_SPILL_CHUNK_MIB,
            max_parallelism: crate::api::context::DEFAULT_MAX_PARALLELISM,
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
        include_patterns.splice(0..0, config_rules.include_patterns);
        exclude_patterns.splice(0..0, config_rules.exclude_patterns);
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
            || relative.components().any(|component| {
                self.excluded_parts
                    .contains(component.as_os_str().to_string_lossy().as_ref())
            })
        {
            return false;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        let explicitly_included = watch_matches_any_pattern(&relative, &self.include_patterns);
        for generated_part in GENERATED_PARTS {
            if let Some(index) = relative.split('/').position(|part| part == *generated_part) {
                let generated_prefix = relative
                    .split('/')
                    .take(index + 1)
                    .collect::<Vec<_>>()
                    .join("/");
                let descendant_is_included = self.include_patterns.iter().any(|pattern| {
                    watch_normalize_pattern(pattern).starts_with(&format!("{generated_prefix}/"))
                });
                if !explicitly_included && !descendant_is_included {
                    return false;
                }
            }
        }
        if watch_matches_any_pattern(&relative, &self.ignore_patterns)
            || watch_matches_any_pattern(&relative, &self.exclude_patterns)
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
        if relative.components().any(|component| {
            self.excluded_parts
                .contains(component.as_os_str().to_string_lossy().as_ref())
        }) {
            return None;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        let explicitly_included = watch_matches_any_pattern(&relative, &self.include_patterns);
        if relative
            .split('/')
            .any(|part| GENERATED_PARTS.contains(&part))
            && !explicitly_included
        {
            return None;
        }
        if self.ignored_by_patterns(&relative)
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
        absolute == self.config_path
            || absolute == self.source_root.join(".codebaseGraphignore")
            || path == self.config_path
    }

    fn is_protected_path(&self, path: &Path) -> bool {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.current_dir.join(path)
        };
        self.protected_roots
            .iter()
            .any(|root| absolute.starts_with(root))
    }

    pub(crate) fn relative_event_path(&self, path: &Path) -> Option<PathBuf> {
        if let Ok(relative) = path.strip_prefix(&self.source_root) {
            return Some(relative.to_path_buf());
        }
        if path.is_relative() {
            let absolute = self.current_dir.join(path);
            if let Ok(relative) = absolute.strip_prefix(&self.source_root) {
                return Some(relative.to_path_buf());
            }
            #[cfg(windows)]
            {
                let absolute = normalize_windows_verbatim_path(&absolute);
                let source_root = normalize_windows_verbatim_path(&self.source_root);
                if let Ok(relative) = absolute.strip_prefix(source_root) {
                    return Some(relative.to_path_buf());
                }
            }
            return Some(path.to_path_buf());
        }
        None
    }

    pub(crate) fn ignored_by_patterns(&self, relative_path: &str) -> bool {
        if !self.include_patterns.is_empty()
            && !watch_matches_any_pattern(relative_path, &self.include_patterns)
        {
            return true;
        }
        watch_matches_any_pattern(relative_path, &self.ignore_patterns)
            || watch_matches_any_pattern(relative_path, &self.exclude_patterns)
    }
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
}

#[derive(Debug, Default)]
pub(crate) struct WatchProbeOutcome {
    pub(crate) delivered: bool,
    pub(crate) queued: VecDeque<WatchMessage>,
    pub(crate) reason: Option<String>,
}

pub(crate) fn start_native_watcher(
    source_root: &Path,
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
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let message = match result {
            Ok(event) => WatchMessage::Event(event),
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
    if path.starts_with(directory) {
        return true;
    }
    if path.is_relative() {
        return current_dir.join(path).starts_with(directory)
            || source_root.join(path).starts_with(directory);
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
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if filter.excluded_parts.contains(name) {
                continue;
            }
            watch_file_snapshot_inner(filter, &path, snapshot)?;
        } else if path.is_file() {
            let Some(relative_path) = filter.relevant_path(&path) else {
                continue;
            };
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
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
                relative_path,
                WatchFileState {
                    modified_nanos,
                    len: metadata.len(),
                },
            );
        }
    }
    Ok(())
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

pub(crate) fn collect_poll_batch(
    filter: &WatchEventFilter,
    previous_snapshot: &mut WatchFileSnapshot,
    poll_interval: Duration,
    debounce: Duration,
    max_wait: Duration,
) -> Result<WatchChangeBatch, String> {
    loop {
        thread::sleep(poll_interval);
        let current_snapshot = watch_file_snapshot(filter)?;
        let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
        *previous_snapshot = current_snapshot;
        if changed_paths.is_empty() {
            continue;
        }

        let started = Instant::now();
        let mut last_relevant = started;
        let mut batch = WatchChangeBatch {
            paths: BTreeSet::new(),
            event_count: 1,
            full_rescan: false,
            overflow_count: 0,
            filtered_event_count: 0,
            path_bytes: 0,
        };
        batch.extend_paths(changed_paths);
        loop {
            let elapsed = started.elapsed();
            if elapsed >= max_wait {
                return Ok(batch);
            }
            let quiet_elapsed = last_relevant.elapsed();
            if quiet_elapsed >= debounce {
                return Ok(batch);
            }
            let timeout = poll_interval
                .min(debounce.saturating_sub(quiet_elapsed))
                .min(max_wait.saturating_sub(elapsed));
            thread::sleep(timeout);
            let current_snapshot = watch_file_snapshot(filter)?;
            let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
            *previous_snapshot = current_snapshot;
            if !changed_paths.is_empty() {
                batch.extend_paths(changed_paths);
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
    let execution = RefreshExecutionPlan::new(request.repo.clone(), materialize_options.clone());

    if config.once {
        let response = execution.execute(Vec::new())?;
        return observer.on_success(None, &refresh_watch_summary(&response), 0, 0);
    }

    let filter = WatchEventFilter::from_options(&runtime.repo_root, &materialize_options)?;
    match config.backend {
        RefreshBackend::Poll => run_poll_watch(config.loop_config, &filter, |batch| {
            refresh_watch_batch(observer, "poll", &execution, batch)
        }),
        RefreshBackend::Native => {
            let (watcher, rx, overflowed) = start_native_watcher(&runtime.repo_root)?;
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
        RefreshBackend::Auto => match start_native_watcher(&runtime.repo_root) {
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
                    observer
                        .on_fallback("poll", probe.reason.as_deref().unwrap_or("probe_failed"))?;
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
        },
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
    let mut previous_snapshot = watch_file_snapshot(filter)?;
    let mut refreshes = 0_usize;
    loop {
        let batch = collect_poll_batch(
            filter,
            &mut previous_snapshot,
            config.poll_interval,
            config.debounce,
            config.max_wait,
        )?;
        if !refresh(&batch)? {
            continue;
        }
        refreshes += 1;
        if config.max_iterations.is_some_and(|max| refreshes >= max) {
            return Ok(());
        }
    }
}

pub(crate) fn run_native_watch(
    config: RefreshLoopConfig,
    filter: &WatchEventFilter,
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    overflowed: Arc<AtomicBool>,
    mut queued: VecDeque<WatchMessage>,
    mut refresh: impl FnMut(&WatchChangeBatch) -> Result<bool, String>,
) -> Result<(), String> {
    let mut refreshes = 0_usize;
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
            Some(&overflowed),
            &mut queued,
            filter,
            config.debounce,
            config.max_wait,
        )?
        else {
            continue;
        };
        if !refresh(&batch)? {
            continue;
        }
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
    base_options: MaterializeOptions,
}

impl RefreshExecutionPlan {
    fn new(selector: RepoSelector, base_options: MaterializeOptions) -> Self {
        Self {
            selector,
            base_options,
        }
    }

    fn resolve_options(&self) -> Result<MaterializeOptions, String> {
        let runtime = resolve_runtime(&self.selector)?;
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
    loop {
        observer.before_attempt(event_count, changed_paths)?;
        match refresh(candidate_paths.clone()) {
            Ok(response) => {
                observer.on_success(&response, event_count, changed_paths)?;
                return Ok(true);
            }
            Err(error) => {
                let retrying = is_retryable_refresh_failure(&error);
                observer.on_error(&error, retrying, event_count, changed_paths)?;
                if !retrying {
                    return Ok(false);
                }
                thread::sleep(delay);
                delay = delay.saturating_mul(2).min(policy.max_delay);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RefreshStatus {
    pub(crate) enabled: bool,
    pub(crate) role: String,
    pub(crate) leader_pid: Option<u32>,
    pub(crate) worker_pid: Option<u32>,
    pub(crate) backend: String,
    pub(crate) refreshing: bool,
    pub(crate) pending: bool,
    pub(crate) last_refresh_unix_ms: Option<u128>,
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
}

impl Default for RefreshStatus {
    fn default() -> Self {
        Self {
            enabled: true,
            role: "starting".to_string(),
            leader_pid: None,
            worker_pid: None,
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
        }
    }
}

#[derive(Debug)]
pub(crate) struct RefreshState {
    status: Mutex<RefreshStatus>,
    graph_lock: RwLock<()>,
}

impl RefreshState {
    pub(crate) fn with_config(config: RefreshServiceConfig) -> Self {
        let status = RefreshStatus {
            worker_memory_mib: config.worker_memory_mib,
            rust_memory_mib: config.rust_memory_mib,
            spill_chunk_mib: config.spill_chunk_mib,
            max_parallelism: config.max_parallelism,
            ..RefreshStatus::default()
        };
        Self {
            status: Mutex::new(status),
            graph_lock: RwLock::new(()),
        }
    }

    pub(crate) fn snapshot(&self) -> RefreshStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| RefreshStatus {
                enabled: false,
                role: "failed".to_string(),
                leader_pid: None,
                worker_pid: None,
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
            })
    }

    pub(crate) fn as_json(&self) -> serde_json::Value {
        let status = self.snapshot();
        json!({
            "enabled": status.enabled,
            "role": status.role,
            "leader_pid": status.leader_pid,
            "worker_pid": status.worker_pid,
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
            status.role = "leader".to_string();
            status.leader_pid = Some(std::process::id());
            status.enabled = true;
        }
    }

    pub(crate) fn mark_standby(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.role = "standby".to_string();
            status.leader_pid = None;
            status.backend = "standby".to_string();
            status.refreshing = false;
            status.enabled = true;
        }
    }

    pub(crate) fn read_guard(&self) -> Result<RwLockReadGuard<'_, ()>, String> {
        self.graph_lock
            .read()
            .map_err(|_| "refresh graph read lock poisoned".to_string())
    }

    pub(crate) fn write_guard(&self) -> Result<RwLockWriteGuard<'_, ()>, String> {
        self.graph_lock
            .write()
            .map_err(|_| "refresh graph write lock poisoned".to_string())
    }

    pub(crate) fn set_backend(&self, backend: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = true;
            status.last_error = None;
        }
    }

    pub(crate) fn set_error(&self, backend: &str, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = true;
            status.refreshing = false;
            status.pending = false;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
        }
    }

    pub(crate) fn disable(&self, backend: &str, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.enabled = false;
            status.role = "disabled".to_string();
            status.leader_pid = None;
            status.refreshing = false;
            status.pending = false;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
        }
    }

    pub(crate) fn mark_pending(&self) {
        if let Ok(mut status) = self.status.lock() {
            status.pending = true;
        }
    }

    pub(crate) fn mark_refreshing(&self, backend: &str) {
        if let Ok(mut status) = self.status.lock() {
            status.backend = backend.to_string();
            status.refreshing = true;
            status.pending = false;
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
            status.pending = retrying;
            status.last_error = Some(error);
            status.last_error_count = status.last_error_count.saturating_add(1);
            status.last_retry_unix_ms = retrying.then_some(unix_ms());
            status.last_event_count = event_count;
            status.last_changed_paths = changed_paths;
        }
    }

    pub(crate) fn mark_refreshed(
        &self,
        backend: &str,
        event_count: usize,
        changed_paths: usize,
        rebuilt: usize,
        deleted: usize,
        database_written: bool,
        overflow_count: usize,
        filtered_event_count: usize,
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
            status.coalesced_event_count = status
                .coalesced_event_count
                .saturating_add(event_count.saturating_sub(1));
            status.overflow_count = status.overflow_count.saturating_add(overflow_count);
            status.filtered_event_count = status
                .filtered_event_count
                .saturating_add(filtered_event_count);
            if database_written {
                status.last_noop_reason = None;
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
        if let Err(error) = run_refresh_service(selector, config, &thread_state) {
            thread_state.set_error("failed", error.clone());
            eprintln!(
                "{}",
                json!({"event": "repository.refresh_error", "message": error})
            );
        }
    });
    state
}

fn run_refresh_service(
    selector: RepoSelector,
    config: RefreshServiceConfig,
    state: &Arc<RefreshState>,
) -> Result<(), String> {
    let runtime = resolve_refresh_runtime(&selector)?;
    if let Err(error) = runtime.require_graph_write() {
        state.disable("disabled", error);
        return Ok(());
    }
    let lock_path = refresh_lock_path(&runtime);
    loop {
        match try_open_locked(&lock_path, LockMode::Exclusive).map_err(|error| {
            format!(
                "failed to acquire refresh ownership {}: {error}",
                lock_path.display()
            )
        })? {
            Some(lease) => {
                state.mark_leader();
                return run_refresh_leader(selector, state, runtime, lease, config);
            }
            None => {
                state.mark_standby();
                thread::sleep(refresh_election_delay());
            }
        }
    }
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

fn run_refresh_leader(
    selector: RepoSelector,
    state: &Arc<RefreshState>,
    runtime: crate::api::context::RepoRuntime,
    _lease: RefreshLease,
    config: RefreshServiceConfig,
) -> Result<(), String> {
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
        intent: MaterializationIntent::Refresh,
        ..MaterializeOptions::default()
    };
    let execution = RefreshExecutionPlan::new(selector, materialize_options.clone());
    let filter = WatchEventFilter::from_options(&runtime.repo_root, &materialize_options)?;
    let loop_config = RefreshLoopConfig {
        poll_interval: Duration::from_millis(500),
        debounce: Duration::from_millis(250),
        max_wait: Duration::from_millis(1_000),
        max_iterations: None,
    };

    if !refresh_batch_with_state(
        state,
        "startup",
        &execution,
        0,
        &BTreeSet::new(),
        true,
        0,
        0,
    )? {
        return Err("startup repository reconciliation failed".to_string());
    }

    match start_native_watcher(&runtime.repo_root) {
        Ok((watcher, rx, overflowed)) => {
            let probe = probe_native_watcher(&runtime.repo_root, &filter, &rx)?;
            if probe.delivered {
                state.set_backend("native");
                match run_service_native_loop(
                    state,
                    loop_config,
                    &execution,
                    &filter,
                    watcher,
                    rx,
                    overflowed,
                    probe.queued,
                ) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        state.set_error("poll", error);
                        let filter = WatchEventFilter::from_options(
                            &runtime.repo_root,
                            &materialize_options,
                        )?;
                        run_service_poll_loop(state, loop_config, &execution, &filter)
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
                run_service_poll_loop(state, loop_config, &execution, &filter)
            }
        }
        Err(error) => {
            state.set_error("poll", error);
            run_service_poll_loop(state, loop_config, &execution, &filter)
        }
    }
}

fn run_service_native_loop(
    state: &Arc<RefreshState>,
    config: RefreshLoopConfig,
    execution: &RefreshExecutionPlan,
    filter: &WatchEventFilter,
    watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    overflowed: Arc<AtomicBool>,
    queued: VecDeque<WatchMessage>,
) -> Result<(), String> {
    run_native_watch(config, filter, watcher, rx, overflowed, queued, |batch| {
        refresh_batch_with_state(
            state,
            "native",
            execution,
            batch.event_count,
            &batch.paths,
            batch.full_rescan,
            batch.overflow_count,
            batch.filtered_event_count,
        )
    })
}

fn run_service_poll_loop(
    state: &Arc<RefreshState>,
    config: RefreshLoopConfig,
    execution: &RefreshExecutionPlan,
    filter: &WatchEventFilter,
) -> Result<(), String> {
    state.set_backend("poll");
    run_poll_watch(config, filter, |batch| {
        refresh_batch_with_state(
            state,
            "poll",
            execution,
            batch.event_count,
            &batch.paths,
            batch.full_rescan,
            batch.overflow_count,
            batch.filtered_event_count,
        )
    })
}

fn refresh_batch_with_state(
    state: &Arc<RefreshState>,
    backend: &str,
    execution: &RefreshExecutionPlan,
    event_count: usize,
    paths: &BTreeSet<String>,
    full_rescan: bool,
    overflow_count: usize,
    filtered_event_count: usize,
) -> Result<bool, String> {
    let mut observer =
        StateRefreshObserver::new(state, backend, overflow_count, filtered_event_count);
    execute_refresh_with_policy(
        &mut observer,
        event_count,
        paths,
        full_rescan,
        RefreshRetryPolicy::default(),
        |candidate_paths| execution.execute(candidate_paths),
    )
}

struct StateRefreshObserver<'a> {
    state: &'a Arc<RefreshState>,
    backend: &'a str,
    guard: Option<RwLockWriteGuard<'a, ()>>,
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
            guard: None,
            overflow_count,
            filtered_event_count,
        }
    }
}

impl RefreshObserver for StateRefreshObserver<'_> {
    fn before_attempt(&mut self, _event_count: usize, _changed_paths: usize) -> Result<(), String> {
        self.state.mark_pending();
        self.guard = Some(self.state.write_guard()?);
        self.state.mark_refreshing(self.backend);
        Ok(())
    }

    fn on_success(
        &mut self,
        response: &NativeSyntaxMaterializationResponse,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        self.guard.take();
        self.state.mark_refreshed(
            self.backend,
            event_count,
            changed_paths,
            response.diff.rebuild_paths().len(),
            response.diff.deleted.len(),
            response.database_written,
            self.overflow_count,
            self.filtered_event_count,
        );
        Ok(())
    }

    fn on_error(
        &mut self,
        error: &str,
        retrying: bool,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        self.guard.take();
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

fn watch_matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|pattern| pattern.trim())
        .filter(|pattern| !pattern.is_empty() && !pattern.starts_with('#'))
        .any(|pattern| watch_glob_matches(path, pattern))
}

fn watch_glob_matches(path: &str, pattern: &str) -> bool {
    let pattern = watch_normalize_pattern(pattern);
    if pattern.ends_with('/') {
        return path.starts_with(pattern.trim_end_matches('/'));
    }
    if !pattern.contains('/')
        && watch_wildcard_match(path.rsplit('/').next().unwrap_or(path), &pattern)
    {
        return true;
    }
    watch_wildcard_match(path, &pattern)
}

fn watch_normalize_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_string()
}

fn watch_wildcard_match(text: &str, pattern: &str) -> bool {
    let (mut text_index, mut pattern_index) = (0_usize, 0_usize);
    let mut star_index = None;
    let mut match_index = 0_usize;
    let text = text.as_bytes();
    let pattern = pattern.as_bytes();
    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            text_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            match_index = text_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            text_index = match_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
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
            include_fts: false,
            semantic_enrichment: false,
            worker_memory_mib: 640,
            rust_memory_mib: 320,
            spill_chunk_mib: 16,
            max_parallelism: 1,
        });

        let status = state.as_json();
        assert_eq!(status["worker_pid"], serde_json::Value::Null);
        assert_eq!(status["memory_limits"]["worker_memory_mib"], 640);
        assert_eq!(status["memory_limits"]["rust_memory_mib"], 320);
        assert_eq!(status["memory_limits"]["spill_chunk_mib"], 16);
        assert_eq!(status["memory_limits"]["max_parallelism"], 1);
        assert_eq!(status["phase_high_water_marks"], serde_json::json!({}));
        assert_eq!(status["spill_bytes"], 0);
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
        );

        let first = plan.resolve_options().unwrap();
        assert_eq!(first.db, Some(generation_one.join("graph.ldb")));
        assert_eq!(first.manifest, Some(generation_one.join("manifest.json")));

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
        assert_eq!(second.db, Some(generation_two.join("graph.ldb")));
        assert_eq!(second.manifest, Some(generation_two.join("manifest.json")));
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
