//! Stable, transport-neutral entry point for codebase graph operations.
//!
//! Embedded clients construct an [`OperationRequest`] and submit it through
//! [`CodebaseGraphApi::execute_operation`]. The same facade is used by the CLI
//! and MCP adapters, so validation, runtime selection, errors, and presentation
//! remain consistent across surfaces.

pub mod catalog;
pub mod context;
pub mod contracts;
pub mod core;
pub mod facade;
pub(crate) mod graph_read;
pub mod lifecycle;
mod materialization;
mod normalization;
pub mod presenter;
mod refresh;

pub use contracts::{
    ApiError, ContextRequest, HealthRequest, MaterializationRequest, McpInstallRequest,
    OperationInvocation, OperationRequest, OperationResponse, OutputFormat, QueryRequest,
    RefreshBackend, RefreshLoopConfig, RefreshRequest, RefreshWatchConfig, RefreshWatchObserver,
    RefreshWatchSummary, RepoSelector, RepositoryLifecycleRequest, SearchRequest,
};
pub use core::OperationDescriptor;
pub use facade::{CodebaseGraphApi, OperationExecutor};
pub use lifecycle::{
    install_mcp_server, supported_mcp_clients, McpClientInstallOptions, McpServerDescriptor,
};

#[cfg(test)]
pub(crate) use refresh::{
    apply_watch_message, collect_poll_batch, collect_watch_batch, probe_native_watcher,
    watch_file_snapshot, watch_snapshot_diff, WatchChangeBatch, WatchEventFilter, WatchMessage,
};

#[cfg(test)]
mod boundary_tests;
