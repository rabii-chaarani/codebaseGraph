from __future__ import annotations

from dataclasses import dataclass
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from typing import Any

from codebase_graph.db import LadybugCodeGraphStore, create_ladybug_database
from codebase_graph.setup.state import derive_setup_paths, load_setup_config


@dataclass(frozen=True, slots=True)
class GraphRuntimeConfig:
    """Carry configuration needed by MCP server and transport surface operations."""
    repo_root: Path
    db_path: Path
    manifest_path: Path | None = None


def runtime_config(
    *,
    repo_root: str | Path,
    config_path: str | Path | None,
    db_path: str | Path | None,
    manifest_path: str | Path | None,
) -> GraphRuntimeConfig:
    """Manage config within MCP server and transport surface.

    This executes the selected workflow and returns a process status code or result object.

    Args:
        repo_root: Repository root used to resolve graph state paths.
        config_path: Setup configuration path used to resolve runtime state.
        db_path: Ladybug database path, or an in-memory database marker.
        manifest_path: Manifest path used to track previously materialized file partitions.

    Returns:
        GraphRuntimeConfig instance populated with data from the MCP server and transport
        surface workflow.

    Raises:
        FileNotFoundError: Raised when validation or runtime preconditions fail.
    """
    root = Path(repo_root).expanduser().resolve()
    config = Path(config_path).expanduser().resolve() if config_path else derive_setup_paths(root).config_path
    payload: dict[str, Any] = {}
    if config.exists():
        payload = load_setup_config(config)
        root = Path(str(payload["repo_root"])).expanduser().resolve()
    elif db_path is None:
        raise FileNotFoundError(f"codebaseGraph setup config is missing: {config}")
    resolved_db = Path(db_path or payload["database_path"]).expanduser().resolve()
    resolved_manifest = (
        Path(manifest_path or payload.get("manifest_path", "")).expanduser().resolve()
        if (manifest_path or payload.get("manifest_path"))
        else None
    )
    if not resolved_db.exists():
        raise FileNotFoundError(f"codebaseGraph database is missing: {resolved_db}")
    return GraphRuntimeConfig(repo_root=root, db_path=resolved_db, manifest_path=resolved_manifest)


def open_graph_store(runtime: GraphRuntimeConfig) -> LadybugCodeGraphStore:
    """Open graph store for MCP server and transport surface.

    Args:
        runtime: Resolved runtime paths and graph database settings.

    Returns:
        LadybugCodeGraphStore instance populated with data from the MCP server and transport
        surface workflow.
    """
    return create_ladybug_database(runtime.db_path, include_fts=True, read_only=True)


def package_version() -> str:
    """Return version for MCP server and transport surface.

    Returns:
        Formatted text returned to the caller.
    """
    try:
        return version("codebase-graph")
    except PackageNotFoundError:
        return "0.1.0"
