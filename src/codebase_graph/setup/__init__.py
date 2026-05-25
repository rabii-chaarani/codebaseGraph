"""Production setup orchestration for repository graph bootstrapping."""

from .installer import McpInstallOptions, McpInstallResult, install_mcp_clients, install_mcp_server
from .orchestrator import SetupError, SetupOptions, SetupResult, run_setup
from .state import (
    CONFIG_NAME,
    DEFAULT_STATE_DIR,
    GraphStatePaths,
    MANIFEST_NAME,
    SetupPaths,
    derive_graph_state_paths,
    derive_setup_paths,
    load_setup_config,
)

__all__ = [
    "CONFIG_NAME",
    "DEFAULT_STATE_DIR",
    "GraphStatePaths",
    "MANIFEST_NAME",
    "McpInstallOptions",
    "McpInstallResult",
    "SetupError",
    "SetupOptions",
    "SetupPaths",
    "SetupResult",
    "derive_graph_state_paths",
    "derive_setup_paths",
    "load_setup_config",
    "install_mcp_clients",
    "install_mcp_server",
    "run_setup",
]
