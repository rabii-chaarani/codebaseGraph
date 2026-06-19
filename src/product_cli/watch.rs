use super::*;

pub(super) fn run_watch<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
    let options = WatchOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", watch_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let backend = options.backend;
    let loop_config = WatchLoopConfig {
        poll_ms: options.poll_ms,
        debounce_ms: options.debounce_ms,
        max_iterations: options.max_iterations,
    };
    let once = options.once;
    let mut materialize_options = options.materialize;
    let source_root = materialize_options
        .source_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .map_err(|error| format!("failed to resolve source root: {error}"))?;
    materialize_options.source_root = Some(source_root.clone());
    let filter = WatchEventFilter::from_options(&source_root, &materialize_options)?;
    if once {
        let (_, response) = materialize(&materialize_options)?;
        write_watch_event(stdout, "refreshed", None, 0, 0, &response)?;
        return Ok(());
    }
    match backend {
        WatchBackend::Poll => run_poll_watch(stdout, loop_config, &materialize_options, &filter),
        WatchBackend::Native => {
            let (watcher, rx) = start_native_watcher(&source_root)?;
            run_native_watch(
                stdout,
                loop_config,
                &materialize_options,
                &filter,
                watcher,
                rx,
                VecDeque::new(),
            )
        }
        WatchBackend::Auto => match start_native_watcher(&source_root) {
            Ok((watcher, rx)) => {
                let probe = probe_native_watcher(&source_root, &filter, &rx)?;
                if probe.delivered {
                    run_native_watch(
                        stdout,
                        loop_config,
                        &materialize_options,
                        &filter,
                        watcher,
                        rx,
                        probe.queued,
                    )
                } else {
                    drop(watcher);
                    write_watch_status(stdout, "fallback", "poll", probe.reason.as_deref())?;
                    run_poll_watch(stdout, loop_config, &materialize_options, &filter)
                }
            }
            Err(error) => {
                write_watch_status(stdout, "fallback", "poll", Some("watcher_start_failed"))?;
                let _ = error;
                run_poll_watch(stdout, loop_config, &materialize_options, &filter)
            }
        },
    }
}

pub(super) fn start_native_watcher(
    source_root: &Path,
) -> Result<(notify::RecommendedWatcher, Receiver<WatchMessage>), String> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let message = match result {
            Ok(event) => WatchMessage::Event(event),
            Err(error) => WatchMessage::Error(error.to_string()),
        };
        let _ = tx.send(message);
    })
    .map_err(|error| format!("failed to start filesystem watcher: {error}"))?;
    watcher
        .watch(source_root, RecursiveMode::Recursive)
        .map_err(|error| format!("failed to watch {}: {error}", source_root.display()))?;
    Ok((watcher, rx))
}

pub(super) fn run_native_watch<W: Write>(
    stdout: &mut W,
    loop_config: WatchLoopConfig,
    materialize_options: &MaterializeOptions,
    filter: &WatchEventFilter,
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchMessage>,
    mut queued: VecDeque<WatchMessage>,
) -> Result<(), String> {
    let mut refreshes = 0_usize;
    loop {
        let first = match queued.pop_front() {
            Some(message) => message,
            None => rx
                .recv()
                .map_err(|error| format!("filesystem watcher stopped: {error}"))?,
        };
        let batch = match collect_watch_batch(
            first,
            &rx,
            &mut queued,
            filter,
            Duration::from_millis(loop_config.debounce_ms),
            watch_max_wait(loop_config.debounce_ms),
        )? {
            Some(batch) => batch,
            None => continue,
        };
        let (_, response) = materialize(materialize_options)?;
        write_watch_event(
            stdout,
            "refreshed",
            Some("native"),
            batch.event_count,
            batch.paths.len(),
            &response,
        )?;
        refreshes += 1;
        if loop_config
            .max_iterations
            .is_some_and(|max| refreshes >= max)
        {
            return Ok(());
        }
    }
}

