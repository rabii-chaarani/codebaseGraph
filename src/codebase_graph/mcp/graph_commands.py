from __future__ import annotations

import argparse
import json
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import Any

from codebase_graph.ontology import CONTEXT_PROFILES
from codebase_graph.retrieval import DETAIL_LEVELS


MAX_GRAPH_QUERY_LIMIT = 1000

PayloadBuilder = Callable[[argparse.Namespace], dict[str, Any]]
ArgumentAdder = Callable[[argparse.ArgumentParser], None]


@dataclass(frozen=True, slots=True)
class GraphCommandSpec:
    """Store graph command spec data."""
    command_name: str
    tool_name: str
    help: str
    description: str
    input_schema: dict[str, Any]
    add_arguments: ArgumentAdder
    payload_from_args: PayloadBuilder
    requires_runtime: bool = True

    def tool_spec(self) -> dict[str, Any]:
        """Return tool spec.

        Returns:
            A dictionary containing the computed payload.
        """
        return {
            "name": self.tool_name,
            "description": self.description,
            "inputSchema": self.input_schema,
        }


def graph_command_specs() -> tuple[GraphCommandSpec, ...]:
    """Return graph command specs.

    Returns:
        A tuple containing the computed values.
    """
    return GRAPH_COMMAND_SPECS


def graph_command_names() -> set[str]:
    """Return graph command names.

    Returns:
        The computed result.
    """
    return {spec.command_name for spec in GRAPH_COMMAND_SPECS}


def graph_tool_specs() -> list[dict[str, Any]]:
    """Return graph tool specs.

    Returns:
        A list containing the computed values.
    """
    return [spec.tool_spec() for spec in GRAPH_COMMAND_SPECS]


def graph_command_spec(command_name: str) -> GraphCommandSpec:
    """Return graph command spec.

    Args:
        command_name: Command name value.

    Returns:
        The computed result.
    """
    for spec in GRAPH_COMMAND_SPECS:
        if spec.command_name == command_name:
            return spec
    raise KeyError(command_name)


def search_arguments_payload(args: argparse.Namespace) -> dict[str, Any]:
    """Search arguments payload.

    Args:
        args: Parsed command-line arguments.

    Returns:
        A dictionary containing the computed payload.
    """
    payload: dict[str, Any] = {
        "limit": args.limit,
        "profile": args.profile,
        "budget": args.budget,
        "context_limit": args.context_limit,
        "detail": args.detail,
    }
    if getattr(args, "query", None):
        payload["query"] = args.query
    if args.max_depth is not None:
        payload["max_depth"] = args.max_depth
    return payload


def _empty_payload(args: argparse.Namespace) -> dict[str, Any]:
    """Return whether empty payload.

    Args:
        args: Parsed command-line arguments.

    Returns:
        A dictionary containing the computed payload.
    """
    return {}


def _architecture_payload(args: argparse.Namespace) -> dict[str, Any]:
    """Process architecture payload.

    Args:
        args: Parsed command-line arguments.

    Returns:
        A dictionary containing the computed payload.
    """
    payload: dict[str, Any] = {}
    if args.group:
        payload["group"] = args.group
    return payload


def _context_payload(args: argparse.Namespace) -> dict[str, Any]:
    """Process context payload.

    Args:
        args: Parsed command-line arguments.

    Returns:
        A dictionary containing the computed payload.
    """
    if not args.query and not (args.node_id and args.node_type):
        raise ValueError("graph-context requires a query or both --node-id and --node-type")
    if (args.node_id and not args.node_type) or (args.node_type and not args.node_id):
        raise ValueError("graph-context explicit lookup requires both --node-id and --node-type")
    payload = search_arguments_payload(args)
    if args.node_id and args.node_type:
        payload["node_id"] = args.node_id
        payload["node_type"] = args.node_type
    return payload


