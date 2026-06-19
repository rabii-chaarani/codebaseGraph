use crate::ladybug_writer::{write_database, LadybugWriteRequest};
use crate::protocol::{
    NativeManifest, NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse,
};
use lbug::{Connection, Database, SystemConfig, Value};
use notify::{
    event::{AccessKind, AccessMode},
    Event, EventKind, RecursiveMode, Watcher,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const GRAPH_SCHEMA_JSON: &str = include_str!("../../assets/graph_schema.json");
const QUERY_HELPERS_JSON: &str = include_str!("../../assets/query_helpers.json");
const ARCHITECTURE_QUERIES_JSON: &str = include_str!("../../assets/architecture_queries.json");
const MCP_TOOL_SPECS_JSON: &str = include_str!("../../assets/mcp_tool_specs.json");
const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_HTTP_BODY_BYTES: usize = 1_000_000;

fn server_command() -> String {
    env::var("CODEBASE_GRAPH_SERVER_COMMAND").unwrap_or_else(|_| "codebase-graph".to_string())
}

mod dispatch;
mod format;
mod graph;
mod install;
mod materialize;
mod mcp;
mod setup;
mod util;
mod watch;

pub use dispatch::{error_exit_code, run, run_from_env, run_process_args};
use format::*;
use graph::*;
use install::*;
use materialize::*;
use mcp::*;
use setup::*;
use util::*;
use watch::*;

#[cfg(test)]
mod tests;
