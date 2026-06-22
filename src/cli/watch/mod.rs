mod batch;
mod command;
mod filter;
mod helpers;
mod native;
mod options;
mod output;
mod poll;
mod refresh;
mod snapshot;
mod types;

pub(in crate::cli) use batch::collect_watch_batch;
pub(in crate::cli) use command::run_watch;
pub(in crate::cli) use filter::WatchEventFilter;
pub(in crate::cli) use native::{probe_native_watcher, start_native_watcher};
pub(in crate::cli) use options::{SetupOptions, WatchLoopConfig};
pub(in crate::cli) use poll::collect_poll_batch;
pub(in crate::cli) use snapshot::{scan_source_snapshots, watch_file_snapshot};
pub(in crate::cli) use types::WatchMessage;

#[cfg(test)]
pub(super) use batch::apply_watch_message;
#[cfg(test)]
pub(super) use options::{WatchBackend, WatchOptions};
#[cfg(test)]
pub(super) use snapshot::watch_snapshot_diff;
#[cfg(test)]
pub(super) use types::WatchChangeBatch;