def _query_payload(args: argparse.Namespace) -> dict[str, Any]:
    """Return query payload.

    Args:
        args: Parsed command-line arguments.

    Returns:
        A dictionary containing the computed payload.
    """
    try:
        parameters = json.loads(args.parameters)
    except json.JSONDecodeError as exc:
        raise ValueError(f"graph-query --parameters must be a JSON object: {exc}") from exc
    if not isinstance(parameters, dict):
        raise ValueError("graph-query --parameters must be a JSON object")
    return {"statement": args.statement, "parameters": parameters, "limit": args.limit}


def add_json_output_arguments(parser: argparse.ArgumentParser) -> None:
    """Add JSON output arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("--pretty", action="store_true", help="Emit indented JSON output")


def add_compact_context_arguments(parser: argparse.ArgumentParser, *, default_format: str = "json") -> None:
    """Add compact context arguments.

    Args:
        parser: The parser used by the operation.
        default_format: Default format value.
    """
    parser.add_argument("--limit", type=int, default=3, help="Maximum search hits to return")
    parser.add_argument("--profile", choices=sorted(CONTEXT_PROFILES), default="brief", help="Context profile")
    parser.add_argument("--budget", type=int, default=600, help="Approximate per-hit context character budget")
    parser.add_argument("--max-depth", type=int, default=None, help="Override the context profile depth")
    parser.add_argument("--context-limit", type=int, default=3, help="Maximum context items per search hit")
    parser.add_argument("--detail", choices=sorted(DETAIL_LEVELS), default="standard", help="Output detail level")
    parser.add_argument("--format", choices=("json", "block"), default=default_format, help="Output format")
    add_json_output_arguments(parser)


def add_runtime_arguments(parser: argparse.ArgumentParser) -> None:
    """Add runtime arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("--repo-root", default=".", help="Repository root containing .codebaseGraph/config.json")
    parser.add_argument("--config", default=None, help="Path to .codebaseGraph/config.json")
    parser.add_argument("--db", default=None, help="Override LadyBugDB path")
    parser.add_argument("--manifest", default=None, help="Override manifest path")


def add_graph_compatibility_arguments(parser: argparse.ArgumentParser) -> None:
    """Add graph compatibility arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("--no-refresh", action="store_true", help="Accepted for search/context command parity")
    parser.add_argument("--json", action="store_true", help="Accepted for search/context command parity; same as --format json")


def _add_graph_health_arguments(parser: argparse.ArgumentParser) -> None:
    """Add graph health arguments.

    Args:
        parser: The parser used by the operation.
    """
    add_runtime_arguments(parser)
    add_json_output_arguments(parser)


def _add_graph_search_arguments(parser: argparse.ArgumentParser) -> None:
    """Add graph search arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("query", help="Search query")
    add_compact_context_arguments(parser, default_format="block")
    add_runtime_arguments(parser)
    add_graph_compatibility_arguments(parser)


def _add_graph_context_arguments(parser: argparse.ArgumentParser) -> None:
    """Add graph context arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("query", nargs="?", help="Search query")
    parser.add_argument("--node-id", default=None, help="Explicit graph node id")
    parser.add_argument("--node-type", default=None, help="Explicit graph node type")
    add_compact_context_arguments(parser, default_format="block")
    add_runtime_arguments(parser)
    add_graph_compatibility_arguments(parser)


def _add_graph_architecture_arguments(parser: argparse.ArgumentParser) -> None:
    """Add graph architecture arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("--group", default=None, help="Optional architecture query group")
    add_json_output_arguments(parser)


def _add_graph_query_arguments(parser: argparse.ArgumentParser) -> None:
    """Add graph query arguments.

    Args:
        parser: The parser used by the operation.
    """
    parser.add_argument("statement", help="Read-only graph query statement")
    parser.add_argument("--parameters", default="{}", help="JSON object with query parameters")
    parser.add_argument("--limit", type=int, default=100, help="Maximum rows to return")
    add_runtime_arguments(parser)
    add_json_output_arguments(parser)


