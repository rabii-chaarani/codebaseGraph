use notify::Event;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug)]
pub(in crate::cli) enum WatchMessage {
    Event(Event),
    Error(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::cli) struct WatchChangeBatch {
    pub(in crate::cli) paths: BTreeSet<String>,
    pub(in crate::cli) event_count: usize,
}

#[derive(Debug, Default)]
pub(in crate::cli) struct WatchProbeOutcome {
    pub(in crate::cli) delivered: bool,
    pub(in crate::cli) queued: VecDeque<WatchMessage>,
    pub(in crate::cli) reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::cli) struct WatchFileState {
    pub(in crate::cli) modified_nanos: u128,
    pub(in crate::cli) len: u64,
}

pub(in crate::cli) type WatchFileSnapshot = BTreeMap<String, WatchFileState>;
