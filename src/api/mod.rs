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
pub mod lifecycle;
pub(crate) mod materialization;
pub mod presenter;

pub use contracts::{
    ApiError, ContextRequest, HealthRequest, MaterializationRequest, OperationRequest,
    OperationResponse, OutputFormat, QueryRequest, RefreshRequest, RepoSelector,
    RepositoryLifecycleRequest, SearchRequest,
};
pub use core::OperationDescriptor;
pub use facade::{CodebaseGraphApi, OperationExecutor};

#[cfg(test)]
mod boundary_tests;
