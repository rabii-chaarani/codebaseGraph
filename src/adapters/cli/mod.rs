mod dispatch;
pub(crate) mod format;
pub(crate) mod graph;
mod install;
pub(crate) mod materialization;
mod materialization_input;
mod mcp_command;
pub(crate) mod reinstall;
pub(crate) mod setup;
pub(crate) mod uninstall;
mod util;
mod watch;

pub use dispatch::{error_exit_code, run};

#[cfg(test)]
mod tests;
