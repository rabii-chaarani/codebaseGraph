mod command;
mod options;
mod verify;

pub(in crate::adapters::cli) use command::run_mcp_install;
pub(in crate::adapters::cli) use options::McpInstallOptions;
pub(in crate::adapters::cli) use verify::attach_install_verification;