def _object_schema(
    properties: dict[str, Any] | None = None,
    *,
    required: Sequence[str] = (),
) -> dict[str, Any]:
    """Return object schema.

    Args:
        properties: Properties value.
        required: Required value.

    Returns:
        A dictionary containing the computed payload.
    """
    schema: dict[str, Any] = {
        "type": "object",
        "properties": properties or {},
        "additionalProperties": False,
    }
    if required:
        schema["required"] = list(required)
    return schema


def _search_schema(*, required: Sequence[str]) -> dict[str, Any]:
    """Search schema.

    Args:
        required: Required value.

    Returns:
        A dictionary containing the computed payload.
    """
    return _object_schema(
        {
            "query": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1},
            "profile": {"type": "string"},
            "budget": {"type": "integer", "minimum": 0},
            "max_depth": {"type": "integer", "minimum": 0},
            "context_limit": {"type": "integer", "minimum": 0},
            "detail": {"type": "string", "enum": sorted(DETAIL_LEVELS)},
            "output_format": {"type": "string", "enum": ["json", "block"], "default": "block"},
            "include_structured_content": {
                "type": "boolean",
                "default": False,
                "description": "Include the MCP structuredContent payload alongside the text result.",
            },
            "node_id": {"type": "string"},
            "node_type": {"type": "string"},
        },
        required=required,
    )


GRAPH_COMMAND_SPECS = (
    GraphCommandSpec(
        command_name="graph-health",
        tool_name="graph_health",
        help="Check configured graph paths",
        description="Check the configured codebaseGraph database path and manifest path.",
        input_schema=_object_schema(),
        add_arguments=_add_graph_health_arguments,
        payload_from_args=_empty_payload,
    ),
    GraphCommandSpec(
        command_name="graph-search",
        tool_name="graph_search",
        help="Search the code graph with compact context",
        description="Search code, documentation, paths, and dependencies with compact graph context.",
        input_schema=_search_schema(required=("query",)),
        add_arguments=_add_graph_search_arguments,
        payload_from_args=search_arguments_payload,
    ),
    GraphCommandSpec(
        command_name="graph-context",
        tool_name="graph_context",
        help="Return compact graph context",
        description="Return compact context for a search query or explicit node_id/node_type pair.",
        input_schema=_search_schema(required=()),
        add_arguments=_add_graph_context_arguments,
        payload_from_args=_context_payload,
    ),
    GraphCommandSpec(
        command_name="graph-schema",
        tool_name="graph_schema",
        help="Return ontology schema, indexes, profiles, and helpers",
        description="Return ontology schema, search indexes, context profiles, and query helper metadata.",
        input_schema=_object_schema(),
        add_arguments=add_json_output_arguments,
        payload_from_args=_empty_payload,
        requires_runtime=False,
    ),
    GraphCommandSpec(
        command_name="graph-query-helpers",
        tool_name="graph_query_helpers",
        help="Return named read-only graph query helpers",
        description="Return named read-only query helpers for common graph exploration tasks.",
        input_schema=_object_schema(),
        add_arguments=add_json_output_arguments,
        payload_from_args=_empty_payload,
        requires_runtime=False,
    ),
    GraphCommandSpec(
        command_name="graph-architecture-queries",
        tool_name="graph_architecture_queries",
        help="Return the architecture-discovery query catalog",
        description="Return the grouped architecture-discovery Cypher catalog for coding-agent first-step orientation.",
        input_schema=_object_schema(
            {
                "group": {
                    "type": "string",
                    "description": "Optional architecture query group to return.",
                },
            }
        ),
        add_arguments=_add_graph_architecture_arguments,
        payload_from_args=_architecture_payload,
        requires_runtime=False,
    ),
    GraphCommandSpec(
        command_name="graph-query",
        tool_name="graph_query",
        help="Execute a restricted read-only graph query",
        description="Execute a restricted read-only graph query against the configured database.",
        input_schema=_object_schema(
            {
                "statement": {"type": "string"},
                "parameters": {"type": "object"},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_GRAPH_QUERY_LIMIT},
            },
            required=("statement",),
        ),
        add_arguments=_add_graph_query_arguments,
        payload_from_args=_query_payload,
    ),
)
