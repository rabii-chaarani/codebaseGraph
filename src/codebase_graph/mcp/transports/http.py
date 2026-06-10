from __future__ import annotations

import secrets
import json
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from codebase_graph.diagnostics import log_event
from codebase_graph.mcp.protocol import SUPPORTED_PROTOCOL_VERSIONS, McpGraphServer, rpc_error
from codebase_graph.mcp.runtime import GraphRuntimeConfig, runtime_config

LOCAL_ORIGINS = {"localhost", "127.0.0.1", "::1"}
MAX_HTTP_BODY_BYTES = 1_000_000


class McpHttpServer(ThreadingHTTPServer):
    """Represent a MCP http server."""
    def __init__(self, server_address: tuple[str, int], handler: type[BaseHTTPRequestHandler]) -> None:
        """Initialize the instance.

        Args:
            server_address: Server address value.
            handler: Handler value.
        """
        super().__init__(server_address, handler)
        self.mcp_runtime: GraphRuntimeConfig
        self.mcp_sessions: dict[str, McpGraphServer]
        self.endpoint_path: str
        self.auth_token: str | None


def build_http_server(
    *,
    repo_root: str | Path = ".",
    config_path: str | Path | None = None,
    db_path: str | Path | None = None,
    manifest_path: str | Path | None = None,
    host: str = "127.0.0.1",
    port: int = 8765,
    endpoint_path: str = "/mcp",
    allow_remote: bool = False,
    auth_token: str | None = None,
) -> McpHttpServer:
    """Build http server.

    Args:
        repo_root: Repo root value.
        config_path: The config path to read or write.
        db_path: The db path to read or write.
        manifest_path: The manifest path to read or write.
        host: Host value.
        port: Port value.
        endpoint_path: The endpoint path to read or write.
        allow_remote: Allow remote value.
        auth_token: Auth token value.

    Returns:
        The computed result.
    """
    if auth_token is not None and not auth_token.strip():
        raise ValueError("MCP HTTP auth token must not be blank")
    if allow_remote and auth_token is None:
        log_event("mcp.http_remote_bind_rejected", level="WARNING", host=host, port=port)
        raise ValueError("MCP HTTP remote bind requires an auth token")
    if not allow_remote and host not in LOCAL_ORIGINS:
        log_event("mcp.http_remote_bind_rejected", level="WARNING", host=host, port=port)
        raise ValueError("MCP HTTP transport may only bind to localhost unless allow_remote is enabled")
    graph_runtime = runtime_config(
        repo_root=repo_root,
        config_path=config_path,
        db_path=db_path,
        manifest_path=manifest_path,
    )
    httpd = McpHttpServer((host, port), _McpHttpHandler)
    httpd.mcp_runtime = graph_runtime
    httpd.mcp_sessions = {}
    httpd.endpoint_path = endpoint_path
    httpd.auth_token = auth_token
    return httpd


