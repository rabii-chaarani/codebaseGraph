#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(test)]
#[global_allocator]
static GLOBAL_ALLOCATOR: storage::maintenance_scheduler::allocation_boundary_test::Allocator =
    storage::maintenance_scheduler::allocation_boundary_test::Allocator;

pub mod adapters;
pub mod agent_hooks;
pub mod api;
mod artifact_store;
mod bootstrap;
mod client_config;
mod client_process;
mod coordinator;
mod daemon_service;
pub mod db_writer;
pub mod error;
mod execution;
mod hash;
mod materialization_worker;
pub(crate) mod mcp_client;
mod normalize;
mod parser;
mod partition_builder;
mod process_lifecycle;
mod profiles;
pub mod protocol;
mod scan;
mod search_index;
mod source_selection;
mod staging_writer;
mod storage;
mod syntax_materializer;

pub use error::MaterializationError;
pub use execution::{execute_materialization_pipeline, plan_materialization};
pub use protocol::{MaterializationInput, MaterializationResult};

pub use adapters::cli;
pub use bootstrap::run_from_env;
