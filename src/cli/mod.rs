mod build;
mod constants;
mod dispatch;
mod format;
mod graph;
mod install;
mod mcp;
mod setup;
mod uninstall;
mod util;
mod watch;

pub use dispatch::{error_exit_code, run, run_from_env, run_process_args};

#[cfg(test)]
mod tests;
