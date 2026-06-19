use super::{
    types::{WatchChangeBatch, WatchMessage},
    WatchEventFilter,
};
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub(in crate::cli) fn collect_watch_batch(
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

pub(in crate::cli) fn apply_watch_message(
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
