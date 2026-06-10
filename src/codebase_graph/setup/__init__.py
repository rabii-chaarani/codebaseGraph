"""Production setup orchestration for repository graph bootstrapping."""

from importlib import import_module
from typing import Any

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

_LAZY_EXPORTS = {
    "McpInstallOptions": (".installer", "McpInstallOptions"),
    "McpInstallResult": (".installer", "McpInstallResult"),
    "SetupError": (".orchestrator", "SetupError"),
    "SetupOptions": (".orchestrator", "SetupOptions"),
    "SetupResult": (".orchestrator", "SetupResult"),
    "install_mcp_clients": (".installer", "install_mcp_clients"),
    "install_mcp_server": (".installer", "install_mcp_server"),
    "run_setup": (".orchestrator", "run_setup"),
}

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


def __getattr__(name: str) -> Any:
    """Return lazily imported package attributes.

    Args:
        name: Name value.

    Returns:
        The computed result.
    """
    if name not in _LAZY_EXPORTS:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    module_name, attribute_name = _LAZY_EXPORTS[name]
    value = getattr(import_module(module_name, __name__), attribute_name)
    globals()[name] = value
    return value
