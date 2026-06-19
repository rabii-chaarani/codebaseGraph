mod client;
mod command;
mod descriptor;
mod fs_util;
mod metadata;
mod options;
mod render;
mod state;
mod verify;

pub(in crate::product_cli) use client::install_mcp_client;
pub(in crate::product_cli) use command::run_mcp_install;
pub(in crate::product_cli) use descriptor::{build_mcp_descriptor, NativeMcpDescriptor};
pub(in crate::product_cli) use fs_util::{
    executable_in_path, expand_path, home_dir, subprocess_error, write_text_atomic,
};
pub(in crate::product_cli) use metadata::{
    adapter_id, default_client_config_path, install_scope, native_client_command,
    supported_install_clients, supported_install_clients_with_all,
};
pub(in crate::product_cli) use options::McpInstallOptions;
pub(in crate::product_cli) use render::{copilot_studio_metadata, render_client_config};
pub(in crate::product_cli) use state::McpHttpState;
pub(in crate::product_cli) use verify::attach_install_verification;
