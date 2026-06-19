use super::*;
use super::{install::*, materialize::*, mcp::*, watch::*};
use notify::{Event, EventKind};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
};

mod dispatch_materialize;
mod fixtures;
mod graph;
mod install;
mod mcp;
mod watch;

use fixtures::*;