pub(super) fn run_poll_watch<W: Write>(
    stdout: &mut W,
    loop_config: WatchLoopConfig,
    materialize_options: &MaterializeOptions,
    filter: &WatchEventFilter,
) -> Result<(), String> {
    let mut previous_snapshot = watch_file_snapshot(filter)?;
    let mut refreshes = 0_usize;
    loop {
        let batch = collect_poll_batch(
            filter,
            &mut previous_snapshot,
            Duration::from_millis(loop_config.poll_ms),
            Duration::from_millis(loop_config.debounce_ms),
            watch_max_wait(loop_config.debounce_ms),
        )?;
        let (_, response) = materialize(materialize_options)?;
        write_watch_event(
            stdout,
            "refreshed",
            Some("poll"),
            batch.event_count,
            batch.paths.len(),
            &response,
        )?;
        refreshes += 1;
        if loop_config
            .max_iterations
            .is_some_and(|max| refreshes >= max)
        {
            return Ok(());
        }
    }
}
#[derive(Debug)]
pub(super) struct WatchOptions {
    pub(super) materialize: MaterializeOptions,
    pub(super) backend: WatchBackend,
    pub(super) poll_ms: u64,
    pub(super) debounce_ms: u64,
    pub(super) max_iterations: Option<usize>,
    pub(super) once: bool,
    pub(super) help: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WatchBackend {
    Auto,
    Native,
    Poll,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WatchLoopConfig {
    pub(super) poll_ms: u64,
    pub(super) debounce_ms: u64,
    pub(super) max_iterations: Option<usize>,
}

impl WatchBackend {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "native" => Ok(Self::Native),
            "poll" => Ok(Self::Poll),
            _ => Err("--watch-backend must be auto, native, or poll".to_string()),
        }
    }
}

impl WatchOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        let mut materialize_args = Vec::new();
        let mut backend = WatchBackend::Auto;
        let mut poll_ms = 500_u64;
        let mut debounce_ms = 250_u64;
        let mut max_iterations = None;
        let mut once = false;
        let mut help = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    help = true;
                    index += 1;
                }
                "--poll-ms" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--poll-ms requires an integer".to_string())?;
                    poll_ms = value
                        .parse()
                        .map_err(|error| format!("--poll-ms must be an integer: {error}"))?;
                    index += 2;
                }
                "--watch-backend" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--watch-backend requires auto, native, or poll".to_string()
                    })?;
                    backend = WatchBackend::parse(value)?;
                    index += 2;
                }
                "--debounce-ms" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--debounce-ms requires an integer".to_string())?;
                    debounce_ms = value
                        .parse()
                        .map_err(|error| format!("--debounce-ms must be an integer: {error}"))?;
                    index += 2;
                }
                "--max-iterations" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--max-iterations requires an integer".to_string())?;
                    max_iterations = Some(value.parse().map_err(|error| {
                        format!("--max-iterations must be an integer: {error}")
                    })?);
                    index += 2;
                }
                "--once" => {
                    once = true;
                    index += 1;
                }
                _ => {
                    materialize_args.push(args[index].clone());
                    index += 1;
                }
            }
        }
        Ok(Self {
            materialize: MaterializeOptions::parse_with_command(&materialize_args, "watch")?,
            backend,
            poll_ms,
            debounce_ms,
            max_iterations,
            once,
            help,
        })
    }
}

#[derive(Debug)]
pub(super) struct SetupOptions {
    pub(super) repo_root: PathBuf,
    pub(super) mode: String,
    pub(super) include_fts: bool,
    pub(super) semantic_enrichment: bool,
    pub(super) semantic_provider_mode: String,
    pub(super) mcp_client: String,
    pub(super) mcp_config_path: Option<PathBuf>,
    pub(super) skip_mcp_config: bool,
    pub(super) dry_run: bool,
    pub(super) instructions_target: String,
    pub(super) help: bool,
}

