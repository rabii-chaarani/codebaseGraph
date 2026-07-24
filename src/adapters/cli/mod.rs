pub(crate) mod constants;
mod dispatch;
pub(crate) mod format;
pub(crate) mod graph;
mod install;
pub(crate) mod materialization;
mod mcp_command;
pub(crate) mod reinstall;
pub(crate) mod setup;
pub(crate) mod uninstall;
mod util;
mod watch;

pub use dispatch::{error_exit_code, run, run_from_env, run_process_args};

#[cfg(test)]
mod tests;
