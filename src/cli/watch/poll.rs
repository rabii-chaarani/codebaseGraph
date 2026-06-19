use super::{
    helpers::watch_max_wait,
    output::write_watch_event,
    snapshot::{watch_file_snapshot, watch_snapshot_diff},
    types::WatchChangeBatch,
    types::WatchFileSnapshot,
    WatchEventFilter, WatchLoopConfig,
};
use crate::cli::build::{materialize, MaterializeOptions};
use std::{
    io::Write,
    time::{Duration, Instant},
};

pub(in crate::cli) fn run_poll_watch<W: Write>(
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

pub(in crate::cli) fn collect_poll_batch(
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