impl SetupOptions {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            repo_root: PathBuf::from("."),
            mode: "changed".to_string(),
            include_fts: true,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            mcp_client: "codex".to_string(),
            mcp_config_path: None,
            skip_mcp_config: false,
            dry_run: false,
            instructions_target: "auto".to_string(),
            help: false,
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => {
                    options.help = true;
                    index += 1;
                }
                "--repo-root" | "--source-root" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--repo-root requires a path".to_string())?;
                    options.repo_root = PathBuf::from(value);
                    index += 2;
                }
                "--mode" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mode requires full or changed".to_string())?;
                    if value != "full" && value != "changed" {
                        return Err("--mode must be full or changed".to_string());
                    }
                    options.mode = value.clone();
                    index += 2;
                }
                "--mcp-client" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mcp-client requires a client id".to_string())?;
                    if value != "none" && !supported_install_clients().contains(&value.as_str()) {
                        return Err(format!(
                            "--mcp-client must be none or one of {}",
                            supported_install_clients().join(", ")
                        ));
                    }
                    options.mcp_client = value.clone();
                    index += 2;
                }
                "--mcp-config-path" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--mcp-config-path requires a path".to_string())?;
                    options.mcp_config_path = Some(PathBuf::from(value));
                    index += 2;
                }
                "--skip-mcp-config" => {
                    options.skip_mcp_config = true;
                    index += 1;
                }
                "--dry-run" => {
                    options.dry_run = true;
                    index += 1;
                }
                "--instructions-target" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--instructions-target requires auto, agents, claude, or skip".to_string()
                    })?;
                    if !matches!(value.as_str(), "auto" | "agents" | "claude" | "skip") {
                        return Err(
                            "--instructions-target must be auto, agents, claude, or skip"
                                .to_string(),
                        );
                    }
                    options.instructions_target = value.clone();
                    index += 2;
                }
                "--no-fts" => {
                    options.include_fts = false;
                    index += 1;
                }
                "--no-semantic-enrichment" => {
                    options.semantic_enrichment = false;
                    index += 1;
                }
                "--semantic-provider-mode" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "--semantic-provider-mode requires local_only".to_string()
                    })?;
                    if value != "local_only" {
                        return Err("--semantic-provider-mode must be local_only".to_string());
                    }
                    options.semantic_provider_mode = value.clone();
                    index += 2;
                }
                "--json" => {
                    index += 1;
                }
                other => {
                    return Err(format!("unknown setup option: {other}\n\n{}", setup_help()));
                }
            }
        }
        Ok(options)
    }
}
#[derive(Debug)]
pub(super) enum WatchMessage {
    Event(Event),
    Error(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct WatchChangeBatch {
    pub(super) paths: BTreeSet<String>,
    pub(super) event_count: usize,
}

#[derive(Debug, Default)]
pub(super) struct WatchProbeOutcome {
    pub(super) delivered: bool,
    pub(super) queued: VecDeque<WatchMessage>,
    pub(super) reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WatchFileState {
    pub(super) modified_nanos: u128,
    pub(super) len: u64,
}

type WatchFileSnapshot = BTreeMap<String, WatchFileState>;

#[derive(Debug)]
pub(super) struct WatchEventFilter {
    pub(super) source_root: PathBuf,
    pub(super) current_dir: PathBuf,
    pub(super) excluded_parts: BTreeSet<String>,
    pub(super) include_patterns: Vec<String>,
    pub(super) exclude_patterns: Vec<String>,
    pub(super) ignore_patterns: Vec<String>,
}

impl WatchEventFilter {
    pub(super) fn from_options(
        source_root: &Path,
        options: &MaterializeOptions,
    ) -> Result<Self, String> {
        let paths = GraphStatePaths::derive(source_root);
        let config_rules = read_materialization_config_rules(&paths.config_path)?;
        let mut include_patterns = config_rules.include_patterns;
        include_patterns.extend(options.include_patterns.clone());
        let mut exclude_patterns = config_rules.exclude_patterns;
        exclude_patterns.extend(options.exclude_patterns.clone());
        Ok(Self {
            source_root: source_root.to_path_buf(),
            current_dir: env::current_dir().unwrap_or_else(|_| source_root.to_path_buf()),
            excluded_parts: default_excluded_parts().into_iter().collect(),
            include_patterns,
            exclude_patterns,
            ignore_patterns: read_codebase_graph_ignore(source_root)?,
        })
    }

    pub(super) fn relevant_paths(&self, event: &Event) -> BTreeSet<String> {
        if !watch_event_refreshes(event) {
            return BTreeSet::new();
        }
        event
            .paths
            .iter()
            .filter_map(|path| self.relevant_path(path))
            .collect()
    }

    pub(super) fn relevant_path(&self, path: &Path) -> Option<String> {
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
        if self.ignored_by_patterns(&relative) {
            None
        } else {
            Some(relative)
        }
    }

    pub(super) fn relative_event_path(&self, path: &Path) -> Option<PathBuf> {
        if let Ok(relative) = path.strip_prefix(&self.source_root) {
            return Some(relative.to_path_buf());
        }
        if path.is_relative() {
            let absolute = self.current_dir.join(path);
            if let Ok(relative) = absolute.strip_prefix(&self.source_root) {
                return Some(relative.to_path_buf());
            }
            return Some(path.to_path_buf());
        }
        None
    }

    pub(super) fn ignored_by_patterns(&self, relative_path: &str) -> bool {
        if !self.include_patterns.is_empty()
            && !watch_matches_any_pattern(relative_path, &self.include_patterns)
        {
            return true;
        }
        watch_matches_any_pattern(relative_path, &self.ignore_patterns)
            || watch_matches_any_pattern(relative_path, &self.exclude_patterns)
    }
}

pub(super) fn watch_event_refreshes(event: &Event) -> bool {
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

pub(super) fn probe_native_watcher(
    source_root: &Path,
    filter: &WatchEventFilter,
    rx: &Receiver<WatchMessage>,
) -> Result<WatchProbeOutcome, String> {
    let timeout = watch_probe_timeout();
    let probe_dir = source_root.join(".codebaseGraph").join("watch-probe");
    let probe_path = probe_dir.join(format!(
        "probe-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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

pub(super) fn watch_probe_timeout() -> Duration {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(750))
}

pub(super) fn watch_probe_skip_write() -> bool {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE").is_ok_and(|value| value == "1")
}

pub(super) fn watch_event_is_under_dir(
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

pub(super) fn watch_path_is_under_dir(
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

pub(super) fn collect_watch_batch(
    first: WatchMessage,
    rx: &Receiver<WatchMessage>,
    queued: &mut VecDeque<WatchMessage>,
    filter: &WatchEventFilter,
    debounce: Duration,
    max_wait: Duration,
) -> Result<Option<WatchChangeBatch>, String> {
    let mut batch = WatchChangeBatch::default();
    apply_watch_message(first, filter, &mut batch)?;
    if batch.paths.is_empty() {
        return Ok(None);
    }

    let started = Instant::now();
    let mut last_relevant = started;
    loop {
        let elapsed = started.elapsed();
        if elapsed >= max_wait {
            return Ok(Some(batch));
        }
        let quiet_elapsed = last_relevant.elapsed();
        if quiet_elapsed >= debounce {
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
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(Some(batch)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("filesystem watcher stopped".to_string())
            }
        }
    }
}

pub(super) fn apply_watch_message(
    message: WatchMessage,
    filter: &WatchEventFilter,
    batch: &mut WatchChangeBatch,
) -> Result<(), String> {
    match message {
        WatchMessage::Event(event) => {
            let paths = filter.relevant_paths(&event);
            if !paths.is_empty() {
                batch.event_count += 1;
                batch.paths.extend(paths);
            }
            Ok(())
        }
        WatchMessage::Error(error) => Err(format!("filesystem watcher error: {error}")),
    }
}

pub(super) fn collect_poll_batch(
    filter: &WatchEventFilter,
    previous_snapshot: &mut WatchFileSnapshot,
    poll_interval: Duration,
    debounce: Duration,
    max_wait: Duration,
) -> Result<WatchChangeBatch, String> {
    loop {
        std::thread::sleep(poll_interval);
        let current_snapshot = watch_file_snapshot(filter)?;
        let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
        *previous_snapshot = current_snapshot;
        if changed_paths.is_empty() {
            continue;
        }

        let started = Instant::now();
        let mut last_relevant = started;
        let mut batch = WatchChangeBatch {
            paths: changed_paths,
            event_count: 1,
        };
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
            std::thread::sleep(timeout);
            let current_snapshot = watch_file_snapshot(filter)?;
            let changed_paths = watch_snapshot_diff(previous_snapshot, &current_snapshot);
            *previous_snapshot = current_snapshot;
            if !changed_paths.is_empty() {
                batch.paths.extend(changed_paths);
                batch.event_count += 1;
                last_relevant = Instant::now();
            }
        }
    }
}

pub(super) fn watch_file_snapshot(filter: &WatchEventFilter) -> Result<WatchFileSnapshot, String> {
    let mut snapshot = BTreeMap::new();
    watch_file_snapshot_inner(filter, &filter.source_root, &mut snapshot)?;
    Ok(snapshot)
}

pub(super) fn watch_file_snapshot_inner(
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

pub(super) fn watch_snapshot_diff(
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

pub(super) fn watch_max_wait(debounce_ms: u64) -> Duration {
    Duration::from_secs(5).max(Duration::from_millis(debounce_ms.saturating_mul(10)))
}

pub(super) fn watch_matches_any_pattern(path: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .map(|pattern| pattern.trim())
        .filter(|pattern| !pattern.is_empty() && !pattern.starts_with('#'))
        .any(|pattern| watch_glob_matches(path, pattern))
}

pub(super) fn watch_glob_matches(path: &str, pattern: &str) -> bool {
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

pub(super) fn watch_normalize_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .to_string()
}

pub(super) fn watch_wildcard_match(text: &str, pattern: &str) -> bool {
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

pub(super) fn write_watch_event<W: Write>(
    stdout: &mut W,
    event: &str,
    backend: Option<&str>,
    event_count: usize,
    changed_paths: usize,
    response: &NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    let backend = backend
        .map(|backend| format!(" backend={backend}"))
        .unwrap_or_default();
    writeln!(
        stdout,
        "watch event={}{} event_count={} changed_paths={} rebuilt={} deleted={} skipped={} database_written={}",
        event,
        backend,
        event_count,
        changed_paths,
        response.diff.rebuild_paths().len(),
        response.diff.deleted.len(),
        response.skipped,
        response.database_written
    )
    .map_err(|error| error.to_string())
}

pub(super) fn write_watch_status<W: Write>(
    stdout: &mut W,
    event: &str,
    backend: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    if let Some(reason) = reason {
        writeln!(
            stdout,
            "watch event={event} backend={backend} reason={reason}"
        )
    } else {
        writeln!(stdout, "watch event={event} backend={backend}")
    }
    .map_err(|error| error.to_string())
}

pub(super) fn scan_source_snapshots(root: &Path) -> Vec<(String, Option<&'static str>)> {
    let mut snapshots = Vec::new();
    scan_source_snapshots_inner(root, root, &mut snapshots);
    snapshots.sort_by(|left, right| left.0.cmp(&right.0));
    snapshots
}

pub(super) fn scan_source_snapshots_inner(
    root: &Path,
    directory: &Path,
    snapshots: &mut Vec<(String, Option<&'static str>)>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if default_excluded_parts().iter().any(|part| part == name) {
            continue;
        }
        if path.is_dir() {
            scan_source_snapshots_inner(root, &path, snapshots);
        } else if path.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
            snapshots.push((relative.to_string(), language_for_path(&path)));
        }
    }
}

pub(super) fn language_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("py") => Some("python"),
        Some("rs") => Some("rust"),
        Some("go") => Some("go"),
        Some("c") | Some("h") => Some("c"),
        Some("cc") | Some("cpp") | Some("cxx") | Some("hpp") | Some("hh") => Some("cpp"),
        Some("f") | Some("f90") | Some("f95") | Some("for") => Some("fortran"),
        _ => None,
    }
}
