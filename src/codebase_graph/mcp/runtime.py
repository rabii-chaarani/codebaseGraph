from __future__ import annotations

from dataclasses import dataclass
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from typing import Any

from codebase_graph.db import LadybugCodeGraphStore, create_ladybug_database
from codebase_graph.setup.state import derive_setup_paths, load_setup_config


@dataclass(frozen=True, slots=True)
class GraphRuntimeConfig:
    """Store configuration for graph runtime operations."""
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
    """Process runtime config.

    Args:
        repo_root: Repo root value.
        config_path: The config path to read or write.
        db_path: The db path to read or write.
        manifest_path: The manifest path to read or write.

    Returns:
        The computed result.
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
    """Open graph store.

    Args:
        runtime: The runtime used by the operation.

    Returns:
        The computed result.
    """
    return create_ladybug_database(runtime.db_path, include_fts=True, read_only=True)


def package_version() -> str:
    """Return the installed package version.

    Returns:
        The computed string.
    """
    try:
        return version("codebase-graph")
    except PackageNotFoundError:
        return "0.1.0"
