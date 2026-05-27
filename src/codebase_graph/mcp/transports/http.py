from __future__ import annotations

import json
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from codebase_graph.mcp.protocol import SUPPORTED_PROTOCOL_VERSIONS, McpGraphServer, rpc_error

LOCAL_ORIGINS = {"localhost", "127.0.0.1", "::1"}
MAX_HTTP_BODY_BYTES = 1_000_000


class McpHttpServer(ThreadingHTTPServer):
    def __init__(self, server_address: tuple[str, int], handler: type[BaseHTTPRequestHandler]) -> None:
        super().__init__(server_address, handler)
        self.mcp_server: McpGraphServer
        self.endpoint_path: str


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
) -> McpHttpServer:
    if not allow_remote and host not in LOCAL_ORIGINS:
        raise ValueError("MCP HTTP transport may only bind to localhost unless allow_remote is enabled")
    graph_server = McpGraphServer.from_paths(
        repo_root=repo_root,
        config_path=config_path,
        db_path=db_path,
        manifest_path=manifest_path,
    )
    httpd = McpHttpServer((host, port), _McpHttpHandler)
    httpd.mcp_server = graph_server
    httpd.endpoint_path = endpoint_path
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
) -> None:
    server = build_http_server(
        repo_root=repo_root,
        config_path=config_path,
        db_path=db_path,
        manifest_path=manifest_path,
        host=host,
        port=port,
        endpoint_path=endpoint_path,
        allow_remote=allow_remote,
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()


class _McpHttpHandler(BaseHTTPRequestHandler):
    server: McpHttpServer

    def do_POST(self) -> None:
        if not self._request_path_matches() or not self._valid_origin():
            return
        if not self._valid_protocol_header():
            return
        length = self._content_length()
        if length is None:
            return
        try:
            message = json.loads(self.rfile.read(length).decode("utf-8"))
        except Exception as exc:
            self._send_json(rpc_error(None, -32700, f"Invalid JSON-RPC payload: {exc}"), status=HTTPStatus.BAD_REQUEST)
            return
        if not isinstance(message, dict):
            self._send_json(rpc_error(None, -32600, "JSON-RPC payload must be an object"), status=HTTPStatus.BAD_REQUEST)
            return
        response = self.server.mcp_server.handle_json_rpc(message)
        if response is None:
            self.send_response(HTTPStatus.ACCEPTED)
            self.end_headers()
            return
        self._send_json(response)

    def do_GET(self) -> None:
        if not self._request_path_matches() or not self._valid_origin():
            return
        self.send_response(HTTPStatus.METHOD_NOT_ALLOWED)
        self.send_header("Allow", "POST")
        self.end_headers()

    def log_message(self, format: str, *args: Any) -> None:
        return

    def _request_path_matches(self) -> bool:
        if urlparse(self.path).path == self.server.endpoint_path:
            return True
        self._send_json(rpc_error(None, -32601, "MCP endpoint not found"), status=HTTPStatus.NOT_FOUND)
        return False

    def _valid_origin(self) -> bool:
        origin = self.headers.get("Origin")
        if not origin:
            return True
        hostname = urlparse(origin).hostname
        if hostname in LOCAL_ORIGINS:
            return True
        self._send_json(rpc_error(None, -32000, "Forbidden origin"), status=HTTPStatus.FORBIDDEN)
        return False

    def _valid_protocol_header(self) -> bool:
        requested = self.headers.get("MCP-Protocol-Version")
        if requested is None:
            return True
        if requested in SUPPORTED_PROTOCOL_VERSIONS:
            return True
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
        raw_length = self.headers.get("Content-Length", "0")
        try:
            length = int(raw_length)
        except ValueError:
            self._send_json(rpc_error(None, -32600, "Content-Length must be an integer"), status=HTTPStatus.BAD_REQUEST)
            return None
        if length < 0:
            self._send_json(rpc_error(None, -32600, "Content-Length must be non-negative"), status=HTTPStatus.BAD_REQUEST)
            return None
        if length > MAX_HTTP_BODY_BYTES:
            self._send_json(
                rpc_error(None, -32000, "MCP request body is too large", {"max_bytes": MAX_HTTP_BODY_BYTES}),
                status=HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
            )
            return None
        return length

    def _send_json(self, payload: dict[str, Any], *, status: HTTPStatus = HTTPStatus.OK) -> None:
        body = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
