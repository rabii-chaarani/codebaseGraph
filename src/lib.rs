pub mod adapters;
pub mod api;
mod artifact_store;
mod bootstrap;
pub mod db_writer;
pub mod error;
mod execution;
mod hash;
mod normalize;
mod parser;
mod partition_builder;
mod profiles;
pub mod protocol;
mod scan;
mod search_index;
mod staging_writer;
mod storage;
mod syntax_materializer;

pub use error::{MaterializationError, MemoryBudgetExceeded};
pub use execution::{execute_materialization_pipeline, plan_materialization};
pub use protocol::{MaterializationInput, MaterializationResult};

pub use adapters::cli;
pub use bootstrap::run_from_env;
