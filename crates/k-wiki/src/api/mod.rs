//! Transport-neutral Knowledge Wiki operations.

mod contracts;
mod facade;
mod registry;

pub use contracts::*;
pub use facade::{OkfWikiApi, WikiOperationExecutor};
pub use registry::{
    mcp_operation_descriptor, mcp_operation_descriptors, operation_descriptor,
    operation_descriptors, operation_id, AccessMode, OperationDescriptor, OperationSurface,
};
