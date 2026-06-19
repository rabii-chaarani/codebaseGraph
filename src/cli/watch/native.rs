use super::{
    batch::collect_watch_batch,
    filter::WatchEventFilter,
    helpers::watch_max_wait,
    output::write_watch_event,
    types::{WatchMessage, WatchProbeOutcome},
    WatchLoopConfig,
};
use crate::cli::build::{materialize_candidate_paths, MaterializeOptions};
use notify::{Event, RecursiveMode, Watcher};
use std::{
    collections::VecDeque,
    env, fs,
    io::Write,
    path::Path,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub(in crate::cli) fn start_native_watcher(
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

pub(in crate::cli) fn run_native_watch<W: Write>(
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
        let (_, response) = materialize_candidate_paths(
            materialize_options,
            batch.paths.iter().cloned().collect(),
        )?;
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

pub(in crate::cli) fn probe_native_watcher(
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

pub(in crate::cli) fn watch_probe_timeout() -> Duration {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_millis(750))
}

pub(in crate::cli) fn watch_probe_skip_write() -> bool {
    env::var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE").is_ok_and(|value| value == "1")
}

pub(in crate::cli) fn watch_event_is_under_dir(
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

pub(in crate::cli) fn watch_path_is_under_dir(
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
