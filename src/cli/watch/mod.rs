mod batch;
mod command;
mod filter;
mod helpers;
mod native;
mod options;
mod output;
mod poll;
mod snapshot;
mod types;

pub(in crate::cli) use command::run_watch;
pub(in crate::cli) use filter::WatchEventFilter;
pub(in crate::cli) use options::{SetupOptions, WatchLoopConfig};
pub(in crate::cli) use snapshot::scan_source_snapshots;

#[cfg(test)]
pub(super) use batch::{apply_watch_message, collect_watch_batch};
#[cfg(test)]
pub(super) use native::probe_native_watcher;
#[cfg(test)]
pub(super) use options::{WatchBackend, WatchOptions};
#[cfg(test)]
pub(super) use poll::collect_poll_batch;
#[cfg(test)]
pub(super) use snapshot::{watch_file_snapshot, watch_snapshot_diff};
#[cfg(test)]
pub(super) use types::{WatchChangeBatch, WatchMessage};