def serve_http(
    *,
    repo_root: str | Path = ".",
    config_path: str | Path | None = None,
    db_path: str | Path | None = None,
    manifest_path: str | Path | None = None,
    host: str = "127.0.0.1",
    port: int = 8765,
    endpoint_path: str = "/mcp",
    allow_remote: bool = False,
    auth_token: str | None = None,
) -> None:
    """Serve http.

    Args:
        repo_root: Repo root value.
        config_path: The config path to read or write.
        db_path: The db path to read or write.
        manifest_path: The manifest path to read or write.
        host: Host value.
        port: Port value.
        endpoint_path: The endpoint path to read or write.
        allow_remote: Allow remote value.
        auth_token: Auth token value.
    """
    server = build_http_server(
        repo_root=repo_root,
        config_path=config_path,
        db_path=db_path,
        manifest_path=manifest_path,
        host=host,
        port=port,
        endpoint_path=endpoint_path,
        allow_remote=allow_remote,
        auth_token=auth_token,
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()


class _McpHttpHandler(BaseHTTPRequestHandler):
    """Represent a MCP http handler."""
    server: McpHttpServer

    def do_POST(self) -> None:
        """Handle an HTTP T request."""
        if not self._request_path_matches() or not self._valid_origin() or not self._valid_auth():
            return
        if not self._valid_protocol_header():
            return
        length = self._content_length()
        if length is None:
            return
        try:
            message = json.loads(self.rfile.read(length).decode("utf-8"))
        except Exception as exc:
            log_event("mcp.http_parse_error", level="WARNING", message=str(exc), client_address=self.client_address[0])
            self._send_json(rpc_error(None, -32700, f"Invalid JSON-RPC payload: {exc}"), status=HTTPStatus.BAD_REQUEST)
            return
        if not isinstance(message, dict):
            self._send_json(rpc_error(None, -32600, "JSON-RPC payload must be an object"), status=HTTPStatus.BAD_REQUEST)
            return
        session_id, server = self._resolve_session(message)
        if server is None:
            return
        response = server.handle_json_rpc(message)
        if response is None:
            self.send_response(HTTPStatus.ACCEPTED)
            self.end_headers()
            return
        headers = {"Mcp-Session-Id": session_id} if str(message.get("method", "")) == "initialize" else None
        self._send_json(response, headers=headers)

    def do_GET(self) -> None:
        """Handle an HTTP T request."""
        if not self._request_path_matches() or not self._valid_origin() or not self._valid_auth():
            return
        self.send_response(HTTPStatus.METHOD_NOT_ALLOWED)
        self.send_header("Allow", "POST")
        self.end_headers()

    def log_message(self, format: str, *args: Any) -> None:
        """Log message.

        Args:
            format: Format value.
            args: Parsed command-line arguments.
        """
        return

    def _resolve_session(self, message: dict[str, Any]) -> tuple[str, McpGraphServer] | tuple[None, None]:
        """Resolve session.

        Args:
            message: The message payload to process.

        Returns:
            A tuple containing the computed values.
        """
        method = str(message.get("method", ""))
        request_id = message.get("id")
        session_id = self.headers.get("Mcp-Session-Id")
        if method == "initialize":
            if session_id and session_id in self.server.mcp_sessions:
                return session_id, self.server.mcp_sessions[session_id]
            session_id = secrets.token_urlsafe(32)
            server = McpGraphServer(self.server.mcp_runtime)
            self.server.mcp_sessions[session_id] = server
            return session_id, server
        if not session_id or session_id not in self.server.mcp_sessions:
            self._send_json(
                rpc_error(request_id, -32002, "MCP session is not initialized"),
                status=HTTPStatus.BAD_REQUEST,
            )
            return None, None
        return session_id, self.server.mcp_sessions[session_id]

    def _request_path_matches(self) -> bool:
        """Process request path matches.

        Returns:
            Whether the check succeeds.
        """
        if urlparse(self.path).path == self.server.endpoint_path:
            return True
        self._send_json(rpc_error(None, -32601, "MCP endpoint not found"), status=HTTPStatus.NOT_FOUND)
        return False

    def _valid_origin(self) -> bool:
        """Return whether valid origin.

        Returns:
            Whether the check succeeds.
        """
        origin = self.headers.get("Origin")
        if not origin:
            return True
        hostname = urlparse(origin).hostname
        if hostname in LOCAL_ORIGINS:
            return True
        log_event(
            "mcp.http_forbidden_origin",
            level="WARNING",
            origin=origin,
            client_address=self.client_address[0],
        )
        self._send_json(rpc_error(None, -32000, "Forbidden origin"), status=HTTPStatus.FORBIDDEN)
        return False

    def _valid_auth(self) -> bool:
        """Return whether valid auth.

        Returns:
            Whether the check succeeds.
        """
        if self.server.auth_token is None:
            return True
        authorization = self.headers.get("Authorization", "")
        prefix = "Bearer "
        if authorization.startswith(prefix) and secrets.compare_digest(authorization[len(prefix) :], self.server.auth_token):
            return True
        log_event(
            "mcp.http_unauthorized",
            level="WARNING",
            client_address=self.client_address[0],
        )
        self._send_json(
            rpc_error(None, -32000, "Unauthorized"),
            status=HTTPStatus.UNAUTHORIZED,
            headers={"WWW-Authenticate": "Bearer"},
        )
        return False

    def _valid_protocol_header(self) -> bool:
        """Return whether valid protocol header.

        Returns:
            Whether the check succeeds.
        """
        requested = self.headers.get("MCP-Protocol-Version")
        if requested is None:
            return True
        if requested in SUPPORTED_PROTOCOL_VERSIONS:
            return True
        log_event(
            "mcp.http_unsupported_protocol",
            level="WARNING",
            requested=requested,
            client_address=self.client_address[0],
        )
        self._send_json(
            rpc_error(
                None,
                -32602,
                "Unsupported MCP protocol version",
                {"supported": list(SUPPORTED_PROTOCOL_VERSIONS), "requested": requested},
            ),
            status=HTTPStatus.BAD_REQUEST,
        )
        return False

    def _content_length(self) -> int | None:
        """Process content length.

        Returns:
            The computed result.
        """
        raw_length = self.headers.get("Content-Length", "0")
        try:
            length = int(raw_length)
        except ValueError:
            log_event(
                "mcp.http_invalid_content_length",
                level="WARNING",
                content_length=raw_length,
                client_address=self.client_address[0],
            )
            self._send_json(rpc_error(None, -32600, "Content-Length must be an integer"), status=HTTPStatus.BAD_REQUEST)
            return None
        if length < 0:
            log_event(
                "mcp.http_invalid_content_length",
                level="WARNING",
                content_length=raw_length,
                client_address=self.client_address[0],
            )
            self._send_json(rpc_error(None, -32600, "Content-Length must be non-negative"), status=HTTPStatus.BAD_REQUEST)
            return None
        if length > MAX_HTTP_BODY_BYTES:
            log_event(
                "mcp.http_body_too_large",
                level="WARNING",
                content_length=length,
                client_address=self.client_address[0],
            )
            self._send_json(
                rpc_error(None, -32000, "MCP request body is too large", {"max_bytes": MAX_HTTP_BODY_BYTES}),
                status=HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
            )
            return None
        return length

    def _send_json(
        self,
        payload: dict[str, Any],
        *,
        status: HTTPStatus = HTTPStatus.OK,
        headers: dict[str, str] | None = None,
    ) -> None:
        """Send JSON.

        Args:
            payload: Payload to process.
            status: Status value.
            headers: Headers value.
        """
        body = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(body)
