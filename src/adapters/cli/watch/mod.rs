mod command;
mod options;
mod output;

pub(in crate::adapters::cli) use command::run_watch;
pub(in crate::adapters::cli) use options::SetupOptions;

#[cfg(test)]
pub(super) use crate::api::refresh::{
    apply_watch_message, collect_poll_batch, collect_watch_batch, probe_native_watcher,
    watch_file_snapshot, watch_snapshot_diff, WatchChangeBatch, WatchEventFilter, WatchMessage,
};
#[cfg(test)]
pub(super) use options::{WatchBackend, WatchOptions};
