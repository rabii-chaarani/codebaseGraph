from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from codebase_graph.paths import MCP_SERVER_NAME

from .runtime import GraphRuntimeConfig, package_version
from .tools import UnknownToolError, call_tool_result, tool_specs

SUPPORTED_PROTOCOL_VERSIONS = ("2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05")
LATEST_PROTOCOL_VERSION = SUPPORTED_PROTOCOL_VERSIONS[0]


@dataclass(slots=True)
class ProtocolSession:
    """Store protocol session data."""
    protocol_version: str | None = None
    initialized: bool = False


class McpGraphServer:
    """Represent a MCP graph server."""
    def __init__(self, runtime: GraphRuntimeConfig) -> None:
        """Initialize the instance.

        Args:
            runtime: The runtime used by the operation.
        """
        self.runtime = runtime
        self.session = ProtocolSession()

    @classmethod
    def from_paths(
        cls,
        *,
        repo_root: str = ".",
        config_path: str | None = None,
        db_path: str | None = None,
        manifest_path: str | None = None,
    ) -> McpGraphServer:
        """Convert paths.

        Args:
            repo_root: Repo root value.
            config_path: The config path to read or write.
            db_path: The db path to read or write.
            manifest_path: The manifest path to read or write.

        Returns:
            The computed result.
        """
        from .runtime import runtime_config

        runtime = runtime_config(
            repo_root=repo_root,
            config_path=config_path,
            db_path=db_path,
            manifest_path=manifest_path,
        )
        return cls(runtime)

    def handle_json_rpc(self, message: dict[str, Any]) -> dict[str, Any] | None:
        """Process one JSON-RPC message for the MCP server.

        Args:
            message: JSON-RPC request or notification payload.

        Returns:
            A JSON-RPC response, or None for notifications.
        """
        method = str(message.get("method", ""))
        request_id = message.get("id")
        if method == "notifications/initialized":
            self.session.initialized = True
            return None
        if method.startswith("notifications/"):
            return None
        # MCP requires initialization before tool calls; enforcing that here
        # keeps individual tools free of transport/session concerns.
        if method in {"tools/list", "tools/call"} and self.session.protocol_version is None:
            return rpc_error(request_id, -32002, "MCP session is not initialized")
        try:
            if method == "initialize":
                result = self._initialize(dict(message.get("params") or {}))
            elif method == "ping":
                result = {}
            elif method == "tools/list":
                result = {"tools": tool_specs()}
            elif method == "tools/call":
                result = self._call_tool(dict(message.get("params") or {}))
            else:
                return rpc_error(request_id, -32601, f"Unsupported MCP method: {method}")
        except UnknownToolError as exc:
            return rpc_error(request_id, -32602, str(exc))
        except ValueError as exc:
            return rpc_error(request_id, -32602, str(exc))
        except Exception as exc:
            return rpc_error(request_id, -32000, str(exc))
        return {"jsonrpc": "2.0", "id": request_id, "result": result}

    def _initialize(self, params: dict[str, Any]) -> dict[str, Any]:
        """Process initialize.

        Args:
            params: Params value.

        Returns:
            A dictionary containing the computed payload.
        """
        requested = str(params.get("protocolVersion") or "")
        protocol_version = negotiate_protocol_version(requested)
        self.session.protocol_version = protocol_version
        return {
            "protocolVersion": protocol_version,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": MCP_SERVER_NAME, "version": package_version()},
        }

    def _call_tool(self, params: dict[str, Any]) -> dict[str, Any]:
        """Call tool.

        Args:
            params: Params value.

        Returns:
            A dictionary containing the computed payload.
        """
        return call_tool_result(
            str(params.get("name", "")),
            dict(params.get("arguments") or {}),
            runtime=self.runtime,
        )


def negotiate_protocol_version(requested: str) -> str:
    """Negotiate protocol version.

    Args:
        requested: Requested value.

    Returns:
        The computed string.
    """
    if requested in SUPPORTED_PROTOCOL_VERSIONS:
        return requested
    return LATEST_PROTOCOL_VERSION


def rpc_error(request_id: Any, code: int, message: str, data: dict[str, Any] | None = None) -> dict[str, Any]:
    """Process RPC error.

    Args:
        request_id: The request id to identify.
        code: Code value.
        message: The message payload to process.
        data: Data value.

    Returns:
        A dictionary containing the computed payload.
    """
    error: dict[str, Any] = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    return {"jsonrpc": "2.0", "id": request_id, "error": error}
