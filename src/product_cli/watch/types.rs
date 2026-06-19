use notify::Event;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug)]
pub(in crate::product_cli) enum WatchMessage {
    Event(Event),
    Error(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::product_cli) struct WatchChangeBatch {
    pub(in crate::product_cli) paths: BTreeSet<String>,
    pub(in crate::product_cli) event_count: usize,
}

#[derive(Debug, Default)]
pub(in crate::product_cli) struct WatchProbeOutcome {
    pub(in crate::product_cli) delivered: bool,
    pub(in crate::product_cli) queued: VecDeque<WatchMessage>,
    pub(in crate::product_cli) reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::product_cli) struct WatchFileState {
    pub(in crate::product_cli) modified_nanos: u128,
    pub(in crate::product_cli) len: u64,
}

pub(in crate::product_cli) type WatchFileSnapshot = BTreeMap<String, WatchFileState>;
